//! Lane B5a: the helpers that the `b5a_*` tests share. Each test file includes
//! this file as a module; as a test target of its own it holds no test.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

pub struct Run {
    pub exit: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Run {
    pub fn doc(&self) -> Value {
        serde_json::from_str(&self.stdout).expect("stdout is one JSON document")
    }

    pub fn lines(&self) -> Vec<Value> {
        self.stdout
            .lines()
            .map(|line| serde_json::from_str(line).expect("each stdout line is JSON"))
            .collect()
    }
}

pub fn run(args: &[&str]) -> Run {
    let output = Command::new(env!("CARGO_BIN_EXE_content_check"))
        .args(args)
        .output()
        .expect("the binary starts");
    Run {
        exit: output.status.code().expect("an exit code"),
        stdout: String::from_utf8(output.stdout).unwrap(),
        stderr: String::from_utf8(output.stderr).unwrap(),
    }
}

pub fn grade(contract: &str, expected: &str, learner: &str) -> Run {
    run(&[
        "grade",
        "--contract",
        contract,
        "--expected",
        expected,
        "--learner",
        learner,
    ])
}

pub fn mutants(contract: &str, expected: &str) -> Run {
    run(&["mutants", "--contract", contract, "--expected", expected])
}

pub fn assert_error(result: &Run, exit: i32) -> String {
    assert_eq!(result.exit, exit, "{}", result.stdout);
    let doc = result.doc();
    assert_eq!(doc["schema"], "cadus.error.v1");
    assert_eq!(doc["exit"], exit);
    let text = doc["error"].as_str().expect("an error text").to_owned();
    assert!(result.stderr.contains(&text), "stderr has the diagnostic");
    text
}

pub fn batch_file(name: &str, lines: &[String]) -> String {
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::write(&path, lines.join("\n") + "\n").unwrap();
    path.to_str().unwrap().to_owned()
}

pub fn mutant_learners(doc: &Value) -> Vec<&str> {
    doc["mutants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|mutant| {
            assert_eq!(mutant["verdict"], "wrong", "{mutant}");
            mutant["learner"].as_str().unwrap()
        })
        .collect()
}
