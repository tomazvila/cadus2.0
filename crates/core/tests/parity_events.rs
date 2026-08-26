//! U5 acceptance: event-stream parity over the 20 committed streams (R5, D4, C2).
//!
//! Every expectation is a LITERAL that the 1.0 oracle produced and this repository
//! committed. `tests/fixtures/events/digests_1_0.json` holds one digest per stream
//! per zone (`scripts/oracle/digest_streams_1_0.py`), and
//! `tests/fixtures/events/selector_1_0.json` holds the 1.0 `compose_session` plan of
//! the 10 seeded states (`scripts/oracle/dump_selector_1_0.py`). No expectation
//! calls the code under test.
//!
//! `tests/fixtures/events/coverage.md` records which branch of spec section 9 each
//! stream reaches, measured against the live 1.0 code by
//! `scripts/oracle/coverage_streams_1_0.py`.
//!
//! A stream whose Rust digest differs from its 1.0 digest is a RUST BUG. The
//! failure report names the stream, prints the first differing 200-byte window of
//! the two blobs, and -- with `CADUS_ORACLE_PYTHON` set -- bisects the stream to the
//! event index after which the two models diverge.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use serde::Deserialize;

use cadus_core::config::Config;
use cadus_core::curriculum::{Curriculum, load_curriculum};
use cadus_core::event::{Event, TaskType, Timestamp, WorkQuality};
use cadus_core::fire::{
    AttemptResult, PropagationKind, apply_attempt, memory_at, quality_q, raw_delta,
};
use cadus_core::learner::{LearnerModel, TopicState};
use cadus_core::projector::{
    PROJECTOR_VERSION, ProjectionInput, Projector, apply_regrades, blob_digest, canonical_blob,
    project, project_incremental,
};
use cadus_core::selector::{SeededSampler, SessionContext, compose_session};

/// The number of committed `stream_N.jsonl` streams.
const STREAMS: usize = 20;

/// The coverage stream `stream_u3_coverage.jsonl`, which `incremental_1_0.json`
/// records beside the numbered streams.
///
/// It is the only committed stream that carries the `profile_reset` incremental
/// divergence class (M3 review round 1, finding #4).
const COVERAGE_STREAM: &str = "stream_u3_coverage.jsonl";

/// The `config_hash` of the default config (spec section 9).
const CONFIG_HASH: &str = "797575e985c12149";

/// The 1.0 fold digest of `stream_1.jsonl` (`docs/plans/M3.md`).
const STREAM_1_UTC: &str = "ba128459985e0815db7446cb2af16452ec07d304b7efaa0952fc6567404245f5";

/// The build instant the oracle pins with `--now`.
const NOW: &str = "2000-01-01T00:00:00Z";

/// The non-UTC zone the digests file carries beside UTC.
const NEW_YORK: &str = "America/New_York";

/// The daily XP goal the oracle folds with.
const GOAL: i64 = 40;

/// The task cap `dump_selector_1_0.py` composed with.
///
/// It is 40, above the longest recorded plan, so the comparison reaches the quiz,
/// multi-step, and drill sections of `compose_session`. At the session cap of 8
/// every seeded plan is already full of remediation, review, and lesson tasks
/// (M3 review round 1, finding #13).
const SELECTOR_N: usize = 40;

/// The longest plan any seeded state records, which is below [`SELECTOR_N`].
///
/// A plan AT the cap would mean the cap truncates it again, so the whole-plan
/// comparison would end early once more.
const SELECTOR_LONGEST_PLAN: usize = 14;

/// The number of seeded selector states.
const SELECTOR_SEEDS: usize = 10;

/// The window either side of the first differing byte, in a mismatch report.
const DIFF_WINDOW: usize = 200;

/// The tolerance of the implicit-credit comparison: the two sides compute the same
/// product from the same inputs, so only the last bit may differ.
const CREDIT_EPSILON: f64 = 1e-12;

// --------------------------------------------------------------------------- //
// The committed oracle index
// --------------------------------------------------------------------------- //

/// One stream's row of `digests_1_0.json`.
#[derive(Debug, Deserialize)]
struct StreamDigests {
    /// The fixture file name.
    stream: String,
    /// The number of events in the file.
    events: usize,
    /// The number of events left after every `regraded` is removed.
    events_without_regrades: usize,
    /// Zone name to blob digest. The key `UTC_no_regrades` is the pre-correction fold.
    digests: BTreeMap<String, String>,
}

/// `digests_1_0.json`, the committed 1.0 digest index.
#[derive(Debug, Deserialize)]
struct DigestIndex {
    /// The oracle that wrote the file.
    oracle: String,
    /// The generator that wrote the streams.
    generator: String,
    /// The pinned `now`.
    now: String,
    /// The daily XP goal.
    goal: i64,
    /// The 1.0 `PROJECTOR_VERSION`.
    projector_version: i64,
    /// The 1.0 `config_hash`.
    config_hash: String,
    /// The zone keys every row carries.
    zones: Vec<String>,
    /// One row per stream, in stream order.
    streams: Vec<StreamDigests>,
}

