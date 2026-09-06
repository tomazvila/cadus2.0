//! Unit08 drafts pass the production gate, exhaust the domain, and stay independent.
#![allow(clippy::unwrap_used)]
#[path = "../examples/unit01/verify.rs"]
mod verify;
use cadus_core::curriculum::load_curriculum;
use cadus_worker::authoring::{
    cli::{AuthorArgs, select_for},
    job::verify_kind,
    prompt::{AuthoringSpec, Kind},
};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn drafts() -> Vec<Value> {
    let mut paths: Vec<_> =
        std::fs::read_dir(root().join("docs/content-foundations/functions-exponentials/templates"))
            .unwrap()
            .map(|p| p.unwrap().path())
            .collect();
    paths.sort();
    paths
        .into_iter()
        .flat_map(|p| {
            serde_json::from_str::<Vec<Value>>(&std::fs::read_to_string(p).unwrap()).unwrap()
        })
        .collect()
}

fn spec(key: &str) -> AuthoringSpec {
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    select_for(
        &curriculum,
        &AuthorArgs {
            kps: vec![key.to_owned()],
            ..AuthorArgs::default()
        },
    )
    .unwrap()
    .remove(0)
}

#[test]
fn all_pending_templates_exhaust_the_real_gate_and_avoid_authored_and_sibling_problems() {
    let rows = drafts();
    assert_eq!(rows.len(), 68);
    let output = root().join("target/unit08/regression");
    std::fs::create_dir_all(&output).unwrap();
    let input = output.join("drafts.json");
    std::fs::write(&input, serde_json::to_string(&rows).unwrap()).unwrap();
    let report = verify::run(&input, &output);
    assert_eq!(report["passed"], 68, "{report}");
    let mut instances = 0;
    for row in report["rows"].as_array().unwrap() {
        assert_eq!(row["evidence"]["exhaustive"], true);
        assert_eq!(row["evidence"]["distinct_instances"], 12);
        instances += row["evidence"]["instances_checked"].as_u64().unwrap();
    }
    assert_eq!(instances, 816);
}

#[test]
fn wrong_samples_small_spaces_and_wrong_contracts_are_rejected() {
    let row = drafts()
        .into_iter()
        .find(|r| r["kp_id"] == "evaluating-functions/kp1")
        .unwrap();
    let spec = spec("evaluating-functions/kp1");
    let mut wrong = row["arguments"].clone();
    wrong["samples"][0]["expected"] = json!(999);
    assert_eq!(
        verify_kind(Kind::Template, &spec, &wrong, &[])
            .unwrap_err()
            .code,
        "sample-agreement"
    );
    let mut small = row["arguments"].clone();
    small["params"]["a"]["values"] = json!([14]);
    assert!(verify_kind(Kind::Template, &spec, &small, &[]).is_err());
    let mut label_spec = spec.clone();
    for item in &mut label_spec.exemplars {
        item.answer_contract = Some(cadus_core::answer::AnswerContract::ReducedRatio);
    }
    assert!(verify_kind(Kind::Template, &label_spec, &row["arguments"], &[]).is_err());
}
