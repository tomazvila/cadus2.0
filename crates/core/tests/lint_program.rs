//! D-VBE1: the topic fields `lt_term`, `program_items` and `area`, the course
//! field `exam_weights`, and the program registry rules. The fields move no
//! hash and no digest. A course with no registry gets no finding.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};

use cadus_core::curriculum::{
    Curriculum, canonical_dump, curriculum_hash, lint_curriculum_program, load_raw_curriculum,
    review_context_digest,
};

/// One topic. `extra` is a block of topic fields, four-space indent.
fn topic(id: &str, extra: &str) -> String {
    format!(
        "  - id: {id}\n    name: {id}\n    core: true\n    difficulty: 0.1\n    drill: false\n    \
         answer_kind: numeric\n    expected_time_secs: 40\n{extra}    knowledge_points:\n      - id: kp1\n        \
         name: kp1\n        key_prerequisites: []\n        exemplars:\n          - problem: 'Find the sum of $2$ and $3$.'\n            \
         answer: \"5\"\n            solution_sketch: '$2+3=5$.'\n"
    )
}

fn tree(name: &str, course_extra: &str, topics: &str, registry: Option<&str>) -> PathBuf {
    let root = std::env::temp_dir().join(format!("cadus-lint-program-{name}"));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("t")).expect("dir");
    fs::write(
        root.join("courses.yaml"),
        format!("courses:\n  - id: t\n    name: T\n    order: 1\n{course_extra}"),
    )
    .expect("catalog");
    let unit = format!("unit: t-units\ncourse: t\nmodule: \"m\"\ntopics:\n{topics}");
    fs::write(root.join("t").join("01-topics.yaml"), unit).expect("unit");
    if let Some(text) = registry {
        fs::write(root.join("t").join("program.yaml"), text).expect("registry");
    }
    root
}

fn load(root: &Path) -> Curriculum {
    let (raw, findings) = load_raw_curriculum(root).expect("loads");
    assert!(findings.is_empty(), "no finding: {findings:?}");
    Curriculum::build(raw).expect("builds")
}

fn codes(root: &Path) -> Vec<String> {
    lint_curriculum_program(root)
        .iter()
        .map(|finding| finding.code.clone())
        .collect()
}

const FIELDS: &str = "    lt_term: \"Lygtys\"\n    program_items: [1, 2]\n    area: models\n";
const WEIGHTS: &str = "    exam_weights: {numbers: 15, models: 50, geometry: 20, data: 15}\n";
const REGISTRY: &str = "program:\n  - {row: 1, lt_term: \"Lygtys\", excluded: null}\n  \
                        - {row: 2, lt_term: \"Nelygybes\"}\n  - {row: 3, excluded: \"Not in the course\"}\n";

#[test]
fn the_fields_load_and_move_no_hash_and_no_digest() {
    let with = load(&tree("fw", WEIGHTS, &topic("topic-a", FIELDS), None));
    let without = load(&tree("fo", "", &topic("topic-a", ""), None));
    let found = with
        .topic(with.idx_of("topic-a").expect("idx"))
        .expect("topic");
    assert_eq!(found.lt_term.as_deref(), Some("Lygtys"));
    assert_eq!(found.program_items, vec![1, 2]);
    assert_eq!(found.area.as_deref(), Some("models"));
    assert_eq!(with.lt_term_of_topic("topic-a"), Some("Lygtys"));
    assert_eq!(without.lt_term_of_topic("topic-a"), None);
    let weights = with
        .course("t")
        .expect("course")
        .exam_weights
        .expect("weights");
    assert_eq!((weights.numbers, weights.models), (15, 50));
    assert_eq!(curriculum_hash(&with), curriculum_hash(&without));
    assert_eq!(canonical_dump(&with), canonical_dump(&without));
    assert_eq!(
        review_context_digest(&with).expect("digest"),
        review_context_digest(&without).expect("digest")
    );
}

