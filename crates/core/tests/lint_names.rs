//! `name_math_unmarked`: a topic or knowledge-point name with math outside
//! `$...$` is a finding. A plain name and a marked name are not.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;

use cadus_core::curriculum::{lint_curriculum, lint_curriculum_full, lint_curriculum_names};

/// A tree of one topic per `(topic name, kp name)` pair. Names are single-quoted.
fn tree(case: &str, pairs: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("cadus-lint-names-{case}"));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("t")).expect("dir");
    fs::write(
        root.join("courses.yaml"),
        "courses:\n  - id: t\n    name: T\n    order: 1\n",
    )
    .expect("catalog");
    let mut topics = String::new();
    for (index, (topic, kp)) in pairs.iter().enumerate() {
        topics.push_str(&format!(
            "  - id: t{index}\n    name: '{topic}'\n    core: true\n    difficulty: 0.1\n    drill: false\n    \
             answer_kind: numeric\n    expected_time_secs: 40\n    knowledge_points:\n      - id: kp{index}\n        \
             name: '{kp}'\n        key_prerequisites: []\n        exemplars:\n          - problem: 'Find the sum of $2$ and $3$.'\n            \
             answer: \"5\"\n            solution_sketch: '$2+3=5$.'\n"
        ));
    }
    let unit = format!("unit: t-units\ncourse: t\nmodule: \"m\"\ntopics:\n{topics}");
    fs::write(root.join("t").join("01-topics.yaml"), unit).expect("unit");
    root
}

fn flagged(case: &str, pairs: &[(&str, &str)]) -> Vec<String> {
    let root = tree(case, pairs);
    lint_curriculum_names(&root)
        .into_iter()
        .inspect(|finding| assert_eq!(finding.code, "name_math_unmarked"))
        .filter_map(|finding| finding.context.and_then(|c| c.get(1).cloned()))
        .collect()
}

#[test]
fn three_names_with_unmarked_math_are_flagged() {
    let names = [
        "From n compoundings per year to A = Pe^rt",
        "Cycle form, S_n, and composing permutations",
        "Antiderivatives of trig, exponential, and 1/x",
    ];
    let pairs: Vec<(&str, &str)> = names.iter().map(|name| ("Plain topic", *name)).collect();
    assert_eq!(flagged("flag-kp", &pairs), names);
    let topics: Vec<(&str, &str)> = names.iter().map(|name| (*name, "Plain point")).collect();
    assert_eq!(flagged("flag-topic", &topics), names);
}

#[test]
fn three_plain_names_and_marked_names_are_not_flagged() {
    let pairs = [
        (
            "Continuous Growth and Compounding",
            "Solve a treatment/control case",
        ),
        ("Converting rates (km/h and m/s)", "Cases modulo n"),
        ("And / Or Inequalities", "The sum $a^2 + b^2$ and $e^{rt}$"),
    ];
    assert!(flagged("plain", &pairs).is_empty());
}

#[test]
fn an_unpaired_dollar_sign_is_flagged() {
    assert_eq!(
        flagged("unpaired", &[("Plain", "Cost of $5")]),
        ["Cost of $5"]
    );
}

#[test]
fn the_parity_lints_do_not_run_the_rule() {
    let root = tree("parity", &[("Plain", "A = Pe^rt")]);
    for findings in [lint_curriculum(&root), lint_curriculum_full(&root)] {
        assert!(findings.iter().all(|f| f.code != "name_math_unmarked"));
    }
    assert_eq!(lint_curriculum_names(&root).len(), 1);
}
