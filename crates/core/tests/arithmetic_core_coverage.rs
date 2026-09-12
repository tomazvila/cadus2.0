//! Whole-file P2.3/P2.4/P2.6 readiness coverage for
//! `curriculum/foundations/00-arithmetic-core.yaml`.
//!
//! The KP list is read from the unit file itself (`serde_norway` into the
//! real [`Unit`] model), not a hand-copied fixture, so this test can never go
//! stale against the curriculum it checks. `OPEN_RESIDUALS` names every
//! knowledge point this content unit has not yet closed; every OTHER
//! knowledge point of the file must be fully closed (>= 4 decidable
//! exemplars, a held-out item, every practice item sketched). A residual
//! entry that is secretly already closed fails its own sanity check below,
//! so the list can only shrink honestly, never rot upward unnoticed.
#![allow(clippy::unwrap_used)]
use std::collections::BTreeSet;
use std::path::Path;

use cadus_core::curriculum::{Unit, load_curriculum};
use cadus_core::readiness::ReadinessIndex;

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

const UNIT_PATH: &str = "curriculum/foundations/00-arithmetic-core.yaml";

/// Knowledge points not yet raised to 4 decidable, held-out, fully-sketched
/// exemplars by this content unit. Every entry here is real (checked below)
/// and genuinely still open — remove an entry the moment its recipe lands.
const OPEN_RESIDUALS: &[&str] = &[
    // All knowledge points now have 4+ decidable exemplars, a held-out item,
    // 3+ practice exemplars, and solution sketches (verified 2026-09-12).
];

fn unit_kp_keys() -> Vec<String> {
    let text = std::fs::read_to_string(root().join(UNIT_PATH)).unwrap();
    let unit: Unit = serde_norway::from_str(&text).unwrap();
    unit.topics
        .iter()
        .flat_map(|topic| {
            topic
                .knowledge_points
                .iter()
                .map(move |kp| format!("{}/{}", topic.id.as_str(), kp.id.as_str()))
        })
        .collect()
}

#[test]
fn every_non_residual_kp_is_practicable_assessable_and_has_solutions() {
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty(), "{findings:?}");
    let index = ReadinessIndex::build(&curriculum);
    let keys = unit_kp_keys();
    assert_eq!(
        keys.len(),
        81,
        "the unit file's own KP count moved; update this count"
    );

    let residuals: BTreeSet<&str> = OPEN_RESIDUALS.iter().copied().collect();
    assert_eq!(
        residuals.len(),
        OPEN_RESIDUALS.len(),
        "a duplicate residual entry"
    );

    let mut failures = Vec::new();
    let mut closed = 0usize;
    for key in &keys {
        let Some(facts) = index.get(key) else {
            failures.push(format!("{key}: not found in the real curriculum"));
            continue;
        };
        let is_closed = facts.decidable.len() >= 4
            && facts.held_out.is_some()
            && facts.practice_exemplars() >= 3
            && facts.solutions;
        let is_residual = residuals.contains(key.as_str());
        if is_residual {
            assert!(
                !is_closed,
                "{key}: listed as an open residual but is already fully closed"
            );
            continue;
        }
        if is_closed {
            closed += 1;
        } else {
            failures.push(format!(
                "{key}: decidable={} held_out={} practice={} solutions={}",
                facts.decidable.len(),
                facts.held_out.is_some(),
                facts.practice_exemplars(),
                facts.solutions
            ));
        }
    }
    for residual in &residuals {
        assert!(
            keys.iter().any(|k| k == residual),
            "{residual}: not a real KP of this unit"
        );
    }
    eprintln!(
        "{closed}/{} non-residual KPs closed; {} residual(s) declared",
        keys.len() - residuals.len(),
        residuals.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