/// One stream's row of `incremental_1_0.json`.
#[derive(Debug, Deserialize)]
struct StreamIncremental {
    /// The fixture file name.
    stream: String,
    /// The number of events in the file.
    events: usize,
    /// The 1.0 full-replay digest.
    full_digest: String,
    /// The splits at which the 1.0 incremental fold leaves the full replay.
    mismatching_splits: Vec<usize>,
    /// Split index, as text, to the 1.0 digest of that divergent incremental fold.
    mismatching_digests: BTreeMap<String, String>,
}

/// `incremental_1_0.json`, the committed 1.0 incremental-fold reference.
#[derive(Debug, Deserialize)]
struct IncrementalIndex {
    /// The oracle that wrote the file.
    oracle: String,
    /// The 1.0 `config_hash`.
    config_hash: String,
    /// The 1.0 `PROJECTOR_VERSION`.
    projector_version: i64,
    /// One row per stream, in stream order.
    streams: Vec<StreamIncremental>,
}

/// One served task of `selector_1_0.json`, in the recorded shape.
///
/// `[task_type, topic, is_remediation, nearly_due, n_problems, time_budget_secs]`.
type TaskRow = (String, Option<String>, bool, bool, Option<i64>, Option<i64>);

/// One seeded state of `selector_1_0.json`.
#[derive(Debug, Deserialize)]
struct SelectorState {
    /// The seed, which is also the stream number.
    seed: usize,
    /// The fixture the state folds from.
    stream: String,
    /// The session id the task ids are keyed on.
    session_id: String,
    /// The enrolled course, from the last `enrolled` event.
    course_id: Option<String>,
    /// The instant the plan composed at: the last event's `ts`.
    t: String,
    /// `[task_type, topic, is_remediation, nearly_due, n_problems,
    /// time_budget_secs]` per served task.
    tasks: Vec<TaskRow>,
}

/// `selector_1_0.json`, the committed 1.0 `compose_session` plans.
#[derive(Debug, Deserialize)]
struct SelectorIndex {
    /// The oracle that wrote the file.
    oracle: String,
    /// The task cap.
    n: usize,
    /// The 1.0 `config_hash`.
    config_hash: String,
    /// The 1.0 `PROJECTOR_VERSION`.
    projector_version: i64,
    /// One entry per seed, in seed order.
    states: Vec<SelectorState>,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/events")
        .join(name)
}

/// The committed digest index, read once for the whole test binary.
fn digest_index() -> &'static DigestIndex {
    static INDEX: OnceLock<DigestIndex> = OnceLock::new();
    INDEX.get_or_init(|| {
        let text = std::fs::read_to_string(fixture("digests_1_0.json")).expect("the index reads");
        serde_json::from_str(&text).expect("the index parses")
    })
}

/// The committed selector index, read once for the whole test binary.
fn selector_index() -> &'static SelectorIndex {
    static INDEX: OnceLock<SelectorIndex> = OnceLock::new();
    INDEX.get_or_init(|| {
        let text = std::fs::read_to_string(fixture("selector_1_0.json")).expect("the index reads");
        serde_json::from_str(&text).expect("the index parses")
    })
}

/// The committed incremental index, read once for the whole test binary.
fn incremental_index() -> &'static IncrementalIndex {
    static INDEX: OnceLock<IncrementalIndex> = OnceLock::new();
    INDEX.get_or_init(|| {
        let text =
            std::fs::read_to_string(fixture("incremental_1_0.json")).expect("the index reads");
        serde_json::from_str(&text).expect("the index parses")
    })
}

/// The incremental row of one stream number.
fn incremental_row(number: usize) -> &'static StreamIncremental {
    let name = format!("stream_{number}.jsonl");
    incremental_index()
        .streams
        .iter()
        .find(|entry| entry.stream == name)
        .unwrap_or_else(|| panic!("{name} has no row in incremental_1_0.json"))
}

/// The row of one stream number.
fn row(number: usize) -> &'static StreamDigests {
    let name = format!("stream_{number}.jsonl");
    digest_index()
        .streams
        .iter()
        .find(|entry| entry.stream == name)
        .unwrap_or_else(|| panic!("{name} has no row in digests_1_0.json"))
}

/// The checked-in curriculum tree, loaded once for the whole test binary.
fn tree() -> &'static Curriculum {
    static TREE: OnceLock<Curriculum> = OnceLock::new();
    TREE.get_or_init(|| {
        let (curriculum, _findings) =
            load_curriculum(&repo_root().join("curriculum")).expect("the tree loads");
        curriculum
    })
}

fn now() -> Timestamp {
    Timestamp::parse(NOW).unwrap()
}

/// Every event of a JSONL fixture, in file order.
fn stream(name: &str) -> Vec<Event> {
    let text = std::fs::read_to_string(fixture(name)).expect("the fixture reads");
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| Event::from_json(line).unwrap_or_else(|error| panic!("{line}: {error}")))
        .collect()
}

/// The full replay of `events` in `tz`, with the default config.
fn fold(events: &[Event], tz: Option<&str>) -> LearnerModel {
    let cfg = Config::default();
    let input = ProjectionInput::new(tree(), &cfg, now())
        .with_timezone(tz)
        .with_goal(GOAL);
    project(events, &input).expect("the fold succeeds")
}

