//! Unit01 drafts pass the production gate, exhaust the domain, and stay independent.
#![allow(clippy::unwrap_used)]
mod common;

use common::reviewed_templates::{
    assert_report_with_authored_collisions, assert_standard_negative,
    assert_template19_replacements, directory_rows, run_rows,
};

const DIRECTORY: &str = "docs/content-foundations/fractions-decimals/templates";

#[test]
fn all_reviewed_templates_retain_their_domains_and_current_gate_verdicts() {
    let rows = directory_rows(DIRECTORY);
    assert_eq!(rows.len(), 79);
    assert_eq!(
        rows.iter()
            .map(|row| row["arguments"]["samples"].as_array().unwrap().len())
            .sum::<usize>(),
        964
    );
    let report = run_rows(&rows, "target/unit01/regression");
    const REPLACED: &[&str] = &[
        "percentages/kp3",
        "ratio-tables-equivalent-ratios/kp3",
        "understanding-ratios/kp3",
        "unit-rates/kp2",
    ];
    assert_report_with_authored_collisions(&report, rows.len(), None, REPLACED);
    let expected: std::collections::BTreeMap<_, _> = rows
        .iter()
        .map(|row| {
            (
                row["kp_id"].as_str().unwrap(),
                row["arguments"]["samples"].as_array().unwrap().len() as u64,
            )
        })
        .collect();
    let checked: u64 = report["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["passed"].as_bool().unwrap())
        .map(|row| {
            let key = row["kp_id"].as_str().unwrap();
            let count = row["evidence"]["instances_checked"].as_u64().unwrap();
            assert_eq!(
                count, expected[key],
                "{key}: pinned exhaustive sample count"
            );
            assert_eq!(
                row["evidence"]["distinct_instances"].as_u64().unwrap(),
                count,
                "{key}: distinct exhaustive instances"
            );
            count
        })
        .sum();
    assert_eq!(checked, 916);
    assert_template19_replacements(&rows, REPLACED);
}

#[test]
fn wrong_samples_small_spaces_and_wrong_contracts_are_rejected() {
    assert_standard_negative(directory_rows(DIRECTORY), "fraction-of-a-number/kp1");
}
