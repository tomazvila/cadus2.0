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
    serde_json::from_str(
        &fs::read_to_string(root.join("docs/content-foundations/unit05-residual/templates.json"))
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn all_96_instances_pass_the_real_worker_and_match_exhaustive_samples() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (curriculum, findings) = load_curriculum(&root.join("curriculum")).unwrap();
    assert!(findings.is_empty());
    let mut problems = BTreeSet::new();
    assert_eq!(rows().len(), 8);
    for row in rows() {
        assert_eq!(row["status"], "pending");
        let key = row["kp_id"].as_str().unwrap();
        let spec = select(&curriculum, &[key.to_owned()]).unwrap().remove(0);
        let body = verify_kind(Kind::Template, &spec, &row["arguments"], &[])
            .unwrap_or_else(|e| panic!("{key}: {e:?}"));
        let doc = from_body(&body).unwrap();
        let compiled = Compiled::new(&doc).unwrap();
        let walk = walk_satisfying(&doc.params, &doc.constraints).unwrap();
        assert_eq!(walk.tuples.len(), 12, "{key}");
        assert!(walk.exhaustive);
        assert_eq!(doc.samples.len(), 12);
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
    assert_eq!(problems.len(), 96);
}

#[test]
fn worker_rejects_wrong_sign_constant_cancellation_and_false_samples() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (curriculum, _) = load_curriculum(&root.join("curriculum")).unwrap();
    for row in rows() {
        let key = row["kp_id"].as_str().unwrap();
        let spec = select(&curriculum, &[key.to_owned()]).unwrap().remove(0);
        for expr in ["(0,0)", "(a-a,b-b)", "(a,0)", "(-a,-b)"] {
            let mut args = row["arguments"].clone();
            args["answer_expr"] = json!(expr);
            assert!(
                verify_kind(Kind::Template, &spec, &args, &[]).is_err(),
                "{key}: {expr}"
            );
        }
        let mut args = row["arguments"].clone();
        args["samples"][0]["expected"] = json!("(999,999)");
        assert!(verify_kind(Kind::Template, &spec, &args, &[]).is_err());
    }
}
