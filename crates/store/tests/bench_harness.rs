//! The benchmark harness itself: the nearest-rank percentiles, the gating,
//! the budget, the report line, and the artifact.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::Path;

use common::bench::{
    Percentiles, artifact_json, budget, dsn_set, percentile, profile, report, rounds, timed,
    timed_rounds, write_artifact, write_artifact_to,
};
use serde_json::json;

/// The nearest-rank rule: the rank is `ceil(percent * n / 100)`, counted from
/// one, and the maximum is the last sorted value.
#[test]
fn the_percentiles_follow_the_nearest_rank_rule() {
    let sorted: Vec<u128> = (1..=10).collect();
    assert_eq!(percentile(&sorted, 50), 5);
    assert_eq!(percentile(&sorted, 95), 10);
    assert_eq!(percentile(&sorted, 99), 10);
    assert_eq!(percentile(&sorted, 0), 1);
    let times = Percentiles::of(&[30, 10, 20]);
    assert_eq!(
        (times.p50, times.p95, times.p99, times.max),
        (20, 30, 30, 30)
    );
    assert_eq!(
        times.json(),
        json!({"p50_ns": 20, "p95_ns": 30, "p99_ns": 30, "max_ns": 30})
    );
    let outcome = std::panic::catch_unwind(|| percentile(&[], 50));
    assert!(outcome.is_err(), "a percentile needs a sample");
}

/// The gates read the environment of the test run: the clock is off, the
/// counting round counts apply, the debug budget is ten times wider, and the
/// throwaway cluster is named.
#[test]
fn the_gates_read_the_environment_of_the_run() {
    assert!(!timed(), "the coverage run sets no CADUS_BENCH");
    assert_eq!(rounds(100, 3), 3);
    assert_eq!(budget(100), if cfg!(debug_assertions) { 1000 } else { 100 });
    assert_eq!(
        profile(),
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );
    assert!(dsn_set("the harness test"));
}

/// The artifact names the benchmark, the profile, the percentiles under the
/// given key, the budget, and every field of the run; the file lands in the
/// given directory.
#[test]
fn the_artifact_carries_every_field_and_lands_in_the_directory() {
    let times = Percentiles::of(&[5, 7]);
    let body = artifact_json("B-test", "serve_ns", &times, 9, &[("samples", json!(2))]);
    let doc: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(doc["benchmark"], json!("B-test"));
    assert_eq!(doc["profile"], json!(profile()));
    assert_eq!(doc["serve_ns"]["p95_ns"], json!(7));
    assert_eq!(doc["p95_budget_ns"], json!(9));
    assert_eq!(doc["samples"], json!(2));
    assert!(body.ends_with('\n'));

    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("bench-harness");
    write_artifact_to(&dir, "harness.json", &body);
    assert_eq!(
        std::fs::read_to_string(dir.join("harness.json")).unwrap(),
        body
    );
    write_artifact("harness-check.json", &body);
    report("harness", &times, 2, ", one field");
}

/// The rounds run in order and keep both halves of every answer.
#[tokio::test]
async fn the_rounds_keep_the_nanoseconds_and_the_values() {
    let (timings, values) =
        timed_rounds(3, |index| async move { (index as u128 * 10, index) }).await;
    assert_eq!(timings, vec![0, 10, 20]);
    assert_eq!(values, vec![0, 1, 2]);
}
