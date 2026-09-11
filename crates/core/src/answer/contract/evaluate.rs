//! Exact arithmetic for explicit answer policies.

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::Signed;

use super::structured::{label_value, named_parts, tolerance_value, validate_shape};
use super::{AnswerContract, AnswerPart, Canon, Undecidable, bounded, canonical_form};
use crate::answer::{Outcome, Rounding, Verdict, rounds_to, same_answer};

/// Decide the authored policy with exact arithmetic and bounded input.
#[must_use]
pub fn check_contract(expected: &str, learner: &str, contract: AnswerContract) -> Outcome {
    match contract.validate_expected(expected) {
        Ok(value) => grade(&value, expected, learner, &contract),
        Err(reason) => Outcome::Undecidable(reason),
    }
}

fn grade(expected: &Canon, text: &str, learner: &str, contract: &AnswerContract) -> Outcome {
    if let Err(reason) = bounded(learner) {
        return Outcome::Undecidable(reason);
    }
    if learner.trim().is_empty() {
        return decided(false);
    }
    if let Some(outcome) = structured_contract(expected, text, learner, contract) {
        return outcome;
    }
    let required_form = !matches!(contract, AnswerContract::RequiredForm { form } if !super::form::accepts(*form, learner));
    let learner = match canonical_form(learner) {
        Ok(value) => value,
        Err(reason) => return Outcome::Undecidable(reason),
    };
    if !required_form || !validate_shape(contract, &learner) {
        return decided(false);
    }
    match contract {
        AnswerContract::Approx { decimals } => approximate(expected, &learner, *decimals),
        AnswerContract::Tolerance { tolerance } => {
            absolute_tolerance(expected, &learner, tolerance)
        }
        _ => decided(same_answer(expected, &learner)),
    }
}

/// Grade contracts whose learner text has a dedicated parser.
fn structured_contract(
    expected: &Canon,
    text: &str,
    learner: &str,
    contract: &AnswerContract,
) -> Option<Outcome> {
    let outcome = match contract {
        AnswerContract::RequiredAssignment => required_assignment(text, learner),
        AnswerContract::Label { options } => {
            decided(label_value(options, learner).as_ref() == Some(expected))
        }
        AnswerContract::Multipart { parts } => multipart(parts, text, learner),
        AnswerContract::List { ordered, member } => {
            super::list::grade(*ordered, member, text, learner)
        }
        AnswerContract::InequalityUnion => match super::union::read(learner) {
            Ok(value) => decided(super::union::equivalent(expected, &value)),
            Err(reason) => Outcome::Undecidable(reason),
        },
        AnswerContract::RequiredInequalityNotation => {
            match (
                super::union::read_with_notation(text),
                super::union::read_with_notation(learner),
            ) {
                (Ok((_, expected_notation)), Ok((value, learner_notation))) => decided(
                    expected_notation == learner_notation
                        && super::union::equivalent(expected, &value),
                ),
                (Err(reason), _) | (_, Err(reason)) => Outcome::Undecidable(reason),
            }
        }
        contract @ (AnswerContract::RequiredSinglePower
        | AnswerContract::RequiredNormalizedScientificNotation
        | AnswerContract::RequiredSimplestRadical) => {
            required_expression_form(contract, text, learner)
        }
        AnswerContract::ReducedRatio => parsed_or_recognized(
            super::notation::reduced_ratio(learner),
            expected,
            super::notation::recognizes_ratio(learner),
        ),
        AnswerContract::AscendingChain => parsed_or_recognized(
            super::notation::ascending_chain(learner),
            expected,
            super::notation::recognizes_chain(learner),
        ),
        AnswerContract::PolynomialRelation => parsed(super::relation::read(learner), expected),
        AnswerContract::RelationSetup => parsed(super::setup::read(learner), expected),
        _ => return None,
    };
    Some(outcome)
}

fn required_assignment(expected: &str, learner: &str) -> Outcome {
    match super::assignment::equivalent(expected, learner) {
        Ok(correct) => decided(correct),
        Err(reason) => Outcome::Undecidable(reason),
    }
}

fn required_expression_form(contract: &AnswerContract, expected: &str, learner: &str) -> Outcome {
    let result = match contract {
        AnswerContract::RequiredSinglePower => super::power::equivalent(expected, learner),
        AnswerContract::RequiredNormalizedScientificNotation => {
            super::scientific::equivalent(expected, learner)
        }
        AnswerContract::RequiredSimplestRadical => super::radical::equivalent(expected, learner),
        _ => unreachable!("caller supplies a required expression contract"),
    };
    match result {
        Ok(correct) => decided(correct),
        Err(reason) => Outcome::Undecidable(reason),
    }
}

fn parsed(value: Result<Canon, Undecidable>, expected: &Canon) -> Outcome {
    match value {
        Ok(value) => decided(&value == expected),
        Err(reason) => Outcome::Undecidable(reason),
    }
}

fn parsed_or_recognized(
    value: Result<Canon, Undecidable>,
    expected: &Canon,
    recognized: bool,
) -> Outcome {
    match value {
        Ok(value) => decided(&value == expected),
        Err(_) if recognized => decided(false),
        Err(reason) => Outcome::Undecidable(reason),
    }
}

fn multipart(parts: &[AnswerPart], expected: &str, learner: &str) -> Outcome {
    let Some(expected) = named_parts(parts, expected) else {
        return refused_parts();
    };
    let Some(learner) = named_parts(parts, learner) else {
        return decided(false);
    };
    let mut correct = true;
    for ((part, expected), learner) in parts.iter().zip(expected).zip(learner) {
        match check_contract(expected, learner, part.contract.clone()) {
            Outcome::Decided(verdict) => correct &= verdict.correct,
            undecidable => return undecidable,
        }
    }
    decided(correct)
}

fn refused_parts() -> Outcome {
    Outcome::Undecidable(Undecidable::new(
        "each named answer part must occur exactly once",
    ))
}

fn decided(correct: bool) -> Outcome {
    Outcome::Decided(Verdict {
        correct,
        notation: false,
    })
}

fn absolute_tolerance(expected: &Canon, learner: &Canon, text: &str) -> Outcome {
    let (Canon::Rational(expected), Canon::Rational(learner)) = (expected, learner) else {
        return decided(false);
    };
    match tolerance_value(text) {
        Ok(tolerance) => decided((expected - learner).abs() <= tolerance),
        Err(reason) => Outcome::Undecidable(reason),
    }
}

fn approximate(expected: &Canon, learner: &Canon, decimals: u8) -> Outcome {
    let Canon::Rational(value) = learner else {
        return decided(false);
    };
    let scale = u32::from(decimals);
    let grid = BigRational::from_integer(BigInt::from(10_u32).pow(scale));
    // Reject extra digits before the integral comparison in the round helper.
    if !(value * grid).is_integer() {
        return decided(false);
    }
    match rounds_to(expected, value, scale) {
        Rounding::Same => decided(true),
        Rounding::Different => decided(false),
        Rounding::NotANumber => {
            Outcome::Undecidable(Undecidable::new("the answer contract requires a number"))
        }
        Rounding::Refused(reason) => Outcome::Undecidable(Undecidable::new(reason)),
    }
}
