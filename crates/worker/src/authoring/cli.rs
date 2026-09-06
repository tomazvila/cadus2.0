//! The operator entry point of the authoring pipeline (spec section 7, row R8).
//!
//! `cadus-worker` runs its tick loop when it gets no argument. `cadus-worker
//! author` runs one authoring pass instead: it reads the curriculum, picks the
//! knowledge points and the kinds the operator named, counts the bank slots
//! those pairs hold, and either PRINTS the plan (`--dry-run`) or runs it.
//!
//! # Why the dry run exists
//!
//! An authoring pass spends money (T3). The plan names the work before the run
//! starts it, so an operator reads the bill first. A dry run therefore builds NO
//! model client, opens no endpoint, and writes no row: it reads the curriculum
//! and the slot counts, and it prints.
//!
//! # What this module holds
//!
//! Every item here is pure but [`plan`], which reads the slot counts. The
//! parser, the selection and the rendering take values and give values, so a
//! test asserts literal text with no process and no database.

use std::fmt::Write as _;

use cadus_core::curriculum::{Curriculum, KnowledgePoint, Topic};
use cadus_core::pool::{kp_key, split_kp_key};
use cadus_store::Db;

use crate::WorkerError;
use crate::authoring::job::{AUTHORING_ATTEMPTS, bank_target, slots_taken};
use crate::authoring::prompt::{AuthoringSpec, KINDS, Kind};

/// The subcommand name of one authoring pass.
pub const AUTHOR: &str = "author";

/// The subcommand name of one readiness audit (D-F5).
pub const READINESS: &str = "readiness";

/// The text `--help` prints.
pub const HELP: &str = "\
cadus-worker — the Cadus background worker

USAGE:
    cadus-worker                    run the tick loop (refill, diagnosis)
    cadus-worker author [OPTIONS]   run one authoring pass
    cadus-worker readiness [OPTS]   audit the content and write the report
    cadus-worker --help             print this text

AUTHOR OPTIONS:
    --kp <topic_id/kp_id>   author for this knowledge point; repeatable.
                            The default is every knowledge point of the tree.
    --kind <kind>           author this kind; repeatable. One of template,
                            teach, hint_ladder, diagnosis. The default is all
                            four.
    --dry-run               print the plan, and make no model call and no write.
    --budget-usd <amount>   required for paid runs; hard shared reservation cap.
    --request-reserve-usd <amount>
                            required upper price bound for each HTTP request,
                            including the largest truncation output and fees.
    --concurrency <1..16>    active knowledge points; default 1. Kinds stay ordered.
    --stale                 list the approved documents an older prompt wrote,
                            and make no model call and no write.

READINESS OPTIONS:
    --course <id>           audit this course only. The default is every course.
    --json <path>           write the JSON report to this file.
    --md <path>             write the Markdown report to this file.

ENVIRONMENT:
    DATABASE_URL       the connection the pass reads and writes (cadus_admin).
    CADUS_CURRICULUM   the curriculum tree. The default is ./curriculum.
    OPENAI_API_KEY     the endpoint key. A run without it is refused; a dry run
                       needs none.
";

/// What the command line asks the process to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Run the tick loop. This is the no-argument case.
    Serve,
    /// Print [`HELP`] and exit 0.
    Help,
    /// Run one authoring pass.
    Author(AuthorArgs),
    /// Run one readiness audit (D-F5).
    Readiness(ReadinessArgs),
}

/// The options of one `readiness` subcommand.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReadinessArgs {
    /// The course the audit covers. `None` covers every course.
    pub course: Option<String>,
    /// The file the JSON report goes to.
    pub json: Option<String>,
    /// The file the Markdown report goes to.
    pub md: Option<String>,
}