// --------------------------------------------------------------------------- //
// Mismatch reporting
// --------------------------------------------------------------------------- //

/// The first differing window of two blobs, [`DIFF_WINDOW`] bytes wide.
///
/// `left_label` and `right_label` name the two sides, so a report never claims a
/// Rust blob came from 1.0.
fn labeled_difference(
    left_label: &str,
    left_blob: &str,
    right_label: &str,
    right_blob: &str,
) -> String {
    let left = left_blob.as_bytes();
    let right = right_blob.as_bytes();
    let at = left
        .iter()
        .zip(right.iter())
        .position(|(a, b)| a != b)
        .unwrap_or_else(|| left.len().min(right.len()));
    let from = at.saturating_sub(DIFF_WINDOW / 2);
    let to_left = (from + DIFF_WINDOW).min(left.len());
    let to_right = (from + DIFF_WINDOW).min(right.len());
    format!(
        "first difference at byte {at} (lengths {} and {})\n  {left_label} ...{}...\n  {right_label} ...{}...",
        left.len(),
        right.len(),
        String::from_utf8_lossy(&left[from..to_left]),
        String::from_utf8_lossy(&right[from..to_right]),
    )
}

/// The first differing window of a Rust blob and a 1.0 blob.
fn first_difference(actual: &str, expected: &str) -> String {
    labeled_difference("rust", actual, "1.0 ", expected)
}

/// The 1.0 blob of a stream file, or `None` when `CADUS_ORACLE_PYTHON` is unset.
fn oracle_blob(path: &Path, tz: Option<&str>) -> Option<String> {
    let python = std::env::var("CADUS_ORACLE_PYTHON").ok()?;
    let mut command = Command::new(&python);
    command
        .arg(repo_root().join("scripts/oracle/dump_projector_1_0.py"))
        .arg(path)
        .arg("--curriculum")
        .arg(repo_root().join("curriculum"))
        .arg("--now")
        .arg("2000-01-01T00:00:00+00:00")
        .arg("--goal")
        .arg(GOAL.to_string());
    if let Some(zone) = tz {
        command.arg("--tz").arg(zone);
    }
    // The 1.0 package imports from its own tree.
    let output = command
        .current_dir("/home/deploy/dev/cadus")
        .output()
        .expect("the oracle runs");
    assert!(
        output.status.success(),
        "the oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).expect("the oracle prints UTF-8");
    Some(text.trim_end_matches('\n').to_owned())
}

/// The event index after which the Rust fold and the 1.0 fold disagree, found by
/// folding every prefix with both sides.
///
/// It returns `None` when `CADUS_ORACLE_PYTHON` is unset, so the diagnosis is an
/// extra, never a requirement. The prefix files go under `CARGO_TARGET_TMPDIR`.
fn divergence_index(name: &str, tz: Option<&str>) -> Option<usize> {
    if std::env::var("CADUS_ORACLE_PYTHON").is_err() {
        return None;
    }
    let events = stream(name);
    let lines: Vec<String> = std::fs::read_to_string(fixture(name))
        .expect("the fixture reads")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect();

    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("bisect-{name}"));
    std::fs::create_dir_all(&dir).expect("the scratch directory is created");
    let path = dir.join("prefix.jsonl");

    // A prefix that already disagrees means the divergence is at or before it, so
    // the first disagreeing prefix names the event that broke parity.
    for split in 1..=events.len() {
        std::fs::write(&path, format!("{}\n", lines[..split].join("\n")))
            .expect("the prefix writes");
        let expected = oracle_blob(&path, tz)?;
        let actual = canonical_blob(&fold(&events[..split], tz)).expect("the blob builds");
        if actual != expected {
            return Some(split - 1);
        }
    }
    None
}

/// Fail with the full report item 2 of the unit asks for.
#[track_caller]
fn report_mismatch(name: &str, tz: Option<&str>, actual: &str, expected: &str) -> ! {
    let zone = tz.unwrap_or("UTC");
    let index = divergence_index(name, tz).map_or_else(
        || "not bisected: CADUS_ORACLE_PYTHON is unset".to_owned(),
        |at| format!("the models diverge after event index {at}"),
    );
    panic!(
        "RUST BUG: {name} in {zone} does not reproduce the 1.0 fold.\n{}\n{index}\n\
         Do NOT change the oracle or the fixture. Fix the port.",
        first_difference(actual, expected)
    );
}

/// Assert that the fold of `name` in `tz` is `expected`, reporting a mismatch in full.
#[track_caller]
fn assert_folds_to(name: &str, tz: Option<&str>, expected_digest: &str) {
    let events = stream(name);
    let model = fold(&events, tz);
    let actual = blob_digest(&model).expect("the digest builds");
    if actual != expected_digest {
        let blob = canonical_blob(&model).expect("the blob builds");
        let expected_blob = oracle_blob(&fixture(name), tz).unwrap_or_else(|| {
            format!("<unavailable: set CADUS_ORACLE_PYTHON; digest {expected_digest}>")
        });
        report_mismatch(name, tz, &blob, &expected_blob);
    }
}

