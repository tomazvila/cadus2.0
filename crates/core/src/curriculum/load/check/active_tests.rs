use serde_norway::Value;

use super::{Checker, validate};
use crate::curriculum::KnowledgePoint;

const SOURCE: &str = r#"
id: kp1
name: Two-step equations
exemplars:
  - problem: "Solve $2x + 3 = 11$."
    answer: "4"
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

/// The block is optional and reads into the typed knowledge point.
#[test]
fn the_try_first_block_loads() {
    let document: Value = serde_norway::from_str(SOURCE).unwrap();
    let point =
        validate::<KnowledgePoint, _>(&document, "unit.yaml", Checker::check_knowledge_point)
            .expect("the blocks read");
    assert_eq!(point.try_first.expect("try_first is retained").answer, "4");
}

/// An unknown key, an ungradable try-first answer and a try-first problem
/// that repeats an exemplar are all refused.
#[test]
fn malformed_blocks_are_refused() {
    let cases = [
        (
            SOURCE.replace("  reveal:", "  hint: 'x'\n  reveal:"),
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
