//! Pending templates retired after their knowledge point was rewritten.
//!
//! A retirement report under `docs/reports/` keeps each removed row whole,
//! with its previous digest and the verdict of the current production gate on
//! it. The checks here prove the row left its pending set, that the report
//! holds the exact row that was removed, and that the gate still gives the
//! recorded verdict, so a retired body cannot come back unnoticed.

use std::collections::BTreeSet;

use super::{json_rows, repo_root};
use cadus_core::curriculum::load_curriculum;
use cadus_worker::authoring::{cli::select, job::verify_kind, prompt::Kind};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// The rows of one retirement report.
pub fn report(name: &str) -> Vec<Value> {
    let path = format!("docs/reports/{name}-retired-pending-templates.json");
    json_rows(&[path.as_str()], None)
}

/// The retired knowledge-point keys of one report.
pub fn keys(name: &str) -> BTreeSet<String> {
    report(name)
        .iter()
        .map(|row| row["kp_key"].as_str().unwrap().to_owned())
        .collect()
}

/// The gate arguments of a retired row: `arguments`, or the `body` of a
/// symbolic-repair row without its storage fields.
fn arguments(row: &Value) -> Value {
    if let Some(arguments) = row.get("arguments") {
        return arguments.clone();
    }
    let mut body = row["body"].clone();
    let fields = body.as_object_mut().unwrap();
    for field in ["v", "topic_id", "answer_kind"] {
        fields.remove(field);
    }
    body
}

/// The current production-gate verdict on a pending row, in the report's shape.
pub fn current_gate(row: &Value) -> Value {
    let (curriculum, findings) = load_curriculum(&repo_root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    let key = row["kp_id"].as_str().unwrap();
    let spec = select(&curriculum, &[key.to_owned()]).unwrap().remove(0);
    match verify_kind(Kind::Template, &spec, &arguments(row), &[]) {
        Ok(_) => json!({"result": "accepted"}),
        Err(error) => json!({"result": "refused", "code": error.code, "message": error.message}),
    }
}

/// Every row of report `name` is gone from `pending`, is the exact row that
/// was removed from one of `sources`, and still gets its recorded gate verdict.
/// Returns the count of retired rows.
pub fn assert_retired(name: &str, sources: &[&str], pending: &BTreeSet<String>) -> usize {
    let rows = report(name);
    assert!(!rows.is_empty(), "{name}: empty retirement report");
    let mut seen = BTreeSet::new();
    for row in &rows {
        let key = row["kp_key"].as_str().unwrap();
        assert!(seen.insert(key.to_owned()), "{name}: {key} retired twice");
        assert!(!pending.contains(key), "{name}: {key} is still pending");
        let body = &row["body"];
        assert_eq!(body["kp_id"], key, "{name}: {key}");
        assert_eq!(body["kind"], "template", "{name}: {key}");
        let source = row["source"].as_str().unwrap();
        assert!(
            sources.contains(&source),
            "{name}: {key}: unknown source {source}"
        );
        let digest = format!("sha256:{:x}", Sha256::digest(body.to_string().as_bytes()));
        assert_eq!(row["previous_digest"], digest, "{name}: {key}: body drift");
        let gate = current_gate(body);
        assert_eq!(row["current_gate"], gate, "{name}: {key}: gate verdict");
        assert_eq!(row["refusal_code"], gate["code"], "{name}: {key}");
        assert_eq!(row["refusal"], gate["message"], "{name}: {key}");
        assert!(
            !row["superseded_by"].as_array().unwrap().is_empty(),
            "{name}: {key}: no superseding commit"
        );
        assert!(
            !row["retirement_reason"].as_str().unwrap().is_empty(),
            "{name}: {key}: no reason"
        );
    }
    rows.len()
}
