//! Pending Unit05 source content passes the production authoring gate exhaustively.
#![allow(clippy::unwrap_used, clippy::panic)]
use cadus_core::{
    answer::{Outcome, check_contract},
    curriculum::load_curriculum,
    template::{Compiled, from_body, walk_satisfying},
};
use cadus_worker::authoring::{cli::select, job::verify_kind, prompt::Kind};
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs, path::Path};

fn rows() -> Vec<Value> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    ["mixtures"]
        .iter()
        .flat_map(|name| {
            let path = root.join(format!(
                "docs/content-foundations/unit05-systems-tail/{name}.json"
            ));
            serde_json::from_str::<Vec<Value>>(&fs::read_to_string(path).unwrap()).unwrap()
        })
        .collect()
}

#[test]
fn every_pending_instance_passes_production_gate_and_contract() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (curriculum, findings) = load_curriculum(&root.join("curriculum")).unwrap();
    assert!(findings.is_empty());
    assert_eq!(rows().len(), 3);
    let mut problems = BTreeSet::new();
    for row in rows() {
        assert_eq!(row["status"], "pending");
        let key = row["kp_id"].as_str().unwrap();
        let spec = select(&curriculum, &[key.to_owned()]).unwrap().remove(0);
        let body = verify_kind(Kind::Template, &spec, &row["arguments"], &[])
            .unwrap_or_else(|e| panic!("{key}: {e:?}"));
        let doc = from_body(&body).unwrap();
        let compiled = Compiled::new(&doc).unwrap();
        let walk = walk_satisfying(&doc.params, &doc.constraints).unwrap();
        assert!(walk.exhaustive);
        assert_eq!(walk.tuples.len(), 12);
        assert_eq!(doc.samples.len(), 12);
        for binding in walk.tuples {
            let sample = doc
                .samples
                .iter()
                .find(|s| s.bindings() == binding)
                .unwrap();
            let instance = compiled.instantiate(binding).unwrap();
            let contract = doc.answer_contract.clone().unwrap();
            assert!(
                matches!(check_contract(&sample.expected.text(), &instance.answer,
                contract.clone()), Outcome::Decided(r) if r.correct)
            );
            for wrong in ["999", "(999,999)", "yes", "no", "none", "all points"] {
                assert!(
                    !matches!(check_contract(&instance.answer, wrong, contract.clone()),
                    Outcome::Decided(r) if r.correct),
                    "{key}: {wrong}"
                );
            }
            assert!(problems.insert(instance.text));
        }
    }
    assert_eq!(problems.len(), 36);
}

#[test]
fn production_gate_rejects_constant_cancelling_and_false_samples() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (curriculum, _) = load_curriculum(&root.join("curriculum")).unwrap();
    for row in rows() {
        let key = row["kp_id"].as_str().unwrap();
        let spec = select(&curriculum, &[key.to_owned()]).unwrap().remove(0);
        for expr in ["(0,0)", "(a-a,b-b)", "(a,0)", "(-a,-b)", "a-a+b-b"] {
            let mut args = row["arguments"].clone();
            args["answer_expr"] = json!(expr);
            assert!(
                verify_kind(Kind::Template, &spec, &args, &[]).is_err(),
                "{key}: {expr}"
            );
        }
        let mut args = row["arguments"].clone();
        args["samples"][0]["expected"] = json!("999");
        assert!(verify_kind(Kind::Template, &spec, &args, &[]).is_err());
    }
}
