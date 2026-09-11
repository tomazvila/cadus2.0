//! Shape and vocabulary checks for reviewed policies.

use std::collections::BTreeSet;

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{Signed, Zero};

use super::{AnswerContract, AnswerPart, Canon, NumericForm, Undecidable, canonical_form};

pub(super) fn tolerance_value(text: &str) -> Result<BigRational, Undecidable> {
    if text.len() <= 80
        && let Ok(Canon::Rational(value)) = canonical_form(text)
        && value.is_positive()
    {
        return Ok(value);
    }
    Err(Undecidable::new(
        "the tolerance must be an exact positive rational of at most 80 characters",
    ))
}

pub(super) fn validate_shape(contract: &AnswerContract, value: &Canon) -> bool {
    match contract {
        AnswerContract::Approx { .. } => number(value),
        AnswerContract::Tolerance { .. } => matches!(value, Canon::Rational(_)),
        AnswerContract::RequiredForm { form } => match form {
            NumericForm::FactoredLinear => matches!(value, Canon::Poly(_)),
            _ => matches!(value, Canon::Rational(_)),
        },
        AnswerContract::Unit { quantity, .. } => {
            matches!(value, Canon::Quantity { quantity: actual, .. } if actual == quantity)
        }
        AnswerContract::Coordinates { arity } => {
            matches!(value, Canon::Tuple(items) if items.len() == usize::from(*arity) && items.iter().all(number))
        }
        AnswerContract::QuotientRemainder { divisor } => quotient_shape(value, *divisor),
        AnswerContract::Set => matches!(value, Canon::Set(_)),
        _ => true,
    }
}

fn number(value: &Canon) -> bool {
    matches!(value, Canon::Rational(_) | Canon::Radical(_))
}

fn quotient_shape(value: &Canon, divisor: Option<u64>) -> bool {
    let Canon::Tuple(items) = value else {
        return false;
    };
    let [Canon::Rational(quotient), Canon::Rational(remainder)] = items.as_slice() else {
        return false;
    };
    quotient.is_integer()
        && remainder.is_integer()
        && remainder >= &BigRational::zero()
        && divisor.is_none_or(|value| remainder < &BigRational::from_integer(BigInt::from(value)))
}

fn choice_key(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub(super) fn validate_labels(options: &[Vec<String>]) -> Result<(), Undecidable> {
    let mut keys = BTreeSet::new();
    if options.is_empty() || options.len() > 32 {
        return Err(Undecidable::new(
            "a label contract requires one to 32 choices",
        ));
    }
    for option in options {
        if option.is_empty() || option.len() > 8 {
            return Err(Undecidable::new(
                "a choice requires one to eight explicit aliases",
            ));
        }
        for alias in option {
            let key = choice_key(alias);
            if key.is_empty() || alias.chars().count() > 80 || !keys.insert(key) {
                return Err(Undecidable::new(
                    "choice aliases must be bounded, nonempty, and unique",
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn label_value(options: &[Vec<String>], text: &str) -> Option<Canon> {
    let key = choice_key(text);
    options
        .iter()
        .find(|aliases| aliases.iter().any(|alias| choice_key(alias) == key))
        .and_then(|aliases| aliases.first())
        .map(|alias| Canon::Label(choice_key(alias)))
}

pub(super) fn validate_parts(parts: &[AnswerPart]) -> Result<(), Undecidable> {
    if parts.is_empty() || parts.len() > 16 {
        return Err(Undecidable::new(
            "a multipart answer requires one to 16 parts",
        ));
    }
    let mut names = BTreeSet::new();
    for part in parts {
        if part.name.is_empty()
            || part.name.len() > 32
            || !part
                .name
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
            || !names.insert(&part.name)
        {
            return Err(Undecidable::new(
                "part names must be bounded unique identifiers",
            ));
        }
        if matches!(
            part.contract,
            AnswerContract::RequiredAssignment
                | AnswerContract::RequiredSimplestRadical
                | AnswerContract::Multipart { .. }
                | AnswerContract::None
        ) {
            return Err(Undecidable::new(
                "multipart parts require flat deterministic contracts",
            ));
        }
        part.contract.validate()?;
    }
    Ok(())
}

pub(super) fn named_parts<'a>(parts: &[AnswerPart], text: &'a str) -> Option<Vec<&'a str>> {
    let fields: Vec<_> = text
        .split(';')
        .map(|field| field.trim().split_once('='))
        .collect::<Option<_>>()?;
    if fields.len() != parts.len() {
        return None;
    }
    let mut names = BTreeSet::new();
    if fields.iter().any(|(name, _)| !names.insert(name.trim())) {
        return None;
    }
    parts
        .iter()
        .map(|part| {
            fields
                .iter()
                .find(|(name, _)| name.trim() == part.name)
                .map(|(_, value)| value.trim())
                .filter(|value| {
                    matches!(part.contract, AnswerContract::Label { .. })
                        || !matches!(canonical_form(value), Ok(Canon::Assign { .. }))
                })
        })
        .collect()
}

pub(super) fn multipart_values(parts: &[AnswerPart], text: &str) -> Result<Canon, Undecidable> {
    let values = named_parts(parts, text)
        .ok_or_else(|| Undecidable::new("each named answer part must occur exactly once"))?;
    parts
        .iter()
        .zip(values)
        .map(|(part, value)| {
            part.contract
                .validate_expected(value)
                .map(|value| Canon::Assign {
                    var: part.name.clone(),
                    value: Box::new(value),
                })
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Canon::Tuple)
}
