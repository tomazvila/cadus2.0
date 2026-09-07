//! Exhaustive recipe evidence and collisions against the checked-in content catalog.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use cadus_core::answer::{Outcome, check_contract};
use cadus_core::curriculum::{Curriculum, load_curriculum};
use cadus_core::instruction::template_instances;
use cadus_core::template::domain::walk_satisfying;
use cadus_core::template::{Compiled, GateSpec, TemplateDoc, gate};
use serde_json::{Value, json};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rows() -> Vec<Value> {
    serde_json::from_str(
        &std::fs::read_to_string(
            root().join("docs/content-foundations/template19-production-gate/drafts.json"),
        )
        .unwrap(),
    )
    .unwrap()
}

fn document(row: &Value, curriculum: &Curriculum) -> Option<TemplateDoc> {
    let (topic_id, _) = row["kp_id"].as_str()?.split_once('/')?;
    let topic = curriculum
        .topics()
        .iter()
        .find(|topic| topic.id.as_str() == topic_id)?;
    let mut body = row
        .get("arguments")
        .or_else(|| row.get("body"))?
        .as_object()?
        .clone();
    body.insert("v".into(), json!(1));
    body.insert("topic_id".into(), json!(topic_id));
    body.insert("answer_kind".into(), json!(topic.answer_kind));
    body.remove("space_size");
    serde_json::from_value(Value::Object(body)).ok()
}

#[test]
fn exact_nineteen_have_exhaustive_material_instances_and_computed_samples() {
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty(), "{findings:?}");
    let rows = rows();
    assert_eq!(rows.len(), 19);
    let mut keys = BTreeSet::new();
    let mut total = 0;
    for row in rows {
        let key = row["kp_id"].as_str().unwrap();
        assert!(keys.insert(key.to_owned()), "duplicate {key}");
        let (topic_id, kp_id) = key.split_once('/').unwrap();
        let topic = curriculum
            .topics()
            .iter()
            .find(|t| t.id.as_str() == topic_id)
            .unwrap();
        let kp = topic
            .knowledge_points
            .iter()
            .find(|kp| kp.id.as_str() == kp_id)
            .unwrap();
        let doc = document(&row, &curriculum).unwrap();
        let spec = GateSpec {
            answer_kind: topic.answer_kind,
            exemplars: &kp.exemplars,
        };
        let verified = gate(&doc, &spec).unwrap_or_else(|error| panic!("{key}: {error}"));
        let walk = walk_satisfying(&doc.params, &doc.constraints).unwrap();
        assert!(verified.exhaustive && walk.exhaustive, "{key}");
        assert_eq!(
            walk.tuples.len(),
            doc.samples.len(),
            "{key}: samples must be exhaustive"
        );
        let samples: BTreeSet<_> = doc.samples.iter().map(|sample| sample.bindings()).collect();
        assert_eq!(
            samples.len(),
            walk.tuples.len(),
            "{key}: repeated sample bindings"
        );
        let compiled = Compiled::new(&doc).unwrap();
        let policy = doc.answer_contract.clone().expect("typed recipe contract");
        let mut statements = BTreeSet::new();
        let mut answers = BTreeSet::new();
        for tuple in walk.tuples {
            assert!(samples.contains(&tuple), "{key}: omitted worked sample");
            let item = compiled.instantiate(tuple).unwrap();
            assert!(
                statements.insert(item.text.clone()),
                "{key}: cosmetic parameter variation"
            );
            answers.insert(item.answer.clone());
            assert_eq!(
                item.answer_contract.as_ref(),
                Some(&policy),
                "{key}: lost contract"
            );
            let sample = doc
                .samples
                .iter()
                .find(|sample| sample.bindings() == item.bindings)
                .unwrap();
            assert!(
                matches!(check_contract(&item.answer, &sample.expected.text(), policy.clone()),
                Outcome::Decided(verdict) if verdict.correct),
                "{key}: wrong worked answer"
            );
            assert!(
                !matches!(check_contract(&item.answer, "999999", policy.clone()),
                Outcome::Decided(verdict) if verdict.correct),
                "{key}: malformed answer accepted"
            );
        }
        assert!(statements.len() >= 12, "{key}");
        assert!(answers.len() >= 2, "{key}: constant answer");
        total += statements.len();
    }
    assert_eq!(total, 236);
}

fn json_files(path: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            json_files(&path, files);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            files.push(path);
        }
    }
}

