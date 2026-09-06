//! Production-gate proof for the pending templates of reviewed curriculum additions.
#![allow(clippy::expect_used, clippy::panic)]
use std::{collections::BTreeSet, path::Path};

use cadus_core::{curriculum::load_curriculum, instruction::template_instances};
use cadus_worker::authoring::{cli::select, job::verify_kind, prompt::Kind};
use serde_json::Value;

#[test]
fn every_checked_in_recipe_passes_the_production_gate() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (curriculum, findings) = load_curriculum(&root.join("curriculum")).expect("curriculum");
    assert!(findings.is_empty(), "{findings:?}");
    let source = std::fs::read_to_string(
        root.join("docs/content-foundations/accepted-template-recipes/drafts.json"),
    )
    .expect("recipe drafts");
    let drafts: Vec<Value> = serde_json::from_str(&source).expect("recipe JSON");
    assert_eq!(drafts.len(), 62);
    let mut keys = BTreeSet::new();
    for draft in drafts {
        let key = draft["kp_id"].as_str().expect("kp_id");
        assert!(keys.insert(key.to_owned()), "duplicate recipe key {key}");
        assert_eq!(draft["kind"], "template");
        let spec = select(&curriculum, &[key.to_owned()])
            .expect("known key")
            .remove(0);
        let body = verify_kind(Kind::Template, &spec, &draft["arguments"], &[])
            .unwrap_or_else(|error| panic!("{key}: {error}"));
        let instances = template_instances(&body);
        assert!(
            instances.len() >= 12,
            "{key}: {} instances",
            instances.len()
        );
        let problems: BTreeSet<_> = instances.iter().map(|item| item.problem.trim()).collect();
        assert_eq!(problems.len(), instances.len(), "{key}: repeated statement");
        for instance in instances {
            assert!(
                spec.exemplars
                    .iter()
                    .all(|source| source.problem.trim() != instance.problem.trim()),
                "{key}: template repeats an authored exemplar"
            );
        }
    }
}
