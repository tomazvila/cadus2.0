//! Offline unit00 draft validation using the unchanged production worker path.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::collections::BTreeSet;
use std::path::Path;

use cadus_core::curriculum::{Curriculum, Topic, load_curriculum};
use cadus_core::learner::problem_text_hash;
use cadus_core::template::{
    Compiled, GateSpec, Instance, check_instance, from_body, walk_satisfying,
};
use cadus_worker::authoring::{
    cli::select,
    job::{document_digest, verify_kind},
    prompt::Kind,
};
use serde_json::{Value, json};

fn normalize(text: &str) -> String {
    text.to_lowercase()
        .replace("evaluate", "compute")
        .replace("\\times", "*")
        .replace("\\div", "/")
        .replace("{,}", "")
        .replace(['{', '}', '$'], "")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .trim_end_matches('.')
        .to_owned()
}

fn operands(text: &str) -> Vec<String> {
    let text = text.replace("{,}", "");
    let mut numbers: Vec<_> = text
        .split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect();
    numbers.sort();
    numbers
}

fn collides(topic: &Topic, instance: &Instance) -> bool {
    let text = normalize(&instance.text);
    let numbers = operands(&instance.text);
    topic
        .knowledge_points
        .iter()
        .flat_map(|kp| &kp.exemplars)
        .chain(topic.diagnostic_exemplar.iter())
        .any(|e| {
            normalize(&e.problem) == text
                || (operands(&e.problem) == numbers
                    && e.canonical_answer()
                        .is_ok_and(|answer| answer == instance.canon))
        })
}

fn inspect(curriculum: &Curriculum, key: &str, body: &str) -> Result<Value, String> {
    let spec = select(curriculum, &[key.to_owned()]).unwrap().remove(0);
    let topic = curriculum
        .topics()
        .iter()
        .find(|t| t.id.as_str() == spec.topic_id)
        .unwrap();
    let doc = from_body(body).map_err(|e| e.to_string())?;
    let compiled = Compiled::new(&doc).map_err(|e| e.to_string())?;
    let walk = walk_satisfying(&doc.params, &doc.constraints).map_err(|e| e.to_string())?;
    if !walk.exhaustive {
        return Err("verification requires an exhaustive domain".to_owned());
    }
    let gate = GateSpec {
        answer_kind: spec.answer_kind,
        exemplars: &spec.exemplars,
    };
    let mut hashes = BTreeSet::new();
    let mut calculations = BTreeSet::new();
    let mut instances = Vec::new();
    for bindings in walk.tuples {
        let instance = compiled.instantiate(bindings).map_err(|e| e.to_string())?;
        check_instance(&doc, &gate, &instance).map_err(|e| format!("{}: {}", e.code, e.message))?;
        if collides(topic, &instance) {
            return Err(format!("authored/sibling collision: {}", instance.text));
        }
        if !hashes.insert(problem_text_hash(&instance.text)) {
            return Err(format!("duplicate generated instance: {}", instance.text));
        }
        calculations.insert((operands(&instance.text), instance.canon.clone()));
        instances.push(json!({"problem":instance.text,"answer":instance.answer,
            "instance_hash":instance.instance_hash}));
    }
    if hashes.len() < 12 {
        return Err(format!("only {} distinct instances; need 12", hashes.len()));
    }
    if calculations.len() < 12 {
        return Err("fewer than twelve distinct operand-answer combinations".to_owned());
    }
    Ok(json!({"kp_id":key,"distinct_valid_instances":hashes.len(),
        "distinct_operand_answer_combinations":calculations.len(),
        "exhaustive":true,"authored_or_sibling_collisions":0,"instances":instances}))
}

fn gate_rows(curriculum: &Curriculum, rows: &[Value]) -> (Vec<Value>, Vec<Value>, Vec<Value>) {
    let (mut stored, mut evidence, mut rejected) = (Vec::new(), Vec::new(), Vec::new());
    for row in rows {
        let key = row["kp_id"].as_str().unwrap();
        let spec = select(curriculum, &[key.to_owned()]).unwrap().remove(0);
        let gated = verify_kind(Kind::Template, &spec, &row["arguments"], &[])
            .map_err(|e| format!("{}: {}", e.code, e.message));
        let outcome =
            gated.and_then(|body| inspect(curriculum, key, &body).map(|proof| (body, proof)));
        match outcome {
            Ok((body, mut proof)) => {
                let digest = document_digest(key, Kind::Template, &body);
                proof["digest"] = json!(digest);
                evidence.push(proof);
                stored.push(json!({"kp_id":key,"kind":"template","status":"pending",
                    "digest":digest,"body":serde_json::from_str::<Value>(&body).unwrap()}));
            }
            Err(reason) => rejected.push(json!({"kp_key":key,"reason":reason})),
        }
    }
    (stored, evidence, rejected)
}

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(
        args.len(),
        2,
        "usage: unit00_template_gate <candidates.json> <output-dir>"
    );
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (curriculum, findings) = load_curriculum(&root.join("curriculum")).unwrap();
    assert!(findings.is_empty(), "curriculum findings: {findings:?}");
    let rows: Vec<Value> =
        serde_json::from_str(&std::fs::read_to_string(&args[0]).unwrap()).unwrap();
    let unit = std::fs::read_to_string(root.join("curriculum/foundations/00-arithmetic-core.yaml"))
        .unwrap();
    for row in &rows {
        let topic = row["kp_id"].as_str().unwrap().split('/').next().unwrap();
        assert!(
            unit.contains(&format!("  - id: {topic}\n")),
            "outside unit00: {topic}"
        );
        assert_eq!(row["kind"], "template");
        assert!(
            row.get("status").is_none(),
            "draft status belongs to the stored row"
        );
    }
    let (stored, evidence, rejected) = gate_rows(&curriculum, &rows);
    let output = Path::new(&args[1]);
    std::fs::create_dir_all(output).unwrap();
    for (name, data) in [
        ("stored.json", &stored),
        ("evidence.json", &evidence),
        ("rejected.json", &rejected),
    ] {
        std::fs::write(
            output.join(name),
            serde_json::to_string_pretty(data).unwrap() + "\n",
        )
        .unwrap();
    }
    println!(
        "{} production-gated templates; {} rejected",
        stored.len(),
        rejected.len()
    );
    for row in &rejected {
        println!("{}", row);
    }
    if !rejected.is_empty() {
        std::process::exit(1);
    }
}
