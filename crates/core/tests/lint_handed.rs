//! `handed_value`: a problem text that gives the learner a value the question
//! exists to make them produce is a finding. A text that states only the
//! rounding is not. A `written` item is not covered.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;

use cadus_core::curriculum::{lint_curriculum, lint_curriculum_full, lint_curriculum_handed};

/// A tree of one topic with `(contract line, problem)` exemplars. A problem is single-quoted.
fn tree(case: &str, items: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("cadus-lint-handed-{case}"));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("t")).expect("dir");
    fs::write(
        root.join("courses.yaml"),
        "courses:\n  - id: t\n    name: T\n    order: 1\n",
    )
    .expect("catalog");
    let mut exemplars = String::new();
    for (contract, problem) in items {
        exemplars.push_str(&format!(
            "          - problem: '{problem}'\n            {contract}answer: \"5\"\n            solution_sketch: '$2+3=5$.'\n"
        ));
    }
    let unit = format!(
        "unit: t-units\ncourse: t\nmodule: \"m\"\ntopics:\n  - id: t0\n    name: 'Plain'\n    core: true\n    difficulty: 0.1\n    drill: false\n    \
         answer_kind: numeric\n    expected_time_secs: 40\n    knowledge_points:\n      - id: kp0\n        \
         name: 'Plain point'\n        key_prerequisites: []\n        exemplars:\n{exemplars}"
    );
    fs::write(root.join("t").join("01-topics.yaml"), unit).expect("unit");
    root
}

#[test]
fn a_handed_value_is_flagged_with_topic_point_item_and_quote() {
    let root = tree(
        "flag",
        &[
            ("", "Find $x$ to the nearest whole number."),
            (
                "",
                "Using $e^{0.3} \\approx 1.34986$, find the population after 10 years.",
            ),
        ],
    );
    let findings = lint_curriculum_handed(&root);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].code, "handed_value");
    assert_eq!(findings[0].topic.as_deref(), Some("t0"));
    let context = findings[0].context.clone().expect("context");
    assert_eq!(&context[..2], ["kp0", "exemplar 2"]);
    assert!(context[2].contains("1.34986"), "{context:?}");
}

#[test]
fn a_rounding_only_text_is_not_flagged() {
    let root = tree(
        "plain",
        &[
            (
                "",
                "Find $e^{0.3} \\cdot 12000$ to the nearest whole number.",
            ),
            ("", "Find the area in terms of $\\pi$."),
        ],
    );
    assert!(lint_curriculum_handed(&root).is_empty());
}

#[test]
fn the_parity_lints_do_not_run_the_rule() {
    let root = tree(
        "parity",
        &[("", "Take $\\sin 30^\\circ = 0.5$ and find $x$.")],
    );
    for findings in [lint_curriculum(&root), lint_curriculum_full(&root)] {
        assert!(findings.iter().all(|f| f.code != "handed_value"));
    }
    assert_eq!(lint_curriculum_handed(&root).len(), 1);
}
