//! The exemplar content rules of the full lint (F-grind-lint).
//!
//! The 1.0 parity harness (`lint.rs`) pins `lint_curriculum` finding-for-
//! finding against committed oracle fixtures, and these rules are 2.0
//! additions the 1.0 linter cannot produce. So this suite builds its own
//! minimal trees and asserts the advisory codes of `lint_curriculum_full`
//! directly: `exemplar_answer`, `exemplar_sketch`, `exemplar_no_ask`,
//! `answer_kind_mismatch`, and `exemplar_count`.

#![allow(clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

use cadus_core::curriculum::lint_curriculum_full;

/// One topic template with a numeric-answer KP.
const TOPIC: &str = r#"
  - id: {id}
    name: {name}
    core: true
    difficulty: 0.1
    drill: false
    answer_kind: {kind}
    expected_time_secs: 40
    knowledge_points:
{points}
"#;

fn unit(topics: &str) -> String {
    format!(
        "unit: t-units\ncourse: t\nmodule: \"m\"\ntopics:\n{}",
        topics
    )
}

fn kp(id: &str, name: &str, exemplars: &str) -> String {
    format!(
        "      - id: {}\n        name: {}\n        key_prerequisites: []\n        exemplars:\n{}\n",
        id, name, exemplars
    )
}

fn exemplar(problem: &str, answer: &str, sketch: Option<&str>) -> String {
    let sketch_line = match sketch {
        Some(text) => format!("\n            solution_sketch: '{}'", text),
        None => String::new(),
    };
    format!(
        "          - problem: '{}'\n            answer: \"{}\"{}",
        problem, answer, sketch_line
    )
}

fn write_tree(name: &str, unit_text: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("cadus-lint-exemplars-{}", name));
    let dir = root.join("t");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&dir).expect("create dir");
    fs::write(
        root.join("courses.yaml"),
        "courses:\n  - id: t\n    name: T\n    order: 1\n",
    )
    .expect("write catalog");
    fs::write(dir.join("01-topics.yaml"), unit_text).expect("write topics");
    root
}

fn codes(root: &Path) -> Vec<String> {
    let mut out: Vec<String> = lint_curriculum_full(root)
        .iter()
        .map(|finding| finding.code.clone())
        .collect();
    out.sort();
    out.dedup();
    out
}

/// A decidable numeric exemplar carries a sketch: the clean shape.
#[test]
fn clean_topic_reports_nothing() {
    let e1 = exemplar("Find the sum of $2$ and $3$.", "5", Some("$2+3=5$."));
    let e2 = exemplar("Find the difference of $9$ and $4$.", "5", Some("$9-4=5$."));
    let e3 = exemplar("Find the product of $2$ and $3$.", "6", Some("$2*3=6$."));
    let e4 = exemplar("Find the quotient of $8$ and $2$.", "4", Some("$8/2=4$."));
    let text = unit(
        &TOPIC
            .replace("{id}", "clean-topic")
            .replace("{name}", "Clean")
            .replace("{kind}", "numeric")
            .replace(
                "{points}",
                &kp("kp1", "Arithmetic", &[e1, e2, e3, e4].join("\n")),
            ),
    );
    let topics = format!(
        "{}    diagnostic_exemplar:\n      problem: 'Find the sum of $4$ and $1$.'\n      answer: \"5\"\n      solution_sketch: '$4+1=5$.'\n",
        text
    );
    let text = topics;
    let root = write_tree("clean", &text);
    let findings: Vec<String> = lint_curriculum_full(&root)
        .iter()
        .map(|finding| finding.code.clone())
        .collect();
    assert!(
        findings.is_empty(),
        "the clean topic must report nothing, got {findings:?}"
    );
}

/// An undecidable answer (prose) is its own finding, and the KP with too few
/// decidable exemplars reports the count finding.
#[test]
fn undecidable_answer_and_low_count_report() {
    let e1 = exemplar("Find the sum of $2$ and $3$.", "5", Some("$2+3=5$."));
    let e2 = exemplar("How many sides does a triangle have?", "Three", Some(""));
    let text = unit(
        &TOPIC
            .replace("{id}", "prose-topic")
            .replace("{name}", "Prose")
            .replace("{kind}", "numeric")
            .replace("{points}", &kp("kp1", "Prose", &[e1, e2].join("\n"))),
    );
    let root = write_tree("prose", &text);
    let found = codes(&root);
    assert!(found.contains(&"exemplar_answer".to_string()), "{found:?}");
    assert!(found.contains(&"exemplar_count".to_string()), "{found:?}");
}

