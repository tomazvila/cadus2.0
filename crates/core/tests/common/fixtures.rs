//! The readers of the answer fixtures under `tests/fixtures/answers`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    unused_imports
)]

use std::path::{Path, PathBuf};

use cadus_core::curriculum::Exemplar;
use cadus_core::curriculum::load::RawCurriculum;
use serde_json::Value;

/// The path of one test fixture.
pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/answers")
        .join(name)
}

/// Read every non-blank line of one fixture as a JSON row of type `T`.
pub fn read_jsonl<T: serde::de::DeserializeOwned>(name: &str) -> Vec<T> {
    let path = fixture(name);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap_or_else(|e| panic!("row {line}: {e}")))
        .collect()
}

/// The authored exemplar named by one reviewed-manifest row, after checking
/// that the manifest still describes the installed problem and answer.
pub fn reviewed_exemplar<'a>(raw: &'a RawCurriculum, row: &Value) -> &'a Exemplar {
    let entry = raw
        .topics()
        .find(|entry| entry.topic.id.as_str() == row["topic_id"].as_str().unwrap())
        .unwrap();
    let kp = entry
        .topic
        .knowledge_points
        .iter()
        .find(|kp| kp.id.as_str() == row["kp_id"].as_str().unwrap())
        .unwrap();
    let item = &kp.exemplars[usize::try_from(row["exemplar_index"].as_u64().unwrap()).unwrap()];
    assert_eq!(item.problem, row["problem"].as_str().unwrap());
    assert_eq!(item.answer, row["answer"].as_str().unwrap());
    item
}

/// Read one JSON fixture stored directly under `tests/fixtures`.
pub fn read_test_json<T: serde::de::DeserializeOwned>(name: &str) -> T {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    serde_json::from_str(
        &std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display())),
    )
    .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()))
}

/// One row of `recovered_2_0.jsonl`: an answer the 1.0 residue held and a 2.0
/// production reads (D-F3, unit f2-grammar).
#[derive(serde::Deserialize)]
pub struct RecoveredRow {
    pub answer: String,
    pub topic_id: String,
    pub kp_id: String,
    pub exemplar_index: i64,
    /// The name of the production that reads the answer.
    pub production: String,
}

/// Read the committed set of answers the 2.0 productions recovered.
pub fn committed_recovered() -> Vec<RecoveredRow> {
    read_jsonl("recovered_2_0.jsonl")
}

/// The identity of every recovered answer: `(topic_id, kp_id, exemplar_index, answer)`.
pub fn recovered_keys() -> std::collections::BTreeSet<(String, String, i64, String)> {
    committed_recovered()
        .into_iter()
        .map(|row| (row.topic_id, row.kp_id, row.exemplar_index, row.answer))
        .collect()
}