// --------------------------------------------------------------------------- //
// The committed index itself
// --------------------------------------------------------------------------- //

#[test]
fn the_digest_index_holds_the_pinned_metadata() {
    let index = digest_index();
    assert_eq!(index.oracle, "scripts/oracle/digest_streams_1_0.py");
    assert_eq!(index.generator, "scripts/oracle/gen_stream_1_0.py");
    assert_eq!(index.now, "2000-01-01T00:00:00+00:00");
    assert_eq!(index.goal, GOAL);
    assert_eq!(index.projector_version, PROJECTOR_VERSION);
    assert_eq!(index.projector_version, 3);
    assert_eq!(index.config_hash, CONFIG_HASH);
    assert_eq!(index.zones, ["UTC", NEW_YORK, "UTC_no_regrades"]);
    assert_eq!(index.streams.len(), STREAMS);
    // The digest `docs/plans/M3.md` pins is the first row's UTC digest.
    assert_eq!(index.streams[0].digests["UTC"], STREAM_1_UTC);
    for number in 1..=STREAMS {
        let entry = row(number);
        assert_eq!(entry.digests.len(), 3, "{}", entry.stream);
        for zone in ["UTC", NEW_YORK, "UTC_no_regrades"] {
            let digest = &entry.digests[zone];
            assert_eq!(digest.len(), 64, "{} {zone}", entry.stream);
            assert!(
                digest.bytes().all(|b| b.is_ascii_hexdigit()),
                "{} {zone}",
                entry.stream
            );
        }
    }
}

#[test]
fn the_incremental_index_holds_the_pinned_metadata() {
    let index = incremental_index();
    assert_eq!(index.oracle, "scripts/oracle/incremental_splits_1_0.py");
    assert_eq!(index.config_hash, CONFIG_HASH);
    assert_eq!(index.projector_version, PROJECTOR_VERSION);
    // The 20 numbered streams and the coverage stream.
    assert_eq!(index.streams.len(), STREAMS + 1);
    assert_eq!(index.streams[STREAMS].stream, COVERAGE_STREAM);
    for number in 1..=STREAMS {
        let entry = incremental_row(number);
        let digests = row(number);
        assert_eq!(entry.events, digests.events, "{}", entry.stream);
        assert_eq!(
            entry.full_digest, digests.digests["UTC"],
            "{}",
            entry.stream
        );
        assert_eq!(
            entry.mismatching_splits.len(),
            entry.mismatching_digests.len(),
            "{}",
            entry.stream
        );
        for split in &entry.mismatching_splits {
            assert!(
                entry.mismatching_digests.contains_key(&split.to_string()),
                "{} split {split} has no 1.0 digest",
                entry.stream
            );
        }
    }
    // `stream_1.jsonl` carries no correction into a later half, so 1.0 agrees with
    // the full replay at every one of its splits (spec section 9).
    assert!(incremental_row(1).mismatching_splits.is_empty());

    // The coverage row: the `profile_reset` class. Its digests are pinned the same
    // way, and `tests/projector.rs` asserts the port lands on them.
    let coverage = incremental_index()
        .streams
        .iter()
        .find(|entry| entry.stream == COVERAGE_STREAM)
        .expect("the coverage stream has a row");
    assert_eq!(coverage.events, 33);
    assert_eq!(
        coverage.mismatching_splits,
        [21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31]
    );
    assert_eq!(coverage.mismatching_digests.len(), 11);
}

#[test]
fn every_committed_stream_holds_its_recorded_event_count() {
    for number in 1..=STREAMS {
        let entry = row(number);
        let events = stream(&entry.stream);
        assert_eq!(events.len(), entry.events, "{}", entry.stream);
        let bare = events
            .iter()
            .filter(|event| event.type_name() != "regraded")
            .count();
        assert_eq!(bare, entry.events_without_regrades, "{}", entry.stream);
    }
}

#[test]
fn the_seeded_family_reaches_every_event_type() {
    // Spec section 9 item 1: every one of the 16 types, the six no-ops included.
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for number in 2..=STREAMS {
        let per_stream: BTreeSet<&str> = stream(&format!("stream_{number}.jsonl"))
            .iter()
            .map(Event::type_name)
            .collect();
        assert_eq!(
            per_stream.len(),
            16,
            "stream_{number}.jsonl misses an event type: {per_stream:?}"
        );
        seen.extend(per_stream);
    }
    for name in Event::TYPE_NAMES {
        assert!(seen.contains(name), "no stream carries a `{name}` event");
    }
}

