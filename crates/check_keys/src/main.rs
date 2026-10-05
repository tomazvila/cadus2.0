//! The key-correctness self-check (C3) — the acceptance command of every
//! content worker.
//!
//! For every exemplar and every approved template sample the tool grades the
//! authored answer against itself through the deterministic grade path:
//!
//! - the authored answer must parse under its contract (or, with no contract,
//!   under the topic's `answer_kind`),
//! - the grader must return CORRECT for it, and never `Outcome::Undecidable`,
//! - a mutated answer (+1 on one numeric component) must grade WRONG; for a
//!   `property` item, whose authored answer is one example of many, the first
//!   near miss that does not grade CORRECT must grade WRONG.
//!
//! Templates come from the approved rows of `content_store` (`kind =
//! 'template'`, read-only), keyed by the serving key `<topic_id>/<kp_id>` of
//! the loaded curriculum. The tool runs offline: no model call, no write.
//!
//! Output: one failure line per defect, `FILE:LINE topic/kp item# reason`,
//! then a summary line per course and one overall line. The exit code is 0
//! when `failed == 0` and 1 otherwise.

mod mutate;
mod policy;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use cadus_core::answer::{AnswerContract, Outcome, check, check_contract};
use cadus_core::curriculum::{
    AnswerKind, Finding, KnowledgePoint, ParsedUnit, Topic, parse_curriculum,
};
use cadus_core::template::{Compiled, TemplateDoc, from_body};

use policy::KeyPolicy;

/// The environment variable that names the curriculum tree (C5 convention).
const CURRICULUM_ENV: &str = "CADUS_CURRICULUM";

/// The tree to check when neither an argument nor the variable names one.
const DEFAULT_CURRICULUM: &str = "curriculum";

/// Where one check happened, for the `FILE:LINE topic/kp` prefix.
#[derive(Debug, Clone)]
struct Loc {
    /// The file path relative to the curriculum root.
    file: String,
    /// The 1-based line of the topic id in the file, or 0 when not found.
    line: usize,
    /// The course slug, for the per-course summary.
    course: String,
    /// The topic id.
    topic: String,
    /// The knowledge point id, empty for a topic-level item.
    kp: String,
}

impl Loc {
    /// The `topic/kp` middle of a failure line.
    fn path(&self) -> String {
        if self.kp.is_empty() {
            self.topic.clone()
        } else {
            format!("{}/{}", self.topic, self.kp)
        }
    }
}

/// One reported defect.
#[derive(Debug)]
struct Failure {
    at: Loc,
    item: String,
    reason: String,
}

/// The outcome of one item check. A failure is already on the failure list.
enum Status {
    /// The item passed every rung it could reach; a note is optional.
    Passed(Option<String>),
    /// The item carries no deterministic answer: teach-only content.
    TeachOnly,
    /// The item failed; the reason is already on the failure list.
    Failed,
}

/// One counter event.
#[derive(Debug, Clone, Copy)]
enum Event {
    Checked,
    Failed,
    Skipped,
}

/// The per-course counters, and the totals.
#[derive(Debug, Default)]
struct Counts {
    checked: usize,
    failed: usize,
    skipped: usize,
}

/// One approved template row of `content_store`.
struct TemplateRow {
    kp_id: String,
    digest: String,
    body: String,
}

