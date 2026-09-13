//! Pending-only evidence for the six isolated U08/U09 complementary KPs.
#![allow(clippy::unwrap_used)]
mod common;
use cadus_worker::authoring::{job::verify_kind, prompt::Kind};
use common::{
    repo_root as root,
    reviewed_templates::{assert_template19_replacements, file_rows, run_rows, spec},
};
use serde_json::{Value, json};

fn drafts() -> Vec<Value> {
    file_rows("docs/content-foundations/hard-complement/templates.json")
}

#[test]
fn archived_rows_have_exact_canonical_replacements_and_current_gate_verdicts() {
    let rows = drafts();
    assert_eq!(rows.len(), 6);
    let report = run_rows(&rows, "target/hard-complement/regression");
    assert_eq!(
        rows.iter()
            .map(|row| row["arguments"]["samples"].as_array().unwrap().len())
            .sum::<usize>(),
        138
    );
    assert_eq!(report["checked"], rows.len(), "{report}");
    assert_eq!(report["passed"], 4, "{report}");
    // Preserve these pending drafts as historical evidence. The restored finite
    // objectives cover single logarithms; the two sum-of-logs drafts must fail
    // closed instead of expanding the reviewed domains.
    for row in report["rows"].as_array().unwrap() {
        let key = row["kp_id"].as_str().unwrap();
        let rejection = match key {
            "common-natural-logarithms/kp1" | "common-natural-logarithms/kp2" => {
                Some("finite-case-unknown:")
            }
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
            let expected = [
                ("natural-exponential-function/kp1", 12),
                ("sine-cosine-parent-graphs/kp1", 20),
                ("sine-cosine-parent-graphs/kp2", 20),
                ("law-of-sines-cosines/kp1", 16),
            ]
            .into_iter()
            .find(|(candidate, _)| *candidate == key)
            .unwrap()
            .1;
            assert_eq!(row["evidence"]["distinct_instances"], expected, "{key}");
            assert_eq!(row["evidence"]["instances_checked"], expected, "{key}");
            let cases = row["evidence"]["finite_cases"].as_array().unwrap();
            if key == "law-of-sines-cosines/kp1" {
                // All ordered pairs are reviewed rehearsal; single primitives stay teach-only.
                let categories = ["sss", "sas", "two-angles-side", "ssa-opposite-pair"];
                let expected_cases: std::collections::BTreeSet<_> = categories
                    .iter()
                    .flat_map(|first| {
                        categories
                            .iter()
                            .map(move |second| format!("pair-{first}--{second}"))
                    })
                    .collect();
                let actual_cases: std::collections::BTreeSet<_> = cases
                    .iter()
                    .map(|case| {
                        assert_eq!(case["role"], "taught_rehearsal");
                        case["case_id"].as_str().unwrap().to_owned()
                    })
                    .collect();
                assert_eq!(actual_cases, expected_cases);
                assert_eq!(cases.len(), 16);
                let current = spec(key);
                let policy = current.finite.as_ref().unwrap();
                policy.validate(key).unwrap();
                assert_eq!(policy.domain.cases.len(), 20);
                assert_eq!(
                    policy
                        .domain
                        .cases
                        .iter()
                        .filter(
                            |case| case.role == cadus_core::curriculum::FiniteCaseRole::TeachOnly
                        )
                        .count(),
                    4
                );
                assert_eq!(
                    row["evidence"]["finite_policy_fingerprint"],
                    policy.fingerprint
                );
            } else {
                assert!(cases.is_empty(), "{key}: unexpected finite policy");
                assert!(
                    row["evidence"]["finite_policy_fingerprint"].is_null(),
                    "{key}"
                );
            }
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
        68
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