/// The options of one `author` subcommand.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AuthorArgs {
    /// The serving keys the operator named. Empty means every knowledge point.
    pub kps: Vec<String>,
    /// The kinds the operator named. Empty means every kind of [`KINDS`].
    pub kinds: Vec<Kind>,
    /// Print the plan and make no call.
    pub dry_run: bool,
    /// List the approved documents an older prompt wrote, and make no call.
    ///
    /// Spec section 2.2, "Prompt digest": a prompt edit marks the affected rows
    /// for re-authoring and never unapproves one, so an operator needs a way to
    /// read the mark (M6 review finding F4).
    pub stale: bool,
    /// Total reservation cap in millionths of a dollar.
    pub budget_micros: Option<u64>,
    /// Provider upper cost bound per HTTP request in millionths of a dollar.
    pub request_reserve_micros: Option<u64>,
    /// Active knowledge points; zero selects the default of one.
    pub concurrency: usize,
}

impl AuthorArgs {
    /// The kinds this pass covers, in the order of [`KINDS`].
    ///
    /// The order is the constant's and not the operator's, so two calls that
    /// name the same kinds in another order print the same plan.
    #[must_use]
    pub fn kinds(&self) -> Vec<Kind> {
        if self.kinds.is_empty() {
            return KINDS.to_vec();
        }
        KINDS
            .into_iter()
            .filter(|kind| self.kinds.contains(kind))
            .collect()
    }
}

/// A command line the process refuses.
///
/// The message is the line the process prints on stderr before it exits 2. It
/// names the argument at fault, so an operator repairs the call without this
/// file.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct CliError(pub String);

/// Read the arguments of the process, without the program name.
///
/// # Errors
///
/// Returns [`CliError`] for an unknown argument, an option with no value, an
/// unknown kind, and an unknown subcommand.
pub fn parse<S: AsRef<str>>(args: &[S]) -> Result<Command, CliError> {
    let mut args = args.iter().map(AsRef::as_ref);
    let Some(first) = args.next() else {
        return Ok(Command::Serve);
    };
    if first == "--help" || first == "-h" {
        return Ok(Command::Help);
    }
    if first == READINESS {
        return parse_readiness(args);
    }
    if first != AUTHOR {
        return Err(CliError(format!(
            "unknown argument `{first}` — run `cadus-worker --help`"
        )));
    }

    let mut parsed = AuthorArgs::default();
    while let Some(argument) = args.next() {
        match argument {
            "--dry-run" => parsed.dry_run = true,
            "--stale" => parsed.stale = true,
            "--budget-usd" | "--request-reserve-usd" => {
                let value = value_of(argument, args.next())?;
                let money = crate::authoring::budget::usd_micros(&value).map_err(CliError)?;
                if argument == "--budget-usd" {
                    parsed.budget_micros = Some(money);
                } else {
                    parsed.request_reserve_micros = Some(money);
                }
            }
            "--concurrency" => {
                let value = value_of(argument, args.next())?;
                parsed.concurrency = value
                    .parse()
                    .ok()
                    .filter(|n| (1..=16).contains(n))
                    .ok_or_else(|| CliError("author concurrency must be in 1..=16".to_owned()))?;
            }
            "--help" | "-h" => return Ok(Command::Help),
            "--kp" => parsed.kps.push(value_of("--kp", args.next())?),
            "--kind" => {
                let raw = value_of("--kind", args.next())?;
                let kind = Kind::from_wire(&raw).ok_or_else(|| {
                    CliError(format!(
                        "unknown kind `{raw}` — the kinds are template, teach, hint_ladder, diagnosis"
                    ))
                })?;
                parsed.kinds.push(kind);
            }
            other => {
                return Err(CliError(format!(
                    "unknown option `{other}` — run `cadus-worker --help`"
                )));
            }
        }
    }
    Ok(Command::Author(parsed))
}

/// Read the options of the `readiness` subcommand.
///
/// # Errors
///
/// Returns [`CliError`] for an unknown option and for an option with no value.
fn parse_readiness<'a>(mut args: impl Iterator<Item = &'a str>) -> Result<Command, CliError> {
    let mut parsed = ReadinessArgs::default();
    while let Some(argument) = args.next() {
        match argument {
            "--help" | "-h" => return Ok(Command::Help),
            "--course" => parsed.course = Some(value_of("--course", args.next())?),
            "--json" => parsed.json = Some(value_of("--json", args.next())?),
            "--md" => parsed.md = Some(value_of("--md", args.next())?),
            other => {
                return Err(CliError(format!(
                    "unknown option `{other}` — run `cadus-worker --help`"
                )));
            }
        }
    }
    Ok(Command::Readiness(parsed))
}