fn main() -> ExitCode {
    let Some(root) = curriculum_root() else {
        eprintln!("usage: check_keys [--curriculum CURRICULUM_DIR]");
        return ExitCode::from(2);
    };
    let parsed = parse_curriculum(&root);
    if let Some(error) = &parsed.error {
        eprintln!("check_keys: {error}");
        return ExitCode::from(2);
    }

    let mut failures: Vec<Failure> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let mut per_course: BTreeMap<String, Counts> = BTreeMap::new();
    let mut totals = Counts::default();

    // Parse-stage findings are defects of the tree itself: a file the loader
    // dropped is content the self-check can never reach.
    for finding in &parsed.findings {
        let course = finding.file.as_deref().and_then(rel_course);
        let bucket = bucket_of(&mut per_course, course.as_deref());
        let file = file_of(finding);
        if finding.fatal {
            record_event(bucket, &mut totals, Event::Failed);
            failures.push(Failure {
                at: Loc {
                    file,
                    line: 0,
                    course: course.unwrap_or_default(),
                    topic: finding.topic.clone().unwrap_or_default(),
                    kp: String::new(),
                },
                item: "curriculum".to_owned(),
                reason: format!("{}: {}", finding.code, finding.message),
            });
        } else {
            notes.push(format!(
                "note: {file}: {}: {}",
                finding.code, finding.message
            ));
            record_event(bucket, &mut totals, Event::Checked);
        }
    }

    // The exemplar pass over every unit file.
    let mut kp_lines: BTreeMap<String, Loc> = BTreeMap::new();
    for unit in &parsed.units {
        check_unit(
            unit,
            &root,
            &mut failures,
            &mut notes,
            &mut per_course,
            &mut totals,
            &mut kp_lines,
        );
    }

    // The template pass over the approved content-store rows.
    let database_url = env::var("DATABASE_URL").ok().filter(|url| !url.is_empty());
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("check_keys: cannot start the tokio runtime: {error}");
            return ExitCode::from(2);
        }
    };
    let template_result = runtime.block_on(run_templates(
        database_url.as_deref(),
        &kp_lines,
        &mut failures,
        &mut notes,
        &mut per_course,
        &mut totals,
    ));
    if let Err(error) = template_result {
        eprintln!("check_keys: {error}");
        return ExitCode::from(2);
    }

    for note in &notes {
        println!("{note}");
    }
    for failure in &failures {
        println!(
            "{}:{} {} {}: {}",
            failure.at.file,
            failure.at.line,
            failure.at.path(),
            failure.item,
            failure.reason
        );
    }
    for (course, counts) in &per_course {
        println!(
            "course {course}: checked={} failed={} skipped_teach_only={}",
            counts.checked, counts.failed, counts.skipped
        );
    }
    println!(
        "checked={} failed={} skipped_teach_only={}",
        totals.checked, totals.failed, totals.skipped
    );
    if totals.failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// The curriculum root: `--curriculum DIR`, a bare positional,
/// `$CADUS_CURRICULUM`, then `curriculum`.
fn curriculum_root() -> Option<PathBuf> {
    let mut args = env::args().skip(1);
    while let Some(argument) = args.next() {
        if argument == "--curriculum" {
            return args.next().map(PathBuf::from);
        }
        if let Some(path) = argument.strip_prefix("--curriculum=") {
            return Some(PathBuf::from(path));
        }
        if !argument.starts_with('-') {
            return Some(PathBuf::from(argument));
        }
    }
    env::var(CURRICULUM_ENV)
        .ok()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| Some(PathBuf::from(DEFAULT_CURRICULUM)))
}

/// The course slug of a relative unit path, `<course>/<file>`.
fn rel_course(rel_path: &str) -> Option<String> {
    rel_path.split('/').next().map(str::to_owned)
}

fn file_of(finding: &Finding) -> String {
    finding
        .file
        .clone()
        .unwrap_or_else(|| "<curriculum>".to_owned())
}

/// The counter bucket of one course, creating the line when it is new.
fn bucket_of<'a>(
    per_course: &'a mut BTreeMap<String, Counts>,
    course: Option<&str>,
) -> &'a mut Counts {
    per_course
        .entry(course.unwrap_or("<unknown>").to_owned())
        .or_default()
}

/// Record one event in one course bucket and in the totals.
fn record_event(bucket: &mut Counts, totals: &mut Counts, event: Event) {
    match event {
        Event::Checked => {
            bucket.checked += 1;
            totals.checked += 1;
        }
        Event::Failed => {
            bucket.failed += 1;
            totals.failed += 1;
        }
        Event::Skipped => {
            bucket.skipped += 1;
            totals.skipped += 1;
        }
    }
}

/// Record one item status: counters, plus the note line of a passed item.
fn record(
    status: Status,
    item: &str,
    at: &Loc,
    notes: &mut Vec<String>,
    per_course: &mut BTreeMap<String, Counts>,
    totals: &mut Counts,
) {
    match status {
        Status::Passed(note) => {
            record_event(
                bucket_of(per_course, Some(&at.course)),
                totals,
                Event::Checked,
            );
            if let Some(reason) = note {
                notes.push(format!(
                    "note: {}:{} {} {item}: {reason}",
                    at.file,
                    at.line,
                    at.path()
                ));
            }
        }
        Status::TeachOnly => record_event(
            bucket_of(per_course, Some(&at.course)),
            totals,
            Event::Skipped,
        ),
        Status::Failed => {
            record_event(
                bucket_of(per_course, Some(&at.course)),
                totals,
                Event::Checked,
            );
            record_event(
                bucket_of(per_course, Some(&at.course)),
                totals,
                Event::Failed,
            );
        }
    }
}

