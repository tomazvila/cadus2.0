//! Small, bounded notation contracts used by reviewed Foundations items.

use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive};

use super::{AnswerContract, Canon, Undecidable, canonical_form};
use crate::answer::normalize;

/// Read the authored answer of a ratio contract or of a chain contract.
pub(super) fn expected(contract: &AnswerContract, text: &str) -> Result<Canon, Undecidable> {
    if matches!(contract, AnswerContract::ReducedRatio) {
        reduced_ratio(text)
    } else {
        ascending_chain(text)
    }
}

/// Read a positive, reduced integer ratio written with one colon.
pub(super) fn reduced_ratio(text: &str) -> Result<Canon, Undecidable> {
    let source = normalize(text).source;
    let mut fields = source.split(':');
    let left = integer(fields.next().unwrap_or_default())?;
    let right = integer(fields.next().unwrap_or_default())?;
    if fields.next().is_some()
        || !left.is_positive()
        || !right.is_positive()
        || left.gcd(&right) != BigInt::one()
    {
        return Err(ratio_refusal());
    }
    Ok(Canon::Tuple(vec![
        Canon::Rational(BigRational::from_integer(left)),
        Canon::Rational(BigRational::from_integer(right)),
    ]))
}

/// Read the learner's ratio: `4:25`, and also the spellings `4/25` and `4 to 25`
/// of the same two numbers. The value must still be two coprime positive integers.
pub(super) fn learner_ratio(text: &str) -> Result<Canon, Undecidable> {
    reduced_ratio(&colon_spelling(text))
}

/// The text with a slash or the word `to` between two integers written as a colon.
fn colon_spelling(text: &str) -> String {
    let source = normalize(text).source;
    let plain = source.trim().trim_end_matches('.').trim();
    let joined = plain.replace(" to ", ":");
    let joined = joined.trim();
    match joined.split_once('/') {
        Some((left, right))
            if !right.contains(['/', ':']) && integer(left).is_ok() && integer(right).is_ok() =>
        {
            format!("{}:{}", left.trim(), right.trim())
        }
        _ => joined.to_owned(),
    }
}

/// Whether the learner supplied exactly two integer fields separated by one colon.
pub(super) fn recognizes_ratio(text: &str) -> bool {
    let source = colon_spelling(text);
    let mut fields = source.split(':');
    let recognized = fields.next().is_some_and(|field| integer(field).is_ok())
        && fields.next().is_some_and(|field| integer(field).is_ok());
    recognized && fields.next().is_none()
}

fn integer(text: &str) -> Result<BigInt, Undecidable> {
    text.trim().parse().map_err(|_| ratio_refusal())
}

fn ratio_refusal() -> Undecidable {
    Undecidable::new(
        "a reduced ratio requires two coprime positive integers separated by one colon",
    )
}

/// Read two to 16 strictly ascending exact rational values joined by `<`.
pub(super) fn ascending_chain(text: &str) -> Result<Canon, Undecidable> {
    let source = normalize(text).source;
    let fields: Vec<_> = source.split('<').map(str::trim).collect();
    if !(2..=16).contains(&fields.len()) || fields.iter().any(|field| field.is_empty()) {
        return Err(chain_refusal());
    }
    let values = fields
        .into_iter()
        .map(canonical_form)
        .map(|value| match value {
            Ok(value @ (Canon::Rational(_) | Canon::Radical(_))) => Ok(value),
            _ => Err(chain_refusal()),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let approximations: Vec<f64> = values
        .iter()
        .map(|value| approximate(value).ok_or_else(chain_refusal))
        .collect::<Result<_, _>>()?;
    if approximations.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(chain_refusal());
    }
    Ok(Canon::List(values))
}

/// Whether the learner supplied two to 16 exact rational fields joined by `<`.
pub(super) fn recognizes_chain(text: &str) -> bool {
    let source = normalize(text).source;
    let fields: Vec<_> = source.split('<').map(str::trim).collect();
    (2..=16).contains(&fields.len())
        && fields.iter().all(|field| {
            matches!(
                canonical_form(field),
                Ok(Canon::Rational(_) | Canon::Radical(_))
            )
        })
}

/// The decimal value of a rational or a root combination, to order two values.
fn approximate(value: &Canon) -> Option<f64> {
    match value {
        Canon::Rational(number) => number.to_f64(),
        Canon::Radical(parts) => {
            let mut sum = 0.0_f64;
            for (basis, coefficient) in parts {
                sum += coefficient.to_f64()?
                    * basis.radicand.to_f64()?.sqrt()
                    * std::f64::consts::PI.powi(i32::try_from(basis.pi).ok()?)
                    * std::f64::consts::E.powi(i32::try_from(basis.e).ok()?);
            }
            Some(sum).filter(|sum| sum.is_finite())
        }
        _ => None,
    }
}

fn chain_refusal() -> Undecidable {
    Undecidable::new("an ascending chain requires two to 16 strictly increasing rational values")
}
