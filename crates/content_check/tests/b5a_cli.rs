//! Lane B5a: the `content_check` binary, subcommands `grade` and `mutants`,
//! the error document and each exit code of this build (0, 2, 4).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "b5a_support.rs"]
mod support;

use serde_json::{Value, json};
use support::{assert_error, batch_file, grade, mutant_learners, mutants, run};

// ---- grade ----

#[test]
fn grade_exact_fraction_against_decimal_is_correct() {
    let result = grade(r#"{"kind":"exact"}"#, "1/2", "0.5");
    assert_eq!(result.exit, 0);
    assert_eq!(result.stdout.lines().count(), 1);
    assert_eq!(
        result.doc(),
        json!({"schema": "cadus.grade.v1", "verdict": "correct", "reason": null,
            "notation": false})
    );
}

#[test]
fn grade_prose_learner_is_ungraded_with_a_reason() {
    let result = grade(r#"{"kind":"exact"}"#, "3", "three");
    assert_eq!(result.exit, 0);
    let doc = result.doc();
    assert_eq!(doc["verdict"], "ungraded");
    assert!(
        doc["reason"]
            .as_str()
            .is_some_and(|reason| !reason.is_empty())
    );
}

#[test]
fn grade_matrix_with_one_wrong_entry_is_wrong() {
    let result = grade(
        r#"{"kind":"matrix","rows":2,"cols":2}"#,
        "[3,-1;-5,2]",
        "[3,1;-5,2]",
    );
    assert_eq!(result.exit, 0);
    assert_eq!(result.doc()["verdict"], "wrong");
    assert_eq!(result.doc()["reason"], Value::Null);
}

#[test]
fn grade_accepts_a_key_that_starts_with_a_minus() {
    let result = grade(r#"{"kind":"exact"}"#, "-3", "-3");
    assert_eq!(result.doc()["verdict"], "correct");
}

#[test]
fn pretty_prints_the_document_on_more_than_one_line() {
    let result = run(&[
        "grade",
        "--pretty",
        "--contract",
        r#"{"kind":"exact"}"#,
        "--expected",
        "1",
        "--learner",
        "1",
    ]);
    assert_eq!(result.exit, 0);
    assert!(result.stdout.lines().count() > 1);
    assert_eq!(result.doc()["verdict"], "correct");
}

// ---- exit 2 ----

#[test]
fn contract_that_does_not_deserialize_is_exit_2() {
    let text = assert_error(&grade(r#"{"kind":"nonsense"}"#, "1", "1"), 2);
    assert!(text.contains("does not deserialize"), "{text}");
    assert_error(&mutants(r#"{"kind":"nonsense"}"#, "1"), 2);
    assert_error(&grade(r#"{"kind":"coordinates","arity":9}"#, "1", "1"), 2);
}

#[test]
fn contract_that_is_not_json_is_exit_2() {
    let text = assert_error(&grade("{kind: exact}", "1", "1"), 2);
    assert!(text.contains("not JSON"), "{text}");
}

#[test]
fn usage_errors_are_exit_2() {
    assert_error(&run(&[]), 2);
    assert_error(&run(&["frobnicate"]), 2);
    assert_error(
        &run(&[
            "grade",
            "--contract",
            r#"{"kind":"exact"}"#,
            "--expected",
            "1",
        ]),
        2,
    );
    assert_error(&run(&["grade", "--expected", "1", "--learner", "1"]), 2);
    assert_error(
        &run(&[
            "grade",
            "--contract",
            r#"{"kind":"exact"}"#,
            "--learner",
            "1",
        ]),
        2,
    );
    assert_error(&run(&["grade", "--colour", "red"]), 2);
    assert_error(&run(&["grade", "--expected"]), 2);
    assert_error(
        &run(&[
            "mutants",
            "--contract",
            r#"{"kind":"exact"}"#,
            "--expected",
            "1",
            "--learner",
            "1",
        ]),
        2,
    );
}

#[test]
fn batch_file_errors_are_exit_2() {
    let text = assert_error(&run(&["grade", "--batch", "/nonexistent/b5a.jsonl"]), 2);
    assert!(text.contains("cannot read"), "{text}");
    assert_error(&run(&["grade", "--batch", "x.jsonl", "--expected", "1"]), 2);
}

#[test]
fn empty_batch_file_is_exit_2() {
    let path = batch_file("b5a_empty.jsonl", &[]);
    std::fs::write(&path, "").unwrap();
    for subcommand in ["grade", "mutants"] {
        let text = assert_error(&run(&[subcommand, "--batch", &path]), 2);
        assert!(text.contains("has no line"), "{text}");
    }
}

#[test]
fn truncated_batch_file_is_exit_2_with_an_error_object_for_the_cut_line() {
    let path = batch_file("b5a_truncated.jsonl", &[]);
    let whole = json!({"id": "a", "contract": {"kind": "exact"}, "expected": "2", "learner": "2"});
    std::fs::write(&path, format!("{whole}\n{{\"id\": \"b\", \"contr")).unwrap();
    let result = run(&["grade", "--batch", &path]);
    assert_eq!(result.exit, 2);
    let docs = result.lines();
    assert_eq!(docs[0]["verdict"], "correct");
    assert_eq!(docs[1]["schema"], "cadus.error.v1");
}

#[test]
fn repeated_option_is_exit_2() {
    let result = run(&[
        "grade",
        "--contract",
        r#"{"kind":"exact"}"#,
        "--expected",
        "3",
        "--learner",
        "3",
        "--learner",
        "4",
    ]);
    let text = assert_error(&result, 2);
    assert!(text.contains("two times"), "{text}");
}

#[cfg(unix)]
#[test]
fn argument_that_is_not_utf8_is_exit_2_and_not_a_panic() {
    use std::os::unix::ffi::OsStrExt;
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_content_check"))
        .args(["grade", "--contract", r#"{"kind":"exact"}"#, "--expected"])
        .arg(std::ffi::OsStr::from_bytes(b"\xff"))
        .args(["--learner", "1"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let doc: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(doc["schema"], "cadus.error.v1");
    assert_eq!(doc["exit"], 2);
    assert!(doc["error"].as_str().unwrap().contains("UTF-8"));
}

// ---- exit 4 ----

// FLOW: lane B5b replaces the stub of `late.rs`; this expected value then changes.
#[test]
fn late_subcommands_are_exit_4_in_this_build() {
    let text = assert_error(&run(&["report", "--all", "--base", "curriculum"]), 4);
    assert!(text.contains("`report`"), "{text}");
    for subcommand in ["row", "diff", "dump-kp", "selftest"] {
        assert_error(&run(&[subcommand]), 4);
    }
}

// ---- batch ----

#[test]
fn grade_batch_keeps_the_order_and_the_ids() {
    let lines = [
        json!({"id": "a", "contract": {"kind": "exact"}, "expected": "1/2", "learner": "0.5"}),
        json!({"id": "b", "contract": {"kind": "exact"}, "expected": "3", "learner": "4"}),
        json!({"id": "c", "contract": {"kind": "exact"}, "expected": "3", "learner": "three"}),
    ]
    .map(|line| line.to_string());
    let result = run(&[
        "grade",
        "--batch",
        &batch_file("b5a_grade_ok.jsonl", &lines),
    ]);
    assert_eq!(result.exit, 0);
    let docs = result.lines();
    let seen: Vec<_> = docs
        .iter()
        .map(|doc| (&doc["id"], &doc["verdict"]))
        .collect();
    assert_eq!(
        seen,
        [
            (&json!("a"), &json!("correct")),
            (&json!("b"), &json!("wrong")),
            (&json!("c"), &json!("ungraded"))
        ]
    );
    assert!(docs.iter().all(|doc| doc["schema"] == "cadus.grade.v1"));
}

#[test]
fn grade_batch_with_one_bad_line_gives_an_error_object_for_that_line_and_exit_2() {
    let lines = [
        json!({"id": "a", "contract": {"kind": "exact"}, "expected": "2", "learner": "2"})
            .to_string(),
        json!({"id": "b", "contract": {"kind": "nonsense"}, "expected": "2", "learner": "2"})
            .to_string(),
        "not json".to_owned(),
        json!({"id": "d", "contract": {"kind": "exact"}, "learner": "2"}).to_string(),
        json!({"id": "e", "contract": {"kind": "exact"}, "expected": "2"}).to_string(),
        json!({"id": "f", "contract": {"kind": "exact"}, "expected": "2", "learner": "5"})
            .to_string(),
    ];
    let result = run(&[
        "grade",
        "--batch",
        &batch_file("b5a_grade_bad.jsonl", &lines),
    ]);
    assert_eq!(result.exit, 2);
    let docs = result.lines();
    assert_eq!(docs.len(), 6);
    assert_eq!(docs[0]["verdict"], "correct");
    for (at, id) in [
        (1, json!("b")),
        (2, Value::Null),
        (3, json!("d")),
        (4, json!("e")),
    ] {
        assert_eq!(docs[at]["schema"], "cadus.error.v1", "{}", docs[at]);
        assert_eq!(docs[at]["exit"], 2);
        assert_eq!(docs[at]["id"], id);
    }
    assert_eq!(docs[5]["id"], "f");
    assert_eq!(docs[5]["verdict"], "wrong");
}

#[test]
fn mutants_batch_gives_one_line_for_each_input_line() {
    let lines = [
        json!({"id": 1, "contract": {"kind": "exact"}, "expected": "10"}),
        json!({"id": 2, "contract": {"kind": "set"}, "expected": "{2, 3}"}),
        json!({"contract": {"kind": "none"}, "expected": "See the solution."}),
    ]
    .map(|line| line.to_string());
    let result = run(&[
        "mutants",
        "--batch",
        &batch_file("b5a_mutants.jsonl", &lines),
    ]);
    assert_eq!(result.exit, 0);
    let docs = result.lines();
    assert_eq!(docs.len(), 3);
    assert_eq!(
        (&docs[0]["id"], &docs[0]["pass"]),
        (&json!(1), &json!(true))
    );
    assert_eq!(
        (&docs[1]["id"], &docs[1]["pass"]),
        (&json!(2), &json!(true))
    );
    assert_eq!(
        (&docs[2]["id"], &docs[2]["pass"]),
        (&Value::Null, &json!(false))
    );
}

// ---- mutants ----

#[test]
fn mutants_label_gives_each_other_option() {
    let contract = r#"{"kind":"label","options":[["Step 1"],["Step 2"],["Step 3"],["Step 4"]]}"#;
    let result = mutants(contract, "Step 3");
    assert_eq!(result.exit, 0);
    let doc = result.doc();
    assert_eq!(doc["schema"], "cadus.mutants.v1");
    assert_eq!(doc["key_verdict"], "correct");
    assert_eq!(doc["key_reason"], Value::Null);
    assert_eq!(mutant_learners(&doc), ["Step 1", "Step 2", "Step 4"]);
    assert!(
        doc["mutants"]
            .as_array()
            .unwrap()
            .iter()
            .all(|m| m["rule"] == "label-other")
    );
    assert_eq!(doc["pass"], true);
}

#[test]
fn mutants_set_removes_one_member() {
    let doc = mutants(r#"{"kind":"set"}"#, "{2, 3}").doc();
    assert_eq!(mutant_learners(&doc), ["{3}"]);
    assert_eq!(doc["mutants"][0]["rule"], "member-removed");
    assert_eq!(doc["pass"], true);
}

#[test]
fn mutants_list_removes_one_member_and_a_single_member_uses_plus_one() {
    let contract = r#"{"kind":"list","ordered":true,"member":{"kind":"exact"}}"#;
    let doc = mutants(contract, "1, 2, 3").doc();
    assert_eq!(mutant_learners(&doc), ["2, 3"]);
    assert_eq!(doc["pass"], true);
    let doc = mutants(r#"{"kind":"set"}"#, "{4}").doc();
    assert_eq!(doc["mutants"][0]["rule"], "plus-one");
    assert_eq!(doc["pass"], true);
}

#[test]
fn mutants_coordinates_adds_one_to_the_first_component() {
    let doc = mutants(r#"{"kind":"coordinates","arity":2}"#, "(46, 54)").doc();
    assert_eq!(mutant_learners(&doc), ["(47, 54)"]);
    assert_eq!(doc["mutants"][0]["rule"], "plus-one");
    assert_eq!(doc["pass"], true);
}

#[test]
fn mutants_plus_one_kinds_pass() {
    for (contract, key) in [
        (r#"{"kind":"exact"}"#, "1/2"),
        (r#"{"kind":"approx","decimals":2}"#, "3.14"),
        (r#"{"kind":"matrix","rows":2,"cols":2}"#, "[3,-1;-5,2]"),
        (r#"{"kind":"inequality_union"}"#, "x < 2"),
    ] {
        let doc = mutants(contract, key).doc();
        assert_eq!(doc["mutants"][0]["rule"], "plus-one", "{contract}");
        assert_eq!(mutant_learners(&doc).len(), 1);
        assert_eq!(doc["pass"], true, "{doc}");
    }
}

#[test]
fn mutants_multipart_gives_one_mutant_for_each_part() {
    let contract = r#"{"kind":"multipart","parts":[{"name":"verdict","contract":{"kind":"label","options":[["converges"],["diverges"]]}},{"name":"L","contract":{"kind":"exact"}}]}"#;
    let doc = mutants(contract, "verdict = converges; L = 1/2").doc();
    let learners = mutant_learners(&doc);
    assert_eq!(learners.len(), 2);
    assert_eq!(learners[0], "verdict = diverges; L = 1/2");
    let changed = learners[1]
        .strip_prefix("verdict = converges; L = ")
        .unwrap();
    assert_ne!(changed, "1/2");
    assert_eq!(doc["mutants"][0]["rule"], "part:verdict:label-other");
    assert_eq!(doc["mutants"][1]["rule"], "part:L:plus-one");
    assert_eq!(doc["pass"], true);
}

#[test]
fn mutants_fail_when_the_key_is_not_correct_or_no_mutant_exists() {
    // The key is not in the vocabulary: the key grades ungraded.
    let doc = mutants(r#"{"kind":"label","options":[["Yes"],["No"]]}"#, "Maybe").doc();
    assert_eq!(doc["key_verdict"], "ungraded");
    assert!(doc["key_reason"].is_string());
    assert_eq!(doc["pass"], false);
    // No numeric leaf under a kind with no whole-key rule: no mutant, and a cause (D40).
    let doc = mutants(r#"{"kind":"polynomial_relation"}"#, "y = x").doc();
    assert_eq!(doc["key_verdict"], "correct");
    assert_eq!(doc["mutants"], json!([]));
    assert_eq!(doc["cause"], "no-numeric-leaf");
    assert_eq!(doc["pass"], false);
    // One option: no other option, and a cause (review case m01).
    let doc = mutants(
        r#"{"kind":"label","options":[["converges","conv"]]}"#,
        "converges",
    )
    .doc();
    assert_eq!(doc["mutants"], json!([]));
    assert_eq!(doc["cause"], "no-other-option");
    assert_eq!(doc["pass"], false);
    let doc = mutants(r#"{"kind":"none"}"#, "See the solution.").doc();
    assert_eq!(doc["cause"], "no-rule");
    assert_eq!(doc["pass"], false);
    // Each candidate grades correct: the entry shows `correct`, and the item fails.
    let doc = mutants(r#"{"kind":"set"}"#, "{7, 7}").doc();
    assert_eq!(doc["mutants"][0]["verdict"], "correct");
    assert_eq!(doc["pass"], false);
}