/// The serving key of one knowledge point, `<topic_id>/<kp_id>`.
fn kp_key(topic: &Topic, kp: &KnowledgePoint) -> String {
    format!("{}/{}", topic.id, kp.id)
}

/// The first line of the file that declares the topic id.
fn find_topic_line(text: &str, topic_id: &str) -> usize {
    for (index, line) in text.lines().enumerate() {
        if line_declares_id(line, topic_id) {
            return index + 1;
        }
    }
    0
}

/// Whether one line declares `id: <id>` or `"id": "<id>"`.
fn line_declares_id(line: &str, id: &str) -> bool {
    let yaml_key = format!("id: {id}");
    if let Some(position) = line.find(&yaml_key) {
        let rest = &line[position + yaml_key.len()..];
        if rest.is_empty() || rest.starts_with([' ', ',', '\t']) {
            return true;
        }
    }
    line.contains(&format!("\"id\": \"{id}\""))
}

/// Walk one unit file: every topic, every knowledge point, every exemplar.
fn check_unit(
    unit: &ParsedUnit,
    root: &Path,
    failures: &mut Vec<Failure>,
    notes: &mut Vec<String>,
    per_course: &mut BTreeMap<String, Counts>,
    totals: &mut Counts,
    kp_lines: &mut BTreeMap<String, Loc>,
) {
    let rel_path = unit.rel_path();
    let text = fs::read_to_string(root.join(&rel_path)).unwrap_or_default();
    let course = unit.course_id.clone();
    for topic in &unit.unit.topics {
        let line = find_topic_line(&text, topic.id.as_str());
        // The topic-level diagnostic exemplar is authored content too.
        if let Some(diagnostic) = &topic.diagnostic_exemplar {
            let at = Loc {
                file: rel_path.clone(),
                line,
                course: course.clone(),
                topic: topic.id.as_str().to_owned(),
                kp: String::new(),
            };
            let status = check_answer(
                &diagnostic.answer,
                diagnostic.answer_contract.as_ref(),
                topic.answer_kind,
                &at,
                "diagnostic",
                failures,
            );
            record(status, "diagnostic", &at, notes, per_course, totals);
        }
        for kp in &topic.knowledge_points {
            let at = Loc {
                file: rel_path.clone(),
                line,
                course: course.clone(),
                topic: topic.id.as_str().to_owned(),
                kp: kp.id.as_str().to_owned(),
            };
            kp_lines.insert(kp_key(topic, kp), at.clone());
            if kp.exemplars.is_empty() {
                record_event(
                    bucket_of(per_course, Some(&at.course)),
                    totals,
                    Event::Skipped,
                );
                continue;
            }
            for (index, exemplar) in kp.exemplars.iter().enumerate() {
                let item = format!("exemplar#{index}");
                let status = check_answer(
                    &exemplar.answer,
                    exemplar.answer_contract.as_ref(),
                    topic.answer_kind,
                    &at,
                    &item,
                    failures,
                );
                record(status, &item, &at, notes, per_course, totals);
            }
        }
    }
}