/// The value of one option, or the refusal an option with no value earns.
///
/// A value that starts with `--` is the next option and not a value: `--kp
/// --dry-run` is a typing slip, and a knowledge point named `--dry-run` does
/// not exist.
fn value_of(option: &str, value: Option<&str>) -> Result<String, CliError> {
    match value {
        Some(text) if !text.starts_with("--") => Ok(text.to_owned()),
        _ => Err(CliError(format!("the option `{option}` needs a value"))),
    }
}

/// The authoring specs of the knowledge points the operator named.
///
/// An empty `keys` list selects EVERY knowledge point of the tree, in curriculum
/// order. A named key selects one, and the answer keeps the order the operator
/// wrote.
///
/// Every spec states no difficulty target. The curriculum carries a topic
/// difficulty number, not the sentence the prompt asks for, so the prompt takes
/// `prompt::DEFAULT_DIFFICULTY`.
///
/// # Errors
///
/// Returns [`CliError`] naming a key the curriculum does not hold.
pub fn select(curriculum: &Curriculum, keys: &[String]) -> Result<Vec<AuthoringSpec>, CliError> {
    if keys.is_empty() {
        return Ok(every_spec(curriculum));
    }
    let mut specs = Vec::with_capacity(keys.len());
    for key in keys {
        specs.push(one_spec(curriculum, key)?);
    }
    Ok(specs)
}

/// Every knowledge point of the tree, in curriculum order.
fn every_spec(curriculum: &Curriculum) -> Vec<AuthoringSpec> {
    let mut specs = Vec::new();
    for topic in curriculum.topics() {
        for kp in &topic.knowledge_points {
            specs.push(spec_of(topic, kp));
        }
    }
    specs
}

/// The spec of one serving key, or the refusal an unknown key earns.
fn one_spec(curriculum: &Curriculum, key: &str) -> Result<AuthoringSpec, CliError> {
    let Some((topic_id, kp_id)) = split_kp_key(key) else {
        return Err(CliError(format!(
            "the knowledge point `{key}` is not a serving key — write it as `<topic_id>/<kp_id>`"
        )));
    };
    curriculum
        .topics()
        .iter()
        .find(|topic| topic.id.as_str() == topic_id)
        .and_then(|topic| {
            topic
                .knowledge_points
                .iter()
                .find(|kp| kp.id.as_str() == kp_id)
                .map(|kp| spec_of(topic, kp))
        })
        .ok_or_else(|| CliError(format!("the curriculum holds no knowledge point `{key}`")))
}

/// One curriculum knowledge point, as the spec the prompt reads.
fn spec_of(topic: &Topic, kp: &KnowledgePoint) -> AuthoringSpec {
    AuthoringSpec {
        kp_id: kp.id.as_str().to_owned(),
        kp_name: kp.name.clone(),
        topic_id: topic.id.as_str().to_owned(),
        topic_name: topic.name.clone(),
        answer_kind: topic.answer_kind,
        difficulty_target: None,
        constraints: kp.constraints.clone(),
        exemplars: kp.exemplars.clone(),
    }
}

/// One line of the plan: what one knowledge point and kind needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanRow {
    /// The serving key `"<topic_id>/<kp_id>"`.
    pub kp_id: String,
    /// The kind this line plans.
    pub kind: Kind,
    /// The approved and pending documents the pair holds now.
    pub taken: i64,
    /// The documents the pair keeps when the bank is full.
    pub target: i64,
}

