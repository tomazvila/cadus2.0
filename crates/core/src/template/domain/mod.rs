//! Parameter domains, their values, and the satisfying-count space (A1, D6).
//!
//! A domain says which values one parameter takes. 1.0 has two forms, an integer
//! range and an explicit choice list (`problem_templates.py:271-292`). 2.0 adds
//! two more. The rational domain draws a numerator and a denominator and reduces
//! the pair by its greatest common divisor, because a knowledge point about
//! fractions needs a fractional parameter and a float never enters an answer
//! (D6). The decimal domain draws a whole number of steps of `10**-scale` and
//! keeps the decimal spelling, because a knowledge point about decimals must
//! serve `0.2` and not `1/5` (M4 review 1, finding 9).
//!
//! # Every value is exact
//!
//! [`Value`] holds an exact rational, an exact rational with the spelling its
//! author wrote, or a text choice. It holds no float, so a rendered statement
//! and a computed answer never carry the `3.00000000000000` of 1.0
//! (`docs/reference/serving-1.0-spec.md` section 8, trap 3).
//!
//! # Every enumeration is bounded
//!
//! [`Domain::values`] materializes the value list, the way 1.0 does, and it
//! refuses a domain of more than [`MAX_DOMAIN_SIZE`] values instead of building
//! it. The rational domain multiplies two ranges, so the bound is the one place
//! that stops a `1..10000` over `1..10000` document from allocating 100 million
//! values.

mod space;
mod values;

use std::collections::BTreeMap;

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, Signed, Zero};
use serde::{Deserialize, Serialize};

use super::constraint::ConstraintError;

pub use space::{
    Params, SatisfyingWalk, SpaceSize, declared_space, enumerate, gcd_of, is_whole, space_size,
    walk_satisfying,
};

/// The largest count of values one domain holds (1.0 `MAX_DOMAIN_SIZE`).
pub const MAX_DOMAIN_SIZE: u64 = 10_000;

/// The largest count of entries one choice domain holds (1.0 `MAX_CHOICES`).
pub const MAX_CHOICES: usize = 24;

/// The largest count of decimal places a decimal domain writes.
///
/// The bound keeps the drawn value inside the width the answer checker reads,
/// and it keeps the written statement short.
pub const MAX_DECIMAL_SCALE: u32 = 9;

/// The largest tuple count the space walk enumerates (1.0 `EXHAUSTIVE_SPACE_LIMIT`).
///
/// At or under the limit the satisfying count is exact, because the walk visits
/// every tuple once. Above it the count is the count of distinct satisfying
/// tuples one sampled walk found, which is a floor of the true count.
pub const EXHAUSTIVE_SPACE_LIMIT: u64 = 4_096;

/// The smallest satisfying count a template needs (1.0 `MIN_SPACE_SIZE`).
///
/// The number is `SERVED_TEXT_MEMORY`: with fewer distinct problems than the
/// per-task memory holds, avoiding a recently served problem means nothing
/// (Hard Rule 4). The gate of U2 owns the check; the constant lives here beside
/// the count it bounds.
pub const MIN_SPACE_SIZE: u64 = 12;

// The second estimator is gone. Above [`EXHAUSTIVE_SPACE_LIMIT`] the count comes
// from the ONE walk of [`walk_satisfying`], which spends the gate's own draw
// budget. 2.0 kept a 4,096-draw rejection-sampling estimate beside a
// 262,144-draw walk, and the two numbers disagreed by two orders of magnitude in
// both directions: a space of 4 tuples read as 244 and a space of 20 tuples read
// as 0 (M4 review 2, findings 2 and 5).

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
    ///
    /// A text that names a number keeps its authored spelling: the renderer
    /// writes `0.2` for the choice value `"0.2"`, and the evaluator computes
    /// with the exact rational `1/5` (M4 review 1, finding 9).
    #[must_use]
    pub fn value(&self) -> Value {
        match self {
            Self::Int(number) => Value::Num(BigRational::from(BigInt::from(*number))),
            Self::Text(text) => match decimal_to_rational(text) {
                Some(number) => Value::Spelled {
                    text: text.clone(),
                    number,
                },
                None => Value::Text(text.clone()),
            },
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
    // The text holds digits and nothing else, so the read succeeds.
    let mantissa: BigInt = mantissa_text.parse().unwrap_or_default();
    let scale = u32::try_from(fraction.chars().count()).unwrap_or(u32::MAX);
    let denominator = BigInt::from(10u8).pow(scale);
    let signed = if negative { -mantissa } else { mantissa };
    Some(BigRational::new(signed, denominator))
}

/// Read a constraint literal into an exact rational.
///
/// The reader takes every form the writer of a literal produces: a signed
/// integer, a signed decimal, and `numerator/denominator`. The `n/d` form is the
/// one [`super::constraint::Term`] writes back for a rational whose denominator
/// is not a power of ten, so the body of a gate-accepted document reads again
/// (M4 review 1, finding 8).
#[must_use]
pub fn literal_to_rational(text: &str) -> Option<BigRational> {
    if let Some(number) = decimal_to_rational(text) {
        return Some(number);
    }
    let (numerator_text, denominator_text) = text.trim().split_once('/')?;
    let numerator = decimal_to_rational(numerator_text)?;
    let denominator = decimal_to_rational(denominator_text)?;
    if denominator.numer().is_zero() {
        return None;
    }
    Some(numerator / denominator)
}

/// One bound parameter value.
///
/// Two values are equal when they name the same exact number, whatever spelling
/// each one carries, and two texts are equal when the texts are equal. The order
/// is the numeric order, and a number sorts before a text.
#[derive(Debug, Clone)]
pub enum Value {
    /// An exact rational. An integer domain and a rational domain both build it.
    Num(BigRational),
    /// An exact rational with the spelling its author wrote.
    ///
    /// A decimal domain and a choice value that reads as a decimal both build
    /// it. The renderer writes `text`, and the evaluator computes with `number`.
    Spelled {
        /// The authored spelling, as the renderer writes it.
        text: String,
        /// The exact value the text names.
        number: BigRational,
    },
    /// A text choice, such as `\times` or `+`.
    Text(String),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self.as_rational(), other.as_rational()) {
            (Some(left), Some(right)) => left == right,
            (None, None) => self.canonical_string() == other.canonical_string(),
            _ => false,
        }
    }
}