#[test]
fn the_coverage_table_is_committed_beside_the_fixtures() {
    // `coverage.md` is the measured answer to spec section 9, and the report the
    // review reads. A stream family with no coverage record is not a parity suite.
    let text = std::fs::read_to_string(fixture("coverage.md")).expect("the table reads");
    assert!(text.contains("scripts/oracle/coverage_streams_1_0.py"));
    for item in 1..=10 {
        assert!(
            text.contains(&format!("\n| {item} | `")),
            "coverage.md records no spec section 9 item {item}"
        );
    }

    // Two guards the seeded family does NOT reach. The table said `all` for a probe
    // beside each of them, and a reviewer read the boundary as measured (M3 review
    // round 1, findings #7 and #15). Each now has a row of its own that reads
    // `none`, and a boundary stream that pins it.
    for (probe, pinned_by) in [
        (
            "diag.placed_balance_zero",
            "boundary/placed_balance_zero.jsonl",
        ),
        (
            "streak.reference_day_at_goal",
            "boundary/streak_reference_day_at_goal.jsonl",
        ),
    ] {
        assert!(
            text.contains(&format!("| `{probe}` |")),
            "coverage.md has no `{probe}` row"
        );
        assert!(
            text.contains(&format!("`{probe}` -- ")),
            "coverage.md does not say where `{probe}` is pinned"
        );
        assert!(
            fixture(pinned_by).exists(),
            "{pinned_by} is missing, so `{probe}` is unpinned"
        );
    }
}

// --------------------------------------------------------------------------- //
// The per-stream properties
// --------------------------------------------------------------------------- //

/// The fold of `name` folds to its committed digest in every recorded zone.
fn check_digests(number: usize) {
    let entry = row(number);
    assert_folds_to(&entry.stream, None, &entry.digests["UTC"]);
    assert_folds_to(&entry.stream, Some(NEW_YORK), &entry.digests[NEW_YORK]);
}

/// Removing every `regraded` event re-folds to the pre-correction digest.
///
/// `apply_regrades` works on copies, so deleting the corrections must give the
/// uncorrected model back. That pins that the corrected events stay intact.
fn check_pre_correction(number: usize) {
    let entry = row(number);
    let bare: Vec<Event> = stream(&entry.stream)
        .into_iter()
        .filter(|event| event.type_name() != "regraded")
        .collect();
    assert_eq!(bare.len(), entry.events_without_regrades);
    let model = fold(&bare, None);
    let actual = blob_digest(&model).expect("the digest builds");
    assert_eq!(
        actual, entry.digests["UTC_no_regrades"],
        "{}: the fold without the corrections is not the pre-correction model",
        entry.stream
    );
}

/// `project_incremental` reproduces the 1.0 result at EVERY split of the stream.
///
/// At most splits that means it equals the full replay. At the splits
/// `incremental_1_0.json` records it does NOT, and that is 1.0 behavior rather
/// than a defect: `project_incremental` seeds FIRe from the cached model, so a
/// `regraded` event in the new half that supersedes a grade the prior half
/// already folded never reaches FIRe (`projector.py:794-830`; 1.0 routes such a
/// stream down the full-replay path instead). The port must diverge at the same
/// splits and to the same model, so both sides are pinned against 1.0 literals.
fn check_incremental(number: usize) {
    let entry = row(number);
    let carry = incremental_row(number);
    let events = stream(&entry.stream);
    let cfg = Config::default();
    let input = ProjectionInput::new(tree(), &cfg, now()).with_goal(GOAL);
    let full_model = project(&events, &input).expect("the fold succeeds");
    let full = canonical_blob(&full_model).expect("the blob builds");
    assert_eq!(
        blob_digest(&full_model).expect("the digest builds"),
        carry.full_digest,
        "{}: the full replay is not the 1.0 full replay",
        entry.stream
    );

    for split in 0..=events.len() {
        let (prior, fresh) = events.split_at(split);
        let cached = project(prior, &input).expect("the prefix folds");
        let incremental =
            project_incremental(&cached, prior, fresh, &input).expect("the resume folds");
        let actual = canonical_blob(&incremental).expect("the blob builds");

        if let Some(expected) = carry.mismatching_digests.get(&split.to_string()) {
            // A correction carried over from the new half. 1.0 lands on its own
            // model here, and so must the port.
            let digest = blob_digest(&incremental).expect("the digest builds");
            assert_eq!(
                &digest, expected,
                "{} split {split}: the carried-over correction gives a different \
                 model than 1.0 gives",
                entry.stream
            );
            assert!(
                actual != full,
                "{} split {split}: 1.0 leaves the full replay here, but the port does not",
                entry.stream
            );
        } else {
            assert!(
                actual == full,
                "{} split {split}: {}",
                entry.stream,
                labeled_difference("incremental", &actual, "full replay", &full)
            );
        }
    }
}

/// `apply_regrades` is idempotent: applying it twice equals applying it once.
fn check_regrade_idempotence(number: usize) {
    let entry = row(number);
    let events = stream(&entry.stream);
    let once = apply_regrades(&events);
    let twice = apply_regrades(&once);
    assert_eq!(
        once, twice,
        "{}: apply_regrades is not idempotent",
        entry.stream
    );
    // The second pass has no correction left to consume, so the fold is unmoved too.
    assert_eq!(
        blob_digest(&fold(&once, None)).expect("the digest builds"),
        entry.digests["UTC"],
        "{}: the pre-corrected stream folds differently",
        entry.stream
    );
}

