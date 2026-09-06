//! New hard-residual drafts run through the current production gate offline.
#![allow(clippy::unwrap_used)]
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
fn exhaustive_pending_families_pass_and_wrong_samples_fail() {
    let rows = drafts();
    let output = root().join("target/hard-residual/regression");
    std::fs::create_dir_all(&output).unwrap();
    let input = output.join("drafts.json");
    std::fs::write(&input, serde_json::to_string(&rows).unwrap()).unwrap();
    let report = verify::run(&input, &output);
    assert_eq!(report["passed"], rows.len(), "{report}");
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
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
