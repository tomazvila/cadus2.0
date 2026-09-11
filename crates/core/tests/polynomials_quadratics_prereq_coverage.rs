//! P2.6 evidence for `07-polynomials-quadratics.yaml`: every topic's
//! `diagnostic_exemplar` is grammar-decidable, no prerequisite edge dangles,
//! and every topic has at least one practicable knowledge point.
#![allow(clippy::unwrap_used)]
use std::collections::BTreeSet;
use std::path::Path;

use cadus_core::curriculum::load_curriculum;
use cadus_core::readiness::{DiagnosticState, PrereqCoverage, ReadinessIndex};

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn unit_topic_ids() -> BTreeSet<String> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/polynomials_quadratics_kps.json");
    let keys: Vec<String> =
        serde_json::from_str(&std::fs::read_to_string(fixture).unwrap()).unwrap();
    keys.into_iter()
        .map(|key| key.split('/').next().unwrap().to_owned())
        .collect()
}

#[test]
fn the_unit_names_thirty_five_distinct_topics() {
    assert_eq!(unit_topic_ids().len(), 35);
}

#[test]
fn every_topic_has_a_decidable_diagnostic_no_dangling_prerequisite_and_a_practicable_kp() {
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty(), "{findings:?}");
    let index = ReadinessIndex::build(&curriculum);
    let coverage = PrereqCoverage::build(&curriculum, &index);
    let topic_ids = unit_topic_ids();
    let mut seen = BTreeSet::new();
    let mut failures = Vec::new();
    for row in &coverage.topics {
        if !topic_ids.contains(&row.topic_id) {
            continue;
        }
        seen.insert(row.topic_id.clone());
        if row.diagnostic != DiagnosticState::Decidable {
            failures.push(format!(
                "{}: diagnostic is {:?}",
                row.topic_id, row.diagnostic
            ));
        }
        if !row.dangling.is_empty() {
            failures.push(format!(
                "{}: dangling prerequisites {:?}",
                row.topic_id, row.dangling
            ));
        }
        if row.practicable_kps == 0 {
            failures.push(format!("{}: no practicable knowledge point", row.topic_id));
        }
    }
    let missing: Vec<_> = topic_ids.difference(&seen).collect();
    assert!(
        missing.is_empty(),
        "topics absent from the coverage audit: {missing:?}"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
