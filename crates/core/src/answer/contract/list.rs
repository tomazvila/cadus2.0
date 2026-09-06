//! Flat homogeneous lists retain multiplicity and their authored order policy.

use super::{AnswerContract, Canon, Undecidable, check_contract};
use crate::answer::{Outcome, Verdict};

pub(super) fn validate(ordered: bool, member: &AnswerContract) -> Result<(), Undecidable> {
    if matches!(
        member,
        AnswerContract::None | AnswerContract::Multipart { .. } | AnswerContract::List { .. }
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
        )
    {
        return Err(Undecidable::new(
            "unordered lists require exact member policies",
        ));
    }
    member.validate()
}

fn values(text: &str) -> Result<Vec<&str>, Undecidable> {
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
            1
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
    let learner = values(learner_text)?;
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
