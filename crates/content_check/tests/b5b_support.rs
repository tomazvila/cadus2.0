//! Lane B5b: the helpers that the `b5b_*` tests share. Each test file includes
//! this file as a module; as a test target of its own it holds no test.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;

use serde_json::{Value, json};

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

/// The copy of the campaign `FLOW` files that these tests read: the golden
/// pairs of the freeze pack and the seeded selftest trees. `FLOW_DIR` names a
/// different place.
pub fn flow(rel: &str) -> String {
    let root = std::env::var("FLOW_DIR")
        .map_or_else(|_| crate_dir().join("tests/fixtures/flow"), PathBuf::from);
    let path = root.join(rel);
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

/// The base tree of a golden packet: the one KP of the packet with its base
/// exemplars (`existing`, the tree at the freeze commit `d2ca1421`), in a unit
/// beside one stub topic per prerequisite. The campaign applied the golden rows
/// to the shipped tree and later rewrote those KPs, so the shipped tree is no
/// longer the base the rows were written against; the packet still is.
pub fn packet_tree(name: &str, packet: &Value) -> String {
    let topic = &packet["topic"];
    let block = &packet["kp_block"];
    let course = packet["course"].as_str().unwrap();
    let field = |entry: &Value, key: &str| entry.get(key).filter(|value| !value.is_null()).cloned();
    let exemplars: Vec<Value> = packet["existing"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            let mut exemplar = serde_json::Map::new();
            for key in ["problem", "answer", "answer_contract", "solution_sketch"] {
                if let Some(value) = field(entry, key) {
                    exemplar.insert(key.to_owned(), value);
                }
            }
            Value::Object(exemplar)
        })
        .collect();
    let stub = |id: &Value, name: &Value| {
        json!({"id": id, "name": name, "core": true, "difficulty": 0.5, "drill": false,
            "answer_kind": "numeric", "expected_time_secs": 180, "prerequisites": [],
            "knowledge_points": [{"id": "kp1", "name": "stub", "key_prerequisites": [],
                "exemplars": []}]})
    };
    let prerequisites = topic["prerequisites"].as_array().unwrap();
    let mut topics: Vec<Value> = prerequisites
        .iter()
        .map(|prerequisite| stub(&prerequisite["id"], &prerequisite["name"]))
        .collect();
    let mut kp = json!({"id": block["id"], "name": block["name"],
        "key_prerequisites": block["key_prerequisites"], "exemplars": exemplars});
    if let Some(constraints) = field(block, "constraints") {
        kp["constraints"] = constraints;
    }
    let target = json!({"id": topic["id"], "name": topic["name"], "core": true,
        "difficulty": 0.5, "drill": false, "answer_kind": topic["answer_kind"],
        "expected_time_secs": 180,
        "prerequisites": prerequisites.iter().map(|p| json!({"id": p["id"], "weight": 1.0, "key": true})).collect::<Vec<_>>(),
        "knowledge_points": [kp]});
    topics.push(target);
    // JSON is YAML: the loader reads the unit as written.
    let unit = json!({"unit": "golden", "course": course, "module": "Golden", "topics": topics});
    let file = packet["file"]
        .as_str()
        .unwrap()
        .trim_start_matches("curriculum/");
    let courses = format!("courses:\n  - id: {course}\n    name: {course}\n    order: 1\n");
    // A directory of its own per call: tests that run in parallel never read a
    // unit file that another test is writing.
    static CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let call = CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = format!("golden-{name}-{}-{call}", std::process::id());
    let root = scratch(&format!("{dir}/courses.yaml"), &courses);
    scratch(&format!("{dir}/{file}"), &unit.to_string());
    root.trim_end_matches("/courses.yaml").to_owned()
}

/// The base tree of the golden packet `name` (see `packet_tree`).
pub fn golden_base(name: &str) -> String {
    let packet = flow(&format!("spec/golden/{name}.packet.json"));
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(&packet).unwrap()).unwrap();
    packet_tree(name, &doc)
}
