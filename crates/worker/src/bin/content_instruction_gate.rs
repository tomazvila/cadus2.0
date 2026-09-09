//! Exhaustive offline instruction review adapter. No database, model, or approval writes.
use std::{collections::BTreeMap, io::Read, path::Path, process::ExitCode};

use cadus_core::{
    curriculum::{Curriculum, curriculum_hash, load_curriculum},
    instruction::ServedInstance,
    template::{Compiled, DomainError, all_hold, enumerate, from_body},
};
use cadus_worker::authoring::{job::verify_kind, prompt::Kind, selection::select};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    templates: Vec<Row>,
    documents: Vec<Row>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    kp_id: String,
    kind: String,
    arguments: Value,
}

fn checked_body(
    curriculum: &Curriculum,
    row: &Row,
    served: &[ServedInstance],
) -> Result<String, String> {
    let specs = select(curriculum, std::slice::from_ref(&row.kp_id)).map_err(|e| e.to_string())?;
    let spec = specs.first().ok_or("no knowledge point selected")?;
    let kind = Kind::from_wire(&row.kind).ok_or("unknown content kind")?;
    verify_kind(kind, spec, &row.arguments, served)
        .map_err(|e| format!("{}: {}", e.code, e.message))
}

fn instances(curriculum: &Curriculum, row: &Row) -> Result<Vec<ServedInstance>, String> {
    if row.kind != "template" {
        return Err("template context contains another content kind".into());
    }
    let body = checked_body(curriculum, row, &[])?;
    let document = from_body(&body).map_err(|e| e.to_string())?;
    let compiled = Compiled::new(&document).map_err(|e| e.to_string())?;
    let tuples = enumerate(&document.params, 100_000).map_err(enumeration_error)?;
    let mut instances = Vec::new();
    for bindings in tuples {
        if !all_hold(&document.constraints, &bindings).map_err(|e| e.to_string())? {
            continue;
        }
        let item = compiled.instantiate(bindings).map_err(|e| e.to_string())?;
        instances.push(ServedInstance {
            problem: item.text,
            answer: item.answer,
        });
    }
    if instances.is_empty() {
        return Err("full finite template context has no admissible instances".into());
    }
    Ok(instances)
}

fn enumeration_error(error: DomainError) -> String {
    match error {
        DomainError::TooLarge { name, count } if count > 100_000 => format!(
            "offline exhaustive context for {name:?} has {count} bindings, which exceeds the limit (100000)"
        ),
        other => other.to_string(),
    }
}

