//! Render every checked-in sibling sample with the production renderer.
use cadus_core::{
    learner::problem_text_hash,
    template::{Sample, render},
};
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path};

pub fn check(root: &Path, candidates: &[Value]) -> Value {
    let own: BTreeSet<_> = candidates
        .iter()
        .map(|r| r["kp_id"].as_str().unwrap())
        .collect();
    let mut documents = Vec::new();
    read_documents(&root.join("docs/content-foundations"), &mut documents);
    let mut hashes = BTreeSet::new();
    let mut seen = BTreeSet::new();
    let mut sample_count = 0;
    let mut recipe_count = 0;
    let mut static_count = 0;
    for row in documents {
        if own.contains(row["kp_id"].as_str().unwrap()) {
            continue;
        }
        let args = row.get("arguments").or_else(|| row.get("body")).unwrap();
        if !seen.insert(args.to_string()) {
            continue;
        }
        recipe_count += 1;
        if let Some(problem) = args["problem"].as_str() {
            hashes.insert(problem_text_hash(problem));
            static_count += 1;
            continue;
        }
        for sample in args["samples"].as_array().unwrap() {
            hashes.insert(hash(args, sample));
            sample_count += 1;
        }
    }
    assert!(recipe_count >= 680 && sample_count >= 7000);
    let mut checked = 0;
    for row in candidates {
        let args = &row["arguments"];
        for sample in args["samples"].as_array().unwrap() {
            assert!(hashes.insert(hash(args, sample)), "collision: {row}");
            checked += 1;
        }
    }
    json!({"sibling_recipes":recipe_count,"sibling_declared_samples":sample_count,"sibling_static_recipe_problems":static_count,
        "candidate_instances":checked,"collisions":0,"scope":"all declared sibling samples, static recipe problems and mutual candidates"})
}

fn hash(args: &Value, sample: &Value) -> String {
    let sample: Sample = serde_json::from_value(sample.clone()).unwrap();
    problem_text_hash(&render(args["statement"].as_str().unwrap(), &sample.bindings()).unwrap())
}

fn read_documents(path: &Path, rows: &mut Vec<Value>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            read_documents(&path, rows);
        } else if path.extension().is_some_and(|x| x == "json") {
            collect(
                &serde_json::from_str::<Value>(&std::fs::read_to_string(path).unwrap()).unwrap(),
                rows,
            );
        }
    }
}

fn collect(value: &Value, rows: &mut Vec<Value>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect(item, rows);
            }
        }
        Value::Object(fields) => {
            if value["kind"] == "template" && value["kp_id"].is_string() {
                rows.push(value.clone());
            }
            for value in fields.values() {
                collect(value, rows);
            }
        }
        _ => {}
    }
}
