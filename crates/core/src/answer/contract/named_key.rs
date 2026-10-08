//! A key written as an equation: `f(x) = 3x - 10`, `h = 5t^2`, `x = 9 - 4y`.
//!
//! The strict reading of the key and of the learner text decides first. This
//! reading runs only when that fails to grade the answer correct, or when the
//! key does not read at all. It never turns a correct verdict into a wrong one.

use super::prefix::name_and_value;
use super::{AnswerContract, NumericForm, check_contract, relation};
use crate::answer::{Outcome, Undecidable, Verdict};

/// The verdict of the equation-key readings, or `None` when none of them applies.
pub(super) fn rescue(key: &str, learner: &str, contract: &AnswerContract) -> Option<Outcome> {
    match contract {
        AnswerContract::Exact => same_equation(key, learner)
            .or_else(|| same_vector_line(key, learner))
            .or_else(|| spelled_word(key, learner))
            .or_else(|| labelled_list(key, learner))
            .or_else(|| named_key(key, learner, contract))
            .or_else(|| plus_minus_disjunction(key, learner)),
        AnswerContract::Function { .. } => named_key(key, learner, contract),
        AnswerContract::RequiredForm {
            form: NumericForm::ExpandedPlaceValue,
        } => without_thousands_commas(key, learner, contract),
        AnswerContract::RequiredForm { .. } => expression_as_equation(key, learner, contract),
        _ => None,
    }
}

/// The key `name = value` against the learner text with the same name, a call of
/// that name, or no name. A function contract takes any name before the formula.
fn named_key(key: &str, learner: &str, contract: &AnswerContract) -> Option<Outcome> {
    let (name, value) = name_and_value(key)?;
    let learner_value = match name_and_value(learner) {
        Some((learner_name, rest)) => {
            let free = matches!(contract, AnswerContract::Function { .. })
                && !name.contains(|ch: char| ch.is_ascii_digit());
            (free || names_agree(name, learner_name)).then_some(rest)?
        }
        None if learner.contains(['=', '<', '>']) => return None,
        None => learner.trim(),
    };
    match check_contract(value, learner_value, contract.clone()) {
        outcome @ Outcome::Decided(_) => Some(outcome),
        Outcome::Undecidable(_) => None,
    }
}

/// Whether two names stand for the same thing: equal, or a name and a call of it
/// (`h` and `h(t)`). Two calls with different arguments are two values.
fn names_agree(key: &str, learner: &str) -> bool {
    let plain = |name: &str| -> String { name.chars().filter(|ch| !ch.is_whitespace()).collect() };
    let (key, learner) = (plain(key), plain(learner));
    if key == learner {
        return true;
    }
    let (key_call, learner_call) = (key.contains('('), learner.contains('('));
    let base = |name: &str| name.split('(').next().unwrap_or_default().to_owned();
    let numeric_call = key_call && key.contains(|ch: char| ch.is_ascii_digit());
    !numeric_call
        && key_call != learner_call
        && !base(&key).is_empty()
        && base(&key) == base(&learner)
}

/// Two equations are the same when their polynomial sides move to one monic form:
/// `x = 9 - 4y` and `x + 4y = 9`, or `y = 2x + 1` and `2x - y + 1 = 0`.
fn same_equation(key: &str, learner: &str) -> Option<Outcome> {
    let single = |text: &str| text.matches('=').count() == 1 && !text.contains(['<', '>']);
    if !single(key) || !single(learner) {
        return None;
    }
    let (key, learner) = (relation::read(key).ok()?, relation::read(learner).ok()?);
    (key == learner).then(correct)
}

/// A key `y = A or y = B` against the learner `y = ±C`, which writes both.
fn plus_minus_disjunction(key: &str, learner: &str) -> Option<Outcome> {
    if !key.contains(" or ") || learner.matches('±').count() != 1 {
        return None;
    }
    let (before, after) = learner.split_once('±')?;
    let (before, after) = (before.trim_end(), after.trim());
    if after.is_empty() || !before.ends_with('=') || before.matches('=').count() != 1 {
        return None;
    }
    let both = format!("{before} {after} or {before} -({after})");
    match check_contract(key, &both, AnswerContract::Exact) {
        outcome @ Outcome::Decided(_) => Some(outcome),
        Outcome::Undecidable(_) => None,
    }
}

