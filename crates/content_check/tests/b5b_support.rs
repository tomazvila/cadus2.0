//! Lane B5b: the helpers that the `b5b_*` tests share. Each test file includes
//! this file as a module; as a test target of its own it holds no test.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

pub struct Run {
    pub exit: i32,
    pub doc: Value,
    pub stderr: String,
}

/// Run the real binary. The stdout text must be exactly one JSON document.
pub fn run(args: &[&str]) -> Run {
    let output = Command::new(env!("CARGO_BIN_EXE_content_check"))
        .args(args)
        .output()
        .expect("the binary starts");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.lines().count(), 1, "one line on stdout: {stdout}");
    Run {
        exit: output.status.code().expect("an exit code"),
        doc: serde_json::from_str(&stdout).expect("stdout is one JSON document"),
        stderr: String::from_utf8(output.stderr).unwrap(),
    }
}

/// Assert the `cadus.error.v1` document and give its text.
pub fn error_text(result: &Run, exit: i32) -> String {
    assert_eq!(result.exit, exit, "{}", result.doc);
    assert_eq!(result.doc["schema"], "cadus.error.v1");
    assert_eq!(result.doc["exit"], exit);
    let text = result.doc["error"].as_str().expect("an error text");
    assert!(result.stderr.contains(text), "stderr has the diagnostic");
    text.to_owned()
}

/// Assert that the object has exactly the keys of the frozen field list.
pub fn assert_keys(doc: &Value, fields: &[&str]) {
    let have: BTreeSet<&str> = doc
        .as_object()
        .expect("a JSON object")
        .keys()
        .map(String::as_str)
        .collect();
    let want: BTreeSet<&str> = fields.iter().copied().collect();
    assert_eq!(have, want);
}

/// The field list of the `Finding` object of `content-check-cli.md`.
pub const FINDING: [&str; 7] = ["ck", "code", "invariant", "kp", "item", "hash", "detail"];

pub fn assert_findings(list: &Value) {
    for finding in list.as_array().expect("a findings list") {
        assert_keys(finding, &FINDING);
    }
}

pub fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn fixture(name: &str) -> String {
    let path = crate_dir().join("tests/fixtures").join(name);
    path.to_str().unwrap().to_owned()
}

/// The root of the repository of this worktree.
pub fn repo() -> String {
    let path = crate_dir().join("../..").canonicalize().unwrap();
    path.to_str().unwrap().to_owned()
}

/// The shipped curriculum of this worktree.
pub fn curriculum() -> String {
    format!("{}/curriculum", repo())
}

/// The `FLOW` directory of the campaign (`FLOW_DIR` names a different place).
pub fn flow(rel: &str) -> String {
    let root = std::env::var("FLOW_DIR")
        .unwrap_or_else(|_| "/home/deploy/.cache/cadus2_scripts/flow".to_owned());
    let path = PathBuf::from(root).join(rel);
    assert!(path.exists(), "{} is not there", path.display());
    path.to_str().unwrap().to_owned()
}

/// Write a scratch file below the target directory and give its path.
pub fn scratch(name: &str, text: &str) -> String {
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
    path.to_str().unwrap().to_owned()
}

/// The `(kp, invariant)` pairs of a findings list.
pub fn pairs(list: &Value) -> Vec<(String, String)> {
    list.as_array()
        .expect("a findings list")
        .iter()
        .map(|finding| {
            let text = |key: &str| finding[key].as_str().unwrap_or("-").to_owned();
            (text("kp"), text("invariant"))
        })
        .collect()
}
