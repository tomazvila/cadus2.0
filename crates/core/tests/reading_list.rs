//! D-RL1: the reading list of a course is presentation only. It loads, it
//! reaches the arena readers, and it moves no hash and no digest.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};

use cadus_core::curriculum::{
    Curriculum, canonical_dump, curriculum_hash, lint_curriculum_full, load_raw_curriculum,
    review_context_digest,
};

const UNIT: &str = r#"unit: t-units
course: t
module: "m"
topics:
  - id: topic-a
    name: Topic A
    core: true
    difficulty: 0.1
    drill: false
    answer_kind: numeric
    expected_time_secs: 40
    knowledge_points:
      - id: kp1
        name: Arithmetic
        key_prerequisites: []
        exemplars:
          - problem: 'Find the sum of $2$ and $3$.'
            answer: "5"
            solution_sketch: '$2+3=5$.'
"#;

const READING: &str = r#"    reading:
      - title: "Free Primer"
        author: "A. Writer"
        access: free
        where: "https://example.org/primer"
        note: "Short pages with examples."
      - title: "Paper Book"
        author: "B. Writer"
        access: book
        where: "Chapter 2, Fractions"
        note: "Practice problems at the back."
"#;

fn tree(name: &str, reading: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("cadus-reading-list-{name}"));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("t")).expect("dir");
    fs::write(
        root.join("courses.yaml"),
        format!("courses:\n  - id: t\n    name: T\n    order: 1\n{reading}"),
    )
    .expect("catalog");
    fs::write(root.join("t").join("01-topics.yaml"), UNIT).expect("unit");
    root
}

fn load(root: &Path) -> Curriculum {
    let (raw, findings) = load_raw_curriculum(root).expect("loads");
    assert!(findings.is_empty(), "no finding: {findings:?}");
    Curriculum::build(raw).expect("builds")
}

#[test]
fn the_list_loads_and_moves_no_hash_and_no_digest() {
    let with = load(&tree("with", READING));
    let without = load(&tree("without", ""));
    assert_eq!(with.course("t").expect("course").reading.len(), 2);
    assert_eq!(
        with.course("t").expect("course").reading[0].where_to_read,
        "https://example.org/primer"
    );
    assert_eq!(curriculum_hash(&with), curriculum_hash(&without));
    assert_eq!(canonical_dump(&with), canonical_dump(&without));
    assert_eq!(
        review_context_digest(&with).expect("digest"),
        review_context_digest(&without).expect("digest")
    );
    let findings = lint_curriculum_full(&tree("lint", READING));
    assert!(
        findings.iter().all(|f| !f.code.starts_with("reading")),
        "{findings:?}"
    );
}

#[test]
fn the_readers_find_the_list_of_the_course() {
    let graph = load(&tree("readers", READING));
    assert_eq!(graph.reading_of_topic("topic-a").len(), 2);
    assert_eq!(graph.reading_of_course("t").len(), 2);
    assert!(graph.reading_of_topic("no-such-topic").is_empty());
    assert!(graph.reading_of_course("no-such-course").is_empty());
}

#[test]
fn the_entry_serializes_with_the_where_key() {
    let graph = load(&tree("json", READING));
    let value = serde_json::to_value(graph.reading_of_course("t")).expect("json");
    assert_eq!(value[0]["where"], "https://example.org/primer");
    assert!(value[0].get("where_to_read").is_none());
}
