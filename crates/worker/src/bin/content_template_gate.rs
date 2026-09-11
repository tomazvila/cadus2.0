//! Offline audit adapter for the production template preflight and document gate.
//! Reads importer-shaped recipes from stdin. It opens no database or model client.

use std::io::{Read, Write};
use std::path::Path;
use std::process::ExitCode;

use cadus_core::curriculum::{Curriculum, curriculum_hash, lint_curriculum, load_curriculum};
use cadus_worker::authoring::{job::verify_kind, prompt::Kind, selection::select};
use serde_json::{Value, json};

#[path = "../../gate_fingerprint.rs"]
mod gate_fingerprint;

fn check(curriculum: &Curriculum, document: &Value) -> Result<(), String> {
    let fields = document.as_object().ok_or("recipe must be an object")?;
    if fields.len() != 3 || document["kind"] != "template" {
        return Err("recipe must hold exactly kp_id, kind and arguments".to_owned());
    }
    let key = document["kp_id"].as_str().ok_or("kp_id must be a string")?;
    let specs = select(curriculum, &[key.to_owned()]).map_err(|error| error.to_string())?;
    let spec = specs.first().ok_or("recipe selected no knowledge point")?;
    verify_kind(Kind::Template, spec, &document["arguments"], &[])
        .map(|_| ())
        .map_err(|error| format!("{}: {}", error.code, error.message))
}

fn run() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let root = args
        .next()
        .ok_or("usage: content_template_gate <curriculum-dir>")?;
    if args.next().is_some() {
        return Err("usage: content_template_gate <curriculum-dir>".to_owned());
    }
    if !lint_curriculum(Path::new(&root)).is_empty() {
        return Err("curriculum lint returned findings; template audit refused".to_owned());
    }
    let (curriculum, findings) =
        load_curriculum(Path::new(&root)).map_err(|error| error.to_string())?;
    if !findings.is_empty() {
        return Err("curriculum load returned findings; template audit refused".to_owned());
    }
    let mut source = String::new();
    std::io::stdin()
        .read_to_string(&mut source)
        .map_err(|error| error.to_string())?;
    let documents: Vec<Value> = serde_json::from_str(&source).map_err(|error| error.to_string())?;
    let recipes: Vec<_> = documents
        .iter()
        .map(|document| {
            let result = check(&curriculum, document);
            json!({"document": document, "accepted": result.is_ok(), "reason": result.err()})
        })
        .collect();
    let mut files = Vec::new();
    gate_fingerprint::collect(Path::new(&root), "yaml", &mut files)
        .map_err(|error| error.to_string())?;
    let curriculum_source_hash = gate_fingerprint::fingerprint(Path::new(&root), files)
        .map_err(|error| error.to_string())?;
    let result = json!({
        "schema_version": 1,
        "gate_source_hash": env!("CADUS_TEMPLATE_GATE_SOURCE_HASH"),
        "review_engine_digest": cadus_core::review_engine::DIGEST,
        "canonical_curriculum_digest": cadus_core::curriculum::review_context_digest(&curriculum)
            .map_err(|error| error.to_string())?,
        "curriculum_source_hash": curriculum_source_hash,
        "curriculum_hash": curriculum_hash(&curriculum),
        "recipes": recipes,
    });
    let mut stdout = std::io::stdout().lock();
    serde_json::to_writer(&mut stdout, &result).map_err(|error| error.to_string())?;
    writeln!(stdout).map_err(|error| error.to_string())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("template gate facts refused: {error}");
            ExitCode::from(2)
        }
    }
}
