//! Exhaustive source-only worker-gate evidence for the five U07 residuals.
#![allow(clippy::unwrap_used, clippy::panic)]
use std::{collections::BTreeSet, fs, path::PathBuf};

use cadus_core::{
    answer::{Outcome, check_contract},
    curriculum::load_curriculum,
    learner::problem_text_hash,
    template::{Compiled, from_body, render, walk_satisfying},
};
use cadus_worker::authoring::{
    cli::select,
    job::{document_digest, verify_kind},
    prompt::Kind,
};
use serde_json::{Value, json};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rows() -> Vec<Value> {
    serde_json::from_str(
        &fs::read_to_string(
            root().join("docs/content-foundations/unit07-complement/templates.json"),
        )
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn exhaustive_production_gate_and_negative_controls() {
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    let mut seen = BTreeSet::new();
    for topic in curriculum.topics() {
        for kp in &topic.knowledge_points {
            for ex in &kp.exemplars {
                seen.insert(problem_text_hash(&ex.problem));
            }
        }
        if let Some(ex) = &topic.diagnostic_exemplar {
            seen.insert(problem_text_hash(&ex.problem));
        }
    }
    let mut evidence = Vec::new();
    assert_eq!(rows().len(), 5);
    for row in rows() {
        let key = row["kp_id"].as_str().unwrap();
        assert_eq!(row["status"], "pending");
        let spec = select(&curriculum, &[key.to_owned()]).unwrap().remove(0);
        assert_eq!(spec.exemplars.len(), 4);
        for ex in &spec.exemplars {
            ex.canonical_answer().unwrap();
        }
        let body = verify_kind(Kind::Template, &spec, &row["arguments"], &[])
            .unwrap_or_else(|e| panic!("{key}: {e:?}"));
        let instances = inspect(&body, &mut seen);
        negative_controls(&row, &spec);
        evidence.push(json!({"kp_id":key,"kind":"template","status":"pending",
            "stage":"production-worker-gate-passed", "digest":document_digest(key,Kind::Template,&body),
            "body":serde_json::from_str::<Value>(&body).unwrap(),"instances":instances}));
    }
    let output = root().join("target/unit07-complement");
    fs::create_dir_all(&output).unwrap();
    fs::write(
        output.join("production-evidence.json"),
        serde_json::to_string_pretty(&evidence).unwrap() + "\n",
    )
    .unwrap();
}

fn inspect(body: &str, seen: &mut BTreeSet<String>) -> Vec<Value> {
    let doc = from_body(body).unwrap();
    let compiled = Compiled::new(&doc).unwrap();
    let walk = walk_satisfying(&doc.params, &doc.constraints).unwrap();
    assert!(walk.exhaustive);
    let capacity = if doc.statement.starts_with("Classify") {
        14
    } else {
        12
    };
    assert_eq!(walk.tuples.len(), capacity);
    assert_eq!(doc.samples.len(), capacity);
    let contract = doc.answer_contract.clone().unwrap();
    let mut rows = Vec::new();
    let mut answers = BTreeSet::new();
    for binding in walk.tuples {
        let sample = doc
            .samples
            .iter()
            .find(|s| s.bindings() == binding)
            .unwrap();
        let item = compiled.instantiate(binding.clone()).unwrap();
        assert!(
            seen.insert(item.instance_hash.clone()),
            "collision: {}",
            item.text
        );
        let outcome = check_contract(&sample.expected.text(), &item.answer, contract.clone());
        assert!(matches!(outcome, Outcome::Decided(r) if r.correct));
        for wrong in wrong_answers(&item.answer) {
            assert!(
                !matches!(check_contract(&item.answer, &wrong, contract.clone()),
                Outcome::Decided(r) if r.correct),
                "wrong accepted: {wrong}"
            );
        }
        answers.insert(item.canon.clone());
        rows.push(
            json!({"problem":item.text,"answer":item.answer,"hash":item.instance_hash,
            "solution_sketch":render(doc.solution_sketch.as_ref().unwrap(), &binding).unwrap(),
            "hints":doc.hints.iter().map(|h| render(h,&binding).unwrap()).collect::<Vec<_>>()}),
        );
    }
    assert!(answers.len() >= 3, "constant or low-entropy family");
    rows
}

fn wrong_answers(answer: &str) -> Vec<String> {
    if !answer.contains(';') {
        return [
            "monomial",
            "binomial",
            "trinomial",
            "GCF",
            "difference of squares",
            "trinomial factoring",
            "unknown",
        ]
        .into_iter()
        .filter(|s| *s != answer)
        .map(str::to_owned)
        .collect();
    }
    let fields: Vec<_> = answer.split("; ").collect();
    let mut wrong = vec![fields[1..].join("; "), format!("{answer}; extra = 0")];
    for (index, field) in fields.iter().enumerate() {
        let (name, value) = field.split_once(" = ").unwrap();
        let replacement = if value == "upward" {
            "downward".into()
        } else if value == "downward" {
            "upward".into()
        } else if value.starts_with('(') {
            "(999, 999)".into()
        } else {
            format!("({value})+1")
        };
        let mut parts = fields.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        parts[index] = format!("{name} = {replacement}");
        wrong.push(parts.join("; "));
    }
    wrong
}

fn negative_controls(row: &Value, spec: &cadus_worker::authoring::prompt::AuthoringSpec) {
    for expr in ["0", "multipart(0,0)", "signcase(0,[0,0])", "a-a"] {
        let mut args = row["arguments"].clone();
        args["answer_expr"] = json!(expr);
        assert!(verify_kind(Kind::Template, spec, &args, &[]).is_err());
    }
    let mut args = row["arguments"].clone();
    args["samples"][0]["expected"] = json!("incorrect label = 999");
    assert!(verify_kind(Kind::Template, spec, &args, &[]).is_err());
}