impl Eq for Value {}

impl Ord for Value {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self.as_rational(), other.as_rational()) {
            (Some(left), Some(right)) => left.cmp(right),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => self.canonical_string().cmp(&other.canonical_string()),
        }
    }
}

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Value {
    /// The canonical string of the value, as the renderer writes it.
    ///
    /// A whole rational writes its digits. A fractional rational writes
    /// `numerator/denominator` in lowest terms, with the sign in front. A text
    /// writes itself, byte for byte, and so does the spelling of a value that
    /// carries one: a decimal reaches the learner as the decimal its author
    /// wrote.
    #[must_use]
    pub fn canonical_string(&self) -> String {
        match self {
            Self::Text(text) | Self::Spelled { text, .. } => text.clone(),
            Self::Num(number) => {
                if number.denom().is_one() {
                    number.numer().to_string()
                } else {
                    format!("{}/{}", number.numer(), number.denom())
                }
            }
        }
    }

    /// Whether the written form needs brackets where a statement splices it in.
    ///
    /// A written form that carries a leading sign or a fraction bar re-reads
    /// when an operator stands beside it: `-3^{2}` is the negative of a square,
    /// and `3/2^{2}` is three over a square. Both forms take brackets, so the
    /// statement asks the question the answer answers (M4 review 1, finding 18).
    /// Every other form is atomic: `12`, the decimal `0.2`, and a text choice
    /// such as `\times` write themselves.
    #[must_use]
    pub fn needs_brackets(&self) -> bool {
        match self {
            Self::Text(_) => false,
            Self::Spelled { text, .. } => text.starts_with('-') || text.contains('/'),
            Self::Num(number) => number.numer().is_negative() || !number.denom().is_one(),
        }
    }

    /// The exact rational of the value, or `None` for a text.
    #[must_use]
    pub const fn as_rational(&self) -> Option<&BigRational> {
        match self {
            Self::Num(number) | Self::Spelled { number, .. } => Some(number),
            Self::Text(_) => None,
        }
    }

    /// The whole number of the value, or `None` when the value is not whole.
    #[must_use]
    pub fn as_integer(&self) -> Option<BigInt> {
        match self.as_rational() {
            Some(number) if number.denom().is_one() => Some(number.numer().clone()),
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
    /// A decimal drawn from a whole-number range, written with a fixed scale.
    ///
    /// `{"kind": "decimal", "low": 1, "high": 20, "scale": 1}` holds the twenty
    /// values 0.1 to 2.0. `low` and `high` count the steps of `10**-scale`, so
    /// the document writes whole numbers only and no float enters (D6). The
    /// value keeps the decimal spelling, so a decimals knowledge point serves
    /// the text its author wrote (M4 review 1, finding 9).
    Decimal {
        /// The lowest step. The value is `low / 10**scale`.
        low: i64,
        /// The highest step. The value is `high / 10**scale`.
        high: i64,
        /// The count of decimal places the domain writes.
        scale: u32,
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
    /// The scale of a decimal domain is past [`MAX_DECIMAL_SCALE`].
    #[error("decimal domain scale {scale} exceeds MAX_DECIMAL_SCALE ({MAX_DECIMAL_SCALE})")]
    DecimalScale {
        /// The scale the domain declares.
        scale: u32,
    },
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
