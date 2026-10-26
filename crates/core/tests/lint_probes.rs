//! The placement probe rule (D-DG4): `diagnostic_probe_undecidable` and
//! `diagnostic_probe_allow_stale` of `lint_curriculum_probes_with`.
//!
//! Each case builds its own tree. The rule is not part of `lint_curriculum` or
//! `lint_curriculum_full`, and the last case checks that.

#![allow(clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

use cadus_core::curriculum::{lint_curriculum, lint_curriculum_full, lint_curriculum_probes_with};

const UNDECIDABLE: &str = "diagnostic_probe_undecidable";
const STALE: &str = "diagnostic_probe_allow_stale";

/// One topic with a probe. `contract` is a line of flow JSON or empty.
/// `probe` is false for a topic with no `diagnostic_exemplar`.
fn topic(id: &str, kind: &str, contract: &str, key: &str, probe: bool) -> String {
    let mut text = format!(
        "  - id: {id}\n    name: {id}\n    core: true\n    difficulty: 0.1\n    drill: false\n    \
         answer_kind: {kind}\n    expected_time_secs: 40\n    knowledge_points:\n      - id: kp1\n        \
         name: kp1\n        key_prerequisites: []\n        exemplars:\n          - problem: 'Find the sum of $2$ and $3$.'\n            \
         answer: \"5\"\n            solution_sketch: '$2+3=5$.'\n"
    );
    if probe {
        text.push_str("    diagnostic_exemplar:\n      problem: 'Find the sum of $4$ and $1$.'\n");
        if !contract.is_empty() {
            text.push_str(&format!("      answer_contract: {contract}\n"));
        }
        text.push_str(&format!("      answer: \"{key}\"\n"));
    }
    text
}

fn write_tree(name: &str, topics: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("cadus-lint-probes-{name}"));
    let dir = root.join("t");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&dir).expect("create dir");
    fs::write(
        root.join("courses.yaml"),
        "courses:\n  - id: t\n    name: T\n    order: 1\n",
    )
    .expect("write catalog");
    let unit = format!("unit: t-units\ncourse: t\nmodule: \"m\"\ntopics:\n{topics}");
    fs::write(dir.join("01-topics.yaml"), unit).expect("write topics");
    root
}

/// The codes of the probe rule on a tree with one topic and this allow-list
/// text. `None` = no allow-list file.
fn probe_codes(name: &str, topics: &str, allow: Option<&str>) -> Vec<String> {
    let root = write_tree(name, topics);
    let path = root.join("diag-probe-allow.txt");
    if let Some(text) = allow {
        fs::write(&path, text).expect("write allow-list");
    }
    codes_at(&root, &path)
}

fn codes_at(root: &Path, allow: &Path) -> Vec<String> {
    lint_curriculum_probes_with(root, allow)
        .iter()
        .map(|finding| finding.code.clone())
        .collect()
}

fn one(code: &str) -> Vec<String> {
    vec![code.to_owned()]
}

#[test]
fn a_numeric_topic_with_a_readable_key_is_clean() {
    let t = topic("a-topic", "numeric", "", "5", true);
    assert!(probe_codes("numeric", &t, None).is_empty());
}

#[test]
fn a_multi_step_topic_with_no_contract_is_a_finding() {
    let t = topic("a-topic", "multi-step", "", "5", true);
    assert_eq!(probe_codes("multi", &t, None), one(UNDECIDABLE));
}

#[test]
fn a_proof_topic_with_no_contract_is_a_finding() {
    let t = topic("a-topic", "proof", "", "5", true);
    assert_eq!(probe_codes("proof", &t, None), one(UNDECIDABLE));
}

#[test]
fn contract_none_and_contract_written_are_findings() {
    let none = topic("a-topic", "numeric", r#"{"kind":"none"}"#, "5", true);
    assert_eq!(probe_codes("none", &none, None), one(UNDECIDABLE));
    let written = topic("a-topic", "numeric", r#"{"kind":"written"}"#, "5", true);
    assert_eq!(probe_codes("written", &written, None), one(UNDECIDABLE));
}

#[test]
fn a_proof_topic_with_an_exact_contract_is_clean() {
    let t = topic("a-topic", "proof", r#"{"kind":"exact"}"#, "3", true);
    assert!(probe_codes("proof-exact", &t, None).is_empty());
}

#[test]
fn a_label_contract_with_a_key_outside_the_options_is_a_finding() {
    let contract = r#"{"kind":"label","options":[["A"],["B"]]}"#;
    let bad = topic("a-topic", "proof", contract, "Z", true);
    assert_eq!(probe_codes("label-bad", &bad, None), one(UNDECIDABLE));
    let good = topic("a-topic", "proof", contract, "A", true);
    assert!(probe_codes("label-good", &good, None).is_empty());
}

#[test]
fn the_message_names_the_cause_and_the_fix() {
    let t = topic("a-topic", "proof", "", "5", true);
    let root = write_tree("message", &t);
    let findings = lint_curriculum_probes_with(&root, &root.join("none.txt"));
    let finding = findings.first().expect("one finding");
    assert_eq!(finding.topic.as_deref(), Some("a-topic"));
    assert!(finding.message.contains("the topic kind is `proof`"));
    assert!(finding.message.contains("section 6)."));
}

#[test]
fn an_allow_listed_undecidable_topic_is_clean() {
    let t = topic("a-topic", "proof", "", "5", true);
    assert!(probe_codes("allowed", &t, Some("a-topic\n")).is_empty());
}

#[test]
fn an_allow_listed_decidable_topic_is_stale() {
    let t = topic("a-topic", "numeric", "", "5", true);
    assert_eq!(probe_codes("stale", &t, Some("a-topic\n")), one(STALE));
}

#[test]
fn an_unknown_allow_listed_id_is_stale() {
    let t = topic("a-topic", "numeric", "", "5", true);
    assert_eq!(probe_codes("unknown", &t, Some("ghost\n")), one(STALE));
}

#[test]
fn comments_blank_lines_and_trailing_spaces_are_read() {
    let t = topic("a-topic", "proof", "", "5", true);
    let allow = "# the allow-list\n\n  \na-topic  # no probe yet (D-DG4)\n";
    assert!(probe_codes("comments", &t, Some(allow)).is_empty());
    assert!(probe_codes("trailing", &t, Some("a-topic   \n")).is_empty());
}

#[test]
fn an_absent_allow_list_is_an_empty_list() {
    let t = topic("a-topic", "proof", "", "5", true);
    assert_eq!(probe_codes("absent", &t, None), one(UNDECIDABLE));
}

#[test]
fn a_topic_with_no_probe_gets_no_probe_finding() {
    let t = topic("a-topic", "proof", "", "5", false);
    assert!(probe_codes("no-probe", &t, None).is_empty());
}

#[test]
fn the_other_lints_do_not_run_the_rule() {
    let t = topic("a-topic", "proof", "", "5", true);
    let root = write_tree("not-in-lints", &t);
    let probe = |findings: Vec<cadus_core::curriculum::Finding>| {
        findings
            .iter()
            .any(|finding| finding.code.starts_with("diagnostic_probe"))
    };
    assert!(!probe(lint_curriculum(&root)));
    assert!(!probe(lint_curriculum_full(&root)));
}
