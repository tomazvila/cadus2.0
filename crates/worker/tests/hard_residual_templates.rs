//! Archived hard-residual drafts retain exact replacements and current gate evidence.
#![allow(clippy::unwrap_used)]
mod common;
use common::reviewed_templates::{
    assert_report_with_authored_collisions, assert_template19_replacements,
};
#[path = "../examples/unit01/verify.rs"]
mod verify;
use cadus_core::curriculum::load_curriculum;
use cadus_worker::authoring::{
    cli::{AuthorArgs, select_for},
    job::verify_kind,
    prompt::Kind,
};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn drafts() -> Vec<Value> {
    let mut paths: Vec<_> = std::fs::read_dir(root().join("docs/content-foundations/u08-u09-hard"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    paths.sort();
    paths
        .into_iter()
        .flat_map(|path| {
            serde_json::from_str::<Vec<Value>>(&std::fs::read_to_string(path).unwrap()).unwrap()
        })
        .collect()
}

#[test]
fn archived_families_keep_canonical_replacements_and_wrong_samples_fail() {
    let rows = drafts();
    let output = root().join("target/hard-residual/regression");
    std::fs::create_dir_all(&output).unwrap();
    let input = output.join("drafts.json");
    std::fs::write(&input, serde_json::to_string(&rows).unwrap()).unwrap();
    let report = verify::run(&input, &output);
    // These two unchanged drafts are already represented by the canonical
    // template19 candidates and their recorded worked examples.
    let replaced = ["logarithm-basics/kp1", "logarithm-basics/kp2"];
    assert_template19_replacements(&rows, &replaced);
    assert_report_with_authored_collisions(&report, rows.len(), None, &replaced);
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    assert_eq!(rows.len(), 5);
    for row in rows {
        assert_eq!(row["status"], "pending");
        let spec = select_for(
            &curriculum,
            &AuthorArgs {
                kps: vec![row["kp_id"].as_str().unwrap().to_owned()],
                ..AuthorArgs::default()
            },
        )
        .unwrap()
        .remove(0);
        let mut wrong = row["arguments"].clone();
        wrong["samples"][0]["expected"] = json!("999");
        assert!(verify_kind(Kind::Template, &spec, &wrong, &[]).is_err());
    }
}
