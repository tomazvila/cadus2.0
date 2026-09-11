//! Emit the Foundations facts consumed by `foundations_content_audit.py`.
//!
//! The adapter deliberately asks the production curriculum model whether each
//! authored expected answer is decidable. The Python audit therefore never
//! approximates the answer grammar.

use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use crate::curriculum::{
    Exemplar, KnowledgePoint, Topic, curriculum_hash, lint_curriculum, load_curriculum,
};
use serde_json::{Value, json};

fn exemplar_row(exemplar: &Exemplar) -> Value {
    let (decidable, reason) = match exemplar.canonical_answer() {
        Ok(_) => (true, None),
        Err(error) => (false, Some(error.reason)),
    };
    json!({
        "answer": exemplar.answer,
        "answer_contract": exemplar.answer_contract,
        "authored_answer_decidable": decidable,
        "problem": exemplar.problem,
        "solution_sketch": exemplar.solution_sketch,
        "undecidable_reason": reason,
    })
}

fn kp_row(topic: &Topic, kp: &KnowledgePoint) -> Value {
    let exemplars: Vec<Value> = kp.exemplars.iter().map(exemplar_row).collect();
    json!({
        "constraints": kp.constraints,
        "exemplars": exemplars,
        "kp_key": format!("{}/{}", topic.id, kp.id),
        "kp_name": kp.name,
        "topic_name": topic.name,
    })
}

fn facts(root: &Path) -> Result<Value, String> {
    let findings = lint_curriculum(root);
    if !findings.is_empty() {
        return Err(format!(
            "curriculum lint returned {} finding(s); audit refused",
            findings.len()
        ));
    }
    let (curriculum, load_findings) = load_curriculum(root).map_err(|error| error.to_string())?;
    if !load_findings.is_empty() {
        return Err(format!(
            "curriculum load returned {} finding(s); audit refused",
            load_findings.len()
        ));
    }
    let mut rows = Vec::new();
    for topic_idx in curriculum.topics_in_course("foundations") {
        let topic = curriculum
            .topic(*topic_idx)
            .ok_or_else(|| "Foundations topic index disappeared".to_owned())?;
        rows.extend(topic.knowledge_points.iter().map(|kp| kp_row(topic, kp)));
    }
    if rows.is_empty() {
        return Err("Foundations contains no knowledge points".to_owned());
    }
    Ok(
        json!({"course": "foundations", "curriculum_hash": curriculum_hash(&curriculum), "kps": rows, "schema_version": 1}),
    )
}

fn run() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let root = args
        .next()
        .ok_or_else(|| "usage: content_audit_facts <curriculum-dir>".to_owned())?;
    if args.next().is_some() {
        return Err("usage: content_audit_facts <curriculum-dir>".to_owned());
    }
    let value = facts(Path::new(&root))?;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    serde_json::to_writer(&mut out, &value).map_err(|error| error.to_string())?;
    writeln!(out).map_err(|error| error.to_string())
}

/// Run the command-line fact adapter.
#[must_use]
pub fn command() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("content audit facts refused: {error}");
            ExitCode::from(2)
        }
    }
}