/// The three rungs of one authored answer: parse, correct, wrong-when-mutated.
///
/// A contract of `None`, or no contract under an undecidable answer kind, is
/// teach-only content and no failure.
fn check_answer(
    answer: &str,
    contract: Option<&AnswerContract>,
    kind: AnswerKind,
    at: &Loc,
    item: &str,
    failures: &mut Vec<Failure>,
) -> Status {
    // Rung 1: the one verdict rule decides, and the authored answer parses.
    match policy::key_policy(answer, contract, kind) {
        KeyPolicy::Verdict => {}
        KeyPolicy::TeachOnly => return Status::TeachOnly,
        KeyPolicy::BadKey(reason) => {
            failures.push(Failure {
                at: at.clone(),
                item: item.to_owned(),
                reason: format!(
                    "authored answer does not parse under {}: {reason}",
                    policy::policy_of(contract, kind)
                ),
            });
            return Status::Failed;
        }
    }
    // Rung 2: the grader returns CORRECT for the authored answer.
    match grade(answer, answer, contract, kind) {
        Outcome::Decided(v) if v.correct => {}
        Outcome::Decided(_) => {
            failures.push(Failure {
                at: at.clone(),
                item: item.to_owned(),
                reason: "the grader returns WRONG for the authored answer itself".to_owned(),
            });
            return Status::Failed;
        }
        Outcome::Undecidable(reason) => {
            failures.push(Failure {
                at: at.clone(),
                item: item.to_owned(),
                reason: format!("the grader returns UNDECIDABLE for the authored answer: {reason}"),
            });
            return Status::Failed;
        }
    }
    // Rung 3 of a property item: the stored answer is one example, so a +1
    // mutant may have the property too. A near miss that the predicate
    // rejects must exist, or the item accepts almost anything.
    if let Some(misses) = contract.and_then(|contract| contract.property_near_misses(answer)) {
        let Some(miss) = misses.iter().find(|learner| {
            !matches!(grade(answer, learner, contract, kind), Outcome::Decided(v) if v.correct)
        }) else {
            failures.push(Failure {
                at: at.clone(),
                item: item.to_owned(),
                reason: "every near miss of the property example grades CORRECT".to_owned(),
            });
            return Status::Failed;
        };
        return mutant_status(answer, miss, contract, kind, at, item, failures);
    }
    // Rung 3: the +1 mutation grades WRONG, or no numeric component exists.
    let Some(mutant) = mutate::mutant_for(answer, contract) else {
        // A closed-choice answer mutates to another option of its vocabulary.
        if let Some(alternative) = contract.and_then(|contract| label_mutant(answer, contract)) {
            return mutant_status(answer, &alternative, contract, kind, at, item, failures);
        }
        return Status::Passed(Some("no numeric component to mutate".to_owned()));
    };
    mutant_status(answer, &mutant, contract, kind, at, item, failures)
}

/// Grade one mutant and turn the verdict into an item status.
fn mutant_status(
    expected: &str,
    mutant: &str,
    contract: Option<&AnswerContract>,
    kind: AnswerKind,
    at: &Loc,
    item: &str,
    failures: &mut Vec<Failure>,
) -> Status {
    match grade(expected, mutant, contract, kind) {
        Outcome::Decided(v) if v.correct => {
            failures.push(Failure {
                at: at.clone(),
                item: item.to_owned(),
                reason: format!("the mutated answer {mutant:?} still grades CORRECT"),
            });
            Status::Failed
        }
        Outcome::Decided(_) => Status::Passed(None),
        Outcome::Undecidable(reason) => {
            failures.push(Failure {
                at: at.clone(),
                item: item.to_owned(),
                reason: format!("the mutated answer {mutant:?} grades UNDECIDABLE: {reason}"),
            });
            Status::Failed
        }
    }
}

/// Grade one pair through the policy the runtime uses.
fn grade(
    expected: &str,
    learner: &str,
    contract: Option<&AnswerContract>,
    kind: AnswerKind,
) -> Outcome {
    match contract {
        Some(contract) => check_contract(expected, learner, contract.clone()),
        None => check(expected, learner, kind),
    }
}

/// One mutant for a closed-choice answer: another option of the vocabulary.
///
/// A label answer ("Yes", "definition") carries no numeric component, so the
/// +1 mutation has nothing to reach; the wrong-answer rung reads a different
/// option instead.
fn label_mutant(expected: &str, contract: &AnswerContract) -> Option<String> {
    let AnswerContract::Label { options } = contract else {
        return None;
    };
    options
        .iter()
        .filter_map(|group| group.first())
        .find(|candidate| {
            matches!(
                check_contract(expected, candidate, contract.clone()),
                Outcome::Decided(v) if !v.correct
            )
        })
        .cloned()
}

// --------------------------------------------------------------------------
// The template pass
// --------------------------------------------------------------------------

