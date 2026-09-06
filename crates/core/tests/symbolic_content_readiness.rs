//! The reviewed symbolic recipes raised 211 Foundations knowledge points
//! across `03-expressions-equations.yaml`, `04-linear-graphs.yaml`, and
//! `05-systems-inequalities.yaml` to 4 decidable exemplars each (a fourth,
//! pedagogically distinct held-out exemplar, plus label/unit/
//! inequality-union answer-contract retrofits on prose exemplars the base
//! grammar could not already decide). This test checks that claim against
//! the REAL curriculum and the REAL `ReadinessIndex`, not against this
//! lane's own generators: every fixture key must be both `practicable`
//! (>= 3 practice items) and `assessable` (a held-out item exists).
//!
//! `tests/fixtures/symbolic_*_kps.json` are the exact serving-key lists this
//! lane's `scripts/authoring/apply_symbolic_*.py` drivers targeted,
//! generated once from this same `ReadinessIndex` and committed so this
//! test needs no Python at run time.
#![allow(clippy::unwrap_used)]
use std::path::Path;

use cadus_core::curriculum::load_curriculum;
use cadus_core::readiness::ReadinessIndex;

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture_keys(name: &str) -> Vec<String> {
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/{name}.json")),
    )
    .unwrap();
    serde_json::from_str(&text).unwrap()
}

fn assert_practicable_and_assessable(index: &ReadinessIndex, keys: &[String]) {
    let mut failures = Vec::new();
    for key in keys {
        match index.get(key) {
            Some(facts) if facts.practice_exemplars() >= 3 && facts.held_out.is_some() => {}
            Some(facts) => failures.push(format!(
                "{key}: decidable={} held_out={:?} practice={}",
                facts.decidable.len(),
                facts.held_out,
                facts.practice_exemplars()
            )),
            None => failures.push(format!("{key}: not found in the real curriculum")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn expressions_equations_kps_are_practicable_and_assessable() {
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    let index = ReadinessIndex::build(&curriculum);
    let keys = fixture_keys("symbolic_expressions_equations_kps");
    assert_eq!(keys.len(), 67, "every KP of the unit reached 4 exemplars");
    assert_practicable_and_assessable(&index, &keys);
}

#[test]
fn linear_graphs_kps_are_practicable_and_assessable() {
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    let index = ReadinessIndex::build(&curriculum);
    let keys = fixture_keys("symbolic_linear_graphs_kps");
    // 69 of 69: interpreting-graphs-qualitatively (3 KPs) and
    // interpreting-linear-models/kp1,kp2 (5 KPs total) closed this round by
    // recasting each free-prose answer as a closed label vocabulary (the
    // same technique the base curriculum already uses for classification
    // answers like "parallel"/"perpendicular"/"neither"), with a fourth
    // held-out exemplar per KP.
    assert_eq!(keys.len(), 69);
    assert_practicable_and_assessable(&index, &keys);
}

#[test]
fn systems_inequalities_kps_are_practicable_and_assessable() {
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    let index = ReadinessIndex::build(&curriculum);
    let keys = fixture_keys("symbolic_systems_inequalities_kps");
    // 75 of 75: interval-notation/kp2,kp3 closed this round. The
    // inequality_union contract compares bounded intervals, infinite rays,
    // and unions as mathematical sets, including endpoint openness.
    assert_eq!(keys.len(), 75);
    assert_practicable_and_assessable(&index, &keys);
}

#[test]
fn the_five_formerly_residual_linear_graphs_kps_are_now_fully_decidable() {
    let (curriculum, _) = load_curriculum(&root().join("curriculum")).unwrap();
    let index = ReadinessIndex::build(&curriculum);
    for key in [
        "interpreting-graphs-qualitatively/kp1",
        "interpreting-graphs-qualitatively/kp2",
        "interpreting-graphs-qualitatively/kp3",
        "interpreting-linear-models/kp1",
        "interpreting-linear-models/kp2",
    ] {
        let facts = index.get(key).unwrap();
        assert_eq!(
            facts.decidable.len(),
            4,
            "{key}: every exemplar now carries a label contract or a base-grammar answer"
        );
        assert_eq!(
            facts.held_out,
            Some(3),
            "{key}: the 4th exemplar is held out"
        );
    }
}

#[test]
fn the_two_formerly_residual_interval_notation_kps_are_now_fully_decidable() {
    let (curriculum, _) = load_curriculum(&root().join("curriculum")).unwrap();
    let index = ReadinessIndex::build(&curriculum);
    let kp2 = index.get("interval-notation/kp2").unwrap();
    assert_eq!(
        kp2.decidable.len(),
        4,
        "every interval and inequality row uses set-equivalence grading"
    );
    let kp3 = index.get("interval-notation/kp3").unwrap();
    assert_eq!(
        kp3.decidable.len(),
        4,
        "every interval union and inequality union uses set-equivalence grading"
    );
}
