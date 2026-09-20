//! `Exemplar::verdict_policy`: the one definition of "verdict exemplar".
//!
//! The first part is the truth table of the frozen interface. The second part
//! shows that readiness, the exemplar pool source and the lint give the same
//! answer as `verdict_policy` for each row of the table.

#![allow(clippy::expect_used)]

mod common;

use cadus_core::answer::AnswerContract;
use cadus_core::curriculum::{AnswerKind, Exemplar, lint_curriculum_full, load_curriculum};
use cadus_core::pool::{ExemplarSource, ProblemSource};
use cadus_core::readiness::{DiagnosticState, PrereqCoverage, ReadinessIndex};
use common::scratch::ScratchTree;

const KINDS: [AnswerKind; 4] = [
    AnswerKind::Numeric,
    AnswerKind::Expression,
    AnswerKind::MultiStep,
    AnswerKind::Proof,
];

/// One contract state of the table: a name, the YAML line, the contract and
/// the authored answer.
struct State {
    name: &'static str,
    yaml: &'static str,
    contract: Option<AnswerContract>,
    answer: &'static str,
}

/// The three contract states of the table, the state "the key does not
/// validate", and the state "no contract and the key does not parse".
fn states() -> Vec<State> {
    vec![
        State {
            name: "valid",
            yaml: "answer_contract: {kind: exact}",
            contract: Some(AnswerContract::Exact),
            answer: "5",
        },
        State {
            name: "none",
            yaml: "answer_contract: {kind: none}",
            contract: Some(AnswerContract::None),
            answer: "5",
        },
        State {
            name: "absent",
            yaml: "",
            contract: None,
            answer: "5",
        },
        State {
            name: "badkey",
            yaml: "answer_contract: {kind: exact}",
            contract: Some(AnswerContract::Exact),
            answer: "many words",
        },
        State {
            name: "prose",
            yaml: "",
            contract: None,
            answer: "many words",
        },
    ]
}

fn exemplar(state: &State) -> Exemplar {
    Exemplar {
        problem: "Find the value.".to_owned(),
        answer_contract: state.contract.clone(),
        answer: state.answer.to_owned(),
        solution_sketch: Some("Count.".to_owned()),
    }
}

/// The result that the frozen truth table gives for one row.
fn expected(state: &State, kind: AnswerKind) -> bool {
    match state.name {
        "valid" => true,
        "absent" => matches!(kind, AnswerKind::Numeric | AnswerKind::Expression),
        _ => false,
    }
}

#[test]
fn the_truth_table_has_twelve_rows_and_each_row_holds() {
    let mut rows = 0;
    for state in states().iter().take(3) {
        for kind in KINDS {
            rows += 1;
            let result = exemplar(state).verdict_policy(kind);
            assert_eq!(
                result.is_ok(),
                expected(state, kind),
                "{} {kind}",
                state.name
            );
        }
    }
    assert_eq!(rows, 12);
}

#[test]
fn the_error_reasons_are_the_reasons_of_the_table() {
    let all = states();
    for kind in KINDS {
        let none = exemplar(&all[1]).verdict_policy(kind).expect_err("none");
        assert_eq!(none.reason, "the item has no deterministic answer contract");
    }
    for kind in [AnswerKind::MultiStep, AnswerKind::Proof] {
        let absent = exemplar(&all[2]).verdict_policy(kind).expect_err("absent");
        assert_eq!(absent.reason, "the answer kind is not decidable");
    }
}

#[test]
fn a_contract_with_a_key_that_does_not_validate_gives_no_verdict() {
    let all = states();
    for kind in KINDS {
        assert!(exemplar(&all[3]).verdict_policy(kind).is_err(), "{kind}");
    }
    assert!(
        exemplar(&all[4])
            .verdict_policy(AnswerKind::Numeric)
            .is_err()
    );
}

#[test]
fn the_examples_of_the_brief_hold() {
    let matrix = Exemplar {
        problem: "Find the inverse.".to_owned(),
        answer_contract: Some(AnswerContract::Matrix { rows: 2, cols: 2 }),
        answer: "[3,-1;-5,2]".to_owned(),
        solution_sketch: None,
    };
    assert!(matrix.verdict_policy(AnswerKind::Numeric).is_ok());
    assert!(matrix.verdict_policy(AnswerKind::Proof).is_ok());
    let pair = Exemplar {
        problem: "Find the interval.".to_owned(),
        answer_contract: None,
        answer: "$(46, 54)$".to_owned(),
        solution_sketch: None,
    };
    assert!(pair.verdict_policy(AnswerKind::Numeric).is_ok());
    assert!(pair.verdict_policy(AnswerKind::Proof).is_err());
}

// ---------------------------------------------------------------------------
// Agreement: readiness, the pool source and the lint against `verdict_policy`
// ---------------------------------------------------------------------------

fn topic_id(state: &State, kind: AnswerKind) -> String {
    format!("{}-{}", state.name, kind.as_str())
}

/// One topic with one knowledge point, one exemplar and one diagnostic
/// exemplar, each in the contract state of the row.
fn topic_yaml(state: &State, kind: AnswerKind) -> String {
    let contract = |indent: &str| {
        if state.yaml.is_empty() {
            String::new()
        } else {
            format!("{indent}{}\n", state.yaml)
        }
    };
    format!(
        "  - id: {id}\n    name: {id}\n    core: true\n    difficulty: 0.1\n    drill: false\n    answer_kind: {kind}\n    expected_time_secs: 40\n    knowledge_points:\n      - id: kp1\n        name: kp1\n        key_prerequisites: []\n        exemplars:\n          - problem: 'Find the value.'\n{c12}            answer: \"{answer}\"\n            solution_sketch: 'Count.'\n    diagnostic_exemplar:\n      problem: 'What is the value?'\n{c6}      answer: \"{answer}\"\n",
        id = topic_id(state, kind),
        kind = kind.as_str(),
        answer = state.answer,
        c12 = contract("            "),
        c6 = contract("      "),
    )
}

