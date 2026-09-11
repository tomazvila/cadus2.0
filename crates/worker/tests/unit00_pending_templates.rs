//! Pending unit00 rows reproduce through the worker without any store or approval.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use cadus_core::{
    curriculum::load_curriculum,
    template::{Compiled, from_body, walk_satisfying},
};
use cadus_worker::authoring::{
    cli::select,
    job::{document_digest, verify_kind},
    prompt::Kind,
};
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[allow(dead_code)]
#[path = "../examples/unit00_template_gate.rs"]
mod template_gate;
use template_gate::{normalize as normalized, operands};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rows(name: &str) -> Vec<Value> {
    let path = root()
        .join("docs/content-foundations/unit00-templates")
        .join(name);
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn every_pending_digest_reproduces_through_the_worker_and_exact_exhaustive_walk() {
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    let drafts = rows("drafts.json");
    let stored = rows("stored-review.json");
    let proofs = rows("gate-evidence.json");
    assert_eq!(drafts.len(), 64);
    assert_eq!(drafts.len(), stored.len());
    assert_eq!(drafts.len(), proofs.len());
    let mut instances = 0;
    let mut keys = BTreeSet::new();
    for ((draft, row), proof) in drafts.iter().zip(&stored).zip(&proofs) {
        let key = draft["kp_id"].as_str().unwrap();
        keys.insert(key);
        assert_eq!(draft["kind"], "template");
        assert_eq!(row["kind"], "template");
        assert!(draft.get("status").is_none());
        assert_eq!(row["status"], "pending");
        assert_eq!(row["kp_id"], draft["kp_id"]);
        assert_eq!(proof["kp_id"], draft["kp_id"]);
        let spec = select(&curriculum, &[key.to_owned()]).unwrap().remove(0);
        let body = verify_kind(Kind::Template, &spec, &draft["arguments"], &[])
            .unwrap_or_else(|e| panic!("{key}: {}: {}", e.code, e.message));
        assert_eq!(
            serde_json::from_str::<Value>(&body).unwrap(),
            row["body"],
            "{key}"
        );
        assert_eq!(
            document_digest(key, Kind::Template, &body),
            row["digest"],
            "{key}"
        );
        assert_eq!(row["digest"], proof["digest"]);
        let topic = curriculum
            .topics()
            .iter()
            .find(|t| t.id.as_str() == spec.topic_id)
            .unwrap();
        instances += check_instances(&body, proof, topic);
    }
    assert_eq!(keys.len(), 64);
    assert_eq!(instances, 1409);
}

fn check_instances(body: &str, proof: &Value, topic: &cadus_core::curriculum::Topic) -> usize {
    let exemplars: Vec<_> = topic
        .knowledge_points
        .iter()
        .flat_map(|k| &k.exemplars)
        .chain(topic.diagnostic_exemplar.iter())
        .collect();
    let forbidden: BTreeSet<_> = exemplars.iter().map(|e| normalized(&e.problem)).collect();
    let doc = from_body(body).unwrap();
    let walk = walk_satisfying(&doc.params, &doc.constraints).unwrap();
    assert!(walk.exhaustive);
    let compiled = Compiled::new(&doc).unwrap();
    let mut hashes = BTreeSet::new();
    let mut calculations = BTreeSet::new();
    let mut actual = Vec::new();
    for binding in walk.tuples {
        let instance = compiled.instantiate(binding).unwrap();
        assert!(
            !forbidden.contains(&normalized(&instance.text)),
            "{}",
            instance.text
        );
        for exemplar in &exemplars {
            assert!(
                operands(&exemplar.problem) != operands(&instance.text)
                    || !exemplar
                        .canonical_answer()
                        .is_ok_and(|answer| answer == instance.canon),
                "operand-answer collision: {}",
                instance.text
            );
        }
        assert!(hashes.insert(instance.instance_hash.clone()));
        calculations.insert((operands(&instance.text), instance.canon.clone()));
        actual.push(
            serde_json::json!({"problem":instance.text,"answer":instance.answer,
            "instance_hash":instance.instance_hash}),
        );
    }
    assert!(hashes.len() >= 12);
    assert!(calculations.len() >= 12);
    assert_eq!(
        calculations.len() as u64,
        proof["distinct_operand_answer_combinations"]
            .as_u64()
            .unwrap()
    );
    assert_eq!(
        hashes.len() as u64,
        proof["distinct_valid_instances"].as_u64().unwrap()
    );
    assert_eq!(actual, proof["instances"].as_array().unwrap().to_vec());
    hashes.len()
}

#[test]
fn all_owned_keys_are_either_pending_or_exactly_reported_blockers() {
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    let unit =
        std::fs::read_to_string(root().join("curriculum/foundations/00-arithmetic-core.yaml"))
            .unwrap();
    let owned: BTreeSet<_> = curriculum
        .topics()
        .iter()
        .filter(|t| unit.contains(&format!("  - id: {}\n", t.id)))
        .flat_map(|t| {
            t.knowledge_points
                .iter()
                .map(|k| format!("{}/{}", t.id, k.id))
        })
        .collect();
    let pending: BTreeSet<_> = rows("drafts.json")
        .iter()
        .map(|r| r["kp_id"].as_str().unwrap().to_owned())
        .collect();
    let blockers: Vec<Value> = serde_json::from_str(
        &std::fs::read_to_string(root().join("docs/reports/unit00-schema-blockers.json")).unwrap(),
    )
    .unwrap();
    let blocked: BTreeSet<_> = blockers
        .iter()
        .map(|r| r["kp_key"].as_str().unwrap().to_owned())
        .collect();
    let semantic_exclusions = BTreeSet::from(["factors-and-multiples/kp1".to_owned()]);
    assert_eq!(owned.len(), 81);
    assert_eq!(blocked.len(), 16);
    assert!(pending.is_disjoint(&blocked));
    assert!(pending.is_disjoint(&semantic_exclusions));
    assert_eq!(
        pending
            .union(&blocked)
            .cloned()
            .collect::<BTreeSet<_>>()
            .union(&semantic_exclusions)
            .cloned()
            .collect::<BTreeSet<_>>(),
        owned
    );
    for blocker in blockers {
        let key = blocker["kp_key"].as_str().unwrap();
        let spec = select(&curriculum, &[key.to_owned()]).unwrap().remove(0);
        let rejection =
            verify_kind(Kind::Template, &spec, &blocker["probe_arguments"], &[]).unwrap_err();
        assert_eq!(rejection.code, blocker["worker_rejection"]["code"]);
        assert_eq!(rejection.message, blocker["worker_rejection"]["message"]);
        check_representation_probe(&blocker);
    }
}

fn check_representation_probe(blocker: &Value) {
    use cadus_core::template::{Bindings, Scalar, answer, parse_answer_expr};
    let Some(probe) = blocker.get("evaluated_probe") else {
        return;
    };
    let ast = parse_answer_expr(probe["answer_expr"].as_str().unwrap()).unwrap();
    let computed = answer(&ast, &Bindings::new()).unwrap();
    assert_eq!(computed.text, probe["actual_text"]);
    assert_ne!(computed.text, probe["required_text"]);
    let mut binding = Bindings::new();
    binding.insert(
        "a".to_owned(),
        Scalar::Text(probe["required_text"].as_str().unwrap().to_owned()).value(),
    );
    let text_error = answer(&parse_answer_expr("a").unwrap(), &binding).unwrap_err();
    assert_eq!(text_error.to_string(), blocker["text_binding_error"]);
}
