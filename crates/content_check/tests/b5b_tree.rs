//! Lane B5b: `dump-kp`, `report` and `diff` with the real binary, on the
//! shipped curriculum of this worktree and on scratch trees.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "b5b_support.rs"]
mod support;

use std::time::Instant;

use serde_json::{Value, json};
use support::{assert_findings, assert_keys, curriculum, error_text, pairs, repo, run, scratch};

const CHAIN_RULE: &str = "calculus-1/chain-rule/kp1";
const BASE_REF: &str = "d2ca1421";

fn dump(kp: &str) -> Value {
    let result = run(&["dump-kp", "--kp", kp, "--base", &curriculum()]);
    assert_eq!(result.exit, 0, "{}", result.doc);
    result.doc
}

fn column<'a>(doc: &'a Value, list: &str, key: &str) -> Vec<&'a Value> {
    doc[list]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| &entry[key])
        .collect()
}

// ---- dump-kp ----

#[test]
fn dump_kp_gives_the_chain_rule_example_of_the_brief() {
    let doc = dump(CHAIN_RULE);
    assert_keys(
        &doc,
        &[
            "schema",
            "kp",
            "file",
            "topic",
            "kp_block",
            "V",
            "U",
            "exemplars",
        ],
    );
    assert_keys(
        &doc["topic"],
        &[
            "id",
            "name",
            "answer_kind",
            "prerequisites",
            "diagnostic_problem",
        ],
    );
    assert_keys(
        &doc["kp_block"],
        &["id", "name", "key_prerequisites", "constraints"],
    );
    for exemplar in doc["exemplars"].as_array().unwrap() {
        assert_keys(
            exemplar,
            &[
                "index",
                "hash",
                "problem",
                "answer",
                "answer_contract",
                "solution_sketch",
                "verdict",
                "status",
                "reason",
            ],
        );
    }
    assert_eq!(doc["schema"], "cadus.kp.v1");
    assert_eq!(
        doc["file"],
        "curriculum/calculus-1/02-differentiation-rules.yaml"
    );
    assert_eq!((&doc["V"], &doc["U"]), (&json!(3), &json!(0)));
    assert_eq!(
        column(&doc, "exemplars", "hash"),
        ["86d2057cb791", "5ad3b567ab04", "03d456790232"]
    );
    assert_eq!(column(&doc, "exemplars", "verdict"), [true, true, true]);
    assert_eq!(
        column(&doc, "exemplars", "status"),
        ["keep", "keep", "keep"]
    );
    assert_eq!(doc["topic"]["answer_kind"], "expression");
}

#[test]
fn dump_kp_gives_no_verdict_for_the_proof_kp_of_the_brief() {
    let doc = dump("proofs/induction-divisibility-proofs/kp1");
    assert_eq!((&doc["V"], &doc["U"]), (&json!(0), &json!(2)));
    assert_eq!(column(&doc, "exemplars", "verdict"), [false, false]);
    assert_eq!(
        column(&doc, "exemplars", "status"),
        ["unmarked", "unmarked"]
    );
}

/// Pack v2, D25: the file is the one whose header has the unit and the course
/// of the KP. The file name is not the unit id in these two units.
#[test]
fn the_file_field_comes_from_the_unit_header_not_from_a_name_pattern() {
    // `unit: the-derivative`; no file has the name `*-the-derivative.yaml`.
    let doc = dump("calculus-1/average-instantaneous-rate/kp1");
    assert_eq!(doc["file"], "curriculum/calculus-1/01-derivative.yaml");
    // `unit: series`; two files have a name `*-series.yaml`.
    let doc = dump("calculus-2/limits-of-sequences/kp1");
    assert_eq!(doc["file"], "curriculum/calculus-2/02-series.yaml");
}

// ---- report ----

const REPORT: [&str; 6] = ["schema", "base", "db_read", "result", "courses", "kps"];
const REPORT_COURSE: [&str; 13] = [
    "course",
    "result",
    "floor",
    "kps",
    "kps_at_goal",
    "kps_v4_to_floor",
    "kps_below_4",
    "exemplars",
    "unmarked",
    "unmarked_p3",
    "label_items",
    "kps_no_teach_page",
    "breaches",
];
const REPORT_KP: [&str; 12] = [
    "kp", "file", "V", "U", "T", "teach", "floor", "goal", "serves", "proof_kp", "existing",
    "findings",
];

