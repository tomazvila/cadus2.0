//! Helpers the event-stream parity tests share: the committed 1.0 indexes,
//! the fold in one zone, the live oracle, and the mismatch report with the
//! bisection to the event that broke parity.

#![allow(clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

use serde::Deserialize;

use cadus_core::event::Event;
use cadus_core::learner::LearnerModel;
use cadus_core::projector::{blob_digest, canonical_blob, project};

use super::events::{first_difference, fixture, input, oracle_blob, repo_root, stream};

/// The daily XP goal the oracle folds with.
pub const GOAL: i64 = 40;

/// One stream's row of `digests_1_0.json`.
#[derive(Debug, Deserialize)]
pub struct StreamDigests {
    /// The fixture file name.
    pub stream: String,
    /// The number of events in the file.
    pub events: usize,
    /// The number of events left after every `regraded` is removed.
    pub events_without_regrades: usize,
    /// Zone name to blob digest. The key `UTC_no_regrades` is the pre-correction fold.
    pub digests: BTreeMap<String, String>,
}

/// `digests_1_0.json`, the committed 1.0 digest index.
#[derive(Debug, Deserialize)]
pub struct DigestIndex {
    /// The oracle that wrote the file.
    pub oracle: String,
    /// The generator that wrote the streams.
    pub generator: String,
    /// The pinned `now`.
    pub now: String,
    /// The daily XP goal.
    pub goal: i64,
    /// The 1.0 `PROJECTOR_VERSION`.
    pub projector_version: i64,
    /// The 1.0 `config_hash`.
    pub config_hash: String,
    /// The zone keys every row carries.
    pub zones: Vec<String>,
    /// One row per stream, in stream order.
    pub streams: Vec<StreamDigests>,
}

/// One stream's row of `incremental_1_0.json`.
#[derive(Debug, Deserialize)]
pub struct StreamIncremental {
    /// The fixture file name.
    pub stream: String,
    /// The number of events in the file.
    pub events: usize,
    /// The 1.0 full-replay digest.
    pub full_digest: String,
    /// The splits at which the 1.0 incremental fold leaves the full replay.
    pub mismatching_splits: Vec<usize>,
    /// Split index, as text, to the 1.0 digest of that divergent incremental fold.
    pub mismatching_digests: BTreeMap<String, String>,
}

/// `incremental_1_0.json`, the committed 1.0 incremental-fold reference.
#[derive(Debug, Deserialize)]
pub struct IncrementalIndex {
    /// The oracle that wrote the file.
    pub oracle: String,
    /// The 1.0 `config_hash`.
    pub config_hash: String,
    /// The 1.0 `PROJECTOR_VERSION`.
    pub projector_version: i64,
    /// One row per stream, in stream order.
    pub streams: Vec<StreamIncremental>,
}

/// One served task of `selector_1_0.json`, in the recorded shape.
///
/// `[task_type, topic, is_remediation, nearly_due, n_problems, time_budget_secs]`.
pub type TaskRow = (String, Option<String>, bool, bool, Option<i64>, Option<i64>);

/// One seeded state of `selector_1_0.json`.
#[derive(Debug, Deserialize)]
pub struct SelectorState {
    /// The seed, which is also the stream number.
    pub seed: usize,
    /// The fixture the state folds from.
    pub stream: String,
    /// The session id the task ids are keyed on.
    pub session_id: String,
    /// The enrolled course, from the last `enrolled` event.
    pub course_id: Option<String>,
    /// The instant the plan composed at: the last event's `ts`.
    pub t: String,
    /// `[task_type, topic, is_remediation, nearly_due, n_problems,
    /// time_budget_secs]` per served task.
    pub tasks: Vec<TaskRow>,
}

/// `selector_1_0.json`, the committed 1.0 `compose_session` plans.
#[derive(Debug, Deserialize)]
pub struct SelectorIndex {
    /// The oracle that wrote the file.
    pub oracle: String,
    /// The task cap.
    pub n: usize,
    /// The 1.0 `config_hash`.
    pub config_hash: String,
    /// The 1.0 `PROJECTOR_VERSION`.
    pub projector_version: i64,
    /// One entry per seed, in seed order.
    pub states: Vec<SelectorState>,
}

