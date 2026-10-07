//! A figure belongs to the item that names it: the schema field `visual:`, the
//! loader check of its index, and the three `visual_*` lint rules.

#![allow(clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

use cadus_core::curriculum::{lint_curriculum, lint_curriculum_full};

const POINT_LINE: &str = "        - kind: number_line\n          min: -8\n          max: 8\n          tick: 1\n          points:\n            - {at: -6, label: A}\n            - {at: 2, label: B}\n";
const AREA_SQUARE: &str = "        - kind: geometry\n          figure:\n            shape: polygon\n            vertices:\n              - {x: 0, y: 0, label: A}\n              - {x: 3, y: 0, label: B}\n              - {x: 3, y: 3, label: C}\n              - {x: 0, y: 3, label: D}\n";

fn tree(name: &str, visuals: &str, exemplar_lines: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("cadus-visual-items-{name}"));
    let dir = root.join("t");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&dir).expect("create dir");
    fs::write(
        root.join("courses.yaml"),
        "courses:\n  - id: t\n    name: T\n    order: 1\n",
    )
    .expect("write catalog");
    let text = format!(
        "unit: t-units\ncourse: t\nmodule: \"m\"\ntopics:\n  - id: figs\n    name: Figs\n    core: true\n    difficulty: 0.1\n    drill: false\n    answer_kind: numeric\n    expected_time_secs: 40\n    knowledge_points:\n      - id: kp1\n        name: Figures\n        key_prerequisites: []\n        visuals:\n{visuals}        exemplars:\n{exemplar_lines}"
    );
    fs::write(dir.join("01-figs.yaml"), text).expect("write unit");
    root
}

fn item(problem: &str, answer: &str, visual: Option<&str>) -> String {
    let visual = visual.map_or_else(String::new, |v| format!("            visual: {v}\n"));
    format!(
        "          - problem: '{problem}'\n            answer: \"{answer}\"\n            solution_sketch: 'Read it off.'\n{visual}"
    )
}

fn visual_codes(root: &Path) -> Vec<String> {
    lint_curriculum_full(root)
        .into_iter()
        .map(|finding| finding.code)
        .filter(|code| code.starts_with("visual_"))
        .collect()
}

#[test]
fn an_item_that_states_the_labels_of_its_figure_is_clean() {
    let root = tree(
        "clean",
        POINT_LINE,
        &item(
            "Point $A$ is at $-6$ and point $B$ is at $2$. Find the distance from $A$ to $B$.",
            "8",
            Some("0"),
        ),
    );
    assert_eq!(visual_codes(&root), Vec::<String>::new());
    let parity = lint_curriculum(&root);
    assert!(
        parity.iter().all(|finding| finding.code != "schema"),
        "got {parity:?}"
    );
}

#[test]
fn an_item_without_the_field_shows_no_figure_and_is_clean() {
    let root = tree("none", POINT_LINE, &item("Find $3+4$.", "7", None));
    assert_eq!(visual_codes(&root), Vec::<String>::new());
    let flagged = tree(
        "flag-off",
        POINT_LINE,
        &item("Find $3+4$.", "7", Some("false")),
    );
    assert_eq!(visual_codes(&flagged), Vec::<String>::new());
}

#[test]
fn a_figure_label_the_item_never_states_is_a_mismatch() {
    let root = tree(
        "mismatch",
        POINT_LINE,
        &item(
            "The temperature is $5$ below zero. Give the integer.",
            "-5",
            Some("0"),
        ),
    );
    assert_eq!(visual_codes(&root), vec!["visual_mismatch"]);
}

#[test]
fn a_vertex_letter_that_is_also_a_quantity_clashes() {
    let root = tree(
        "clash",
        AREA_SQUARE,
        &item(
            "Square $ABCD$ has side $3$. Find the area $A$.",
            "9",
            Some("0"),
        ),
    );
    assert_eq!(visual_codes(&root), vec!["visual_letter_clash"]);
}

#[test]
fn a_problem_that_points_at_a_drawing_needs_a_named_figure() {
    let root = tree(
        "missing",
        POINT_LINE,
        &item(
            "The diagram shows a point at $2$. Give its value.",
            "2",
            None,
        ),
    );
    assert_eq!(visual_codes(&root), vec!["visual_missing"]);
}

#[test]
fn the_loader_refuses_an_index_past_the_figure_list() {
    let root = tree("range", POINT_LINE, &item("Find $3+4$.", "7", Some("1")));
    let findings = lint_curriculum(&root);
    assert!(
        findings
            .iter()
            .any(|finding| finding.message.contains("names visual 1")),
        "got {findings:?}"
    );
}

#[test]
fn visual_true_is_refused() {
    let root = tree("true", POINT_LINE, &item("Find $3+4$.", "7", Some("true")));
    assert!(!lint_curriculum(&root).is_empty());
}
