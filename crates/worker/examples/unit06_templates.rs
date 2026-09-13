//! Regenerate the source-bound current technical receipt; historical review artifacts stay immutable.
#![allow(clippy::unwrap_used)]
#[path = "../tests/support/unit06_current_evidence.rs"]
mod current_evidence;
use std::{fs, path::Path};
fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let receipt = current_evidence::current_receipt(&root);
    // Refuse a fixture that changed while its native gates were running.
    let mut sources_at_write = receipt["sources"].clone();
    sources_at_write["candidate_sha256"] = serde_json::json!(current_evidence::hash_bytes(
        &fs::read(root.join(current_evidence::CANDIDATES)).unwrap()));
    current_evidence::source_matches(&receipt, &sources_at_write).expect("candidate source changed during generation");
    let output = root.join(current_evidence::RECEIPT);
    fs::write(&output, serde_json::to_string_pretty(&receipt).unwrap() + "\n").unwrap();
    println!("78 native current-template gates passed; {} distinct instances; technical receipt: {}",
        receipt["valid_distinct_instances"], output.display());
}
