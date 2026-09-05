//! The benchmark harness of the store: the cluster gate, the round counts, the
//! report line, and the artifact document.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_testkit::bench::{Percentiles, benchmarks_are_on, profile};
use common::bench::{artifact_json, dsn_set, report, rounds, timed_rounds};
use serde_json::json;

/// The gates read the environment of the test run: the clock is off, the
/// counting round counts apply, and the throwaway cluster is named.
#[test]
fn the_gates_read_the_environment_of_the_run() {
    assert!(!benchmarks_are_on(), "the coverage run sets no CADUS_BENCH");
    assert_eq!(rounds(100, 3), 3);
    assert!(dsn_set("the harness test"));
}

/// The artifact names the benchmark, the profile, the percentiles under the
/// given key, the budget, and every field of the run; the report line prints.
#[test]
fn the_artifact_carries_every_field() {
    let times = Percentiles::of(&[5, 7]);
    let body = artifact_json("B-test", "serve_ns", &times, 9, &[("samples", json!(2))]);
    let doc: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(doc["benchmark"], json!("B-test"));
    assert_eq!(doc["profile"], json!(profile()));
    assert_eq!(doc["serve_ns"]["p95_ns"], json!(7));
    assert_eq!(doc["p95_budget_ns"], json!(9));
    assert_eq!(doc["samples"], json!(2));
    assert!(body.ends_with('\n'));
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
