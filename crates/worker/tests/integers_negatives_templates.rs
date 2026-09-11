//! Production-gate regression for the reviewed Unit 02 pending templates.
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::{collections::BTreeSet, path::PathBuf};

use cadus_core::curriculum::load_curriculum;
use cadus_worker::authoring::{
    cli::{AuthorArgs, select_for},
    job::verify,
};
use serde_json::Value;

#[test]
fn every_reviewed_integer_template_passes_the_production_gate() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (curriculum, findings) = load_curriculum(&root.join("curriculum")).unwrap();
    assert!(findings.is_empty(), "{findings:#?}");
    let specs = select_for(
        &curriculum,
        &AuthorArgs {
            course: Some("foundations".to_owned()),
            ..AuthorArgs::default()
        },
    )
    .unwrap();
    let dedicated: Vec<Value> = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/content-foundations/integers-negatives/templates.json"
    )))
    .unwrap();
    assert_eq!(dedicated.len(), 25);
    let unit_keys: BTreeSet<String> = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/integers_negatives_recipes_kps.json"
    )))
    .unwrap();
    let global: Vec<Value> = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/content-foundations/zero-api-completion/drafts.json"
    )))
    .unwrap();
    let global = global.into_iter().filter(|row| {
        row["kind"] == "template"
            && row["kp_id"]
                .as_str()
                .is_some_and(|key| unit_keys.contains(key))
    });

    let mut seen = BTreeSet::new();
    let mut refusals = Vec::new();
    for (source, row) in dedicated
        .into_iter()
        .map(|row| ("dedicated", row))
        .chain(global.map(|row| ("zero-api", row)))
    {
        let key = row["kp_id"].as_str().expect("template kp_id");
        if source == "dedicated" {
            assert!(seen.insert(key.to_owned()), "duplicate template key {key}");
        }
        let (topic_id, kp_id) = key.split_once("/").expect("topic/kp key");
        let spec = specs
            .iter()
            .find(|spec| spec.topic_id == topic_id && spec.kp_id == kp_id)
            .unwrap_or_else(|| panic!("template key {key} is absent from curriculum"));
        if let Err(error) = verify(spec, &row["arguments"]) {
            refusals.push(format!("{source}:{key}: {error}"));
        }
    }
    assert!(refusals.is_empty(), "{}", refusals.join("\n"));
}