fn audit(curriculum: &Curriculum, input: Input) -> Result<Value, String> {
    if input.templates.is_empty() || input.documents.is_empty() {
        return Err("templates and documents must both be nonempty".into());
    }
    let mut served = BTreeMap::new();
    for row in &input.templates {
        let all = instances(curriculum, row).map_err(|e| format!("{}: {e}", row.kp_id))?;
        if served.insert(row.kp_id.clone(), all).is_some() {
            return Err(format!("{}: duplicate selected template", row.kp_id));
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut results = Vec::new();
    for row in &input.documents {
        if !matches!(row.kind.as_str(), "teach" | "hint_ladder") {
            return Err("documents must be Teach or hint ladders".into());
        }
        if !seen.insert((&row.kp_id, &row.kind)) {
            return Err(format!("{}: duplicate instruction kind", row.kp_id));
        }
        let context = served
            .get(&row.kp_id)
            .ok_or_else(|| format!("{}: no selected template", row.kp_id))?;
        let verdict = checked_body(curriculum, row, context);
        results.push(
            json!({"kp_id":row.kp_id,"kind":row.kind,"accepted":verdict.is_ok(),
            "reason":verdict.err(),"practice_instances":context.len(),"exhaustive":true}),
        );
    }
    Ok(
        json!({"schema_version":1,"curriculum_hash":curriculum_hash(curriculum),
        "boundary":"offline production gate; semantic AI review and serving approval still required",
        "documents":results}),
    )
}

fn run() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let root = args
        .next()
        .ok_or("usage: content_instruction_gate <curriculum-dir>")?;
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    let (curriculum, findings) = load_curriculum(Path::new(&root)).map_err(|e| e.to_string())?;
    if !findings.is_empty() {
        return Err("curriculum has findings".into());
    }
    let mut source = String::new();
    std::io::stdin()
        .read_to_string(&mut source)
        .map_err(|e| e.to_string())?;
    let input = serde_json::from_str(&source).map_err(|e| e.to_string())?;
    let mut result = audit(&curriculum, input)?;
    result["input_sha256"] = json!(format!("{:x}", Sha256::digest(source.as_bytes())));
    result["gate_source_hash"] = json!(env!("CADUS_TEMPLATE_GATE_SOURCE_HASH"));
    serde_json::to_writer(std::io::stdout().lock(), &result).map_err(|e| e.to_string())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("instruction gate refused: {error}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn fixture() -> (Curriculum, Value) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../curriculum");
        let (curriculum, findings) = load_curriculum(&root).unwrap();
        assert!(findings.is_empty());
        let values: Vec<_> = (25..=48).filter(|n| n % 10 + 6 >= 10).collect();
        let samples: Vec<_> = values
            .iter()
            .map(|n| json!({"params":{"a":n},"expected":(n+26).to_string()}))
            .collect();
        let template = json!({"kp_id":"addition-with-carrying/kp1","kind":"template","arguments":{
            "statement":"Compute ${a} + 26$.","params":{"a":{"kind":"choice","values":values}},
            "constraints":[],"answer_expr":"a+26","samples":samples,"distractors":[],
            "solution_sketch":"Add ones and exchange ten ones for one ten, then add tens including the carry.",
            "hints":["Which place needs an exchange?"]}});
        let hint = json!({"kp_id":"addition-with-carrying/kp1","kind":"hint_ladder","arguments":{"hints":[
            "Which digits share the same place value?","What happens when the ones exceed a full ten?",
            "Have you included the carried ten when adding the tens column?"]}});
        (
            curriculum,
            json!({"templates":[template],"documents":[hint]}),
        )
    }

    #[test]
    fn full_context_accepts_method_hints_and_rejects_answer_leakage() {
        let (curriculum, mut input) = fixture();
        let result = audit(&curriculum, serde_json::from_value(input.clone()).unwrap()).unwrap();
        assert_eq!(result["documents"][0]["practice_instances"], 16);
        assert_eq!(result["documents"][0]["accepted"], true);
        input["documents"][0]["arguments"]["hints"][2] = json!("The answer is 51.");
        let result = audit(&curriculum, serde_json::from_value(input).unwrap()).unwrap();
        assert_eq!(result["documents"][0]["accepted"], false);
    }

    #[test]
    fn offline_context_enumerates_beyond_the_interactive_sampling_limit() {
        let (curriculum, mut input) = fixture();
        let args = &mut input["templates"][0]["arguments"];
        args["params"] =
            json!({"a":{"kind":"int","low":10,"high":99},"b":{"kind":"int","low":10,"high":99}});
        args["constraints"] = json!([]);
        args["statement"] = json!("Compute $ {a} + {b} $.");
        args["answer_expr"] = json!("a+b");
        args["samples"] = json!([
            {"params":{"a":10,"b":10},"expected":"20"},
            {"params":{"a":10,"b":99},"expected":"109"},
            {"params":{"a":99,"b":10},"expected":"109"},
            {"params":{"a":99,"b":99},"expected":"198"}
        ]);
        let result = audit(&curriculum, serde_json::from_value(input).unwrap()).unwrap();
        assert_eq!(result["documents"][0]["practice_instances"], 8100);
        assert_eq!(result["documents"][0]["exhaustive"], true);
    }

    #[test]
    fn offline_context_reports_its_100000_binding_limit_before_allocating_tuples() {
        let (curriculum, mut input) = fixture();
        let args = &mut input["templates"][0]["arguments"];
        args["params"] = json!({"a":{"kind":"int","low":1,"high":101},"b":{"kind":"int","low":1,"high":1000}});
        args["constraints"] = json!([]);
        args["statement"] = json!("Compute $ {a} + {b} $.");
        args["answer_expr"] = json!("a+b");
        args["samples"] = json!([
            {"params":{"a":1,"b":1},"expected":"2"},
            {"params":{"a":1,"b":1000},"expected":"1001"},
            {"params":{"a":101,"b":1},"expected":"102"},
            {"params":{"a":101,"b":1000},"expected":"1101"}
        ]);
        let error = audit(&curriculum, serde_json::from_value(input).unwrap()).unwrap_err();
        assert!(error.contains("101000 bindings"));
        assert!(error.contains("limit (100000)"));
        assert!(!error.contains("MAX_DOMAIN_SIZE (10000)"));
    }

    #[test]
    fn missing_context_and_duplicate_selected_templates_are_refused() {
        let (curriculum, mut input) = fixture();
        input["documents"][0]["kp_id"] = json!("adding-integers/kp1");
        let error = audit(&curriculum, serde_json::from_value(input.clone()).unwrap()).unwrap_err();
        assert!(error.contains("no selected template"));
        let duplicate = input["templates"][0].clone();
        input["templates"].as_array_mut().unwrap().push(duplicate);
        let error = audit(&curriculum, serde_json::from_value(input).unwrap()).unwrap_err();
        assert!(error.contains("duplicate selected template"));
    }
}
