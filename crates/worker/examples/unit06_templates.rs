//! Gate offline unit06 recipes and export importable pending drafts and exact refusals.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::{collections::BTreeSet, fs, path::Path};

use cadus_core::{
    curriculum::load_curriculum,
    learner::problem_text_hash,
    template::{Compiled, from_body, walk_satisfying},
};
use cadus_worker::authoring::{
    cli::select,
    job::{document_digest, verify_kind},
    prompt::Kind,
};
use serde_json::{Value, json};

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory = root.join("docs/content-foundations/unit06-correction");
    let rows: Vec<Value> = serde_json::from_str(
        &fs::read_to_string(
            root.join("crates/worker/tests/fixtures/unit06-template-candidates.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let (curriculum, findings) = load_curriculum(&root.join("curriculum")).unwrap();
    assert!(findings.is_empty(), "{findings:?}");
    let keys: Vec<String> = rows
        .iter()
        .map(|r| r["kp_id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(keys.len(), 78);
    let specs = select(&curriculum, &keys).unwrap();
    write(
        &directory.join("authored-verification.json"),
        &authored_evidence(&specs),
    );
    let authored = authored_hashes(&specs, &curriculum);
    let mut drafts = Vec::new();
    let mut review = Vec::new();
    let mut blockers = Vec::new();
    let mut siblings = BTreeSet::new();
    for row in rows {
        let key = row["kp_id"].as_str().unwrap();
        let spec = select(&curriculum, &[key.to_owned()]).unwrap().remove(0);
        match verify_kind(Kind::Template, &spec, &row["arguments"], &[]) {
            Ok(_) if key == "estimating-square-roots/kp3" && !comparison_outcomes_vary(&row) => {
                blockers.push(json!({"kp_id":key,"code":"semantic-family",
                    "message":"generated comparisons do not exercise both possible orderings",
                    "answer_kind":spec.answer_kind.as_str(),"shared_contract":spec.template_contract()}));
            }
            Ok(body) => {
                let evidence = check(
                    key,
                    &spec,
                    &row["arguments"],
                    &body,
                    &authored,
                    &mut siblings,
                );
                review.push(evidence);
                drafts.push(row);
            }
            Err(reason) => blockers.push(json!({"kp_id":key,"code":reason.code,"message":reason.message,
                "answer_kind":spec.answer_kind.as_str(),"shared_contract":spec.template_contract()})),
        }
    }
    write(&directory.join("drafts.json"), &json!(drafts));
    write(&directory.join("pending-review.json"), &json!(review));
    write(&directory.join("schema-blockers.json"), &json!(blockers));
    println!(
        "{} candidates; {} pending templates; {} exact gate blockers; {} distinct instances",
        keys.len(),
        drafts.len(),
        blockers.len(),
        siblings.len()
    );
}

fn comparison_outcomes_vary(row: &Value) -> bool {
    row["arguments"]["samples"]
        .as_array()
        .is_some_and(|samples| {
            samples
                .iter()
                .filter_map(|sample| sample["expected"].as_str())
                .collect::<BTreeSet<_>>()
                .len()
                > 1
        })
}

fn check(
    key: &str,
    spec: &cadus_worker::authoring::prompt::AuthoringSpec,
    arguments: &Value,
    body: &str,
    authored: &BTreeSet<String>,
    siblings: &mut BTreeSet<String>,
) -> Value {
    let doc = from_body(body).unwrap();
    let compiled = Compiled::new(&doc).unwrap();
    let walk = walk_satisfying(&doc.params, &doc.constraints).unwrap();
    assert!(walk.exhaustive, "{key}: requires exhaustive proof");
    let mut instances = Vec::new();
    for tuple in walk.tuples {
        let instance = compiled.instantiate(tuple).unwrap();
        let hash = problem_text_hash(instance.text.trim());
        assert!(
            !authored.contains(&hash),
            "{key}: authored collision {}",
            instance.text
        );
        assert!(
            siblings.insert(hash),
            "{key}: sibling collision {}",
            instance.text
        );
        instances.push(json!({"problem":instance.text,"answer":instance.answer,
            "solution_sketch":cadus_core::template::render(doc.solution_sketch.as_ref().unwrap(), &instance.bindings).unwrap()}));
    }
    assert!(instances.len() >= 12, "{key}: fewer than twelve instances");
    // A corrupted oracle must be refused by the same production gate.
    let mut wrong = arguments.clone();
    let expected = wrong["samples"][0]["expected"].as_str().unwrap().to_owned();
    wrong["samples"][0]["expected"] = json!(format!("({expected})+1"));
    let refusal =
        verify_kind(Kind::Template, spec, &wrong, &[]).expect_err("corrupted oracle accepted");
    assert_eq!(refusal.code, "sample-agreement", "{key}: {refusal:?}");
    json!({"kp_id":key,"kind":"template","status":"pending","storage":"offline; no DB row created",
        "digest":document_digest(key, Kind::Template, body),
        "body":serde_json::from_str::<Value>(body).unwrap(),"instances":instances,
        "valid_distinct_instances":instances.len(),"authored_collisions":0,"sibling_collisions":0,
        "negative_control":{"code":refusal.code,"message":refusal.message}})
}

fn authored_evidence(specs: &[cadus_worker::authoring::prompt::AuthoringSpec]) -> Value {
    let rows: Vec<Value> = specs.iter().map(|spec| {
        let exemplars: Vec<Value> = spec.exemplars.iter().map(|exemplar| {
            assert!(exemplar.canonical_answer().is_ok(), "{}: undecidable {}", spec.topic_id, exemplar.answer);
            assert!(exemplar.solution_sketch.as_ref().is_some_and(|s| !s.trim().is_empty()));
            json!({"problem":exemplar.problem,"answer":exemplar.answer,
                "answer_contract":exemplar.answer_contract,"solution_sketch":exemplar.solution_sketch,
                "decidable":true})
        }).collect();
        assert!(exemplars.len() >= 4);
        json!({"kp_id":format!("{}/{}", spec.topic_id, spec.kp_id),"exemplars":exemplars})
    }).collect();
    json!(rows)
}

fn authored_hashes(
    specs: &[cadus_worker::authoring::prompt::AuthoringSpec],
    curriculum: &cadus_core::curriculum::Curriculum,
) -> BTreeSet<String> {
    let mut hashes = BTreeSet::new();
    for spec in specs {
        for item in &spec.exemplars {
            hashes.insert(problem_text_hash(item.problem.trim()));
        }
    }
    for topic in curriculum.topics() {
        if specs.iter().any(|s| s.topic_id == topic.id.as_str())
            && let Some(item) = &topic.diagnostic_exemplar
        {
            hashes.insert(problem_text_hash(item.problem.trim()));
        }
    }
    hashes
}

fn write(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_string_pretty(value).unwrap() + "\n").unwrap();
}

#[cfg(test)]
mod tests {
    use super::comparison_outcomes_vary;
    use serde_json::json;

    #[test]
    fn comparison_family_requires_both_outcomes() {
        assert!(!comparison_outcomes_vary(&json!({"arguments":{"samples":[
            {"expected":"2"}, {"expected":"2"}
        ]}})));
        assert!(comparison_outcomes_vary(&json!({"arguments":{"samples":[
            {"expected":"1"}, {"expected":"2"}
        ]}})));
    }
}
