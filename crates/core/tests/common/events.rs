//! Helpers the event-stream tests share: the checked-in curriculum tree, the
//! JSONL fixtures under `tests/fixtures/events`, the oracle build instant, and
//! the mismatch window of two blobs.

#![allow(clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use cadus_core::config::Config;
use cadus_core::curriculum::{Curriculum, load_curriculum};
use cadus_core::event::{Event, Timestamp};
use cadus_core::learner::LearnerModel;
use cadus_core::projector::{ProjectionInput, project};

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

/// The default config, built once for the whole test binary.
pub fn cfg() -> &'static Config {
    static CFG: OnceLock<Config> = OnceLock::new();
    CFG.get_or_init(Config::default)
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

/// The full replay of `events` over the checked-in tree, with the default config.
#[must_use]
pub fn fold(events: &[Event]) -> LearnerModel {
    project(events, &input()).expect("the fold succeeds")
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
