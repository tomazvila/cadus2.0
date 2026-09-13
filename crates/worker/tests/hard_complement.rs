//! Pending-only evidence for the six isolated U08/U09 complementary KPs.
#![allow(clippy::unwrap_used)]
mod common;
use cadus_worker::authoring::{job::verify_kind, prompt::Kind};
use common::{
    repo_root as root,
    reviewed_templates::{
        assert_template19_replacements, file_rows, run_rows, spec,
    },
};
use serde_json::{Value, json};

fn drafts() -> Vec<Value> {
    file_rows("docs/content-foundations/hard-complement/templates.json")
}

#[test]
fn archived_rows_have_exact_canonical_replacements_and_current_gate_verdicts() {
    let rows = drafts();
    let report = run_rows(&rows, "target/hard-complement/regression");
    assert_eq!(
        rows.iter()
            .map(|row| row["arguments"]["samples"].as_array().unwrap().len())
            .sum::<usize>(),
        138
    );
    assert_eq!(report["checked"], rows.len(), "{report}");
    assert_eq!(report["passed"], 3, "{report}");
    // Preserve these pending drafts as historical evidence. The restored finite
    // objectives cover single logarithms; the two sum-of-logs drafts must fail
    // closed instead of expanding the reviewed domains.
    for row in report["rows"].as_array().unwrap() {
        let key = row["kp_id"].as_str().unwrap();
        let rejection = match key {
            "common-natural-logarithms/kp1" | "common-natural-logarithms/kp2" => {
                Some("finite-case-unknown:")
            }
            "law-of-sines-cosines/kp1" => Some("authored/sibling collision:"),
            _ => None,
        };
        if let Some(prefix) = rejection {
            assert_eq!(row["passed"], false, "{key}: {row}");
            assert!(
                row["rejection"].as_str().unwrap().starts_with(prefix),
                "{key}: {row}"
            );
        } else {
            assert_eq!(row["passed"], true, "{key}: {row}");
            assert_eq!(row["evidence"]["exhaustive"], true, "{key}: {row}");
            assert!(row["evidence"]["distinct_instances"].as_u64().unwrap() >= 12);
        }
    }
    assert_eq!(
        report["rows"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["passed"] == true)
            .map(|row| row["evidence"]["instances_checked"].as_u64().unwrap())
            .sum::<u64>(),
        52
    );
    assert_template19_replacements(&rows, &["law-of-sines-cosines/kp1"]);
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

#[path = "hard_complement/collisions.rs"]
mod collisions;

#[test]
fn all_declared_sibling_samples_are_collision_free() {
    let report = collisions::check(&root(), &drafts());
    let path = root().join("target/hard-complement");
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(
        path.join("collisions.json"),
        serde_json::to_string_pretty(&report).unwrap(),
    )
    .unwrap();
}