fn statement_key(text: &str) -> String {
    text.replace("\\left", "")
        .replace("\\right", "")
        .replace('$', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn recipe_objects<'a>(value: &'a Value, rows: &mut Vec<&'a Value>) {
    match value {
        Value::Object(fields) => {
            if value["kind"] == "template"
                && value["kp_id"].is_string()
                && value.get("status").is_none_or(|status| status == "pending")
            {
                rows.push(value);
            }
            for child in fields.values() {
                recipe_objects(child, rows);
            }
        }
        Value::Array(items) => {
            for child in items {
                recipe_objects(child, rows);
            }
        }
        _ => {}
    }
}

#[test]
fn repaired_statements_do_not_collide_with_other_catalog_templates_or_exemplars() {
    let (curriculum, _) = load_curriculum(&root().join("curriculum")).unwrap();
    let mut repaired = BTreeMap::new();
    let mut identities = BTreeMap::new();
    for row in rows() {
        let key = row["kp_id"].as_str().unwrap().to_owned();
        let doc = document(&row, &curriculum).unwrap();
        let identity = serde_json::to_string(&doc).unwrap();
        identities.insert(key.clone(), identity.clone());
        for item in template_instances(&identity) {
            assert!(
                repaired
                    .insert(statement_key(&item.problem), key.clone())
                    .is_none()
            );
        }
    }
    assert_eq!(repaired.len(), 236);
    for topic in curriculum.topics() {
        if let Some(diagnostic) = &topic.diagnostic_exemplar {
            assert!(
                !repaired.contains_key(&statement_key(&diagnostic.problem)),
                "template collides with diagnostic {}",
                topic.id
            );
        }
        for kp in &topic.knowledge_points {
            for exemplar in &kp.exemplars {
                assert!(
                    !repaired.contains_key(&statement_key(&exemplar.problem)),
                    "template collides with exemplar {}/{}",
                    topic.id,
                    kp.id
                );
            }
        }
    }
    let mut files = Vec::new();
    json_files(&root().join("docs"), &mut files);
    let mut seen = BTreeSet::new();
    let mut checked = 0;
    for path in files {
        let value: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let mut candidates = Vec::new();
        recipe_objects(&value, &mut candidates);
        for row in candidates {
            let Some(doc) = document(row, &curriculum) else {
                continue;
            };
            let key = row["kp_id"].as_str().unwrap();
            let identity = serde_json::to_string(&doc).unwrap();
            if identities.get(key) == Some(&identity)
                || !seen.insert((key.to_owned(), identity.clone()))
            {
                continue;
            }
            for item in template_instances(&identity) {
                checked += 1;
                assert!(
                    !repaired.contains_key(&statement_key(&item.problem)),
                    "{key} at {} collides with repaired {}",
                    path.display(),
                    repaired
                        .get(&statement_key(&item.problem))
                        .map_or("", String::as_str)
                );
            }
        }
    }
    println!(
        "template19 collision coverage: {} other distinct recipes, {checked} instances, 236 repaired instances",
        seen.len()
    );
    assert!(seen.len() >= 700, "catalog coverage: {}", seen.len());
    assert!(checked >= 8400, "catalog instance coverage: {checked}");
}

#[test]
fn exhaustive_review_packet_uses_the_real_rust_renderer() {
    let (curriculum, _) = load_curriculum(&root().join("curriculum")).unwrap();
    let worked: Vec<Value> = serde_json::from_str(
        &std::fs::read_to_string(
            root().join("docs/content-foundations/template19-production-gate/worked-samples.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(worked.len(), 236);
    for row in rows() {
        let doc = document(&row, &curriculum).unwrap();
        let compiled = Compiled::new(&doc).unwrap();
        for sample in &doc.samples {
            let params = serde_json::to_value(&sample.params).unwrap();
            let evidence = worked
                .iter()
                .find(|item| item["kp_id"] == row["kp_id"] && item["params"] == params)
                .expect("worked sample in review packet");
            let item = compiled.instantiate(sample.bindings()).unwrap();
            assert_eq!(evidence["problem"], item.text);
            assert_eq!(evidence["answer"], sample.expected.text());
            assert_eq!(
                evidence["worked_solution"],
                cadus_core::template::render(
                    doc.solution_sketch.as_deref().unwrap(),
                    &sample.bindings()
                )
                .unwrap()
            );
        }
    }
}
