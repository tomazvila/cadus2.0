//! Parameter domains, their values, and the satisfying-count space (A1, D6).
//!
//! A domain says which values one parameter takes. 1.0 has two forms, an integer
//! range and an explicit choice list (`problem_templates.py:271-292`). 2.0 adds a
//! third, the rational domain, because a knowledge point about fractions needs a
//! fractional parameter and a float never enters an answer (D6): the domain draws
//! a numerator and a denominator and reduces the pair by its greatest common
//! divisor.
//!
//! # Every value is exact
//!
//! [`Value`] holds an exact rational or a text choice. It holds no float, so a
//! rendered statement and a computed answer never carry the `3.00000000000000`
//! of 1.0 (`docs/reference/serving-1.0-spec.md` section 8, trap 3).
//!
//! # Every enumeration is bounded
//!
//! [`Domain::values`] materializes the value list, the way 1.0 does, and it
//! refuses a domain of more than [`MAX_DOMAIN_SIZE`] values instead of building
//! it. The rational domain multiplies two ranges, so the bound is the one place
//! that stops a `1..10000` over `1..10000` document from allocating 100 million
//! values.

use std::collections::{BTreeMap, BTreeSet};

use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{One, Signed, Zero};
use serde::{Deserialize, Serialize};

use super::constraint::{Constraint, ConstraintError, all_hold};

/// The largest count of values one domain holds (1.0 `MAX_DOMAIN_SIZE`).
pub const MAX_DOMAIN_SIZE: u64 = 10_000;

/// The largest count of entries one choice domain holds (1.0 `MAX_CHOICES`).
pub const MAX_CHOICES: usize = 24;

/// The largest tuple count the space walk enumerates (1.0 `EXHAUSTIVE_SPACE_LIMIT`).
///
/// At or under the limit the satisfying count is exact, because the walk visits
/// every tuple once. Above it the count is a rejection-sampling estimate.
pub const EXHAUSTIVE_SPACE_LIMIT: u64 = 4_096;

/// The smallest satisfying count a template needs (1.0 `MIN_SPACE_SIZE`).
///
/// The number is `SERVED_TEXT_MEMORY`: with fewer distinct problems than the
/// per-task memory holds, avoiding a recently served problem means nothing
/// (Hard Rule 4). The gate of U2 owns the check; the constant lives here beside
/// the count it bounds.
pub const MIN_SPACE_SIZE: u64 = 12;

/// The count of tuples the estimator draws above [`EXHAUSTIVE_SPACE_LIMIT`].
///
/// The estimator draws as many tuples as the exhaustive walk visits, so the two
/// branches of [`space_size`] cost the same work.
pub const ESTIMATE_SAMPLES: u32 = 4_096;

/// The seed of the estimator.
///
/// The seed is a constant, so `space_size` is a function of the document alone
/// and a reviewer reproduces the stored number from the body (C6).
pub const ESTIMATE_SEED: u64 = 0;

/// A scalar the document writes: a whole number, or a text.
///
/// JSON has one number type and it reads `1.5` as a float. A float in a value the
/// answer then computes with is the trap D6 forbids, so this type takes a whole
/// number or a string and refuses every other JSON scalar. A decimal is written
/// as a string, and [`Scalar::rational`] reads it exactly.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Scalar {
    /// A whole number, as JSON writes it.
    Int(i64),
    /// A text. A decimal, a LaTeX fragment such as `\times`, or an operator sign.
    Text(String),
}

impl Scalar {
    /// The text of the scalar, as the renderer writes it.
    #[must_use]
    pub fn text(&self) -> String {
        match self {
            Self::Int(value) => value.to_string(),
            Self::Text(value) => value.clone(),
        }
    }

    /// The exact rational of the scalar, when the scalar names one.
    ///
    /// A whole number is its own rational. A text is a rational when it is a
    /// signed integer or a signed decimal, so the document writes `1.5` as the
    /// string `"1.5"` and no float ever enters (D6).
    #[must_use]
    pub fn rational(&self) -> Option<BigRational> {
        match self {
            Self::Int(value) => Some(BigRational::from(BigInt::from(*value))),
            Self::Text(value) => decimal_to_rational(value),
        }
    }

    /// The value the scalar binds to.
    #[must_use]
    pub fn value(&self) -> Value {
        match self.rational() {
            Some(number) => Value::Num(number),
            None => Value::Text(self.text()),
        }
    }
}

