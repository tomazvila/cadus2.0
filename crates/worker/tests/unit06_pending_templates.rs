//! Current-contract regression for every unit06 pending template and exact schema refusal.
#![allow(clippy::unwrap_used, clippy::panic)]
use std::{collections::BTreeMap, fs, path::PathBuf};

use cadus_core::{curriculum, template};
use cadus_worker::authoring::{cli, job, prompt};
use serde_json::{Value, json};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(file: &str) -> Vec<Value> {
    if file == "candidates.json" {
        return serde_json::from_str(
            &fs::read_to_string(
                root().join("crates/worker/tests/fixtures/unit06-template-candidates.json"),
            )
            .unwrap(),
        )
        .unwrap();
    }
    serde_json::from_str(
        &fs::read_to_string(
            root()
                .join("docs/content-foundations/unit06-correction")
                .join(file),
        )
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn every_owned_candidate_reproduces_its_current_production_verdict() {
    let (curriculum, findings) = curriculum::load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    let pending: BTreeMap<String, Value> = read("pending-review.json")
        .into_iter()
        .map(|r| (r["kp_id"].as_str().unwrap().to_owned(), r))
        .collect();
    let blockers: BTreeMap<String, Value> = read("schema-blockers.json")
        .into_iter()
        .map(|r| (r["kp_id"].as_str().unwrap().to_owned(), r))
        .collect();
    assert_eq!((pending.len(), blockers.len()), (53, 25));
    for row in read("candidates.json") {
        let key = row["kp_id"].as_str().unwrap();
        let spec = cli::select(&curriculum, &[key.to_owned()])
            .unwrap()
            .remove(0);
        match job::verify_kind(prompt::Kind::Template, &spec, &row["arguments"], &[]) {
            Ok(_) if key == "estimating-square-roots/kp3" => {
                assert_eq!(blockers[key]["code"], "semantic-family");
                assert!(!pending.contains_key(key));
            }
            Ok(body) => {
                let evidence = &pending[key];
                assert_eq!(
                    evidence["body"],
                    serde_json::from_str::<Value>(&body).unwrap()
                );
                assert_eq!(evidence["status"], "pending");
                assert_eq!(
                    evidence["digest"],
                    job::document_digest(key, prompt::Kind::Template, &body)
                );
                assert!(!blockers.contains_key(key));
            }
            Err(reason) => {
                assert_eq!(blockers[key]["code"], reason.code, "{key}");
                assert_eq!(blockers[key]["message"], reason.message, "{key}");
                assert!(!pending.contains_key(key));
            }
        }
    }
}

#[test]
fn every_instance_rejects_an_off_by_one_answer_and_matches_its_recorded_derivation() {
    let mut count = 0;
    for row in read("pending-review.json") {
        let doc = template::from_body(&row["body"].to_string()).unwrap();
        let compiled = template::Compiled::new(&doc).unwrap();
        let walked = template::walk_satisfying(&doc.params, &doc.constraints).unwrap();
        assert!(walked.exhaustive);
        assert_eq!(walked.tuples.len(), 12);
        for (tuple, recorded) in walked
            .tuples
            .into_iter()
            .zip(row["instances"].as_array().unwrap())
        {
            let instance = compiled.instantiate(tuple).unwrap();
            assert_eq!(recorded["problem"], instance.text);
            assert_eq!(recorded["answer"], instance.answer);
            let sketch = cadus_core::template::render(
                doc.solution_sketch.as_ref().unwrap(),
                &instance.bindings,
            )
            .unwrap();
            assert_eq!(recorded["solution_sketch"], sketch);
            let wrong = format!("({})+1", instance.answer);
            let policy = doc
                .answer_contract
                .clone()
                .unwrap_or(cadus_core::answer::AnswerContract::Exact);
            let outcome = cadus_core::answer::check_contract(&instance.answer, &wrong, policy);
            assert!(
                matches!(outcome, cadus_core::answer::Outcome::Decided(v) if !v.correct),
                "{}: {wrong}",
                row["kp_id"]
            );
            count += 1;
        }
    }
    assert_eq!(count, 636);
}

#[test]
fn rejected_candidates_never_appear_in_the_import_bundle() {
    let drafts = read("drafts.json");
    let pending = read("pending-review.json");
    assert_eq!(drafts.len(), pending.len());
    for draft in drafts {
        assert_eq!(draft["kind"], "template");
        assert!(pending.iter().any(|r| r["kp_id"] == draft["kp_id"]));
        assert_eq!(draft.get("approved"), None);
    }
    assert_eq!(json!(read("schema-blockers.json").len()), 25);
}