#[test]
fn report_of_one_course_fails_today_and_lists_each_kp_below_the_floor() {
    let result = run(&["report", "--course", "precalculus", "--base", &curriculum()]);
    assert_eq!(result.exit, 1);
    let doc = result.doc;
    assert_keys(&doc, &REPORT);
    assert_eq!(
        (&doc["result"], &doc["db_read"]),
        (&json!("FAIL"), &json!(false))
    );
    let course = &doc["courses"][0];
    assert_keys(course, &REPORT_COURSE);
    assert_eq!(doc["courses"].as_array().unwrap().len(), 1);
    assert_eq!(
        (&course["course"], &course["floor"]),
        (&json!("precalculus"), &json!(6))
    );
    assert_eq!(course["kps_no_teach_page"], Value::Null);
    let kps = doc["kps"].as_array().unwrap();
    assert_eq!(course["kps"], kps.len());
    for kp in kps {
        assert_keys(kp, &REPORT_KP);
        assert_findings(&kp["findings"]);
        assert_eq!((&kp["T"], &kp["teach"]), (&Value::Null, &Value::Null));
        for existing in kp["existing"].as_array().unwrap() {
            assert_keys(existing, &["hash", "status", "verdict", "kind"]);
        }
        // Each KP is compared with its floor: below the floor is a finding.
        let v = kp["V"].as_u64().unwrap();
        let has_i2 = pairs(&kp["findings"]).iter().any(|(_, rule)| rule == "I2");
        assert_eq!(has_i2, v < 6, "{}", kp["kp"]);
        assert_eq!(kp["goal"], v >= 6);
        assert_eq!(kp["serves"], v >= 4);
    }
    let below: u64 = ["kps_v4_to_floor", "kps_below_4"]
        .iter()
        .map(|key| course[key].as_u64().unwrap())
        .sum();
    assert!(below > 0);
    assert_eq!(course["breaches"]["I2"], below);
}

#[test]
fn report_of_the_full_tree_has_3138_kps_and_runs_in_less_than_60_seconds() {
    let start = Instant::now();
    let result = run(&["report", "--all", "--base", &curriculum()]);
    assert!(start.elapsed().as_secs() < 60, "{:?}", start.elapsed());
    assert_eq!(result.exit, 1);
    assert_eq!(result.doc["kps"].as_array().unwrap().len(), 3138);
    assert_eq!(result.doc["courses"].as_array().unwrap().len(), 13);
    let count: u64 = column(&result.doc, "courses", "kps")
        .iter()
        .map(|kps| kps.as_u64().unwrap())
        .sum();
    assert_eq!(count, 3138);
    // Pack v2, D25: the two units whose file name is not the unit id.
    let files = column(&result.doc, "kps", "file");
    for file in [
        "curriculum/calculus-1/01-derivative.yaml",
        "curriculum/calculus-2/02-series.yaml",
    ] {
        assert!(files.iter().any(|have| *have == file), "{file}");
    }
    // "No verdict" is never "pass": an exemplar with no verdict counts zero.
    let proof_kp = "proofs/induction-divisibility-proofs/kp1";
    let kps = result.doc["kps"].as_array().unwrap();
    let kp = kps.iter().find(|kp| kp["kp"] == proof_kp).unwrap();
    assert_eq!((&kp["V"], &kp["U"]), (&json!(0), &json!(2)));
    assert_eq!((&kp["goal"], &kp["serves"]), (&json!(false), &json!(false)));
    assert_eq!(column(kp, "existing", "verdict"), [false, false]);
    let rules: Vec<String> = pairs(&kp["findings"])
        .into_iter()
        .map(|pair| pair.1)
        .collect();
    assert_eq!(rules, ["I2", "I4", "I4"]);
}

#[test]
fn report_refuses_bad_course_options() {
    let base = curriculum();
    let text = error_text(&run(&["report", "--course", "no-such", "--base", &base]), 2);
    assert!(text.contains("`no-such`"), "{text}");
    error_text(&run(&["report", "--base", &base]), 2);
    error_text(
        &run(&[
            "report",
            "--all",
            "--course",
            "precalculus",
            "--base",
            &base,
        ]),
        2,
    );
}

// ---- scratch trees for `diff` and `report --since` ----

