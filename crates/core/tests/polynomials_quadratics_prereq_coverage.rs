//! P2.6 evidence for `07-polynomials-quadratics.yaml`: every topic's
//! `diagnostic_exemplar` is grammar-decidable (the pinned list
//! `NO_VERDICT_DIAGNOSTICS` names each exception), no prerequisite edge
//! dangles, and every topic has at least one practicable knowledge point.
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

/// The `multi-step` topics whose `diagnostic_exemplar` has no answer contract.
///
/// `Exemplar::verdict_policy` gives no verdict for such an item, because the
/// grader has no checker for the `multi-step` kind. The test pins this list:
/// if an author adds a contract to one of these items, remove its id here.
const NO_VERDICT_DIAGNOSTICS: [&str; 8] = [
    "quadratic-equations-factoring",
    "square-root-property",
    "completing-the-square",
    "completing-square-leading-coefficient",
    "applying-the-quadratic-formula",
    "quadratic-formula",
    "converting-to-vertex-form",
    "quadratic-applications",
];

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
        let no_verdict = NO_VERDICT_DIAGNOSTICS.contains(&row.topic_id.as_str());
        let want = if no_verdict {
            DiagnosticState::Undecidable
        } else {
            DiagnosticState::Decidable
        };
        if row.diagnostic != want {
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
