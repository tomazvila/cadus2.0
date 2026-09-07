//! Current-contract regression for every reviewed Unit06 template artifact.
#![allow(clippy::unwrap_used, clippy::panic)]
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
};

use cadus_core::{curriculum, template};
use cadus_worker::authoring::{cli, job, prompt};
use serde_json::{Value, json};

const SUPERSEDED_BY: &str = "docs/content-foundations/template19-production-gate/drafts.json";
const SUPERSEDED_CANONICAL_DIGESTS: [(&str, &str); 3] = [
    ("pythagorean-converse/kp3", "sha256:350315d088dc0313"),
    ("radical-equations-basic/kp2", "sha256:c5005dc8bce57196"),
    ("radical-equations-basic/kp3", "sha256:c1063117806e183a"),
];

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

fn template19_rows() -> BTreeMap<String, Value> {
    serde_json::from_str::<Vec<Value>>(&fs::read_to_string(root().join(SUPERSEDED_BY)).unwrap())
        .unwrap()
        .into_iter()
        .map(|row| (row["kp_id"].as_str().unwrap().to_owned(), row))
        .collect()
}

fn template19_sources() -> BTreeMap<String, Value> {
    let path = root().join("docs/content-foundations/template19-production-gate/sources.json");
    serde_json::from_str::<Vec<Value>>(&fs::read_to_string(path).unwrap())
        .unwrap()
        .into_iter()
        .map(|row| (row["kp_id"].as_str().unwrap().to_owned(), row))
        .collect()
}

#[test]
fn every_owned_candidate_matches_its_current_gate_and_review_record() {
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
    assert_eq!((pending.len(), blockers.len()), (78, 0));
    let mut statuses = BTreeMap::new();
    let canonical = template19_rows();
    let sources = template19_sources();
    let mut superseded = BTreeSet::new();
    for row in read("candidates.json") {
        let key = row["kp_id"].as_str().unwrap();
        let spec = cli::select(&curriculum, &[key.to_owned()])
            .unwrap()
            .remove(0);
        let body = job::verify_kind(prompt::Kind::Template, &spec, &row["arguments"], &[])
            .unwrap_or_else(|reason| panic!("{key}: {reason:?}"));
        let evidence = &pending[key];
        assert_eq!(
            evidence["body"],
            serde_json::from_str::<Value>(&body).unwrap()
        );
        let status = evidence["status"].as_str().unwrap();
        assert!(
            matches!(status, "pending" | "superseded"),
            "{key}: {status}"
        );
        *statuses.entry(status).or_insert(0) += 1;
        if status == "superseded" {
            assert_eq!(evidence["superseded_by"], SUPERSEDED_BY, "{key}");
            let replacement = &canonical[key];
            assert_eq!(replacement["kind"], "template", "{key}");
            assert_ne!(replacement["arguments"], row["arguments"], "{key}");
            let replacement_body = job::verify_kind(
                prompt::Kind::Template,
                &spec,
                &replacement["arguments"],
                &[],
            )
            .unwrap_or_else(|reason| panic!("{key}: canonical replacement: {reason:?}"));
            let expected_digest = SUPERSEDED_CANONICAL_DIGESTS
                .iter()
                .find_map(|(candidate, digest)| (*candidate == key).then_some(*digest))
                .unwrap_or_else(|| panic!("{key}: no reviewed canonical digest"));
            assert_eq!(
                job::document_digest(key, prompt::Kind::Template, &replacement_body),
                expected_digest,
                "{key}: canonical replacement body drifted"
            );
            let replacement_body: Value = serde_json::from_str(&replacement_body).unwrap();
            assert_eq!(
                replacement_body["answer_contract"], sources[key]["contract"],
                "{key}"
            );
            assert_eq!(sources[key]["repaired"], true, "{key}");
            assert_eq!(sources[key]["samples"], 12, "{key}");
            assert!(
                sources[key]["sources"].as_array().unwrap().iter().any(
                    |source| source == "docs/content-foundations/unit06-correction/drafts.json"
                ),
                "{key}"
            );
            superseded.insert(key.to_owned());
        } else {
            assert_eq!(evidence.get("superseded_by"), None, "{key}");
        }
        assert_eq!(
            evidence["digest"],
            job::document_digest(key, prompt::Kind::Template, &body)
        );
    }
    assert_eq!(
        statuses,
        BTreeMap::from([("pending", 75), ("superseded", 3)])
    );
    assert_eq!(
        superseded,
        BTreeSet::from([
            "pythagorean-converse/kp3".to_owned(),
            "radical-equations-basic/kp2".to_owned(),
            "radical-equations-basic/kp3".to_owned(),
        ])
    );
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
    assert_eq!(count, 936);
}

#[test]
fn every_current_candidate_is_reviewed_pending_only_or_explicitly_superseded() {
    let drafts = read("drafts.json");
    let pending = read("pending-review.json");
    assert_eq!(drafts.len(), pending.len());
    for draft in drafts {
        assert_eq!(draft["kind"], "template");
        let review = pending
            .iter()
            .find(|review| review["kp_id"] == draft["kp_id"])
            .unwrap();
        assert!(matches!(
            review["status"].as_str(),
            Some("pending" | "superseded")
        ));
        assert_eq!(draft.get("approved"), None);
    }
    assert_eq!(json!(read("schema-blockers.json").len()), 0);
}
