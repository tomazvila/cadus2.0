use serde_norway::Value;

use super::{Checker, validate};
use crate::curriculum::{KnowledgePoint, StepRef};

const SOURCE: &str = r#"
id: kp1
name: Two-step equations
exemplars:
  - problem: "Solve $2x + 3 = 11$."
    answer: "4"
step_check:
  step: 'Subtract $3$'
  question: 'Why is $3$ subtracted before dividing by $2$?'
  options:
    - 'Undo the last operation first'
    - 'Division always comes first'
    - 'Subtraction always comes first'
  answer: 'Undo the last operation first'
  why: 'Building $2x + 3$ multiplies first and adds last, so solving undoes them in reverse.'
try_first:
  problem: 'Solve $5x + 1 = 21$.'
  answer_contract: {"kind":"exact"}
  answer: "4"
  reveal: 'Undoing the operations in reverse order solved it.'
"#;

fn findings(source: &str) -> Vec<String> {
    let document: Value = serde_norway::from_str(source).unwrap();
    validate::<KnowledgePoint, _>(&document, "unit.yaml", Checker::check_knowledge_point)
        .err()
        .unwrap_or_default()
        .into_iter()
        .map(|finding| finding.message)
        .collect()
}

/// Both blocks are optional and read into the typed knowledge point.
#[test]
fn the_active_example_blocks_load() {
    let document: Value = serde_norway::from_str(SOURCE).unwrap();
    let point =
        validate::<KnowledgePoint, _>(&document, "unit.yaml", Checker::check_knowledge_point)
            .expect("the blocks read");
    let check = point.step_check.expect("the step check is retained");
    assert_eq!(check.step, StepRef::Text("Subtract $3$".to_owned()));
    assert_eq!(check.options.len(), 3);
    assert_eq!(point.try_first.expect("try_first is retained").answer, "4");

    let plain = SOURCE.split("step_check:").next().unwrap();
    assert!(findings(plain).is_empty());
    let numbered = SOURCE.replace("step: 'Subtract $3$'", "step: 2");
    assert!(findings(&numbered).is_empty());
}

/// An answer that is not one of the options fails with a message naming it.
#[test]
fn an_answer_outside_the_options_fails_clearly() {
    let bad = SOURCE.replace(
        "answer: 'Undo the last operation first'",
        "answer: 'Undo the last step first'",
    );
    let found = findings(&bad);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0]
            .contains("step_check.answer \"Undo the last step first\" is not one of its options"),
        "{found:?}"
    );
}

/// Option counts, step zero, unknown keys, an ungradable try-first answer and
/// a try-first problem that repeats an exemplar are all refused.
#[test]
fn malformed_blocks_are_refused() {
    let cases = [
        (
            SOURCE.replace("    - 'Subtraction always comes first'\n", ""),
            "give 3 to 4 options, not 2",
        ),
        (
            SOURCE.replace("step: 'Subtract $3$'", "step: 0"),
            "steps are numbered from 1",
        ),
        (
            SOURCE.replace("  why: 'Building", "  hint: 'x'\n  why: 'Building"),
            "unknown field `hint`",
        ),
        (
            SOURCE.replace("answer: \"4\"\n  reveal", "answer: \"4 +\"\n  reveal"),
            "try_first.answer \"4 +\" does not grade",
        ),
        (
            SOURCE.replace(
                "problem: 'Solve $5x + 1 = 21$.'",
                "problem: \"Solve $2x + 3 = 11$.\"",
            ),
            "try_first.problem repeats an exemplar",
        ),
    ];
    for (source, expected) in cases {
        let found = findings(&source);
        assert!(
            found.iter().any(|message| message.contains(expected)),
            "expected {expected:?} in {found:?}"
        );
    }
}