impl PlanRow {
    /// The documents this pass authors for the pair: 1, or 0 for a full bank.
    ///
    /// One pass authors at most ONE document per pair
    /// ([`author_one`](crate::authoring::job::author_one)), whatever the bank
    /// is short of. A template bank of three therefore fills over three passes,
    /// and every pass gets a review of what the pass before it stored (C6).
    #[must_use]
    pub const fn to_author(&self) -> i64 {
        if self.taken < self.target { 1 } else { 0 }
    }
}

/// Count the bank slots of every pair the pass covers.
///
/// The read is the `slots_taken` of the loop itself, so the plan states the
/// loop's own decision and not a second rule.
///
/// # Errors
///
/// Returns [`WorkerError::Store`] when a count fails or the bound expires.
pub async fn plan(
    db: &Db,
    specs: &[AuthoringSpec],
    kinds: &[Kind],
) -> Result<Vec<PlanRow>, WorkerError> {
    let mut rows = Vec::with_capacity(specs.len().saturating_mul(kinds.len()));
    for spec in specs {
        let key = kp_key(&spec.topic_id, &spec.kp_id);
        for kind in kinds {
            let taken = slots_taken(db, &key, *kind).await?;
            rows.push(PlanRow {
                kp_id: key.clone(),
                kind: *kind,
                taken,
                target: bank_target(*kind),
            });
        }
    }
    Ok(rows)
}

/// The documents the plan authors: one per pair whose bank is short.
#[must_use]
pub fn documents(rows: &[PlanRow]) -> i64 {
    rows.iter().map(PlanRow::to_author).sum()
}

/// The model calls the plan spends: the floor and the ceiling (T3).
///
/// One document takes one call when the gate accepts the first reply, and
/// [`AUTHORING_ATTEMPTS`](crate::authoring::job::AUTHORING_ATTEMPTS) calls when
/// every attempt is refused. Each author attempt also includes the transport
/// retry bound. The pair is the operator's bill before the pass
/// runs.
#[must_use]
pub fn call_bounds(rows: &[PlanRow]) -> (i64, i64) {
    let documents = documents(rows);
    (
        documents,
        documents
            .saturating_mul(i64::from(AUTHORING_ATTEMPTS))
            .saturating_mul(i64::from(cadus_model_client::MAX_ATTEMPTS)),
    )
}

/// The plan, as the text the process prints on stdout.
///
/// One line names the columns, one line follows per pair, and the last line
/// totals the pass. The `author` column is 1 or 0, because one pass authors at
/// most one document per pair. Every field is fixed, so an operator reads the
/// text and a test asserts it.
#[must_use]
pub fn render_plan(rows: &[PlanRow], dry_run: bool) -> String {
    let mut text = String::from("authoring plan\nkp_id kind taken target author\n");
    for row in rows {
        let _ = writeln!(
            text,
            "{} {} {} {} {}",
            row.kp_id,
            row.kind.as_str(),
            row.taken,
            row.target,
            row.to_author()
        );
    }
    let (least, most) = call_bounds(rows);
    let _ = writeln!(
        text,
        "plan: pairs {}, documents {}, model calls {least} to {most}",
        rows.len(),
        documents(rows)
    );
    if dry_run {
        text.push_str("dry run: no model call and no write\n");
    }
    text
}

/// The result of one pass, as the text the process prints on stdout.
///
/// The counts are the counts of
/// [`BatchReport`](crate::authoring::job::BatchReport), one line per kind, and
/// the decline lines name the knowledge points that used every attempt.
#[must_use]
pub fn render_batch(kind: Kind, report: &crate::authoring::job::BatchReport) -> String {
    let mut text = String::new();
    let _ = writeln!(
        text,
        "{}: stored {} skipped {} declined {} calls {} alerts {}",
        kind.as_str(),
        report.stored,
        report.skipped,
        report.declined,
        report.calls,
        report.alerts
    );
    for decline in &report.declines {
        let _ = writeln!(
            text,
            "declined {} {} after {} attempts: {}",
            decline.kp_id,
            decline.kind.as_str(),
            decline.attempts,
            decline.reasons.last().map_or("(no reason)", String::as_str)
        );
    }
    text
}
