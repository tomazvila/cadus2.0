//! Exhaustive production-gate coverage for setup-preserving translation recipes.
#![allow(clippy::unwrap_used, clippy::panic)]
mod common;
use std::{collections::BTreeSet, fs, path::PathBuf};

use cadus_core::{
    answer::{AnswerContract, Outcome, check_contract},
    curriculum::load_curriculum,
    learner::problem_text_hash,
    template::{Compiled, from_body, walk_satisfying},
};
use cadus_worker::authoring::{cli::select, job::verify_kind, prompt::Kind};
use common::template_batch::verified_body;
use serde_json::Value;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rows() -> Vec<Value> {
    ["unit03-translation", "unit05-relation-translation"]
        .iter()
        .flat_map(|directory| {
            let path = root()
                .join("docs/content-foundations")
                .join(directory)
                .join("templates.json");
            serde_json::from_str::<Vec<Value>>(&fs::read_to_string(path).unwrap()).unwrap()
        })
        .collect()
}

#[test]
fn all_sixty_instances_pass_the_current_worker_and_remain_distinct() {
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty(), "{findings:?}");
    let mut problem_hashes = BTreeSet::new();
    for topic in curriculum.topics() {
        for kp in &topic.knowledge_points {
            for exemplar in &kp.exemplars {
                problem_hashes.insert(problem_text_hash(&exemplar.problem));
            }
        }
    }
    let rows = rows();
    assert_eq!(rows.len(), 5);
    let mut rendered = 0;
    for row in rows {
        let key = row["kp_id"].as_str().unwrap();
        assert_eq!(row["status"], "pending");
        let body = verified_body(&curriculum, &row);
        let doc = from_body(&body).unwrap();
        assert!(matches!(
            doc.answer_contract,
            Some(AnswerContract::RelationSetup)
        ));
        let sketch = doc.solution_sketch.as_ref().unwrap();
        for name in doc.params.keys() {
            let placeholder = format!("{{{name}}}");
            assert!(
                doc.statement.contains(&placeholder),
                "{key}: {name} statement"
            );
            assert!(sketch.contains(&placeholder), "{key}: {name} sketch");
        }
        let compiled = Compiled::new(&doc).unwrap();
        let walk = walk_satisfying(&doc.params, &doc.constraints).unwrap();
        assert!(walk.exhaustive);
        assert_eq!(walk.tuples.len(), 12);
        assert_eq!(doc.samples.len(), 12);
        let mut answers = BTreeSet::new();
        for binding in walk.tuples {
            let sample_index = doc
                .samples
                .iter()
                .position(|sample| sample.bindings() == binding)
                .unwrap();
            let sample = &doc.samples[sample_index];
            let instance = compiled.instantiate(binding).unwrap();
            assert!(problem_hashes.insert(instance.instance_hash.clone()));
            assert!(answers.insert(instance.answer.clone()));
            assert_eq!(reconstruct(&instance.text), instance.answer);
            assert!(matches!(
                check_contract(
                    &sample.expected.text(),
                    &instance.answer,
                    doc.answer_contract.clone().unwrap()
                ),
                Outcome::Decided(verdict) if verdict.correct
            ));
            rendered += 1;
        }
    }
    assert_eq!(rendered, 60);
}

#[test]
fn current_gate_rejects_unknowns_malformed_shapes_and_contract_bypass() {
    let (curriculum, _) = load_curriculum(&root().join("curriculum")).unwrap();
    for row in rows() {
        let key = row["kp_id"].as_str().unwrap();
        let spec = select(&curriculum, &[key.to_owned()]).unwrap().remove(0);
        for expression in [
            "relationform((x+y,3),0)",
            "relationform((yes,3),0)",
            "relationform((x,3),9)",
            "relationform((3,4),0)",
            "relationform((x,3,4),0)",
        ] {
            let mut arguments = row["arguments"].clone();
            arguments["answer_expr"] = serde_json::json!(expression);
            assert!(
                verify_kind(Kind::Template, &spec, &arguments, &[]).is_err(),
                "{key}: {expression}"
            );
        }
    }
}

fn reconstruct(sentence: &str) -> String {
    let pieces: Vec<_> = sentence.split('$').collect();
    let values: Vec<_> = pieces.iter().skip(1).step_by(2).copied().collect();
    let shape = pieces
        .iter()
        .step_by(2)
        .copied()
        .collect::<Vec<_>>()
        .join("{}");
    match (shape.as_str(), values.as_slice()) {
        (
            "Write an equation: {} times a number {} plus {} is {}. Preserve the setup; do not solve.",
            [b, x, a, c],
        ) => format!("{b}*{x} + {a} = {c}"),
        (
            "Write an equation: {} less than the quotient of {} and {} is {}. Preserve the setup; do not solve.",
            [a, x, b, c],
        ) => format!("{x}/{b} - {a} = {c}"),
        (
            "Write an equation: {} times the sum of {} and {} is {}. Preserve the setup; do not solve.",
            [a, x, b, c],
        ) => format!("{a}*({x} + {b}) = {c}"),
        (
            "Write an inequality: {} more than {} times {} is at most {}. Preserve the setup; do not solve.",
            [a, b, x, c],
        ) => format!("{b}*{x} + {a} <= {c}"),
        (
            "Write an inequality: the quotient of {} and {} exceeds {}. Preserve the setup; do not solve.",
            [x, a, c],
        ) => format!("{x}/{a} > {c}"),
        _ => panic!("unsupported visible statement: {sentence}"),
    }
}
