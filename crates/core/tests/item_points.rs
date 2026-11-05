//! The item field `points` (D-VBE3): the loader range check, the place outside
//! the canonical dump, and the credit by named part of a multipart answer.

#![allow(clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

use cadus_core::answer::{AnswerContract, multipart_credit};
use cadus_core::curriculum::{lint_curriculum, load_curriculum};

fn tree(name: &str, points_line: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("cadus-item-points-{name}"));
    let dir = root.join("t");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&dir).expect("create dir");
    fs::write(
        root.join("courses.yaml"),
        "courses:\n  - id: t\n    name: T\n    order: 1\n",
    )
    .expect("write catalog");
    let text = format!(
        "unit: t-units\ncourse: t\nmodule: \"m\"\ntopics:\n  - id: pts\n    name: Points\n    core: true\n    difficulty: 0.1\n    drill: false\n    answer_kind: numeric\n    expected_time_secs: 40\n    knowledge_points:\n      - id: kp1\n        name: One\n        key_prerequisites: []\n        exemplars:\n          - problem: 'Find $3+4$.'\n            answer: \"7\"\n            solution_sketch: 'Add.'\n{points_line}"
    );
    fs::write(dir.join("01-pts.yaml"), text).expect("write unit");
    root
}

/// A tree whose topic carries `topic_line` (a whole line with its indent).
fn part_tree(name: &str, topic_line: &str) -> PathBuf {
    let root = tree(name, "");
    let file = root.join("t").join("01-pts.yaml");
    let text = fs::read_to_string(&file).expect("read unit");
    let text = text.replace(
        "    expected_time_secs: 40\n",
        &format!("    expected_time_secs: 40\n{topic_line}"),
    );
    fs::write(&file, text).expect("write unit");
    root
}

fn schema_findings(root: &Path) -> Vec<String> {
    lint_curriculum(root)
        .into_iter()
        .filter(|finding| finding.code == "schema")
        .map(|finding| finding.message)
        .collect()
}

#[test]
fn a_points_value_from_one_to_eleven_loads() {
    for value in ["1", "6", "11"] {
        let root = tree(
            &format!("ok{value}"),
            &format!("            points: {value}\n"),
        );
        assert!(schema_findings(&root).is_empty(), "points {value}");
        let (graph, _) = load_curriculum(&root).expect("the tree loads");
        let idx = graph.idx_of("pts").expect("topic");
        let points = graph.knowledge_points(idx)[0].exemplars[0].points;
        assert_eq!(points, Some(value.parse().expect("a number")));
    }
    let root = tree("absent", "");
    assert!(schema_findings(&root).is_empty());
}

#[test]
fn a_points_value_outside_the_range_is_a_schema_finding() {
    for value in ["0", "12", "-1", "2.5", "'many'"] {
        let root = tree("bad", &format!("            points: {value}\n"));
        assert!(
            !schema_findings(&root).is_empty(),
            "points {value} must be refused"
        );
    }
}

#[test]
fn points_stay_out_of_the_canonical_dump() {
    let with = tree("dump-with", "            points: 5\n");
    let without = tree("dump-without", "");
    let dump = |root: &Path| {
        let (graph, _) = load_curriculum(root).expect("the tree loads");
        cadus_core::curriculum::review_context_digest(&graph).expect("a digest")
    };
    assert_eq!(dump(&with), dump(&without));
}

fn parts() -> AnswerContract {
    serde_json::from_value(serde_json::json!({
        "kind": "multipart",
        "parts": [
            {"name": "a", "contract": {"kind": "exact"}},
            {"name": "b", "contract": {"kind": "exact"}},
            {"name": "c", "contract": {"kind": "exact"}}
        ]
    }))
    .expect("a contract")
}

#[test]
fn a_multipart_answer_earns_the_parts_it_got_right() {
    let contract = parts();
    let expected = "a = 3; b = 23; c = 136";
    assert_eq!(
        multipart_credit(&contract, expected, "a = 3; b = 23; c = 136"),
        Some((3, 3))
    );
    assert_eq!(
        multipart_credit(&contract, expected, "a = 3; b = 20; c = 136"),
        Some((2, 3))
    );
    assert_eq!(
        multipart_credit(&contract, expected, "a = 1; b = 2; c = 3"),
        Some((0, 3))
    );
    assert_eq!(
        multipart_credit(&contract, expected, "garbage"),
        Some((0, 3))
    );
    assert_eq!(multipart_credit(&AnswerContract::Exact, "1", "1"), None);
}

#[test]
fn an_exam_part_of_one_or_two_loads_and_stays_out_of_the_dump() {
    let plain = part_tree("part-none", "");
    for value in ["1", "2"] {
        let root = part_tree(
            &format!("part-{value}"),
            &format!("    exam_part: {value}\n"),
        );
        assert!(schema_findings(&root).is_empty(), "exam_part {value}");
        let (graph, _) = load_curriculum(&root).expect("the tree loads");
        let idx = graph.idx_of("pts").expect("topic");
        let part = graph.topic(idx).expect("topic").exam_part;
        assert_eq!(part, Some(value.parse().expect("a number")));
        let digest = |tree: &Path| {
            let (graph, _) = load_curriculum(tree).expect("the tree loads");
            cadus_core::curriculum::review_context_digest(&graph).expect("a digest")
        };
        assert_eq!(digest(&root), digest(&plain));
    }
}

#[test]
fn an_exam_part_outside_one_and_two_is_a_schema_finding() {
    for value in ["0", "3", "'one'", "1.5"] {
        let root = part_tree("part-bad", &format!("    exam_part: {value}\n"));
        assert!(
            !schema_findings(&root).is_empty(),
            "exam_part {value} must be refused"
        );
    }
}

#[test]
fn a_paper_form_of_pools_or_whole_loads_and_other_values_are_refused() {
    let plain = part_tree("form-none", "");
    for value in ["pools", "whole"] {
        let root = part_tree(
            &format!("form-{value}"),
            &format!("    paper_form: {value}\n"),
        );
        assert!(schema_findings(&root).is_empty(), "paper_form {value}");
        let (graph, _) = load_curriculum(&root).expect("the tree loads");
        let idx = graph.idx_of("pts").expect("topic");
        assert_eq!(
            graph.topic(idx).expect("topic").paper_form.as_deref(),
            Some(value)
        );
        let digest = |tree: &Path| {
            let (graph, _) = load_curriculum(tree).expect("the tree loads");
            cadus_core::curriculum::review_context_digest(&graph).expect("a digest")
        };
        assert_eq!(digest(&root), digest(&plain));
    }
    for value in ["all", "1", "'Whole'"] {
        let root = part_tree("form-bad", &format!("    paper_form: {value}\n"));
        assert!(
            !schema_findings(&root).is_empty(),
            "paper_form {value} must be refused"
        );
    }
}
