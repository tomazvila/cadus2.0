//! The readers of the answer fixtures under `tests/fixtures/answers`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    unused_imports
)]

use std::path::{Path, PathBuf};

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
