//! Full finite-space evidence for real unit01 candidates; no model or database.
use cadus_core::curriculum::{Curriculum, load_curriculum};
use cadus_core::template::{GateSpec, from_body, gate};
#[path = "instances.rs"]
mod instances;
use cadus_worker::authoring::{
    cli::{AuthorArgs, select_for},
    job::verify_kind,
    prompt::{AuthoringSpec, Kind},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, path::Path};

type Result<T> = std::result::Result<T, String>;

pub fn run(path: &Path, output: &Path) -> Value {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (curriculum, findings) = load_curriculum(&root.join("curriculum")).unwrap();
    assert!(findings.is_empty(), "{findings:?}");
    let specs = select_for(
        &curriculum,
        &AuthorArgs {
            course: Some("foundations".to_owned()),
            ..AuthorArgs::default()
        },
    )
    .unwrap();
    let drafts: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut occupied = instances::authored(&curriculum, &root.join("docs/content-foundations"));
    let mut reports = Vec::new();
    let mut accepted = Vec::new();
    for row in drafts {
        let key = row["kp_id"].as_str().unwrap();
        let spec = specs
            .iter()
            .find(|s| format!("{}/{}", s.topic_id, s.kp_id) == key)
            .unwrap();
        match check(spec, &row["arguments"], &mut occupied) {
            Ok((body, evidence)) => {
                accepted
                    .push(json!({"kp_id":key,"kind":"template","status":"pending","body":body}));
                reports.push(json!({"kp_id":key,"passed":true,"evidence":evidence}));
            }
            Err(message) => reports.push(json!({"kp_id":key,"passed":false,"rejection":message})),
        }
    }
    std::fs::create_dir_all(output).unwrap();
    write(output, "bodies.json", &json!(accepted));
    write(output, "facts.json", &facts(&curriculum));
    let report = json!({"checked":reports.len(),"passed":accepted.len(),"rows":reports,
        "candidate_sha256":format!("{:x}",Sha256::digest(std::fs::read(path).unwrap())),
        "unit_sha256":format!("{:x}",Sha256::digest(std::fs::read(root.join("curriculum/foundations/01-fractions-decimals.yaml")).unwrap()))});
    write(output, "gate.json", &report);
    report
}

fn check(
    spec: &AuthoringSpec,
    args: &Value,
    occupied: &mut BTreeSet<String>,
) -> Result<(Value, Value)> {
    let body = verify_kind(Kind::Template, spec, args, &[])
        .map_err(|e| format!("{}: {}", e.code, e.message))?;
    let doc = from_body(&body).map_err(|e| e.to_string())?;
    let gate_spec = GateSpec {
        answer_kind: spec.answer_kind,
        exemplars: &spec.exemplars,
        finite: None,
    };
    let verified = gate(&doc, &gate_spec).map_err(|e| format!("{}: {}", e.code, e.message))?;
    if !verified.exhaustive {
        return Err("finite recipe gate was not exhaustive".to_owned());
    }
    let (problems, instances) = instances::check(&doc, occupied)?;
    if problems.len() < 12 || problems.len() as u64 != verified.instances_checked {
        return Err(format!(
            "sample/instance coverage: {} distinct, {} gated",
            problems.len(),
            verified.instances_checked
        ));
    }
    occupied.extend(problems);
    Ok((
        serde_json::from_str(&body).unwrap(),
        json!({
            "exhaustive":true,"instances_checked":verified.instances_checked,
            "distinct_instances":instances.len(),"authored_sibling_collisions":0,
            "balanced_math":true,"answer_contract":doc.answer_contract,"instances":instances
        }),
    ))
}

fn facts(curriculum: &Curriculum) -> Value {
    let mut rows = Vec::new();
    for idx in curriculum.topics_in_course("foundations") {
        let topic = curriculum.topic(*idx).unwrap();
        for kp in &topic.knowledge_points {
            let exemplars:Vec<_>=kp.exemplars.iter().map(|e|{
                let result=e.canonical_answer();
                json!({"problem":e.problem,"answer":e.answer,"solution_sketch":e.solution_sketch,
                    "answer_contract":e.answer_contract,"authored_answer_decidable":result.is_ok(),
                    "undecidable_reason":result.err().map(|e|e.reason)})
            }).collect();
            rows.push(json!({"kp_key":format!("{}/{}",topic.id,kp.id),"exemplars":exemplars}));
        }
    }
    json!({"schema_version":1,"course":"foundations","kps":rows})
}

fn write(output: &Path, name: &str, value: &Value) {
    std::fs::write(
        output.join(name),
        serde_json::to_string_pretty(value).unwrap() + "\n",
    )
    .unwrap();
}