#[test]
fn a_wrong_area_or_a_wrong_weight_sum_is_a_schema_finding() {
    let bad_area = tree("ba", "", &topic("topic-a", "    area: algebra\n"), None);
    let (_, findings) = load_raw_curriculum(&bad_area).expect("loads");
    assert!(findings.iter().any(|f| f.code == "schema"), "{findings:?}");
    let sum = "    exam_weights: {numbers: 10, models: 50, geometry: 20, data: 15}\n";
    let bad_sum = tree("bs", sum, &topic("topic-a", ""), None);
    let (_, findings) = load_raw_curriculum(&bad_sum).expect("loads");
    assert!(
        findings.iter().any(|f| f.message.contains("add up to 100")),
        "{findings:?}"
    );
}

#[test]
fn a_covered_registry_gives_no_finding() {
    let root = tree("ok", "", &topic("topic-a", FIELDS), Some(REGISTRY));
    assert!(
        codes(&root).is_empty(),
        "{:?}",
        lint_curriculum_program(&root)
    );
}

#[test]
fn an_uncovered_row_without_a_reason_is_a_finding() {
    let topics = topic("topic-a", "    program_items: [1]\n");
    let root = tree("un", "", &topics, Some(REGISTRY));
    assert_eq!(codes(&root), vec!["program_item_uncovered".to_owned()]);
    let message = &lint_curriculum_program(&root)[0].message;
    assert!(message.contains("row 2"), "{message}");
}

#[test]
fn a_topic_that_names_a_missing_row_is_a_finding() {
    let topics = topic("topic-a", "    program_items: [1, 2, 9]\n");
    let root = tree("kn", "", &topics, Some(REGISTRY));
    assert_eq!(codes(&root), vec!["program_item_unknown".to_owned()]);
}

#[test]
fn a_course_with_no_registry_gets_no_finding() {
    let root = tree("nr", "", &topic("topic-a", FIELDS), None);
    assert!(codes(&root).is_empty());
}

#[test]
fn a_broken_registry_is_a_finding() {
    let root = tree("br", "", &topic("topic-a", ""), Some("program: [1, 2]\n"));
    assert_eq!(codes(&root), vec!["program_registry".to_owned()]);
}

#[test]
fn a_row_named_only_by_a_topic_of_another_course_is_uncovered() {
    let root = tree(
        "xc",
        "",
        &topic("topic-a", "    program_items: [2]\n"),
        Some(REGISTRY),
    );
    fs::write(
        root.join("courses.yaml"),
        "courses:\n  - id: t\n    name: T\n    order: 1\n  - id: u\n    name: U\n    order: 2\n",
    )
    .expect("catalog");
    fs::create_dir_all(root.join("u")).expect("dir");
    let other = topic("topic-b", "    program_items: [1]\n");
    let unit = format!("unit: u-units\ncourse: u\nmodule: \"m\"\ntopics:\n{other}");
    fs::write(root.join("u").join("01-topics.yaml"), unit).expect("unit");
    let findings = lint_curriculum_program(&root);
    let codes: Vec<&str> = findings.iter().map(|f| f.code.as_str()).collect();
    assert_eq!(codes, vec!["program_item_uncovered"], "{findings:?}");
    assert!(
        findings[0].message.starts_with("t:"),
        "{}",
        findings[0].message
    );
    assert!(
        findings[0].message.contains("row 1"),
        "{}",
        findings[0].message
    );
}

#[test]
fn a_duplicate_row_number_is_a_registry_finding() {
    let registry = "program:\n  - {row: 1}\n  - {row: 1}\n";
    let root = tree(
        "dup",
        "",
        &topic("topic-a", "    program_items: [1]\n"),
        Some(registry),
    );
    assert_eq!(codes(&root), vec!["program_registry".to_owned()]);
    let message = &lint_curriculum_program(&root)[0].message;
    assert!(
        message.contains("row number 1 appears more than once"),
        "{message}"
    );
}

#[test]
fn a_row_number_zero_is_a_registry_finding() {
    let registry = "program:\n  - {row: 0, excluded: \"Not a row\"}\n  - {row: 1}\n";
    let root = tree(
        "zero",
        "",
        &topic("topic-a", "    program_items: [1]\n"),
        Some(registry),
    );
    assert_eq!(codes(&root), vec!["program_registry".to_owned()]);
    let message = &lint_curriculum_program(&root)[0].message;
    assert!(message.contains("row number 0"), "{message}");
}