/// `repNum` and `memoryBase` stay at or above zero after EVERY event.
fn check_invariants_after_every_event(number: usize) {
    let entry = row(number);
    let cfg = Config::default();
    let corrected = apply_regrades(&stream(&entry.stream));
    let mut projector = Projector::new(tree(), &cfg).with_goal(GOAL);
    for (index, event) in corrected.iter().enumerate() {
        projector.apply(event, true);
        for (tid, state) in projector.topics() {
            assert!(
                state.rep_num >= 0.0,
                "{} event {index} ({}): {tid} has repNum {}",
                entry.stream,
                event.type_name(),
                state.rep_num
            );
            assert!(
                state.memory_base >= 0.0,
                "{} event {index} ({}): {tid} has memoryBase {}",
                entry.stream,
                event.type_name(),
                state.memory_base
            );
        }
    }
}

/// `memory_at` is non-increasing in `t` for every `t >= t0`.
fn check_memory_is_non_increasing(number: usize) {
    let entry = row(number);
    let model = fold(&stream(&entry.stream), None);
    // A ladder of offsets from `t0`, in days, out past the longest interval table entry.
    let offsets = [
        0.0_f64, 0.5, 1.0, 2.0, 7.0, 30.0, 90.0, 365.0, 1000.0, 5000.0,
    ];
    for (tid, state) in &model.topics {
        let Some(t0) = state.t0 else { continue };
        let mut previous = f64::INFINITY;
        for offset in offsets {
            #[allow(clippy::cast_possible_truncation)]
            let t_us = t0.micros() + (offset * 86_400_000_000.0) as i64;
            let memory = memory_at(state, t_us);
            assert!(
                memory >= 0.0,
                "{}: {tid} has memory {memory} at t0 + {offset} days",
                entry.stream
            );
            assert!(
                memory <= previous,
                "{}: {tid} memory rose from {previous} to {memory} at t0 + {offset} days",
                entry.stream
            );
            previous = memory;
        }
    }
}

/// The implicit credit a neighbor absorbs never exceeds the direct credit the same
/// grade would give that neighbor.
///
/// The fold runs FIRe on a passing `lesson_result` and on every `review_result`
/// (`projector.py:226-255`), so the check walks those events, and drives
/// `apply_attempt` on the states the fold holds at that instant.
fn check_implicit_credit(number: usize) {
    let entry = row(number);
    let cfg = Config::default();
    let corrected = apply_regrades(&stream(&entry.stream));
    let mut projector = Projector::new(tree(), &cfg).with_goal(GOAL);
    let mut checked = 0_usize;

    for event in &corrected {
        let graded: Option<(&str, bool, WorkQuality, bool)> = match event {
            Event::LessonResult(body) if body.passed => {
                Some((body.topic.as_str(), true, body.quality_tier, body.assisted))
            }
            Event::ReviewResult(body) => Some((
                body.topic.as_str(),
                body.passed,
                body.quality_tier,
                body.assisted,
            )),
            _ => None,
        };

        if let Some((topic, passed, quality, assisted)) = graded {
            let states: BTreeMap<String, TopicState> = projector.topics().clone();
            let t_us = event.ts().micros();
            let attempt = AttemptResult::new(topic, passed, quality).with_assisted(assisted);
            let (_new_states, props) = apply_attempt(&states, &attempt, tree(), &cfg, t_us);
            let grade = quality_q(quality);
            let default = TopicState::default();

            for prop in &props {
                let recipient = states.get(&prop.topic).unwrap_or(&default);
                // The direct credit the SAME grade gives this recipient at this instant.
                let direct = match prop.kind {
                    PropagationKind::Credit => {
                        raw_delta(grade, memory_at(recipient, t_us), true, &cfg, assisted)
                    }
                    // A failure carries no early factor, so the recipient's direct
                    // delta is the explicit topic's own delta.
                    PropagationKind::Penalty => raw_delta(grade, 0.0, false, &cfg, assisted),
                };
                assert!(
                    prop.weight > 0.0 && prop.weight <= 1.0,
                    "{}: {} carries weight {}",
                    entry.stream,
                    prop.topic,
                    prop.weight
                );
                assert!(
                    prop.raw_delta.abs() <= direct.abs() + CREDIT_EPSILON,
                    "{}: implicit {} on {} is {} but the direct credit is {}",
                    entry.stream,
                    prop.kind.as_str(),
                    prop.topic,
                    prop.raw_delta,
                    direct
                );
                checked += 1;
            }
        }
        projector.apply(event, true);
    }

    assert!(
        checked > 0,
        "{}: no propagation was checked, so the property is vacuous",
        entry.stream
    );
}