/// Read a signed integer or a signed decimal into an exact rational.
///
/// The reader takes ASCII digits, one optional sign, and at most one point. It
/// never calls a float parser, so `0.1` is `1/10` and not the nearest double.
#[must_use]
pub fn decimal_to_rational(text: &str) -> Option<BigRational> {
    let trimmed = text.trim();
    let (negative, digits) = match trimmed.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, trimmed.strip_prefix('+').unwrap_or(trimmed)),
    };
    let (whole, fraction) = match digits.split_once('.') {
        Some((whole, fraction)) => (whole, fraction),
        None => (digits, ""),
    };
    if whole.is_empty() && fraction.is_empty() {
        return None;
    }
    if !whole.chars().all(|c| c.is_ascii_digit()) || !fraction.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let mut mantissa_text = String::with_capacity(whole.len() + fraction.len());
    mantissa_text.push_str(whole);
    mantissa_text.push_str(fraction);
    let mantissa: BigInt = mantissa_text.parse().ok()?;
    let scale = u32::try_from(fraction.chars().count()).ok()?;
    let denominator = BigInt::from(10u8).pow(scale);
    let signed = if negative { -mantissa } else { mantissa };
    Some(BigRational::new(signed, denominator))
}

/// One bound parameter value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Value {
    /// An exact rational. An integer domain and a rational domain both build it.
    Num(BigRational),
    /// A text choice, such as `\times` or `+`.
    Text(String),
}

impl Value {
    /// The canonical string of the value, as the renderer writes it.
    ///
    /// A whole rational writes its digits. A fractional rational writes
    /// `numerator/denominator` in lowest terms, with the sign in front. A text
    /// writes itself, byte for byte.
    #[must_use]
    pub fn canonical_string(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Num(number) => {
                if number.denom().is_one() {
                    number.numer().to_string()
                } else {
                    format!("{}/{}", number.numer(), number.denom())
                }
            }
        }
    }

    /// The exact rational of the value, or `None` for a text.
    #[must_use]
    pub const fn as_rational(&self) -> Option<&BigRational> {
        match self {
            Self::Num(number) => Some(number),
            Self::Text(_) => None,
        }
    }

    /// The whole number of the value, or `None` when the value is not whole.
    #[must_use]
    pub fn as_integer(&self) -> Option<BigInt> {
        match self {
            Self::Num(number) if number.denom().is_one() => Some(number.numer().clone()),
            _ => None,
        }
    }
}

/// The bound tuple of one instance: parameter name to value.
///
/// The map is ordered, so the cartesian walk, the rendered statement, and the
/// recorded bindings of a pool row are the same on every machine and every run.
pub type Bindings = BTreeMap<String, Value>;

/// An inclusive whole-number range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntRange {
    /// The lowest value of the range.
    pub low: i64,
    /// The highest value of the range. Never below `low`.
    pub high: i64,
}

impl IntRange {
    /// The count of values in the range, or `None` when the range is empty.
    #[must_use]
    pub fn count(self) -> Option<u64> {
        let span = i128::from(self.high) - i128::from(self.low);
        if span < 0 {
            return None;
        }
        u64::try_from(span + 1).ok()
    }
}

/// The domain of one parameter (A1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Domain {
    /// An inclusive integer range (1.0 `IntDomain`).
    Int {
        /// The lowest value.
        low: i64,
        /// The highest value.
        high: i64,
    },
    /// An explicit list of values (1.0 `ChoiceDomain`).
    Choice {
        /// The values, in the order the document writes them.
        values: Vec<Scalar>,
    },
    /// A reduced fraction drawn from a numerator range over a denominator range.
    ///
    /// The pair reduces by its greatest common divisor, so the domain holds
    /// reduced fractions only and `2/4` and `1/2` are one value.
    Rational {
        /// The numerator range.
        num: IntRange,
        /// The denominator range. It never holds zero.
        den: IntRange,
    },
}

/// A domain the instantiator refuses.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DomainError {
    /// The integer range runs backwards.
    #[error("int domain {low}..{high} is empty")]
    EmptyRange {
        /// The lowest declared value.
        low: i64,
        /// The highest declared value.
        high: i64,
    },
    /// The domain holds more values than [`MAX_DOMAIN_SIZE`].
    #[error(
        "domain {name:?} holds {count} values, which exceeds MAX_DOMAIN_SIZE ({MAX_DOMAIN_SIZE})"
    )]
    TooLarge {
        /// The parameter the domain belongs to.
        name: String,
        /// The count of values the domain holds.
        count: u64,
    },
    /// The choice list is empty.
    #[error("a choice domain needs a non-empty 'values' list")]
    EmptyChoice,
    /// The denominator range of a rational domain holds zero.
    #[error("the denominator range {low}..{high} of a rational domain holds zero")]
    ZeroDenominator {
        /// The lowest declared denominator.
        low: i64,
        /// The highest declared denominator.
        high: i64,
    },
    /// A constraint refused the tuple while the space was counted.
    #[error("{0}")]
    Constraint(#[from] ConstraintError),
}

