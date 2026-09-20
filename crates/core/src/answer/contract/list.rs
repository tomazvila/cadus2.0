//! Flat homogeneous lists retain multiplicity and their authored order policy.

use super::{AnswerContract, Canon, Undecidable, check_contract};
use crate::answer::{Outcome, Verdict};

pub(super) fn validate(ordered: bool, member: &AnswerContract) -> Result<(), Undecidable> {
    if matches!(
        member,
        AnswerContract::None
            | AnswerContract::RequiredAssignment
            | AnswerContract::RequiredSimplestRadical
            | AnswerContract::Multipart { .. }
            | AnswerContract::List { .. }
    ) {
        return Err(Undecidable::new(
            "a list requires a flat deterministic member contract",
        ));
    }
    if !ordered
        && matches!(
            member,
            AnswerContract::Approx { .. }
                | AnswerContract::Tolerance { .. }
                | AnswerContract::RequiredForm { .. }
                | AnswerContract::RequiredInequalityNotation
                | AnswerContract::RequiredSinglePower
                | AnswerContract::RequiredNormalizedScientificNotation
                | AnswerContract::Function { .. }
        )
    {
        return Err(Undecidable::new(
            "unordered lists require exact member policies",
        ));
    }
    member.validate()
}

fn values(text: &str) -> Result<Vec<&str>, Undecidable> {
    values_with(text, false)
}

/// Split a list, folding a glued three-digit thousands group into one member.
///
/// The plain [`values`] split reads every comma as a list separator, so
/// `89, 698, 712, 1,205` splits into five members and an ordering answer can
/// never match its four-item authored answer. This split keeps the comma in the
/// member when it carries no space and three digits follow it, so the member
/// reads as one grouped number (`1,205`) the way the whole-answer rule of the V4
/// table reads it. A following digit or a space makes it a separator again, so
/// `1, 205` stays two members.
fn values_grouped(text: &str) -> Result<Vec<&str>, Undecidable> {
    values_with(text, true)
}

fn values_with(text: &str, merge_thousands: bool) -> Result<Vec<&str>, Undecidable> {
    let text = text.trim();
    if text.ends_with(" and") || text.starts_with("and ") {
        return Err(bad_list());
    }
    let text = text
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .unwrap_or(text);
    let mut parts = Vec::new();
    let mut depth = 0_usize;
    let mut start = 0;
    let mut skip_until = 0;
    for (at, ch) in text.char_indices() {
        if at < skip_until {
            continue;
        }
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.checked_sub(1).ok_or_else(bad_list)?,
            _ => {}
        }
        let width = if depth == 0 && ch == ',' {
            if merge_thousands && is_thousands_comma(text, at) {
                0
            } else {
                1
            }
        } else if depth == 0 && text[at..].starts_with(" and ") {
            5
        } else {
            0
        };
        if width != 0 {
            parts.push(text[start..at].trim());
            start = at + width;
            skip_until = start;
        }
    }
    parts.push(text[start..].trim());
    if depth != 0 || parts.len() > 32 || parts.iter().any(|part| part.is_empty()) {
        return Err(bad_list());
    }
    Ok(parts)
}

/// Whether the comma at `at` is the separator of a glued three-digit group.
///
/// The comma must have one to three digits in front of it and exactly three
/// digits immediately behind it, with no space after it. That is the group shape
/// of the V4 table, and it is the same shape [`crate::answer::parse`] folds in a
/// bare comma list.
fn is_thousands_comma(text: &str, at: usize) -> bool {
    let bytes = text.as_bytes();
    let mut left = 0_usize;
    let mut cursor = at;
    while cursor > 0 && bytes[cursor - 1].is_ascii_digit() {
        left += 1;
        cursor -= 1;
    }
    if !(1..=3).contains(&left) {
        return false;
    }
    let after = bytes.get(at + 1..).unwrap_or_default();
    if after.len() < 3 || !after[..3].iter().all(u8::is_ascii_digit) {
        return false;
    }
    !matches!(after.get(3), Some(byte) if byte.is_ascii_digit())
}

fn bad_list() -> Undecidable {
    Undecidable::new("a list requires one to 32 complete members")
}

pub(super) fn expected(
    ordered: bool,
    member: &AnswerContract,
    text: &str,
) -> Result<Canon, Undecidable> {
    let mut items = values(text)?
        .into_iter()
        .map(|part| member.validate_expected(part))
        .collect::<Result<Vec<_>, _>>()?;
    if !ordered {
        items.sort();
    }
    Ok(Canon::List(items))
}

pub(super) fn grade(
    ordered: bool,
    member: &AnswerContract,
    expected_text: &str,
    learner: &str,
) -> Outcome {
    let result = if ordered {
        ordered_grade(member, expected_text, learner)
    } else {
        expected(false, member, expected_text)
            .and_then(|value| expected(false, member, learner).map(|learner| value == learner))
    };
    match result {
        Ok(correct) => Outcome::Decided(Verdict {
            correct,
            notation: false,
        }),
        Err(reason) => Outcome::Undecidable(reason),
    }
}

fn ordered_grade(
    member: &AnswerContract,
    expected_text: &str,
    learner_text: &str,
) -> Result<bool, Undecidable> {
    let expected = values(expected_text)?;
    let mut learner = values(learner_text)?;
    // A learner who writes the displayed thousands notation (`1,205`) splits
    // into more members than the authored answer. When the grouped split alone
    // restores the authored count, that is the one reading of the answer and it
    // is graded. Otherwise the plain split stands, so no answer gains a second
    // reading by accident.
    if expected.len() != learner.len()
        && let Ok(grouped) = values_grouped(learner_text)
        && grouped.len() == expected.len()
    {
        learner = grouped;
    }
    if expected.len() != learner.len() {
        let sample = expected.first().ok_or_else(bad_list)?;
        for given in learner {
            if let Outcome::Undecidable(reason) = check_contract(sample, given, member.clone()) {
                return Err(reason);
            }
        }
        return Ok(false);
    }
    let mut correct = true;
    for (expected, learner) in expected.into_iter().zip(learner) {
        match check_contract(expected, learner, member.clone()) {
            Outcome::Decided(verdict) => correct &= verdict.correct,
            Outcome::Undecidable(reason) => return Err(reason),
        }
    }
    Ok(correct)
}
