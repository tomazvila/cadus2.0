//! Shared readers, gate inputs, and process runners for local authoring fixtures.

use std::path::Path;

use cadus_core::curriculum::Curriculum;
use cadus_core::instruction::ServedInstance;
use cadus_worker::authoring::{cli::select, job::verify_kind, prompt::Kind};
use serde_json::Value;

/// Read every draft file named by a manifest and enforce its source-size limit.
pub fn draft_rows(dir: &Path, max_lines: usize) -> Vec<Value> {
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("manifest.json")).expect("read draft manifest"),
    )
    .expect("parse draft manifest");
    let mut rows = Vec::new();
    for file in manifest["files"].as_array().expect("manifest files") {
        let path = dir.join(file.as_str().expect("draft filename"));
        let text = std::fs::read_to_string(&path).expect("read draft file");
        assert!(
            text.lines().count() < max_lines,
            "{} holds {max_lines} lines or more",
            path.display()
        );
        rows.append(&mut serde_json::from_str(&text).expect("parse draft rows"));
    }
    rows
}

/// Verify one draft with the dense answer set required by every hint ladder.
pub fn draft_rejection(
    curriculum: &Curriculum,
    draft: &Value,
    mut instances: Vec<ServedInstance>,
) -> Option<(String, String, String)> {
    let key = draft["kp_id"].as_str().expect("kp id").to_owned();
    let specs = select(curriculum, std::slice::from_ref(&key)).expect("select kp");
    let kind = Kind::from_wire(draft["kind"].as_str().expect("kind")).expect("known kind");
    if kind == Kind::HintLadder {
        instances.extend((0..=1000).map(|answer| ServedInstance {
            problem: format!("A different practice problem with answer {answer}"),
            answer: answer.to_string(),
        }));
    }
    verify_kind(kind, &specs[0], &draft["arguments"], &instances)
        .err()
        .map(|rejection| (key, kind.as_str().to_owned(), rejection.message))
}

/// Run the generic local-draft importer through the real worker binary.
pub async fn import_local_drafts(manifest: &Path, dsn: &str) -> std::process::Output {
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/authoring/import_local_drafts.py");
    tokio::process::Command::new("python3")
        .arg(script)
        .arg("--manifest")
        .arg(manifest)
        .arg("--worker")
        .arg(env!("CARGO_BIN_EXE_cadus-worker"))
        .env("DATABASE_URL", dsn)
        .output()
        .await
        .expect("run local-draft importer")
}

/// Run the zero-cost pilot importer, optionally including template drafts.
pub async fn import_pilot_drafts(dsn: &str, include_templates: bool) -> std::process::Output {
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/authoring/import_pilot_drafts.py");
    let mut command = tokio::process::Command::new("python3");
    command
        .arg(script)
        .arg("--worker")
        .arg(env!("CARGO_BIN_EXE_cadus-worker"));
    if include_templates {
        command.arg("--include-templates");
    }
    command
        .env("DATABASE_URL", dsn)
        .output()
        .await
        .expect("run pilot importer")
}
