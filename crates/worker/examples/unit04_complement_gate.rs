//! Offline production-gate evidence for the scoped Unit04 complement.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::{collections::BTreeSet, fs, path::Path};

use cadus_core::{
    answer::{Outcome, check_contract},
    curriculum::load_curriculum,
    learner::problem_text_hash,
    template::{
        Compiled, answer_for_contract, from_body, parse_answer_expr, render, walk_satisfying,
    },
};
use cadus_worker::authoring::{
    cli::select,
    job::{document_digest, verify_kind},
    prompt::{AuthoringSpec, Kind},
};
use serde_json::{Value, json};

fn check_row(row: &Value, spec: &AuthoringSpec, seen: &mut BTreeSet<String>) -> Value {
    let key = row["kp_id"].as_str().unwrap();
    assert_eq!(row["status"], "pending");
    let body = verify_kind(Kind::Template, spec, &row["arguments"], &[])
        .unwrap_or_else(|err| panic!("{key}: {err:?}"));
    let doc = from_body(&body).unwrap();
    let compiled = Compiled::new(&doc).unwrap();
    let ast = parse_answer_expr(&doc.answer_expr).unwrap();
    let walk = walk_satisfying(&doc.params, &doc.constraints).unwrap();
    assert!(walk.exhaustive);
    assert!(walk.tuples.len() >= 12);
    let mut answers = BTreeSet::new();
    let mut instances = Vec::new();
    for bindings in walk.tuples {
        let expected = answer_for_contract(&ast, &bindings, doc.answer_contract.as_ref()).unwrap();
        let instance = compiled.instantiate(bindings).unwrap();
        assert_eq!(expected.text, instance.answer);
        assert!(
            seen.insert(problem_text_hash(instance.text.trim())),
            "{key}: collision"
        );
        answers.insert(instance.canon.clone());
        instances.push(json!({"problem":instance.text,"answer":instance.answer,
            "params": instance.bindings.iter().map(|(k,v)| (k.clone(),v.canonical_string())).collect::<std::collections::BTreeMap<_,_>>(),
            "solution_sketch":render(doc.solution_sketch.as_ref().unwrap(), &instance.bindings).unwrap()}));
    }
    assert!(answers.len() >= 12, "{key}: constant/collapsed outputs");
    negative_controls(row, spec);
    json!({"kp_key":key,"digest":document_digest(key,Kind::Template,&body),
        "exhaustive":true,"instances":instances,"distinct_answers":answers.len(),
        "space_size":serde_json::to_value(doc.space_size).unwrap(),
        "negative_sample_controls":row["arguments"]["samples"].as_array().unwrap().len(),
        "state":"offline pending; no import or approval","gate":"verify_kind + answer_for_contract"})
}

fn negative_controls(row: &Value, spec: &AuthoringSpec) {
    let samples = row["arguments"]["samples"].as_array().unwrap();
    for i in 0..samples.len() {
        let mut wrong = row["arguments"].clone();
        wrong["samples"][i]["expected"] = samples[(i + 1) % samples.len()]["expected"].clone();
        let error =
            verify_kind(Kind::Template, spec, &wrong, &[]).expect_err("wrong sample passed");
        assert_eq!(error.code, "sample-agreement", "{error:?}");
    }
    let mut small = row["arguments"].clone();
    let first = small["params"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    small["params"][&first]["values"]
        .as_array_mut()
        .unwrap()
        .truncate(1);
    let error = verify_kind(Kind::Template, spec, &small, &[]).expect_err("small domain passed");
    assert_eq!(error.code, "space-floor", "{error:?}");
}

fn authored(spec: &AuthoringSpec) {
    assert!(spec.exemplars.len() >= 4);
    for row in &spec.exemplars {
        row.canonical_answer().unwrap();
        let contract = row.answer_contract.clone().unwrap();
        let result = check_contract("999999", &row.answer, contract);
        assert!(!matches!(result, Outcome::Decided(v) if v.correct));
    }
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory = root.join("docs/content-foundations/unit04-complement");
    let rows: Vec<Value> =
        serde_json::from_str(&fs::read_to_string(directory.join("templates.json")).unwrap())
            .unwrap();
    let (curriculum, findings) = load_curriculum(&root.join("curriculum")).unwrap();
    assert!(findings.is_empty(), "{findings:?}");
    let keys: Vec<_> = rows
        .iter()
        .map(|r| r["kp_id"].as_str().unwrap().to_owned())
        .collect();
    let specs = select(&curriculum, &keys).unwrap();
    let mut seen = BTreeSet::new();
    for idx in curriculum.topics_in_course("foundations") {
        for kp in &curriculum.topic(*idx).unwrap().knowledge_points {
            for exemplar in &kp.exemplars {
                seen.insert(problem_text_hash(exemplar.problem.trim()));
            }
        }
    }
    let mut evidence = Vec::new();
    for row in &rows {
        let spec = specs
            .iter()
            .find(|s| format!("{}/{}", s.topic_id, s.kp_id) == row["kp_id"].as_str().unwrap())
            .unwrap();
        authored(spec);
        evidence.push(check_row(row, spec, &mut seen));
    }
    let out = std::env::args().nth(1).expect("output JSON path required");
    fs::write(out, serde_json::to_string_pretty(&evidence).unwrap() + "\n").unwrap();
    println!(
        "{} KPs passed production gates and all negative controls",
        rows.len()
    );
}
