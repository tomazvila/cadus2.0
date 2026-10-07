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

/// The name of a part as a learner writes it: underscores become spaces.
fn spoken(name: &str) -> String {
    name.replace('_', " ")
}

/// The initials of a part name, "leading_coefficient" -> "lc".
fn initials(name: &str) -> String {
    name.split('_')
        .filter_map(|word| word.chars().next())
        .collect()
}

/// The part one comma-separated piece of a multipart answer names, with the
/// rest of the piece after the label. The label is the part name, the same
/// name with spaces, or its initials when they are unique among the parts.
fn labelled<'a>(names: &'a [String], piece: &str) -> Option<(&'a str, String)> {
    let lower = piece.trim().to_lowercase();
    for name in names {
        let unique_initials = names
            .iter()
            .filter(|n| initials(n) == initials(name))
            .count()
            == 1;
        let mut labels = vec![name.to_lowercase(), spoken(name).to_lowercase()];
        if unique_initials && initials(name).len() > 1 {
            labels.push(initials(name).to_lowercase());
        }
        // The longest label first, so "leading coefficient" beats "lead".
        labels.sort_by_key(|label| std::cmp::Reverse(label.len()));
        for label in labels {
            if let Some(rest) = lower.strip_prefix(&label)
                && rest.starts_with(|c: char| c.is_whitespace() || c == '=' || c == ':')
            {
                let value =
                    rest.trim_start_matches(|c: char| c.is_whitespace() || c == '=' || c == ':');
                return Some((name.as_str(), value.trim().to_owned()));
            }
        }
    }
    None
}

/// Rewrite a multipart answer that labels its parts loosely ("degree 3, lc 4",
/// "leading coefficient: 4") into the `name = value; name = value` spelling.
/// Returns `None` unless every piece carries a label of a distinct part.
pub(super) fn part_labels(contract: &AnswerContract, answer: &str) -> Option<String> {
    let AnswerContract::Multipart { parts } = contract else {
        return None;
    };
    let names: Vec<String> = parts.iter().map(|part| part.name.clone()).collect();
    let pieces: Vec<&str> = answer.split([',', ';']).collect();
    if pieces.len() != names.len() || answer.chars().count() > MAX_ANSWER_CHARS {
        return None;
    }
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for piece in pieces {
        let (name, value) = labelled(&names, piece)?;
        if value.is_empty() || !seen.insert(name) {
            return None;
        }
        out.push(format!("{name} = {value}"));
    }
    Some(out.join("; "))
}