/// Check every approved template against the loaded curriculum.
///
/// The read is one SELECT over `content_store`, on a read-only session.
async fn run_templates(
    database_url: Option<&str>,
    kp_lines: &BTreeMap<String, Loc>,
    failures: &mut Vec<Failure>,
    notes: &mut Vec<String>,
    per_course: &mut BTreeMap<String, Counts>,
    totals: &mut Counts,
) -> Result<(), String> {
    let Some(url) = database_url else {
        notes.push("note: DATABASE_URL is unset; the template pass is skipped".to_owned());
        return Ok(());
    };
    let rows = fetch_templates(url).await?;
    for row in rows {
        let Some(at) = kp_lines.get(&row.kp_id) else {
            continue; // a template of a knowledge point outside this curriculum
        };
        check_template(&row, at, failures, notes, per_course, totals);
    }
    Ok(())
}

/// Read the approved template rows, read-only.
async fn fetch_templates(url: &str) -> Result<Vec<TemplateRow>, String> {
    use sqlx::Row;
    let options = url
        .parse::<sqlx::postgres::PgConnectOptions>()
        .map_err(|error| format!("cannot parse DATABASE_URL: {error}"))?
        // The self-check never writes: the session refuses a write up front.
        .options([("default_transaction_read_only", "on")]);
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect_with(options)
        .await
        .map_err(|error| format!("cannot connect to the content store: {error}"))?;
    let rows = sqlx::query(
        "SELECT kp_id, digest, body FROM content_store \
         WHERE kind = 'template' AND status = 'approved' \
         ORDER BY approved_at DESC NULLS LAST, created_at DESC, digest",
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("cannot read content_store: {error}"))?;
    let mut out = Vec::new();
    for row in rows {
        let body = row
            .try_get::<serde_json::Value, _>("body")
            .map_err(|error| format!("content_store.body is not JSON: {error}"))?;
        out.push(TemplateRow {
            kp_id: row.try_get("kp_id").map_err(|e| e.to_string())?,
            digest: row.try_get("digest").map_err(|e| e.to_string())?,
            body: body.to_string(),
        });
    }
    pool.close().await;
    Ok(out)
}

/// Check one approved template document: grammar, every sample, every mutant.
fn check_template(
    row: &TemplateRow,
    at: &Loc,
    failures: &mut Vec<Failure>,
    notes: &mut Vec<String>,
    per_course: &mut BTreeMap<String, Counts>,
    totals: &mut Counts,
) {
    let item = format!("template#{}", &row.digest[..row.digest.len().min(8)]);
    let document: TemplateDoc = match from_body(&row.body) {
        Ok(document) => document,
        Err(error) => {
            record_event(
                bucket_of(per_course, Some(&at.course)),
                totals,
                Event::Failed,
            );
            failures.push(Failure {
                at: at.clone(),
                item,
                reason: format!("the template document does not parse: {error}"),
            });
            return;
        }
    };
    let contract = document.answer_contract.as_ref();
    if !policy::has_checker(contract, document.answer_kind) {
        record_event(
            bucket_of(per_course, Some(&at.course)),
            totals,
            Event::Skipped,
        );
        return;
    }
    if document.samples.is_empty() {
        record_event(
            bucket_of(per_course, Some(&at.course)),
            totals,
            Event::Skipped,
        );
        notes.push(format!(
            "note: {}:{} {item}: the approved template declares no samples",
            at.file, at.line
        ));
        return;
    }
    let compiled = match Compiled::new(&document) {
        Ok(compiled) => compiled,
        Err(error) => {
            record_event(
                bucket_of(per_course, Some(&at.course)),
                totals,
                Event::Failed,
            );
            failures.push(Failure {
                at: at.clone(),
                item,
                reason: format!("the template does not compile: {error}"),
            });
            return;
        }
    };
    for (index, sample) in document.samples.iter().enumerate() {
        let sample_item = format!("{item} sample#{index}");
        let instance = match compiled.instantiate(sample.bindings()) {
            Ok(instance) => instance,
            Err(error) => {
                record_event(
                    bucket_of(per_course, Some(&at.course)),
                    totals,
                    Event::Failed,
                );
                failures.push(Failure {
                    at: at.clone(),
                    item: sample_item,
                    reason: format!("the sample does not instantiate: {error}"),
                });
                continue;
            }
        };
        let status = check_answer(
            &instance.answer,
            contract,
            document.answer_kind,
            at,
            &sample_item,
            failures,
        );
        record(status, &sample_item, at, notes, per_course, totals);
    }
}