/// The start of the one unit file of a tree.
const UNIT_HEAD: &str = "unit: t-units\ncourse: t\nmodule: \"m\"\ntopics:\n";

/// A tree of course `t` with the given topics.
fn write_tree(label: &str, topics: &str) -> ScratchTree {
    let tree = ScratchTree::new(label);
    tree.courses(&["t"])
        .write("t/01-topics.yaml", &format!("{UNIT_HEAD}{topics}"));
    tree
}

/// The topics of the table: one topic for each row.
fn table_topics() -> String {
    let mut text = String::new();
    for state in &states() {
        for kind in KINDS {
            text.push_str(&topic_yaml(state, kind));
        }
    }
    text
}

/// The count that the `exemplar_count` finding of the lint gives for one KP.
fn lint_decidable(messages: &[String], topic: &str) -> usize {
    let prefix = format!("{topic}/kp1: ");
    let message = messages
        .iter()
        .find(|message| message.starts_with(&prefix) && message.contains("decidable exemplar(s)"))
        .expect("each KP of the tree is below the held-out minimum");
    let count = message[prefix.len()..].split(' ').next().expect("a count");
    count.parse().expect("a number")
}

#[test]
fn readiness_the_pool_source_and_the_lint_agree_with_the_verdict_policy() {
    let tree = write_tree("verdict-policy", &table_topics());
    let root = tree.root();
    let (curriculum, _) = load_curriculum(root).expect("the tree loads");
    let index = ReadinessIndex::build(&curriculum);
    let coverage = PrereqCoverage::build(&curriculum, &index);
    let messages: Vec<String> = lint_curriculum_full(root)
        .iter()
        .map(|finding| finding.message.clone())
        .collect();
    for state in &states() {
        for kind in KINDS {
            let id = topic_id(state, kind);
            let item = exemplar(state);
            let verdict = item.verdict_policy(kind).is_ok();
            assert_eq!(verdict, expected(state, kind), "{id}");
            // Readiness: `kp_facts` counts the exemplar or not.
            let facts = index.get(&format!("{id}/kp1")).expect("facts");
            assert_eq!(facts.decidable.len(), usize::from(verdict), "{id}");
            // Readiness: the diagnostic state.
            let topic = coverage
                .course("t")
                .into_iter()
                .find(|topic| topic.topic_id == id)
                .expect("coverage");
            let want = if verdict {
                DiagnosticState::Decidable
            } else {
                DiagnosticState::Undecidable
            };
            assert_eq!(topic.diagnostic, want, "{id}");
            // The pool source with the topic kind.
            let items = [item];
            let source = ExemplarSource::new("kp", &items).with_topic_kind(kind);
            assert_eq!(source.fill("kp", 1, 0).is_ok(), verdict, "{id}");
            assert_eq!(source.refusals().is_empty(), verdict, "{id}");
            // The lint: the decidable count of the KP.
            assert_eq!(lint_decidable(&messages, &id), usize::from(verdict), "{id}");
        }
    }
}

/// A source with no topic kind keeps `canonical_answer`, for a caller that
/// has no topic.
#[test]
fn a_pool_source_with_no_topic_kind_reads_the_canonical_answer() {
    let all = states();
    let items = [exemplar(&all[2])];
    let source = ExemplarSource::new("kp", &items);
    assert!(source.fill("kp", 1, 0).is_ok());
    assert!(source.refusals().is_empty());
}

/// A topic with no diagnostic exemplar reads as `Missing`.
#[test]
fn a_topic_with_no_diagnostic_exemplar_is_missing() {
    let all = states();
    let yaml = topic_yaml(&all[2], AnswerKind::Numeric);
    let cut = yaml.find("    diagnostic_exemplar:").expect("diagnostic");
    let tree = write_tree("verdict-policy-missing", &yaml[..cut]);
    let (curriculum, _) = load_curriculum(tree.root()).expect("the tree loads");
    let index = ReadinessIndex::build(&curriculum);
    let coverage = PrereqCoverage::build(&curriculum, &index);
    assert_eq!(coverage.course("t")[0].diagnostic, DiagnosticState::Missing);
}

/// V of a KP is the count of verdict exemplars with different problem
/// statements, and the last one is the held-out item.
#[test]
fn v_counts_each_problem_statement_one_time() {
    let mut yaml = "  - id: v-topic\n    name: v-topic\n    core: true\n    difficulty: 0.1\n    drill: false\n    answer_kind: numeric\n    expected_time_secs: 40\n    knowledge_points:\n      - id: kp1\n        name: kp1\n        key_prerequisites: []\n        exemplars:\n".to_owned();
    for (problem, answer) in [
        ("a", "1"),
        ("a", "1"),
        ("b", "2"),
        ("c", "many words"),
        ("d", "4"),
    ] {
        yaml.push_str(&format!(
            "          - problem: 'Find {problem}.'\n            answer: \"{answer}\"\n            solution_sketch: 'Count.'\n"
        ));
    }
    let tree = write_tree("verdict-policy-v", &yaml);
    let (curriculum, _) = load_curriculum(tree.root()).expect("the tree loads");
    let index = ReadinessIndex::build(&curriculum);
    let facts = index.get("v-topic/kp1").expect("facts");
    assert_eq!(facts.decidable, vec![0, 2, 4]);
    assert_eq!(facts.held_out, Some(4));
    assert_eq!(facts.practice_exemplars(), 2);
}