/// Declare the per-stream property tests, one test function per stream, so the
/// runner spreads the folds across threads.
macro_rules! stream_tests {
    ($($name:ident => $number:literal),* $(,)?) => {
        $(
            mod $name {
                use super::*;

                #[test]
                fn folds_to_the_committed_digests() {
                    check_digests($number);
                }

                #[test]
                fn without_the_corrections_folds_to_the_pre_correction_digest() {
                    check_pre_correction($number);
                }

                #[test]
                fn incremental_reproduces_the_1_0_fold_at_every_split() {
                    check_incremental($number);
                }

                #[test]
                fn apply_regrades_is_idempotent() {
                    check_regrade_idempotence($number);
                }

                #[test]
                fn holds_the_floor_invariants_after_every_event() {
                    check_invariants_after_every_event($number);
                }

                #[test]
                fn memory_is_non_increasing_after_t0() {
                    check_memory_is_non_increasing($number);
                }

                #[test]
                fn implicit_credit_never_exceeds_the_direct_credit() {
                    check_implicit_credit($number);
                }
            }
        )*
    };
}

stream_tests! {
    stream_1 => 1,
    stream_2 => 2,
    stream_3 => 3,
    stream_4 => 4,
    stream_5 => 5,
    stream_6 => 6,
    stream_7 => 7,
    stream_8 => 8,
    stream_9 => 9,
    stream_10 => 10,
    stream_11 => 11,
    stream_12 => 12,
    stream_13 => 13,
    stream_14 => 14,
    stream_15 => 15,
    stream_16 => 16,
    stream_17 => 17,
    stream_18 => 18,
    stream_19 => 19,
    stream_20 => 20,
}

// --------------------------------------------------------------------------- //
// The live 1.0 oracle
// --------------------------------------------------------------------------- //