/// A tree with the one KP `calculus-1/chain-rule/kp1` and the given exemplars
/// (JSON objects with the fields of an exemplar; a `null` field is left out).
fn chain_rule_tree(name: &str, exemplars: &[Value]) -> String {
    let mut unit = String::from(
        "unit: differentiation-rules\ncourse: calculus-1\nmodule: \"M\"\ntopics:\n  - id: chain-rule\n    \
name: Chain Rule\n    core: true\n    difficulty: 0.5\n    drill: false\n    answer_kind: expression\n    \
expected_time_secs: 180\n    prerequisites: []\n    knowledge_points:\n      - id: kp1\n        \
name: The chain rule\n        key_prerequisites: []\n        exemplars:\n",
    );
    for exemplar in exemplars {
        let mut lead = "          - ";
        for field in ["problem", "answer", "answer_contract", "solution_sketch"] {
            if !exemplar[field].is_null() {
                unit += &format!("{lead}{field}: {}\n", exemplar[field]);
                lead = "            ";
            }
        }
    }
    let courses = "courses:\n  - id: calculus-1\n    name: Calculus 1\n    order: 1\n";
    let root = scratch(&format!("{name}/courses.yaml"), courses);
    scratch(&format!("{name}/calculus-1/02-rules.yaml"), &unit);
    root.trim_end_matches("/courses.yaml").to_owned()
}

/// The base exemplars of the chain-rule KP as tree entries.
fn base_exemplars() -> Vec<Value> {
    dump(CHAIN_RULE)["exemplars"].as_array().unwrap().clone()
}

fn exemplar(problem: &str, answer: &str, contract: Value) -> Value {
    json!({"problem": problem, "answer": answer, "answer_contract": contract})
}

fn diff(tree: Option<&str>, kp: &str) -> Value {
    let repo = repo();
    let mut args = vec!["diff", "--base", BASE_REF, "--kp", kp, "--repo", &repo];
    args.extend(tree.iter().flat_map(|tree| ["--tree", *tree]));
    let result = run(&args);
    assert_eq!(result.exit, 0, "{}", result.doc);
    result.doc
}

const DIFF: [&str; 14] = [
    "schema",
    "kp",
    "V_before",
    "V_after",
    "U_before",
    "U_after",
    "exemplars_before",
    "exemplars_after",
    "kept",
    "removed",
    "added",
    "changed_bytes",
    "i16",
    "findings",
];

#[test]
fn diff_of_an_unchanged_tree_has_equal_counts() {
    let doc = diff(None, CHAIN_RULE);
    assert_keys(&doc, &DIFF);
    assert_eq!(doc["schema"], "cadus.diff.v1");
    assert_eq!((&doc["V_before"], &doc["V_after"]), (&json!(3), &json!(3)));
    assert_eq!((&doc["U_before"], &doc["U_after"]), (&json!(0), &json!(0)));
    assert_eq!(
        (&doc["exemplars_before"], &doc["exemplars_after"]),
        (&json!(3), &json!(3))
    );
    assert_eq!(doc["kept"].as_array().unwrap().len(), 3);
    assert_eq!((&doc["added"], &doc["removed"]), (&json!([]), &json!([])));
    assert_eq!(
        (&doc["changed_bytes"], &doc["findings"]),
        (&json!([]), &json!([]))
    );
    assert_eq!(doc["i16"], true);
}

#[test]
fn diff_permits_new_verdict_exemplars() {
    let mut exemplars = base_exemplars();
    exemplars.push(exemplar(
        "Find the new value a.",
        "41",
        json!({"kind": "exact"}),
    ));
    // The first kept exemplar gets an explicit contract: its bytes change.
    exemplars[0]["answer_contract"] = json!({"kind": "exact"});
    let tree = chain_rule_tree("b5b_diff_more", &exemplars);
    let doc = diff(Some(&tree), CHAIN_RULE);
    assert_eq!((&doc["V_before"], &doc["V_after"]), (&json!(3), &json!(4)));
    assert_eq!(doc["added"].as_array().unwrap().len(), 1);
    assert_eq!(doc["changed_bytes"], json!(["86d2057cb791"]));
    assert_eq!((&doc["i16"], &doc["findings"]), (&json!(true), &json!([])));
}

