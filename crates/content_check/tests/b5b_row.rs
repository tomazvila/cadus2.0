//! Lane B5b: `row` and `selftest` with the real binary, and the exit codes of
//! the five subcommands.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "b5b_support.rs"]
mod support;

use serde_json::{Value, json};
use support::{
    Run, assert_findings, assert_keys, curriculum, error_text, fixture, flow, pairs, run, scratch,
};

const ROWCHECK: [&str; 9] = [
    "schema", "kp", "pass", "V_before", "V_after", "U_before", "U_after", "floor", "findings",
];

// ---- the golden rows of the freeze pack ----

fn golden(name: &str) -> Run {
    let row = flow(&format!("spec/golden/{name}.row.json"));
    let packet = flow(&format!("spec/golden/{name}.packet.json"));
    run(&[
        "row",
        "--row",
        &row,
        "--packet",
        &packet,
        "--base",
        &curriculum(),
    ])
}

fn assert_golden_pass(name: &str, v_before: u64, u: (u64, u64)) {
    let result = golden(name);
    assert_eq!(result.exit, 0, "{}", result.doc);
    assert_keys(&result.doc, &ROWCHECK);
    assert_eq!(result.doc["schema"], "cadus.rowcheck.v1");
    assert_eq!(
        (&result.doc["pass"], &result.doc["findings"]),
        (&json!(true), &json!([]))
    );
    assert_eq!(
        (&result.doc["V_before"], &result.doc["V_after"]),
        (&json!(v_before), &json!(6))
    );
    assert_eq!(
        (&result.doc["U_before"], &result.doc["U_after"]),
        (&json!(u.0), &json!(u.1))
    );
    assert_eq!(result.doc["floor"], 6);
}

#[test]
fn golden_row_linalg_inverse_passes() {
    assert_golden_pass("linalg-inverse", 2, (0, 0));
}

#[test]
fn golden_row_stats_ci_passes() {
    assert_golden_pass("stats-ci", 2, (0, 0));
}

/// The proof row has no `function` item. Its one P3 item is the legal U = 1.
#[test]
fn golden_row_proof_induction_passes() {
    assert_golden_pass("proof-induction", 0, (2, 1));
}

// FLOW: enable after B4b
#[test]
#[ignore = "the `function` contract kind comes with lane B4b"]
fn golden_row_calc_chain_rule_passes_after_b4b() {
    assert_golden_pass("calc-chain-rule", 3, (0, 0));
}

// ---- rows against the fixture tree ----

const WORDS: [&str; 4] = ["iota", "kappa", "lambda", "sigma"];

fn new_item(n: usize, answer: &str) -> Value {
    let word = WORDS[n];
    json!({"id": format!("n{}", n + 1), "replaces": null, "rule": "R1",
        "problem": format!("Task {word}: compute the {word}-sum for the set row."),
        "answer": answer, "answer_contract": {"kind": "exact"},
        "solution_sketch": format!("Add the parts of the {word} set in the order of the list. \
The total of the parts is {answer}; thus the value is {answer}.")})
}

/// The hashes of the exemplars of a fixture KP.
fn hashes(kp: &str) -> Vec<String> {
    let doc = run(&["dump-kp", "--kp", kp, "--base", &fixture("tree")]).doc;
    let exemplars = doc["exemplars"].as_array().unwrap();
    exemplars
        .iter()
        .map(|one| one["hash"].as_str().unwrap().to_owned())
        .collect()
}

fn keeps(kp: &str) -> Vec<Value> {
    hashes(kp)
        .iter()
        .map(|hash| json!({"keep": hash}))
        .collect()
}

fn check_row(name: &str, kp: &str, items: Vec<Value>, more: &[&str]) -> Run {
    let row = json!({"schema": "cadus.row.v1", "kp": kp, "status": "done",
        "spec_version": "v1", "template_version": "author-1", "items": items});
    let path = scratch(&format!("b5b_row_{name}.row.json"), &row.to_string());
    let mut args = vec!["row", "--row", &path, "--base"];
    let base = fixture("tree");
    args.push(&base);
    args.extend(more);
    run(&args)
}

const THREE: &str = "precalculus/fx/kp2";