#[test]
fn the_live_oracle_reproduces_every_committed_digest() {
    let Ok(python) = std::env::var("CADUS_ORACLE_PYTHON") else {
        eprintln!("skipped: CADUS_ORACLE_PYTHON is not set");
        return;
    };
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("digests_live.json");
    let output = Command::new(&python)
        .arg(repo_root().join("scripts/oracle/digest_streams_1_0.py"))
        .arg("--fixtures")
        .arg(fixture("."))
        .arg("--curriculum")
        .arg(repo_root().join("curriculum"))
        .arg("--out")
        .arg(&out)
        .current_dir("/home/deploy/dev/cadus")
        .output()
        .expect("the oracle runs");
    assert!(
        output.status.success(),
        "the oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let live: DigestIndex =
        serde_json::from_str(&std::fs::read_to_string(&out).expect("the live index reads"))
            .expect("the live index parses");
    let committed = digest_index();
    assert_eq!(live.streams.len(), committed.streams.len());
    assert_eq!(live.config_hash, committed.config_hash);
    assert_eq!(live.projector_version, committed.projector_version);
    for (fresh, old) in live.streams.iter().zip(committed.streams.iter()) {
        assert_eq!(fresh.stream, old.stream);
        assert_eq!(fresh.events, old.events, "{}", old.stream);
        assert_eq!(
            fresh.digests, old.digests,
            "{}: the live 1.0 fold moved away from the committed digests",
            old.stream
        );
    }
}

#[test]
fn the_live_oracle_reproduces_the_committed_incremental_reference() {
    let Ok(python) = std::env::var("CADUS_ORACLE_PYTHON") else {
        eprintln!("skipped: CADUS_ORACLE_PYTHON is not set");
        return;
    };
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("incremental_live.json");
    let output = Command::new(&python)
        .arg(repo_root().join("scripts/oracle/incremental_splits_1_0.py"))
        .arg("--fixtures")
        .arg(fixture("."))
        .arg("--curriculum")
        .arg(repo_root().join("curriculum"))
        .arg("--out")
        .arg(&out)
        .current_dir("/home/deploy/dev/cadus")
        .output()
        .expect("the oracle runs");
    assert!(
        output.status.success(),
        "the oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let live: IncrementalIndex =
        serde_json::from_str(&std::fs::read_to_string(&out).expect("the live index reads"))
            .expect("the live index parses");
    let committed = incremental_index();
    assert_eq!(live.streams.len(), committed.streams.len());
    for (fresh, old) in live.streams.iter().zip(committed.streams.iter()) {
        assert_eq!(fresh.stream, old.stream);
        assert_eq!(fresh.full_digest, old.full_digest, "{}", old.stream);
        assert_eq!(
            fresh.mismatching_splits, old.mismatching_splits,
            "{}: the live 1.0 incremental fold diverges at other splits now",
            old.stream
        );
        assert_eq!(
            fresh.mismatching_digests, old.mismatching_digests,
            "{}: the live 1.0 incremental fold gives another model now",
            old.stream
        );
    }
}

// --------------------------------------------------------------------------- //
// The selector oracle (U4's 1.0 `compose_session` reference)
// --------------------------------------------------------------------------- //

/// The last `enrolled` course of a stream, which the oracle passed as `course_id`.
fn enrolled_course(events: &[Event]) -> Option<String> {
    let mut course = None;
    for event in events {
        if let Event::Enrolled(body) = event {
            course = Some(body.course.as_str().to_owned());
        }
    }
    course
}

/// The 2.0 task row in the shape `selector_1_0.json` records.
fn task_rows(plan_tasks: &[cadus_core::selector::Task]) -> Vec<TaskRow> {
    plan_tasks
        .iter()
        .map(|task| {
            // Trap T11: the quiz sample is a documented non-parity, so a quiz row
            // carries no topic and no size on either side, and compares by presence
            // only. `topic`, `n_problems`, and `time_budget_secs` all read the
            // sampled questions.
            let quiz = task.task_type == TaskType::Quiz;
            let topic = if quiz { None } else { task.topic.clone() };
            let n_problems = if quiz { None } else { task.n_problems };
            let budget = if quiz { None } else { task.time_budget_secs };
            (
                task.task_type.as_str().to_owned(),
                topic,
                task.is_remediation,
                task.nearly_due,
                n_problems,
                budget,
            )
        })
        .collect()
}

#[test]
fn the_selector_index_holds_the_pinned_metadata() {
    let index = selector_index();
    assert_eq!(index.oracle, "scripts/oracle/dump_selector_1_0.py");
    assert_eq!(index.n, SELECTOR_N);
    assert_eq!(index.config_hash, CONFIG_HASH);
    assert_eq!(index.projector_version, PROJECTOR_VERSION);
    assert_eq!(index.states.len(), SELECTOR_SEEDS);
    // Every section of `compose_session` reaches the comparison. Without the drill
    // and the multi-step rows the cap truncated the plan (finding #13).
    let kinds: BTreeSet<&str> = index
        .states
        .iter()
        .flat_map(|state| state.tasks.iter().map(|task| task.0.as_str()))
        .collect();
    for kind in ["review", "lesson", "quiz", "multi-step", "drill"] {
        assert!(
            kinds.contains(kind),
            "no seeded state records a `{kind}` task"
        );
    }
    for (position, state) in index.states.iter().enumerate() {
        assert_eq!(state.seed, position + 1);
        assert_eq!(state.stream, format!("stream_{}.jsonl", state.seed));
        assert_eq!(state.session_id, format!("s{}", state.seed));
        // A plan AT the cap is a truncated plan, and a truncated plan hides
        // whatever `compose_session` appends last (finding #13).
        assert!(
            state.tasks.len() < SELECTOR_N,
            "{}: the plan fills the cap, so the comparison stops early",
            state.stream
        );
        assert!(
            state.tasks.len() <= SELECTOR_LONGEST_PLAN,
            "{}: a plan is longer than the recorded longest plan",
            state.stream
        );
    }
}

#[test]
fn compose_session_reproduces_the_1_0_plan_of_every_seeded_state() {
    let cfg = Config::default();
    for state in &selector_index().states {
        let events = stream(&state.stream);
        let model = fold(&events, None);
        let course = enrolled_course(&events);
        assert_eq!(
            course.as_deref(),
            state.course_id.as_deref(),
            "{}: the enrolled course differs from the oracle's",
            state.stream
        );

        // The oracle composed at the last event's timestamp, which is the fold's
        // own `t_ref`. Both sides read that instant off the same committed stream.
        let t_us = events
            .iter()
            .map(|event| event.ts().micros())
            .max()
            .expect("the stream is not empty");
        assert_eq!(
            Timestamp::from_micros(t_us).to_wire_string().unwrap(),
            state.t,
            "{}: the composition instant differs from the oracle's",
            state.stream
        );

        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let mut sampler = SeededSampler::new(state.seed as u64);
        let ctx = SessionContext::default()
            .with_session_id(&state.session_id)
            .with_course(course.as_deref())
            .with_pending_remediation(&model.pending_remediation)
            .with_quiz_state(Some(&model.quiz))
            .with_limit(Some(SELECTOR_N));
        let plan = compose_session(&model.topics, tree(), &cfg, t_us, &mut sampler, &ctx);

        let actual = task_rows(&plan.tasks);
        assert_eq!(
            actual, state.tasks,
            "RUST BUG: {} (seed {}) composes a different session than 1.0.\n  \
             rust {actual:?}\n  1.0  {:?}\n\
             Do NOT change the oracle or the fixture. Fix the port.",
            state.stream, state.seed, state.tasks
        );
    }
}

#[test]
fn the_live_oracle_reproduces_the_committed_selector_plans() {
    let Ok(python) = std::env::var("CADUS_ORACLE_PYTHON") else {
        eprintln!("skipped: CADUS_ORACLE_PYTHON is not set");
        return;
    };
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("selector_live.json");
    let output = Command::new(&python)
        .arg(repo_root().join("scripts/oracle/dump_selector_1_0.py"))
        .arg("--fixtures")
        .arg(fixture("."))
        .arg("--curriculum")
        .arg(repo_root().join("curriculum"))
        .arg("--out")
        .arg(&out)
        .current_dir("/home/deploy/dev/cadus")
        .output()
        .expect("the oracle runs");
    assert!(
        output.status.success(),
        "the oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let live: SelectorIndex =
        serde_json::from_str(&std::fs::read_to_string(&out).expect("the live index reads"))
            .expect("the live index parses");
    let committed = selector_index();
    assert_eq!(live.states.len(), committed.states.len());
    for (fresh, old) in live.states.iter().zip(committed.states.iter()) {
        assert_eq!(fresh.stream, old.stream);
        assert_eq!(
            fresh.tasks, old.tasks,
            "{}: the live 1.0 selector moved away from the committed plan",
            old.stream
        );
    }
}
