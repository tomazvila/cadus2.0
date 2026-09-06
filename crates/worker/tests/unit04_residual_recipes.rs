//! Pending-only Unit04 recipes, through the actual production authoring verifier.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::{collections::BTreeSet, path::PathBuf};

use cadus_core::{
    answer::{AnswerContract, Outcome, check_contract},
    curriculum::load_curriculum,
    template::{Compiled, TemplateDoc, answer_for_contract, walk_satisfying},
};
use cadus_worker::authoring::{cli::select, job::verify_kind, prompt::Kind};
use serde_json::{Value, json};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn drafts() -> Vec<Value> {
    serde_json::from_str(
        &std::fs::read_to_string(
            root().join("docs/content-foundations/unit04-residual/drafts.json"),
        )
        .unwrap(),
    )
    .unwrap()
}

fn assert_distinct(doc: &TemplateDoc) -> Vec<Value> {
    let walk = walk_satisfying(&doc.params, &doc.constraints).unwrap();
    assert!(walk.exhaustive);
    assert_eq!(walk.tuples.len(), 12);
    let compiled = Compiled::new(doc).unwrap();
    let mut problems = BTreeSet::new();
    let mut answers = BTreeSet::new();
    let mut output = Vec::new();
    for bindings in walk.tuples {
        let instance = compiled.instantiate(bindings.clone()).unwrap();
        let evaluated = answer_for_contract(
            compiled.answer_ast(),
            &bindings,
            doc.answer_contract.as_ref(),
        )
        .unwrap();
        assert_eq!(evaluated.text, instance.answer);
        assert!(problems.insert(instance.text.clone()), "duplicate problem");
        assert!(answers.insert(instance.canon), "collapsed answer family");
        let params: serde_json::Map<String, Value> = bindings
            .iter()
            .map(|(key, value)| (key.clone(), json!(value.canonical_string())))
            .collect();
        output.push(json!({"params": params, "problem": instance.text, "answer": instance.answer}));
    }
    output
}

#[test]
fn pending_recipes_pass_production_and_export_exhaustive_instances() {
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty(), "{findings:?}");
    let drafts = drafts();
    assert_eq!(drafts.len(), 10);
    let mut keys = BTreeSet::new();
    let mut all_problems: BTreeSet<String> = curriculum
        .topics()
        .iter()
        .flat_map(|t| &t.knowledge_points)
        .flat_map(|k| &k.exemplars)
        .map(|e| e.problem.trim().to_owned())
        .collect();
    let mut exports = Vec::new();
    for draft in drafts {
        let key = draft["kp_id"].as_str().unwrap();
        assert!(keys.insert(key.to_owned()));
        assert_eq!(draft["status"], "pending");
        assert_eq!(draft["kind"], "template");
        let spec = select(&curriculum, &[key.to_owned()]).unwrap().remove(0);
        assert_eq!(spec.exemplars.len(), 4, "{key}");
        for exemplar in &spec.exemplars {
            assert!(exemplar.canonical_answer().is_ok(), "{key}");
            assert!(
                exemplar
                    .solution_sketch
                    .as_ref()
                    .is_some_and(|s| s.len() > 35)
            );
        }
        let body = verify_kind(Kind::Template, &spec, &draft["arguments"], &[])
            .unwrap_or_else(|error| panic!("{key}: {error}"));
        let doc: TemplateDoc = serde_json::from_str(&body).unwrap();
        let instances = assert_distinct(&doc);
        for instance in &instances {
            assert!(
                all_problems.insert(instance["problem"].as_str().unwrap().trim().to_owned()),
                "{key}: cross-KP or authored collision"
            );
        }
        exports.push(json!({"kp_id": key, "instances": instances}));
        let mut wrong = draft["arguments"].clone();
        wrong["samples"][0]["expected"] = json!("99999");
        assert!(
            verify_kind(Kind::Template, &spec, &wrong, &[]).is_err(),
            "{key}"
        );
    }
    let output = root().join("target/unit04-evidence");
    std::fs::create_dir_all(&output).unwrap();
    std::fs::write(
        output.join("instances.json"),
        serde_json::to_string_pretty(&exports).unwrap(),
    )
    .unwrap();
}

#[test]
fn output_contracts_refuse_wrong_shape_and_wrong_mathematics() {
    for (contract, expected, wrong) in [
        (AnswerContract::Coordinates { arity: 2 }, "(3,5)", "(5,3)"),
        (AnswerContract::Coordinates { arity: 2 }, "(3,5)", "3"),
        (AnswerContract::PolynomialRelation, "y=2*x+3", "y=3*x+2"),
        (AnswerContract::PolynomialRelation, "y=2*x+3", "2*x+3"),
    ] {
        assert!(
            !matches!(check_contract(expected, wrong, contract), Outcome::Decided(v) if v.correct)
        );
    }
}