#[test]
fn a_row_that_lifts_the_kp_to_the_floor_passes() {
    let mut items = keeps(THREE);
    items.extend((0..3).map(|n| new_item(n, &format!("{}", 50 + n))));
    let result = check_row("pass", THREE, items, &[]);
    assert_eq!(result.exit, 0, "{}", result.doc);
    assert_eq!(
        (&result.doc["V_before"], &result.doc["V_after"]),
        (&json!(3), &json!(6))
    );
    assert_eq!(result.doc["pass"], true);
}

#[test]
fn a_row_below_the_floor_has_the_i2_finding() {
    let mut items = keeps(THREE);
    items.push(new_item(0, "50"));
    let hard = check_row("hard", THREE, items.clone(), &[]);
    assert_eq!(hard.exit, 1);
    assert_eq!(hard.doc["pass"], false);
    assert_findings(&hard.doc["findings"]);
    assert_eq!(
        pairs(&hard.doc["findings"]),
        [(THREE.to_owned(), "I2".to_owned())]
    );
    assert_eq!(hard.doc["findings"][0]["code"], "invariant:I2");
    items.push(new_item(1, "51"));
    let goal = check_row("goal", THREE, items, &[]);
    let detail = goal.doc["findings"][0]["detail"].as_str().unwrap();
    assert!(detail.starts_with("below-goal"), "{detail}");
}

#[test]
fn two_equal_problems_are_the_finding_duplicate() {
    let mut items = keeps(THREE);
    items.extend([
        new_item(0, "50"),
        new_item(1, "51"),
        new_item(2, "52"),
        new_item(2, "52"),
    ]);
    let result = check_row("equal", THREE, items, &[]);
    assert_eq!(result.exit, 1);
    let finding = &result.doc["findings"][0];
    assert_eq!(
        (&finding["code"], &finding["invariant"]),
        (&json!("duplicate"), &json!("I5"))
    );
    assert_eq!(
        (&finding["ck"], &finding["item"]),
        (&json!("CK7"), &json!("n3"))
    );
}

#[test]
fn an_unknown_keep_hash_and_a_dropped_keep_are_base_coverage() {
    let mut items = keeps(THREE);
    items[0] = json!({"keep": "ffffffffffff"});
    items.extend((0..4).map(|n| new_item(n, &format!("{}", 50 + n))));
    let result = check_row("coverage", THREE, items, &[]);
    assert_eq!(result.exit, 1);
    let findings = result.doc["findings"].as_array().unwrap();
    let codes: Vec<&Value> = findings.iter().map(|finding| &finding["code"]).collect();
    assert_eq!(codes, ["base-coverage", "base-coverage"]);
    assert_eq!(findings[0]["hash"], "ffffffffffff");
    assert_eq!(findings[1]["hash"], hashes(THREE)[0].as_str());
    assert_eq!(findings[0]["ck"], "CK2");
}

#[test]
fn a_problem_of_a_different_kp_of_the_course_is_a_duplicate() {
    let mut items = keeps(THREE);
    items.extend((0..3).map(|n| new_item(n, &format!("{}", 50 + n))));
    // The first exemplar of kp1 of the same topic.
    items[3]["problem"] = json!("Task alpha: compute the alpha-sum for the set one.");
    let result = check_row("course", THREE, items, &[]);
    let detail = result.doc["findings"][0]["detail"].as_str().unwrap();
    assert!(
        detail.contains("different KP of the course"),
        "{}",
        result.doc
    );
}

#[test]
fn the_teach_page_problem_of_the_packet_is_a_duplicate() {
    let mut items = keeps(THREE);
    items.extend((0..3).map(|n| new_item(n, &format!("{}", 50 + n))));
    let problem = items[4]["problem"].clone();
    let packet = json!({"teach_page": {"worked_example": {"problem": problem}}});
    let path = scratch("b5b_row_teach.packet.json", &packet.to_string());
    let result = check_row("teach", THREE, items.clone(), &["--packet", &path]);
    assert_eq!(result.exit, 1);
    let detail = result.doc["findings"][0]["detail"].as_str().unwrap();
    assert!(detail.contains("teach page"), "{detail}");
    // A packet with no teach page gives no teach-page rule.
    let empty = scratch("b5b_row_no_teach.packet.json", "{}");
    assert_eq!(
        check_row("no_teach", THREE, items, &["--packet", &empty]).exit,
        0
    );
}

