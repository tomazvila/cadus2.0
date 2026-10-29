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