/// Two lines written as vector sums or as named components.
fn same_vector_line(key: &str, learner: &str) -> Option<Outcome> {
    super::vector_line::same_line(key, learner).then(correct)
}

/// A learner who spells the value with one word: `zero` for `0`, `Pi` for `pi`,
/// `z is free` for the key `z`. Only the whole text is read.
fn spelled_word(key: &str, learner: &str) -> Option<Outcome> {
    let text = learner.trim().trim_end_matches('.').trim();
    let lower = text.to_lowercase();
    let digits = [
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
    ];
    let spelled = if let Some(at) = digits.iter().position(|word| *word == lower) {
        at.to_string()
    } else if lower == "pi" {
        "pi".to_owned()
    } else {
        let name = lower
            .strip_suffix(" is free")
            .or_else(|| lower.strip_suffix(" is a free variable"))?;
        (name.chars().count() == 1 && name.chars().all(|ch| ch.is_ascii_alphabetic()))
            .then(|| name.to_owned())?
    };
    match check_contract(key, &spelled, AnswerContract::Exact) {
        outcome @ Outcome::Decided(_) => Some(outcome),
        Outcome::Undecidable(_) => None,
    }
}

fn correct() -> Outcome {
    Outcome::Decided(Verdict {
        correct: true,
        notation: false,
    })
}

/// The learner text with the commas of thousands removed: `4,000 + 500 + 6`.
fn without_thousands_commas(
    key: &str,
    learner: &str,
    contract: &AnswerContract,
) -> Option<Outcome> {
    let chars: Vec<char> = learner.chars().collect();
    let digits = |at: usize| chars.get(at).is_some_and(char::is_ascii_digit);
    let mut plain = String::new();
    for (at, &ch) in chars.iter().enumerate() {
        let grouped = ch == ','
            && at > 0
            && digits(at - 1)
            && (1..=3).all(|step| digits(at + step))
            && !digits(at + 4);
        if !grouped {
            plain.push(ch);
        }
    }
    if plain == learner {
        return None;
    }
    match check_contract(key, &plain, contract.clone()) {
        outcome @ Outcome::Decided(_) => Some(outcome),
        Outcome::Undecidable(_) => None,
    }
}

/// An expression key against the learner equation `expression = 0`: the learner
/// wrote an equation where the key is an expression, so the answer has no verdict.
fn expression_as_equation(key: &str, learner: &str, contract: &AnswerContract) -> Option<Outcome> {
    let (left, right) = learner.split_once('=')?;
    let side = match (left.trim(), right.trim()) {
        (side, "0") | ("0", side) if !side.contains(['=', '<', '>']) => side,
        _ => return None,
    };
    match check_contract(key, side, contract.clone()) {
        Outcome::Decided(verdict) if verdict.correct => Some(Outcome::Undecidable(
            Undecidable::new("the answer is an equation and the authored answer is an expression"),
        )),
        _ => None,
    }
}

/// A bare list key against labelled members: `amplitude 3, period π/2` for `3, pi/2`.
/// A label is a leading word that the reader does not know as a name or a function.
fn labelled_list(key: &str, learner: &str) -> Option<Outcome> {
    if key.contains(['=', ';', '(']) || key.matches(',').count() == 0 {
        return None;
    }
    let pieces: Vec<&str> = learner.split(',').map(str::trim).collect();
    if pieces.len() != key.split(',').count() {
        return None;
    }
    let mut values = Vec::new();
    for piece in pieces {
        let (word, rest) = piece.split_once(' ')?;
        let unknown = word.chars().count() >= 3
            && word.chars().all(char::is_alphabetic)
            && matches!(
                check_contract("0", word, AnswerContract::Exact),
                Outcome::Undecidable(reason)
                    if reason.reason == "a name that is not a function or variable"
            );
        if !unknown || rest.trim().is_empty() {
            return None;
        }
        values.push(rest.trim());
    }
    match check_contract(key, &values.join(", "), AnswerContract::Exact) {
        outcome @ Outcome::Decided(_) => Some(outcome),
        Outcome::Undecidable(_) => None,
    }
}
