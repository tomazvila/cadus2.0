//! The exact number of a visual specification.
//!
//! An author writes a coordinate as a YAML number or as a string. Both reach
//! [`Scalar`], and [`Scalar::value`] gives the exact [`BigRational`]. The visual
//! module never rounds before it validates, so a tick check answers "the point
//! sits on a tick" and never "the point sits near a tick".

use std::fmt;

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{ToPrimitive, Zero};
use serde::de::Deserializer;
use serde::{Deserialize, Serialize};

use super::VisualError;

/// One authored number of a visual, kept in the text the author wrote.
///
/// The struct keeps the text and not the parsed value for two reasons. The
/// authored curriculum round-trips through serde without a change, and the
/// derived [`Eq`] of the curriculum model stays valid.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct Scalar(String);

/// The three YAML forms an author writes: `3`, `1.5`, and `"3/4"`.
#[derive(Deserialize)]
#[serde(untagged)]
enum RawScalar {
    Int(i64),
    Float(f64),
    Text(String),
}

impl<'de> Deserialize<'de> for Scalar {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = RawScalar::deserialize(deserializer)?;
        let text = match raw {
            RawScalar::Int(value) => value.to_string(),
            RawScalar::Float(value) => value.to_string(),
            RawScalar::Text(value) => value,
        };
        Ok(Self(text))
    }
}

impl Scalar {
    /// The scalar of one integer.
    #[must_use]
    pub fn from_i64(value: i64) -> Self {
        Self(value.to_string())
    }

    /// The text the author wrote.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The exact value.
    ///
    /// The accepted forms are an integer, a decimal, and a fraction `a/b`. Each
    /// form takes a leading `-`. Every other text gives
    /// [`VisualError::NotANumber`].
    pub fn value(&self) -> Result<BigRational, VisualError> {
        parse_exact(self.0.trim()).ok_or_else(|| VisualError::NotANumber {
            text: self.0.clone(),
        })
    }

    /// The value as an `f64`, for the render pass only.
    ///
    /// The render pass draws pixels, so it rounds. Validation runs first and
    /// runs on the exact value.
    pub fn to_f64(&self) -> Result<f64, VisualError> {
        let exact = self.value()?;
        exact.to_f64().ok_or_else(|| VisualError::NotANumber {
            text: self.0.clone(),
        })
    }
}

impl fmt::Display for Scalar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<i64> for Scalar {
    fn from(value: i64) -> Self {
        Self::from_i64(value)
    }
}

impl From<&str> for Scalar {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

/// Parse an integer, a decimal, or a fraction into an exact rational.
fn parse_exact(text: &str) -> Option<BigRational> {
    if let Some((left, right)) = text.split_once('/') {
        let numerator = parse_decimal(left.trim())?;
        let denominator = parse_decimal(right.trim())?;
        if denominator.is_zero() {
            return None;
        }
        return Some(numerator / denominator);
    }
    parse_decimal(text)
}

/// Parse an integer or a decimal into an exact rational.
fn parse_decimal(text: &str) -> Option<BigRational> {
    let (sign, digits) = match text.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, text.strip_prefix('+').unwrap_or(text)),
    };
    if digits.is_empty() {
        return None;
    }
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    if whole.is_empty() && fraction.is_empty() {
        return None;
    }
    if !whole.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    if !fraction.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let joined = format!("{whole}{fraction}");
    let numerator = joined.parse::<BigInt>().ok()?;
    let denominator = BigInt::from(10u8).pow(u32::try_from(fraction.len()).ok()?);
    let value = BigRational::new(numerator, denominator);
    Some(if sign < 0 { -value } else { value })
}

#[cfg(test)]
mod tests {
    use num_bigint::BigInt;
    use num_rational::BigRational;

    use super::Scalar;
    use crate::visual::VisualError;

    fn ratio(numerator: i64, denominator: i64) -> BigRational {
        BigRational::new(BigInt::from(numerator), BigInt::from(denominator))
    }

    #[test]
    fn the_three_authored_forms_give_the_same_exact_value() {
        assert_eq!(Scalar::from("3").value().unwrap(), ratio(3, 1));
        assert_eq!(Scalar::from("0.75").value().unwrap(), ratio(3, 4));
        assert_eq!(Scalar::from("3/4").value().unwrap(), ratio(3, 4));
        assert_eq!(Scalar::from("-1.5").value().unwrap(), ratio(-3, 2));
        assert_eq!(Scalar::from("+2").value().unwrap(), ratio(2, 1));
        assert_eq!(Scalar::from(" 5 ").value().unwrap(), ratio(5, 1));
        assert_eq!(Scalar::from("1.5/0.5").value().unwrap(), ratio(3, 1));
        assert_eq!(Scalar::from(".5").value().unwrap(), ratio(1, 2));
    }

    #[test]
    fn a_bad_text_and_a_zero_denominator_give_not_a_number() {
        for text in ["", "x", "1/0", "1/", "--2", "1.2.3", "1 2", "nan"] {
            assert!(
                matches!(
                    Scalar::from(text).value(),
                    Err(VisualError::NotANumber { .. })
                ),
                "text {text:?} passed the parser"
            );
        }
    }

    #[test]
    fn the_render_pass_reads_the_value_as_a_float() {
        assert!((Scalar::from("0.25").to_f64().unwrap() - 0.25).abs() < f64::EPSILON);
        assert!(Scalar::from("oops").to_f64().is_err());
    }

    #[test]
    fn the_scalar_deserializes_from_a_number_and_from_a_string() {
        let parsed: Vec<Scalar> = serde_norway::from_str("[3, 1.5, \"3/4\"]").unwrap();
        assert_eq!(parsed[0].as_str(), "3");
        assert_eq!(parsed[1].as_str(), "1.5");
        assert_eq!(parsed[2].as_str(), "3/4");
        assert_eq!(Scalar::from_i64(-2).to_string(), "-2");
        assert_eq!(Scalar::from(7_i64), Scalar::from("7"));
    }
}
