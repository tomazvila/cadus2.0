//! Source-scoped input representations; mathematical decisions remain in core.

use std::collections::BTreeSet;

use cadus_core::answer::{AnswerContract, MAX_ANSWER_CHARS, Outcome, check};
use cadus_core::curriculum::AnswerKind;

use crate::state::ServedProblem;

const MAX_MEMBERS: usize = 32;

/// Convert integer factor-pair notation into a list for the captured checker.
pub(super) fn factor_pairs(
    served: &ServedProblem,
    answer: &str,
    kind: AnswerKind,
) -> Option<String> {
    if served.serve_topic.as_deref().or(served.topic.as_deref()) != Some("factors-and-multiples")
        || served.kp.as_deref() != Some("kp1")
        || !matches!(kind, AnswerKind::Numeric | AnswerKind::Expression)
        || !matches!(
            served.expected.answer_contract.as_ref(),
            None | Some(AnswerContract::Exact) | Some(AnswerContract::List { ordered: false, .. })
        )
        || answer.chars().count() > MAX_ANSWER_CHARS
        || served.expected.answer.chars().count() > MAX_ANSWER_CHARS
    {
        return None;
    }

    let expected = served
        .expected
        .answer
        .split(',')
        .map(positive_integer)
        .collect::<Option<Vec<_>>>()?;
    let unique_expected: BTreeSet<_> = expected.iter().copied().collect();
    if expected.len() > MAX_MEMBERS || expected.len() != unique_expected.len() {
        return None;
    }
    let target = expected.iter().max()?.to_string();
    let notation = answer
        .replace("\\times", "*")
        .replace("\\cdot", "*")
        .replace(['x', 'X', '\u{00d7}', '\u{00b7}'], "*");
    let mut pairs = BTreeSet::new();
    let mut factors = BTreeSet::new();
    for pair in notation.split(',') {
        let mut operands = pair.split('*');
        let left = positive_integer(operands.next()?)?;
        let right = positive_integer(operands.next()?)?;
        if operands.next().is_some()
            || !pairs.insert((left.min(right), left.max(right)))
            || pairs.len() > MAX_MEMBERS
            || !matches!(
                check(&target, &format!("{left}*{right}"), AnswerKind::Numeric),
                Outcome::Decided(verdict) if verdict.correct
            )
        {
            return None;
        }
        factors.insert(left);
        factors.insert(right);
    }

    // Preserve authored order; missing or extra factors still reach the checker.
    let mut listed = Vec::new();
    for value in expected {
        if factors.remove(&value) {
            listed.push(value.to_string());
        }
    }
    listed.extend(factors.into_iter().map(|value| value.to_string()));
    Some(listed.join(", "))
}

fn positive_integer(text: &str) -> Option<u64> {
    let text = text.trim();
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse::<u64>().ok().filter(|value| *value > 0)
}

/// Read the number of a counted answer ("8 packs") to a "how many" question.
///
/// Core decides which words may fall away (`cadus_core::answer::count_answer`);
/// this adapter only supplies the served question and keeps the contract.
pub(super) fn count_noun(served: &ServedProblem, answer: &str, kind: AnswerKind) -> Option<String> {
    if !matches!(kind, AnswerKind::Numeric | AnswerKind::Expression)
        || !matches!(
            served.expected.answer_contract.as_ref(),
            None | Some(AnswerContract::Exact)
        )
    {
        return None;
    }
    cadus_core::answer::count_answer(&served.text, &served.expected.answer, answer)
}

/// Read a measured answer without its decoration ("A = (x+4)(x+5) m^2",
/// "≈7.1", "7.1 m"): core decides what may fall away from the served question;
/// the stripped text is graded under the item's own contract.
pub(super) fn measured_noun(
    served: &ServedProblem,
    answer: &str,
    kind: AnswerKind,
) -> Option<String> {
    if !matches!(kind, AnswerKind::Numeric | AnswerKind::Expression)
        || !matches!(
            served.expected.answer_contract.as_ref(),
            None | Some(
                AnswerContract::Exact
                    | AnswerContract::Approx { .. }
                    | AnswerContract::Tolerance { .. }
                    | AnswerContract::RequiredForm { .. }
                    | AnswerContract::RequiredSimplestRadical
                    | AnswerContract::RequiredNormalizedScientificNotation
            )
        )
    {
        return None;
    }
    cadus_core::answer::measured_answer(&served.text, &served.expected.answer, answer)
}
