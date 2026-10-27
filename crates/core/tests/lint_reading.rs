//! D-RL1: the full lint reports the reading-list rules. The parity lint stays silent.

#![allow(clippy::expect_used)]

use std::fs;
use std::path::PathBuf;

use cadus_core::curriculum::{lint_curriculum, lint_curriculum_full};

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
          - problem: 'Find the sum of $4$ and $3$.'
            answer: "7"
            solution_sketch: '$4+3=7$.'
          - problem: 'Find the sum of $5$ and $3$.'
            answer: "8"
            solution_sketch: '$5+3=8$.'
          - problem: 'Find the sum of $6$ and $3$.'
            answer: "9"
            solution_sketch: '$6+3=9$.'
    diagnostic_exemplar:
      problem: 'Find the sum of $4$ and $1$.'
      answer: "5"
      solution_sketch: '$4+1=5$.'
"#;

fn entry(title: &str, author: &str, access: &str, place: &str, note: &str) -> String {
    format!(
        "      - title: \"{title}\"\n        author: \"{author}\"\n        access: {access}\n        where: \"{place}\"\n        note: \"{note}\"\n"
    )
}

fn good() -> String {
    entry(
        "Primer",
        "A. Writer",
        "free",
        "https://example.org/p",
        "A short note.",
    )
}

fn tree(name: &str, entries: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("cadus-lint-reading-{name}"));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("t")).expect("dir");
    fs::write(
        root.join("courses.yaml"),
        format!("courses:\n  - id: t\n    name: T\n    order: 1\n    reading:\n{entries}"),
    )
    .expect("catalog");
    fs::write(root.join("t").join("01-topics.yaml"), UNIT).expect("unit");
    root
}

fn full_codes(name: &str, entries: &str) -> Vec<String> {
    lint_curriculum_full(&tree(name, entries))
        .iter()
        .map(|f| f.code.clone())
        .collect()
}

fn one_bad(name: &str, bad: &str) {
    let root = tree(name, bad);
    let full = lint_curriculum_full(&root);
    assert_eq!(full.len(), 1, "{name}: {full:?}");
    assert_eq!(full[0].code, "reading_entry");
    assert!(
        full[0].message.starts_with("course t: reading entry 1: "),
        "{}",
        full[0].message
    );
    assert!(
        lint_curriculum(&root).is_empty(),
        "{name}: parity lint stays silent"
    );
}

#[test]
fn a_good_list_is_clean() {
    assert!(full_codes("good", &good()).is_empty());
}

#[test]
fn each_bad_entry_gives_reading_entry() {
    one_bad(
        "title",
        &entry("  ", "A", "free", "https://example.org/p", "n"),
    );
    one_bad(
        "author",
        &entry("T", "", "free", "https://example.org/p", "n"),
    );
    one_bad(
        "note",
        &entry("T", "A", "free", "https://example.org/p", " "),
    );
    one_bad(
        "access",
        &entry("T", "A", "paid", "https://example.org/p", "n"),
    );
    one_bad("where-empty", &entry("T", "A", "book", "", "n"));
    one_bad(
        "http",
        &entry("T", "A", "free", "http://example.org/x", "n"),
    );
    one_bad("space", &entry("T", "A", "free", "https://a b", "n"));
}

#[test]
fn a_chapter_note_may_hold_spaces() {
    assert!(full_codes("chapter", &entry("T", "A", "book", "Chapter 3, p. 5", "n")).is_empty());
    assert!(
        full_codes(
            "httpie",
            &entry("T", "A", "book", "httpie manual, chapter 2", "n")
        )
        .is_empty()
    );
}

#[test]
fn the_note_limit_is_159_characters() {
    assert!(
        full_codes(
            "n159",
            &entry("T", "A", "free", "https://example.org/p", &"x".repeat(159))
        )
        .is_empty()
    );
    one_bad(
        "n160",
        &entry("T", "A", "free", "https://example.org/p", &"x".repeat(160)),
    );
}

#[test]
fn seven_entries_are_too_long_and_six_are_not() {
    assert!(full_codes("six", &good().repeat(6)).is_empty());
    assert_eq!(
        full_codes("seven", &good().repeat(7)),
        vec!["reading_too_long".to_owned()]
    );
}