/// A decidable exemplar without a solution_sketch reports the sketch finding.
#[test]
fn missing_sketch_reports() {
    let e1 = exemplar("Find the sum of $2$ and $3$.", "5", Some("$2+3=5$."));
    let e2 = exemplar("Find the difference of $9$ and $4$.", "5", Some("$9-4=5$."));
    let e3 = exemplar("Find the product of $2$ and $3$.", "6", None);
    let e4 = exemplar("Find the quotient of $8$ and $2$.", "4", Some("$8/2=4$."));
    let text = unit(
        &TOPIC
            .replace("{id}", "sketch-topic")
            .replace("{name}", "Sketch")
            .replace("{kind}", "numeric")
            .replace(
                "{points}",
                &kp("kp1", "Sketch", &[e1, e2, e3, e4].join("\n")),
            ),
    );
    let root = write_tree("sketch", &text);
    let found = codes(&root);
    assert!(found.contains(&"exemplar_sketch".to_string()), "{found:?}");
    assert!(!found.contains(&"exemplar_count".to_string()), "{found:?}");
}

/// A statement with no question mark and no task verb reports the no-ask
/// finding — the trapezoid-formula defect of the grind ("Use the formula
/// for a shape with…" never says what to give back).
#[test]
fn statement_without_ask_reports() {
    let e1 = exemplar(
        "Use $A=lw$ to find the area when $l = 8$ and $w = 5$.",
        "40",
        Some("$8*5=40$."),
    );
    let e2 = exemplar(
        "Use $A=lw$ for a rectangle with $l = 8$ and $w = 5$.",
        "40",
        Some("$8*5=40$."),
    );
    let e3 = exemplar(
        "Find the area of a rectangle with sides $8$ and $5$.",
        "40",
        Some("$8*5=40$."),
    );
    let e4 = exemplar(
        "Find the area of a rectangle with sides $9$ and $6$.",
        "54",
        Some("$9*6=54$."),
    );
    let text = unit(
        &TOPIC
            .replace("{id}", "ask-topic")
            .replace("{name}", "Ask")
            .replace("{kind}", "numeric")
            .replace("{points}", &kp("kp1", "Ask", &[e1, e2, e3, e4].join("\n"))),
    );
    let root = write_tree("ask", &text);
    let found = codes(&root);
    assert!(found.contains(&"exemplar_no_ask".to_string()), "{found:?}");
    // The sibling with "to find" passes the ask heuristic.
    assert!(!found.contains(&"exemplar_answer".to_string()), "{found:?}");
}

/// A `multi-step` topic with plain single-answer exemplars reports the
/// answer-kind mismatch: the grader routes every answer through the topic
/// kind and grades it ungraded (ISSUE-11).
#[test]
fn answer_kind_mismatch_reports() {
    let e1 = exemplar("Find the sum of $2$ and $3$.", "5", Some("$2+3=5$."));
    let e2 = exemplar("Find the difference of $9$ and $4$.", "5", Some("$9-4=5$."));
    let e3 = exemplar("Find the product of $2$ and $3$.", "6", Some("$2*3=6$."));
    let e4 = exemplar("Find the quotient of $8$ and $2$.", "4", Some("$8/2=4$."));
    let text = unit(
        &TOPIC
            .replace("{id}", "multistep-topic")
            .replace("{name}", "MultiStep")
            .replace("{kind}", "multi-step")
            .replace(
                "{points}",
                &kp("kp1", "MultiStep", &[e1, e2, e3, e4].join("\n")),
            ),
    );
    let root = write_tree("multistep", &text);
    let found = codes(&root);
    assert!(
        found.contains(&"answer_kind_mismatch".to_string()),
        "{found:?}"
    );
}

/// The diagnostic exemplar is checked like any other.
#[test]
fn diagnostic_exemplar_is_checked() {
    let e1 = exemplar("Find the sum of $2$ and $3$.", "5", Some("$2+3=5$."));
    let e2 = exemplar("Find the difference of $9$ and $4$.", "5", Some("$9-4=5$."));
    let e3 = exemplar("Find the product of $2$ and $3$.", "6", Some("$2*3=6$."));
    let e4 = exemplar("Find the quotient of $8$ and $2$.", "4", Some("$8/2=4$."));
    let topics = TOPIC
        .replace("{id}", "diag-topic")
        .replace("{name}", "Diag")
        .replace("{kind}", "numeric")
        .replace("{points}", &kp("kp1", "Diag", &[e1, e2, e3, e4].join("\n")))
        + "\n    diagnostic_exemplar:\n      problem: 'What is the total?'\n      answer: \"The total is 7\"\n";
    let text = unit(&topics);
    let root = write_tree("diag", &text);
    let found = codes(&root);
    assert!(found.contains(&"exemplar_answer".to_string()), "{found:?}");
}