#[test]
fn new_items_get_the_rules_i12_i13_i14() {
    let mut items = keeps(THREE);
    items.extend((0..3).map(|n| new_item(n, &format!("{}", 50 + n))));
    items[3]["solution_sketch"] = json!("Use the formula.");
    items[4].as_object_mut().unwrap().remove("answer_contract");
    items[5]["answer"] = json!("$52$");
    let result = check_row("new_rules", THREE, items, &[]);
    let rules: Vec<String> = pairs(&result.doc["findings"])
        .into_iter()
        .map(|pair| pair.1)
        .collect();
    assert_eq!(rules, ["I13", "I14", "I12"], "{}", result.doc);
}

/// I10 runs on the file state after the row. The file `01-labels-bad.yaml`
/// breaches it; the row is the cause only if it adds a label item.
#[test]
fn the_file_rule_i10_reads_the_file_state_after_the_row() {
    let kp = "precalculus/longest/kp2";
    let quiet = check_row("i10_quiet", kp, keeps(kp), &[]);
    assert_eq!(quiet.exit, 0, "{}", quiet.doc);
    let mut items = keeps(kp);
    items.truncate(7);
    items.push(json!({"id": "n1", "replaces": null, "rule": "R15",
        "problem": "Task sigma: name the class of the sigma-object of the set row.",
        "answer": "helicoid", "answer_contract": {"kind": "label",
            "options": [["helicoid"], ["cone"], ["tube"], ["disk"]]},
        "solution_sketch": "Read the definition of each class for the sigma-object. Only one \
class fits each property; thus the class is the key."}));
    let result = check_row("i10", kp, items, &[]);
    let found = pairs(&result.doc["findings"]);
    assert!(
        found.contains(&(kp.to_owned(), "I10".to_owned())),
        "{}",
        result.doc
    );
}

// ---- exit 2 ----

#[test]
fn bad_input_is_exit_2_with_the_error_document() {
    let base = fixture("tree");
    let none = scratch("b5b_row_empty.json", "");
    let cut = scratch(
        "b5b_row_cut.json",
        r#"{"kp": "precalculus/fx/kp2", "items": [{"ke"#,
    );
    for args in [
        vec!["row", "--row", "/no/such/row.json", "--base", &base],
        // An empty file and a truncated file (pack v9).
        vec!["row", "--row", &none, "--base", &base],
        vec!["row", "--row", &cut, "--base", &base],
        vec!["row", "--base", &base],
        vec!["row", "--row", &none, "--base", &base, "--other", "1"],
        vec!["dump-kp", "--kp", "precalculus/fx/kp99", "--base", &base],
        vec!["dump-kp", "--kp", THREE, "--base", "/no/such/tree"],
        vec!["dump-kp", "--kp", THREE, "--base"],
        vec!["selftest", "--tree", &base, "--expected", &none],
        vec!["report", "--all", "--base", &base, "--db", "not a dsn"],
    ] {
        error_text(&run(&args), 2);
    }
}

