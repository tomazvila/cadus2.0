//! Pending-only evidence for the six isolated U08/U09 complementary KPs.
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
    serde_json::from_str(
        &std::fs::read_to_string(
            root().join("docs/content-foundations/hard-complement/templates.json"),
        )
        .unwrap(),
    )
    .unwrap()
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
fn production_gate_is_exhaustive_and_collision_free() {
    let path = root().join("docs/content-foundations/hard-complement/templates.json");
    let report = verify::run(&path, &root().join("target/hard-complement/regression"));
    assert_eq!(report["passed"], report["checked"], "{report}");
    assert_eq!(report["passed"], drafts().len());
    for row in report["rows"].as_array().unwrap() {
        assert_eq!(row["evidence"]["exhaustive"], true);
        assert!(row["evidence"]["distinct_instances"].as_u64().unwrap() >= 12);
    }
}

#[test]
fn corrupt_samples_hidden_inputs_small_spaces_and_wrong_contracts_fail_closed() {
    for row in drafts() {
        assert_eq!(row["status"], "pending");
        let spec = spec(row["kp_id"].as_str().unwrap());
        let arguments = &row["arguments"];
        let mut wrong = arguments.clone();
        wrong["samples"][0]["expected"] = json!("9999");
        assert!(verify_kind(Kind::Template, &spec, &wrong, &[]).is_err());
        let mut hidden = arguments.clone();
        hidden["statement"] = json!("Evaluate the expression with ${a}$.");
        assert!(verify_kind(Kind::Template, &spec, &hidden, &[]).is_err());
        let mut small = arguments.clone();
        small["params"]["a"]["values"] = json!([1]);
        small["params"]["b"]["values"] = json!([2]);
        assert!(verify_kind(Kind::Template, &spec, &small, &[]).is_err());
        let mut incompatible = spec.clone();
        for exemplar in &mut incompatible.exemplars {
            exemplar.answer_contract = Some(cadus_core::answer::AnswerContract::ReducedRatio);
        }
        assert!(verify_kind(Kind::Template, &incompatible, arguments, &[]).is_err());
    }
}
