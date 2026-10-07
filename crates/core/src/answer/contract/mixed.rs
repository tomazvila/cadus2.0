//! The mixed number as a required form: a whole number, a separator, and a
//! proper fraction in lowest terms. The module also writes the notation hint
//! of a mixed number item whose learner text has the right value in a wrong form.

use num_integer::Integer;
use num_traits::{One, Zero};

use super::{AnswerContract, NumericForm};
use crate::answer::{Ast, Outcome, normalize, parse};

/// Whether `tree` is a mixed number: `4 2/5`, `4⅖`, `4+2/5`, `4_2/5`, `4 and 2/5`,
/// each with a proper fraction in lowest terms, and a minus sign in front.
pub(super) fn is_mixed_number(tree: &Ast) -> bool {
    let node = match tree {
        Ast::Neg(inner) => inner.as_ref(),
        other => other,
    };
    match node {
        Ast::Mixed {
            numerator,
            denominator,
            ..
        } => numerator.gcd(denominator).is_one(),
        Ast::Add(terms) if !matches!(tree, Ast::Neg(_)) => match terms.as_slice() {
            [
                Ast::Integer(_),
                Ast::Fraction {
                    numerator,
                    denominator,
                },
            ] => {
                !numerator.is_zero()
                    && numerator < denominator
                    && numerator.gcd(denominator).is_one()
            }
            _ => false,
        },
        _ => false,
    }
}

/// The notation hint of a wrong `mixed_number` answer, or `None`.
///
/// Two cases carry a hint. `a * b/c` (or `a x b/c`) means `a` times `b/c`, so
/// the verdict stays wrong; when `a + b/c` is the key, the hint says so. A text
/// that has the value of the key and is no mixed number gets the form hint.
pub(super) fn hint(expected: &str, learner: &str, contract: &AnswerContract) -> Option<String> {
    let AnswerContract::RequiredForm {
        form: NumericForm::MixedNumber,
    } = contract
    else {
        return None;
    };
    if !matches!(
        super::check_contract(expected, learner, contract.clone()),
        Outcome::Decided(verdict) if !verdict.correct
    ) {
        return None;
    }
    if let Some((whole, numerator, denominator)) = times_reading(learner) {
        let spelled = format!("{whole} {numerator}/{denominator}");
        let mixed = matches!(
            super::check_contract(expected, &spelled, contract.clone()),
            Outcome::Decided(verdict) if verdict.correct
        );
        if mixed {
            let typed = learner.trim();
            let glyph = glyph(&numerator, &denominator).map_or_else(
                || format!("{whole} and {numerator}/{denominator}"),
                |glyph| format!("{whole}{glyph}"),
            );
            return Some(format!(
                "{typed} means {whole} times {numerator}/{denominator}. \
                 For the mixed number {glyph} write {spelled}."
            ));
        }
        return None;
    }
    let same_value = matches!(
        super::check_contract(expected, learner, AnswerContract::Exact),
        Outcome::Decided(verdict) if verdict.correct
    );
    same_value.then(|| {
        format!(
            "Write the mixed number as a whole number, a space, then the fraction, like {}",
            expected.trim()
        )
    })
}

/// The parts of `a * b/c`, `a x b/c`, `a × b/c`: a whole number times a fraction.
fn times_reading(learner: &str) -> Option<(String, String, String)> {
    let source = normalize(learner).source;
    let (head, tail) = split_times(&source)?;
    let (numerator, denominator) = tail.split_once('/')?;
    let digits = |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    let (head, numerator, denominator) = (head.trim(), numerator.trim(), denominator.trim());
    // The reading must stand as a tree of the grammar as well.
    parse(&source).ok()?;
    (digits(head) && digits(numerator) && digits(denominator)).then(|| {
        (
            head.to_owned(),
            numerator.to_owned(),
            denominator.to_owned(),
        )
    })
}

/// Split at the one times sign: `*`, or a spaced `x` or `X`.
fn split_times(source: &str) -> Option<(&str, &str)> {
    if let Some(parts) = source.split_once('*') {
        return Some(parts);
    }
    ["x", "X"].iter().find_map(|letter| {
        let at = source.find(&format!(" {letter} "))?;
        Some((&source[..at], &source[at + 3..]))
    })
}

/// The vulgar glyph of a fraction, when the lexer table holds one.
fn glyph(numerator: &str, denominator: &str) -> Option<char> {
    crate::answer::lexer::vulgar_glyph(numerator, denominator)
}