#[test]
fn a_row_with_bad_fields_is_exit_2() {
    let no_kp = scratch("b5b_row_no_kp.json", r#"{"items": []}"#);
    let blocked = scratch(
        "b5b_row_blocked.json",
        r#"{"kp": "precalculus/fx/kp2", "status": "blocked"}"#,
    );
    let base = fixture("tree");
    error_text(&run(&["row", "--row", &no_kp, "--base", &base]), 2);
    let text = error_text(&run(&["row", "--row", &blocked, "--base", &base]), 2);
    assert!(text.contains("blocked"), "{text}");
    for (field, value) in [
        ("problem", Value::Null),
        ("answer", Value::Null),
        ("answer_contract", json!({"kind": "no-such-kind"})),
    ] {
        let mut item = new_item(0, "50");
        item[field] = value;
        let text = error_text(
            &check_row(&format!("bad_{field}"), THREE, vec![item], &[]),
            2,
        );
        assert!(text.starts_with("the row item `n1`"), "{text}");
    }
    let mut no_id = new_item(0, "50");
    no_id.as_object_mut().unwrap().remove("id");
    no_id["problem"] = Value::Null;
    let text = error_text(&check_row("bad_no_id", THREE, vec![no_id], &[]), 2);
    assert!(text.starts_with("the row item `-`"), "{text}");
}

// ---- exit 3 ----

#[test]
fn a_tree_that_the_loader_refuses_is_exit_3() {
    let refused = fixture("tree-refused");
    let text = error_text(&run(&["report", "--all", "--base", &refused]), 3);
    assert!(text.contains("distractor_note"), "{text}");
    error_text(
        &run(&["dump-kp", "--kp", "precalculus/fx/kp1", "--base", &refused]),
        3,
    );
    let seeded = flow("selftest/seeded-tree-unknown-field");
    error_text(&run(&["report", "--all", "--base", &seeded]), 3);
}

// ---- selftest ----

const SELFTEST: [&str; 6] = ["schema", "pass", "expected", "flagged", "missing", "extra"];

#[test]
fn selftest_finds_each_defect_of_the_fixture_trees() {
    let result = run(&[
        "selftest",
        "--tree",
        &fixture("tree"),
        "--expected",
        &fixture("expected-fixture.json"),
    ]);
    assert_eq!(result.exit, 0, "{}", result.doc);
    assert_keys(&result.doc, &SELFTEST);
    assert_eq!(
        result.doc,
        json!({"schema": "cadus.selftest.v1", "pass": true, "expected": 19, "flagged": 19,
            "missing": [], "extra": []})
    );
}

/// The seeded trees of lane B5a, with the `tree` field of pack v9.
#[test]
fn selftest_finds_each_defect_of_the_seeded_trees() {
    let tree = flow("selftest/seeded-tree");
    let result = run(&[
        "selftest",
        "--tree",
        &tree,
        "--expected",
        &fixture("expected-seeded-v9.json"),
    ]);
    assert_eq!(result.exit, 0, "{}", result.doc);
    assert_eq!(
        (&result.doc["expected"], &result.doc["flagged"]),
        (&json!(10), &json!(10))
    );
    assert_eq!(result.doc["pass"], true);
}

#[test]
fn selftest_reports_a_missing_defect_and_an_extra_finding() {
    // Defect 1 names a clean KP. The I2 finding of kp2 is not in the list.
    let expected = json!({"defects": [
        {"n": 1, "where": "precalculus/fx/kp1", "code": "duplicate"},
        {"n": 2, "where": "precalculus/fx/kp3", "code": "invariant:I2"}]});
    let path = scratch("b5b_selftest_missing.json", &expected.to_string());
    let result = run(&["selftest", "--tree", &fixture("tree"), "--expected", &path]);
    assert_eq!(result.exit, 1);
    assert_eq!(
        (&result.doc["pass"], &result.doc["missing"]),
        (&json!(false), &json!([1]))
    );
    assert_eq!(
        (&result.doc["expected"], &result.doc["flagged"]),
        (&json!(2), &json!(1))
    );
    assert_findings(&result.doc["extra"]);
    assert_eq!(result.doc["extra"].as_array().unwrap().len(), 17);
}

#[test]
fn selftest_reports_a_refused_tree_that_the_list_does_not_name() {
    let path = scratch("b5b_selftest_refused.json", r#"{"defects": []}"#);
    let result = run(&[
        "selftest",
        "--tree",
        &fixture("tree-refused"),
        "--expected",
        &path,
    ]);
    assert_eq!(result.exit, 1);
    let extra = &result.doc["extra"][0];
    assert_eq!(
        (&extra["code"], &extra["kp"]),
        (&json!("invariant:I1"), &json!(""))
    );
}

#[test]
fn selftest_refuses_a_bad_expected_file_and_an_absent_tree() {
    let tree = fixture("tree");
    let no_list = scratch("b5b_selftest_no_list.json", "{}");
    let no_code = scratch(
        "b5b_selftest_no_code.json",
        r#"{"defects": [{"n": 1, "where": "a/b/kp1"}]}"#,
    );
    let absent = scratch(
        "b5b_selftest_absent.json",
        r#"{"defects": [{"n": 1, "tree": "no-such-tree", "where": "a/b/kp1", "code": "sketch"}]}"#,
    );
    for expected in [&no_list, &no_code, &absent] {
        error_text(
            &run(&["selftest", "--tree", &tree, "--expected", expected]),
            2,
        );
    }
}
