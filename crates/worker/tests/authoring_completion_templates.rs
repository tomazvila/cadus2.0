//! The audited closed-family recipes produce exact practice templates.
#![allow(clippy::panic, clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use cadus_core::{curriculum::load_curriculum, instruction::template_instances};
use cadus_worker::authoring::{cli::select, completion::generate, job::verify_kind, prompt::Kind};

const KEYS: &[&str] = &[
    "mixed-numbers/kp1",
    "mixed-numbers/kp2",
    "mixed-numbers/kp3",
    "decimal-multiplication-powers-of-ten/kp1",
    "decimal-multiplication-powers-of-ten/kp3",
    "decimal-addition-subtraction/kp1",
    "decimal-operations/kp1",
    "adding-integers/kp3",
    "signed-decimal-operations/kp1",
    "signed-decimal-operations/kp2",
    "negative-fractions-decimals/kp3",
    "perfect-square-roots/kp2",
    "perfect-square-roots/kp3",
    "square-roots/kp2",
    "square-roots/kp3",
    "radical-operations/kp1",
    "dividing-radicals/kp1",
    "radical-exponent-conversion/kp3",
    "rational-exponents/kp1",
    "rational-exponents/kp2",
    "rational-expressions/kp3",
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn every_audited_family_produces_exact_distinct_practice() {
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    for key in KEYS {
        let spec = select(&curriculum, &[(*key).to_owned()]).unwrap().remove(0);
        let proposals = generate(&spec, &[]);
        let repeated = generate(&spec, &[]);
        assert_eq!(proposals.drafts, repeated.drafts, "{key}");
        let draft = proposals
            .drafts
            .iter()
            .find(|row| row["kind"] == "template")
            .unwrap_or_else(|| panic!("{key}: {}", proposals.refusals.join("; ")));
        let body = verify_kind(Kind::Template, &spec, &draft["arguments"], &[]).unwrap();
        let instances = template_instances(&body);
        assert!(instances.len() >= 12, "{key}: {}", instances.len());
        for instance in instances {
            assert!(
                spec.exemplars
                    .iter()
                    .all(|exemplar| exemplar.problem.trim() != instance.problem.trim()),
                "{key}: a practice item repeats an authored exemplar: {}",
                instance.problem
            );
        }
    }
}

#[test]
fn the_recipe_set_has_one_entry_for_each_reviewed_residual() {
    assert_eq!(KEYS.len(), 21);
}