/// The committed digest index, read once for the whole test binary.
pub fn digest_index() -> &'static DigestIndex {
    static INDEX: OnceLock<DigestIndex> = OnceLock::new();
    INDEX.get_or_init(|| {
        let text = std::fs::read_to_string(fixture("digests_1_0.json")).expect("the index reads");
        serde_json::from_str(&text).expect("the index parses")
    })
}

/// The committed selector index, read once for the whole test binary.
pub fn selector_index() -> &'static SelectorIndex {
    static INDEX: OnceLock<SelectorIndex> = OnceLock::new();
    INDEX.get_or_init(|| {
        let text = std::fs::read_to_string(fixture("selector_1_0.json")).expect("the index reads");
        serde_json::from_str(&text).expect("the index parses")
    })
}

/// The committed incremental index, read once for the whole test binary.
pub fn incremental_index() -> &'static IncrementalIndex {
    static INDEX: OnceLock<IncrementalIndex> = OnceLock::new();
    INDEX.get_or_init(|| {
        let text =
            std::fs::read_to_string(fixture("incremental_1_0.json")).expect("the index reads");
        serde_json::from_str(&text).expect("the index parses")
    })
}

/// The incremental row of one stream number.
pub fn incremental_row(number: usize) -> &'static StreamIncremental {
    let name = format!("stream_{number}.jsonl");
    incremental_index()
        .streams
        .iter()
        .find(|entry| entry.stream == name)
        .unwrap_or_else(|| panic!("{name} has no row in incremental_1_0.json"))
}

/// The row of one stream number.
pub fn row(number: usize) -> &'static StreamDigests {
    let name = format!("stream_{number}.jsonl");
    digest_index()
        .streams
        .iter()
        .find(|entry| entry.stream == name)
        .unwrap_or_else(|| panic!("{name} has no row in digests_1_0.json"))
}

/// The full replay of `events` in `tz`, with the default config.
pub fn fold_in(events: &[Event], tz: Option<&str>) -> LearnerModel {
    let input = input().with_timezone(tz).with_goal(GOAL);
    project(events, &input).expect("the fold succeeds")
}

/// The event index after which the Rust fold and the 1.0 fold disagree, found by
/// folding every prefix with both sides.
///
/// It returns `None` when `CADUS_ORACLE_PYTHON` is unset, so the diagnosis is an
/// extra, never a requirement. The prefix files go under `CARGO_TARGET_TMPDIR`.
pub fn divergence_index(name: &str, tz: Option<&str>) -> Option<usize> {
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
        let expected = oracle_blob(&path, tz, Some(GOAL))?;
        let actual = canonical_blob(&fold_in(&events[..split], tz)).expect("the blob builds");
        if actual != expected {
            return Some(split - 1);
        }
    }
    None
}

/// Fail with the full report item 2 of the unit asks for.
#[track_caller]
pub fn report_mismatch(name: &str, tz: Option<&str>, actual: &str, expected: &str) -> ! {
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
pub fn assert_folds_to(name: &str, tz: Option<&str>, expected_digest: &str) {
    let events = stream(name);
    let model = fold_in(&events, tz);
    let actual = blob_digest(&model).expect("the digest builds");
    if actual != expected_digest {
        let blob = canonical_blob(&model).expect("the blob builds");
        let expected_blob = oracle_blob(&fixture(name), tz, Some(GOAL)).unwrap_or_else(|| {
            format!("<unavailable: set CADUS_ORACLE_PYTHON; digest {expected_digest}>")
        });
        report_mismatch(name, tz, &blob, &expected_blob);
    }
}

/// Run one 1.0 oracle script over the fixture directory, write its index to
/// `out_name` under `CARGO_TARGET_TMPDIR`, and return the index text.
pub fn oracle_index(python: &str, script: &str, out_name: &str) -> String {
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join(out_name);
    let output = Command::new(python)
        .arg(repo_root().join(script))
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
    std::fs::read_to_string(&out).expect("the live index reads")
}