#[test]
fn diff_reports_each_i16_breach() {
    let base = base_exemplars();
    let exemplars = vec![
        base[0].clone(),
        exemplar(
            "Explain the chain rule.",
            "See the solution.",
            json!({"kind": "none"}),
        ),
    ];
    let tree = chain_rule_tree("b5b_diff_fewer", &exemplars);
    let doc = diff(Some(&tree), CHAIN_RULE);
    assert_eq!((&doc["V_before"], &doc["V_after"]), (&json!(3), &json!(1)));
    assert_eq!((&doc["U_before"], &doc["U_after"]), (&json!(0), &json!(1)));
    assert_eq!(doc["removed"].as_array().unwrap().len(), 2);
    assert_eq!(doc["i16"], false);
    assert_findings(&doc["findings"]);
    let details: Vec<&str> = column(&doc, "findings", "detail")
        .iter()
        .map(|detail| detail.as_str().unwrap())
        .collect();
    assert_eq!(details.len(), 3, "{details:?}");
    assert!(
        details[0].starts_with("I16: V went down: 3 to 1"),
        "{details:?}"
    );
    assert!(
        details[1].starts_with("I16: U went up: 0 to 1"),
        "{details:?}"
    );
    assert!(
        details[2].contains("no new verdict exemplar"),
        "{details:?}"
    );
    assert_eq!(column(&doc, "findings", "code"), ["invariant:I16"; 3]);
}

#[test]
fn diff_reads_a_kp_that_the_base_does_not_have_as_empty() {
    let courses = "courses:\n  - id: calculus-1\n    name: Calculus 1\n    order: 1\n";
    let root = scratch("b5b_diff_new/courses.yaml", courses);
    let unit = std::fs::read_to_string(format!(
        "{}/calculus-1/02-rules.yaml",
        chain_rule_tree(
            "b5b_diff_new_source",
            &[exemplar("Find b.", "7", Value::Null)]
        )
    ));
    scratch(
        "b5b_diff_new/calculus-1/02-rules.yaml",
        &unit.unwrap().replace("id: chain-rule", "id: new-topic"),
    );
    let doc = diff(
        Some(root.trim_end_matches("/courses.yaml")),
        "calculus-1/new-topic/kp1",
    );
    assert_eq!(
        (&doc["exemplars_before"], &doc["V_before"]),
        (&json!(0), &json!(0))
    );
    assert_eq!((&doc["V_after"], &doc["i16"]), (&json!(1), &json!(true)));
}

#[test]
fn diff_gives_exit_2_for_a_bad_ref_and_for_an_unknown_kp() {
    let repo = repo();
    let text = error_text(
        &run(&[
            "diff",
            "--base",
            "no-such-ref",
            "--kp",
            CHAIN_RULE,
            "--repo",
            &repo,
        ]),
        2,
    );
    assert!(
        text.starts_with("git archive no-such-ref curriculum"),
        "{text}"
    );
    let text = error_text(
        &run(&[
            "diff", "--base", BASE_REF, "--kp", "a/b/kp9", "--repo", &repo,
        ]),
        2,
    );
    assert!(text.contains("`a/b/kp9`"), "{text}");
    error_text(
        &run(&[
            "diff", "--base", BASE_REF, "--kp", CHAIN_RULE, "--repo", "/no/such",
        ]),
        2,
    );
}

// ---- report --since ----

#[test]
fn report_since_runs_i12_i13_i14_for_the_new_exemplars_only() {
    let mut exemplars = base_exemplars();
    exemplars.push(exemplar("Find the new value c.", "43", Value::Null));
    let tree = chain_rule_tree("b5b_since", &exemplars);
    let repo = repo();
    let rules = |args: &[&str]| -> Vec<(String, Value)> {
        let result = run(args);
        assert_eq!(result.exit, 1);
        let findings = result.doc["kps"][0]["findings"].as_array().unwrap().clone();
        findings
            .into_iter()
            .map(|finding| {
                (
                    finding["invariant"].as_str().unwrap().to_owned(),
                    finding["hash"].clone(),
                )
            })
            .collect()
    };
    // Without `--since` the three rules do not run.
    let plain = rules(&["report", "--all", "--base", &tree]);
    assert_eq!(plain, [("I2".to_owned(), Value::Null)]);
    // With `--since`, the one new exemplar has no sketch and no contract. The
    // old exemplars have no contract too, and they get no finding.
    let since = rules(&[
        "report", "--all", "--base", &tree, "--since", BASE_REF, "--repo", &repo,
    ]);
    let names: Vec<&str> = since.iter().map(|(rule, _)| rule.as_str()).collect();
    assert_eq!(names, ["I2", "I13", "I14"]);
    assert_eq!(since[1].1, since[2].1);
    assert!(since[1].1.is_string());
}
