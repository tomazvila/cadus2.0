//! Helpers the event-stream tests share: the checked-in curriculum tree, the
//! JSONL fixtures under `tests/fixtures/events`, the oracle build instant, and
//! the mismatch window of two blobs.

#![allow(clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use cadus_core::config::Config;
use cadus_core::curriculum::{Curriculum, load_curriculum};
use cadus_core::event::{Event, Timestamp};
use cadus_core::learner::LearnerModel;
use cadus_core::projector::{ProjectionInput, project};

use super::{graph, knowledge_point, topic};

/// The build instant the oracle pins with `--now` (`dump_projector_1_0.py`).
///
/// Only `built_from_ts` carries it, and the parity blob drops that field.
pub const NOW: &str = "2000-01-01T00:00:00Z";

/// The repository root.
#[must_use]
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// One fixture under `tests/fixtures/events`.
#[must_use]
pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/events")
        .join(name)
}

/// The checked-in curriculum tree, loaded once for the whole test binary.
pub fn tree() -> &'static Curriculum {
    static TREE: OnceLock<Curriculum> = OnceLock::new();
    TREE.get_or_init(|| {
        let (curriculum, _findings) =
            load_curriculum(&repo_root().join("curriculum")).expect("the tree loads");
        curriculum
    })
}

/// The 1.0 parity config, built once for the whole test binary.
///
/// `mastery.confirm_inferred` is OFF (D-F6): the 1.0 oracle counts a placed and
/// a floor topic as progress, and every digest in `digests_1_0.json` carries
/// that rule. With the flag ON the fold is a 2.0 behavior, and the tests of
/// `crates/core/tests/selector_confirm.rs` pin it.
pub fn cfg() -> &'static Config {
    static CFG: OnceLock<Config> = OnceLock::new();
    CFG.get_or_init(|| {
        let mut cfg = Config::default();
        cfg.mastery.confirm_inferred = false;
        cfg
    })
}

/// The regrade graph of `tests/test_regrade.py:54-65`: one topic, two knowledge points.
#[must_use]
pub fn regrade_graph() -> Curriculum {
    let mut subtraction = topic("subtraction-facts", &[], 0.2, &[]);
    subtraction.knowledge_points = vec![knowledge_point("kp1", &[]), knowledge_point("kp2", &[])];
    graph(vec![subtraction])
}

/// The oracle build instant as a timestamp.
#[must_use]
pub fn now() -> Timestamp {
    Timestamp::parse(NOW).unwrap()
}

/// The projection input of the oracle: the tree, the default config, [`NOW`].
#[must_use]
pub fn input() -> ProjectionInput<'static> {
    ProjectionInput::new(tree(), cfg(), now())
}

/// One event from its wire JSON. The wire form is the literal; a test never
/// builds a body field by field.
#[must_use]
pub fn event(json: &str) -> Event {
    Event::from_json(json).unwrap_or_else(|error| panic!("{json}: {error}"))
}

/// Every event of a JSONL fixture, in file order.
#[must_use]
pub fn stream(name: &str) -> Vec<Event> {
    let text = std::fs::read_to_string(fixture(name)).expect("the fixture reads");
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(event)
        .collect()
}

/// The `projector_version` the 1.0 oracle stamped into every committed blob.
///
/// The 2.0 fold stands at `PROJECTOR_VERSION` 4 (D-F2). The stamp names the version
/// of the FOLD, not the result of the fold, and a committed stream holds no ungraded
/// attempt, so the two folds still agree on every other byte.
pub const ORACLE_PROJECTOR_VERSION: i64 = 3;

/// Restamp a 2.0 model with the 1.0 `projector_version` for a parity comparison.
///
/// This is the ONE place a parity test touches the stamp. A test that compares two
/// 2.0 folds against each other never calls it.
#[must_use]
pub fn oracle_stamp(mut model: LearnerModel) -> LearnerModel {
    model.projector_version = Some(ORACLE_PROJECTOR_VERSION);
    model
}

/// The full replay of `events` over the checked-in tree, with the default config,
/// restamped for the 1.0 comparison.
#[must_use]
pub fn fold(events: &[Event]) -> LearnerModel {
    oracle_stamp(project(events, &input()).expect("the fold succeeds"))
}

/// The first differing window of two blobs, `window` bytes wide.
///
/// `left_label` and `right_label` name the two sides, so a report never claims a
/// Rust blob came from 1.0.
#[must_use]
pub fn labeled_difference(
    left_label: &str,
    left_blob: &str,
    right_label: &str,
    right_blob: &str,
    window: usize,
) -> String {
    let left = left_blob.as_bytes();
    let right = right_blob.as_bytes();
    let at = left
        .iter()
        .zip(right.iter())
        .position(|(a, b)| a != b)
        .unwrap_or_else(|| left.len().min(right.len()));
    let from = at.saturating_sub(window / 2);
    let to_left = (from + window).min(left.len());
    let to_right = (from + window).min(right.len());
    format!(
        "first difference at byte {at} (lengths {} and {})\n  {left_label} ...{}...\n  {right_label} ...{}...",
        left.len(),
        right.len(),
        String::from_utf8_lossy(&left[from..to_left]),
        String::from_utf8_lossy(&right[from..to_right]),
    )
}

/// The first differing window of a Rust blob and an expected blob.
#[must_use]
pub fn first_difference(actual: &str, expected: &str) -> String {
    labeled_difference("actual  ", actual, "expected", expected, 120)
}

/// Assert that two blobs are equal, with a readable window on a mismatch.
#[track_caller]
pub fn assert_same_blob(actual: &str, expected: &str, what: &str) {
    assert!(
        actual == expected,
        "{what}: {}",
        first_difference(actual, expected)
    );
}

/// The 1.0 blob of one stream file, or `None` when `CADUS_ORACLE_PYTHON` is
/// unset. `goal` adds `--goal` to the oracle call.
pub fn oracle_blob(path: &Path, tz: Option<&str>, goal: Option<i64>) -> Option<String> {
    let python = std::env::var("CADUS_ORACLE_PYTHON").ok()?;
    let mut command = Command::new(&python);
    command
        .arg(repo_root().join("scripts/oracle/dump_projector_1_0.py"))
        .arg(path)
        .arg("--curriculum")
        .arg(repo_root().join("curriculum"))
        .arg("--now")
        .arg("2000-01-01T00:00:00+00:00");
    if let Some(zone) = tz {
        command.arg("--tz").arg(zone);
    }
    if let Some(goal) = goal {
        command.arg("--goal").arg(goal.to_string());
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

/// The 1.0 blob of one fixture stream at the default goal, or `None` when
/// `CADUS_ORACLE_PYTHON` is unset.
pub fn live_oracle_blob(name: &str, tz: Option<&str>) -> Option<String> {
    oracle_blob(&fixture(name), tz, None)
}
