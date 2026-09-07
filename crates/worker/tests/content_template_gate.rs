//! The content audit uses the production worker gate without a database or model.

#![allow(clippy::unwrap_used)]

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::{Value, json};

fn recipe() -> Value {
    json!({
        "kp_id": "perfect-squares/kp1", "kind": "template",
        "arguments": {
            "statement": "Compute ${a}^{{2}}$.",
            "params": {"a": {"kind": "int", "low": 1, "high": 12}},
            "constraints": [], "answer_expr": "a**2",
            "solution_sketch": "${a} \\times {a}$ gives the answer.",
            "hints": ["What does squaring a number mean?"], "distractors": [],
            "samples": [{"params": {"a": 1}, "expected": "1"},
                        {"params": {"a": 12}, "expected": "144"}]
        }
    })
}

#[test]
fn offline_adapter_rejects_wrong_contract_bad_samples_and_unknown_keys() {
    let good = recipe();
    let mut wrong_contract = good.clone();
    wrong_contract["arguments"]["answer_contract"] = json!({"kind": "none"});
    let mut wrong_answer = good.clone();
    wrong_answer["arguments"]["samples"][0]["expected"] = json!("999");
    let mut unknown = good.clone();
    unknown["kp_id"] = json!("no-such-topic/kp1");
    let mut metadata_only = good.clone();
    metadata_only.as_object_mut().unwrap().remove("arguments");
    let documents = json!([good, wrong_contract, wrong_answer, unknown, metadata_only]);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut child = Command::new(env!("CARGO_BIN_EXE_content_template_gate"))
        .arg(root.join("curriculum"))
        .env("DATABASE_URL", "unusable-offline-audit")
        .env("OPENAI_BASE_URL", "unusable-offline-audit")
        .env_remove("OPENAI_API_KEY")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(documents.to_string().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema_version"], 1);
    assert_eq!(
        report["gate_source_hash"],
        env!("CADUS_TEMPLATE_GATE_SOURCE_HASH")
    );
    assert_eq!(report["curriculum_source_hash"].as_str().unwrap().len(), 64);
    assert!(!report["curriculum_hash"].as_str().unwrap().is_empty());
    let results = report["recipes"].as_array().unwrap();
    assert_eq!(results.len(), 5);
    assert_eq!(results[0]["accepted"], true, "{}", results[0]);
    for result in &results[1..] {
        assert_eq!(result["accepted"], false, "{result}");
        assert!(!result["reason"].as_str().unwrap().is_empty());
    }
    assert_eq!(results[1]["document"], documents[1]);
}
