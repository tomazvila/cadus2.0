//! Current production gates, including probes excluded by semantic review.
#![allow(clippy::unwrap_used, clippy::panic)]
use std::{fs, path::Path};

use cadus_core::curriculum::load_curriculum;
use cadus_worker::authoring::{cli::select, job::verify_kind, prompt::Kind};
use serde_json::{Value, json};

#[test]
fn current_worker_gate_evidence_and_explicit_residuals() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = root.join("docs/content-foundations/unit05-inequalities");
    let (curriculum, findings) = load_curriculum(&root.join("curriculum")).unwrap();
    assert!(findings.is_empty());
    let mut evidence = Vec::new();
    for filename in ["templates.json", "blockers.json"] {
        let rows: Vec<Value> =
            serde_json::from_str(&fs::read_to_string(out.join(filename)).unwrap()).unwrap();
        for row in rows {
            let key = row["kp_id"].as_str().unwrap();
            let spec = select(&curriculum, &[key.to_owned()]).unwrap().remove(0);
            let result = verify_kind(Kind::Template, &spec, &row["arguments"], &[]);
            let expected = row["expected_gate"].as_str().unwrap_or("accepted");
            match result {
                Ok(body) => {
                    assert_eq!(expected, "accepted", "{key}");
                    evidence.push(json!({"kp_id":key,"source_status":row["status"],
                        "gate":"accepted","body":body}));
                }
                Err(error) => {
                    assert_eq!(expected, error.code, "{key}: {}", error.message);
                    evidence.push(json!({"kp_id":key,"source_status":row["status"],
                        "gate":"rejected","code":error.code,"message":error.message}));
                }
            }
        }
    }
    assert_eq!(evidence.len(), 27);
    if let Some(path) = std::env::var_os("CADUS_U05_GATE_EVIDENCE") {
        fs::write(
            path,
            serde_json::to_string_pretty(&evidence).unwrap() + "\n",
        )
        .unwrap();
    }
}