impl Domain {
    /// The count of distinct values the domain holds.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError`] for an empty range, an empty choice list, a
    /// denominator range that holds zero, and a domain past [`MAX_DOMAIN_SIZE`].
    pub fn size(&self, name: &str) -> Result<u64, DomainError> {
        match self {
            Self::Int { low, high } => {
                let count = IntRange {
                    low: *low,
                    high: *high,
                }
                .count()
                .ok_or(DomainError::EmptyRange {
                    low: *low,
                    high: *high,
                })?;
                bounded(name, count)
            }
            Self::Choice { values } => {
                if values.is_empty() {
                    return Err(DomainError::EmptyChoice);
                }
                bounded(name, values.len() as u64)
            }
            Self::Rational { .. } => Ok(self.values(name)?.len() as u64),
        }
    }

    /// Every value the domain holds, in a deterministic order.
    ///
    /// A rational domain reduces every pair and drops the repeats, so the list
    /// holds each fraction once and the count is a count of distinct problems.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError`] for the same five cases [`Domain::size`] names.
    pub fn values(&self, name: &str) -> Result<Vec<Value>, DomainError> {
        match self {
            Self::Int { low, high } => {
                let count = IntRange {
                    low: *low,
                    high: *high,
                }
                .count()
                .ok_or(DomainError::EmptyRange {
                    low: *low,
                    high: *high,
                })?;
                bounded(name, count)?;
                let mut out = Vec::with_capacity(count as usize);
                let mut current = i128::from(*low);
                while current <= i128::from(*high) {
                    out.push(Value::Num(BigRational::from(BigInt::from(current))));
                    current += 1;
                }
                Ok(out)
            }
            Self::Choice { values } => {
                if values.is_empty() {
                    return Err(DomainError::EmptyChoice);
                }
                bounded(name, values.len() as u64)?;
                Ok(values.iter().map(Scalar::value).collect())
            }
            Self::Rational { num, den } => {
                if den.low <= 0 && den.high >= 0 {
                    return Err(DomainError::ZeroDenominator {
                        low: den.low,
                        high: den.high,
                    });
                }
                let numerators = num.count().ok_or(DomainError::EmptyRange {
                    low: num.low,
                    high: num.high,
                })?;
                let denominators = den.count().ok_or(DomainError::EmptyRange {
                    low: den.low,
                    high: den.high,
                })?;
                bounded(name, numerators.saturating_mul(denominators))?;
                let mut seen = BTreeSet::new();
                let mut numerator = i128::from(num.low);
                while numerator <= i128::from(num.high) {
                    let mut denominator = i128::from(den.low);
                    while denominator <= i128::from(den.high) {
                        if denominator != 0 {
                            seen.insert(reduced(numerator, denominator));
                        }
                        denominator += 1;
                    }
                    numerator += 1;
                }
                Ok(seen.into_iter().map(Value::Num).collect())
            }
        }
    }
}

/// Refuse a domain of more than [`MAX_DOMAIN_SIZE`] values.
fn bounded(name: &str, count: u64) -> Result<u64, DomainError> {
    if count > MAX_DOMAIN_SIZE {
        return Err(DomainError::TooLarge {
            name: name.to_string(),
            count,
        });
    }
    Ok(count)
}

/// Build the reduced fraction of a numerator over a non-zero denominator.
fn reduced(numerator: i128, denominator: i128) -> BigRational {
    BigRational::new(BigInt::from(numerator), BigInt::from(denominator))
}

/// The declared parameters of a template document, in name order.
pub type Params = BTreeMap<String, Domain>;

/// The count of satisfying tuples of a document (spec section 2.3, decision 1).
///
/// The count is the count of tuples the constraints accept, and never the product
/// of the domain sizes. 1.0 stores the product and saturates it at
/// `MAX_DOMAIN_SIZE`, so a 1.0 `space_size` of 10,000 reads "at least 10,000"
/// (spec section 8, trap 9). 2.0 stores a count or a labeled estimate, so the
/// number never lies about what it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SpaceSize {
    /// The exact count. The walk visited every tuple.
    Exact(u64),
    /// A rejection-sampling estimate, with the evidence it rests on.
    Estimated {
        /// The estimated count of satisfying tuples.
        estimate: u64,
        /// The count of tuples the estimator drew.
        samples: u32,
        /// The count of drawn tuples the constraints accepted.
        hits: u32,
    },
}

