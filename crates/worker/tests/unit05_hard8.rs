//! Exhaustive production-worker verification of the pending Unit05 checkpoint.
#![allow(clippy::unwrap_used, clippy::panic)]
use std::{collections::BTreeSet, fs, path::Path};

use cadus_core::{
    answer::{Outcome, check_contract},
    curriculum::load_curriculum,
    template::{Compiled, from_body, walk_satisfying},
};
use cadus_worker::authoring::{cli::select, job::verify_kind, prompt::Kind};
use serde_json::{Value, json};

fn rows() -> Vec<Value> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    [
        "union-templates.json",
        "boundary-graph-templates.json",
        "degenerate-templates.json",
    ]
    .into_iter()
    .flat_map(|name| {
        let path = root
            .join("docs/content-foundations/unit05-hard8")
            .join(name);
        serde_json::from_str::<Vec<Value>>(&fs::read_to_string(path).unwrap()).unwrap()
    })
    .collect()
}

#[test]
fn all_pending_instances_pass_the_real_worker_and_match_exhaustive_samples() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (curriculum, findings) = load_curriculum(&root.join("curriculum")).unwrap();
    assert!(findings.is_empty());
    let mut problems = BTreeSet::new();
    assert_eq!(rows().len(), 7);
    for row in rows() {
        assert_eq!(row["status"], "pending");
        let key = row["kp_id"].as_str().unwrap();
        let spec = select(&curriculum, &[key.to_owned()]).unwrap().remove(0);
        let body = verify_kind(Kind::Template, &spec, &row["arguments"], &[])
            .unwrap_or_else(|e| panic!("{key}: {e:?}"));
        let doc = from_body(&body).unwrap();
        let compiled = Compiled::new(&doc).unwrap();
        let walk = walk_satisfying(&doc.params, &doc.constraints).unwrap();
        let count = if key == "basic-absolute-value-inequalities/kp3" {
            16
        } else {
            12
        };
        assert_eq!(walk.tuples.len(), count, "{key}");
        assert!(walk.exhaustive);
        assert_eq!(doc.samples.len(), count);
        let mut answers = BTreeSet::new();
        for binding in walk.tuples {
            let sample = doc
                .samples
                .iter()
                .find(|s| s.bindings() == binding)
                .unwrap();
            let instance = compiled.instantiate(binding).unwrap();
            let result = check_contract(
                &sample.expected.text(),
                &instance.answer,
                doc.answer_contract.clone().unwrap(),
            );
            assert!(matches!(result, Outcome::Decided(r) if r.correct));
            assert!(problems.insert(instance.text));
            assert!(answers.insert(instance.answer), "{key}: repeated answer");
        }
    }
    assert_eq!(problems.len(), 88);
}

#[test]
fn worker_rejects_constant_cancellation_and_false_samples() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (curriculum, _) = load_curriculum(&root.join("curriculum")).unwrap();
    for row in rows() {
        let key = row["kp_id"].as_str().unwrap();
        let spec = select(&curriculum, &[key.to_owned()]).unwrap().remove(0);
        for expr in ["0", "a-a", "b-b", "-999"] {
            let mut args = row["arguments"].clone();
            args["answer_expr"] = json!(expr);
            assert!(
                verify_kind(Kind::Template, &spec, &args, &[]).is_err(),
                "{key}: {expr}"
            );
        }
        let mut args = row["arguments"].clone();
        args["samples"][0]["expected"] = json!("x <= 999");
        assert!(verify_kind(Kind::Template, &spec, &args, &[]).is_err());
    }
}
