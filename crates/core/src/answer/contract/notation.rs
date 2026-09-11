//! Small, bounded notation contracts used by reviewed Foundations items.

use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{One, Signed};

use super::{Canon, Undecidable, canonical_form};
use crate::answer::normalize;

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

/// Whether the learner supplied exactly two integer fields separated by one colon.
pub(super) fn recognizes_ratio(text: &str) -> bool {
    let source = normalize(text).source;
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
            Ok(Canon::Rational(number)) => Ok(number),
            _ => Err(chain_refusal()),
        })
        .collect::<Result<Vec<_>, _>>()?;
    if values.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(chain_refusal());
    }
    Ok(Canon::List(
        values.into_iter().map(Canon::Rational).collect(),
    ))
}

/// Whether the learner supplied two to 16 exact rational fields joined by `<`.
pub(super) fn recognizes_chain(text: &str) -> bool {
    let source = normalize(text).source;
    let fields: Vec<_> = source.split('<').map(str::trim).collect();
    (2..=16).contains(&fields.len())
        && fields
            .iter()
            .all(|field| matches!(canonical_form(field), Ok(Canon::Rational(_))))
}

fn chain_refusal() -> Undecidable {
    Undecidable::new("an ascending chain requires two to 16 strictly increasing rational values")
}