impl SpaceSize {
    /// The count of satisfying tuples, exact or estimated.
    #[must_use]
    pub const fn count(self) -> u64 {
        match self {
            Self::Exact(count) => count,
            Self::Estimated { estimate, .. } => estimate,
        }
    }

    /// Whether the count is exact.
    #[must_use]
    pub const fn is_exact(self) -> bool {
        matches!(self, Self::Exact(_))
    }
}

/// The product of the declared domain sizes, saturating at [`u64::MAX`].
///
/// # Errors
///
/// Returns [`DomainError`] when one domain is empty or past its bound.
pub fn declared_space(params: &Params) -> Result<u64, DomainError> {
    let mut product: u64 = 1;
    for (name, domain) in params {
        product = product.saturating_mul(domain.size(name)?);
    }
    Ok(product)
}

/// Every tuple of the declared domains, in lexicographic order of the names.
///
/// # Errors
///
/// Returns [`DomainError`] when one domain is empty, past its bound, or when the
/// product of the domain sizes is past `limit`.
pub fn enumerate(params: &Params, limit: u64) -> Result<Vec<Bindings>, DomainError> {
    let mut tuples: Vec<Bindings> = vec![Bindings::new()];
    for (name, domain) in params {
        let values = domain.values(name)?;
        let width = u64::try_from(values.len()).unwrap_or(u64::MAX);
        let grown = u64::try_from(tuples.len())
            .unwrap_or(u64::MAX)
            .saturating_mul(width);
        if grown > limit {
            return Err(DomainError::TooLarge {
                name: name.clone(),
                count: grown,
            });
        }
        let mut next = Vec::with_capacity(grown as usize);
        for tuple in &tuples {
            for value in &values {
                let mut extended = tuple.clone();
                extended.insert(name.clone(), value.clone());
                next.push(extended);
            }
        }
        tuples = next;
    }
    Ok(tuples)
}

/// Count the tuples the constraints accept (spec section 2.3, decision 1).
///
/// At or under [`EXHAUSTIVE_SPACE_LIMIT`] declared tuples the walk visits every
/// tuple and the count is exact. Above the limit the function draws
/// [`ESTIMATE_SAMPLES`] tuples from the seed [`ESTIMATE_SEED`] and scales the hit
/// rate by the declared product, and it records the sample count with the
/// estimate.
///
/// # Errors
///
/// Returns [`DomainError`] when a domain is empty or past its bound, and when a
/// constraint cannot decide on a tuple.
pub fn space_size(params: &Params, constraints: &[Constraint]) -> Result<SpaceSize, DomainError> {
    let declared = declared_space(params)?;
    if declared <= EXHAUSTIVE_SPACE_LIMIT {
        let mut count: u64 = 0;
        for tuple in enumerate(params, EXHAUSTIVE_SPACE_LIMIT)? {
            if all_hold(constraints, &tuple)? {
                count += 1;
            }
        }
        return Ok(SpaceSize::Exact(count));
    }
    let plan = super::draw::DrawPlan::new(params)?;
    let mut rng = super::draw::rng_from_seed(ESTIMATE_SEED);
    let mut hits: u32 = 0;
    for _ in 0..ESTIMATE_SAMPLES {
        let tuple = plan.draw(&mut rng);
        if all_hold(constraints, &tuple)? {
            hits += 1;
        }
    }
    let estimate = u128::from(declared) * u128::from(hits) / u128::from(ESTIMATE_SAMPLES);
    Ok(SpaceSize::Estimated {
        estimate: u64::try_from(estimate).unwrap_or(u64::MAX),
        samples: ESTIMATE_SAMPLES,
        hits,
    })
}

/// The greatest common divisor of two whole numbers, as a non-negative number.
#[must_use]
pub fn gcd_of(left: &BigInt, right: &BigInt) -> BigInt {
    let gcd = left.gcd(right);
    if gcd.is_negative() { -gcd } else { gcd }
}

/// Whether the rational is a whole number.
#[must_use]
pub fn is_whole(number: &BigRational) -> bool {
    number.denom().is_one()
}

/// Whether the rational is zero.
#[must_use]
pub fn is_zero(number: &BigRational) -> bool {
    number.numer().is_zero()
}
