//! The wire shape of a term: the one-key objects of the constraint grammar.

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, Signed, Zero};
use serde::{Deserialize, Serialize};

use super::{Term, i64_of};
use crate::template::domain::{Scalar, is_whole, literal_to_rational};

/// The wire shape of a [`Term`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub(super) enum TermRepr {
    /// A bare string names a parameter.
    Param(String),
    /// Every other term is a one-key object.
    Op(TermOp),
}

/// The one-key object shapes of a term.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum TermOp {
    /// `{"lit": 3}` or `{"lit": "1.5"}`.
    Lit(Scalar),
    /// `{"add": [term, term, ...]}`.
    Add(Vec<TermRepr>),
    /// `{"sub": [term, term]}`.
    Sub(Vec<TermRepr>),
    /// `{"mul": [term, term, ...]}`.
    Mul(Vec<TermRepr>),
    /// `{"abs": term}`.
    Abs(Box<TermRepr>),
    /// `{"mod": [term, term]}`.
    Mod(Vec<TermRepr>),
    /// `{"digit_sum": term}`.
    DigitSum(Box<TermRepr>),
}

impl TryFrom<TermRepr> for Term {
    type Error = String;

    fn try_from(repr: TermRepr) -> Result<Self, Self::Error> {
        match repr {
            TermRepr::Param(name) => Ok(Self::Param(name)),
            TermRepr::Op(TermOp::Lit(scalar)) => {
                literal_rational(&scalar).map(Self::Lit).ok_or_else(|| {
                    format!(
                        "a lit term needs a whole number, a decimal string, or 'n/d', not {:?}",
                        scalar.text()
                    )
                })
            }
            TermRepr::Op(TermOp::Add(items)) => Ok(Self::Add(many("add", items, 2)?)),
            TermRepr::Op(TermOp::Mul(items)) => Ok(Self::Mul(many("mul", items, 2)?)),
            TermRepr::Op(TermOp::Sub(items)) => {
                let (left, right) = pair("sub", items)?;
                Ok(Self::Sub(Box::new(left), Box::new(right)))
            }
            TermRepr::Op(TermOp::Mod(items)) => {
                let (left, right) = pair("mod", items)?;
                Ok(Self::Mod(Box::new(left), Box::new(right)))
            }
            TermRepr::Op(TermOp::Abs(inner)) => Ok(Self::Abs(Box::new(Self::try_from(*inner)?))),
            TermRepr::Op(TermOp::DigitSum(inner)) => {
                Ok(Self::DigitSum(Box::new(Self::try_from(*inner)?)))
            }
        }
    }
}

/// The exact rational one literal scalar names.
///
/// The reader takes back every form [`write_rational`] writes, the `n/d` form
/// included, so a body the gate accepted reads again (M4 review 1, finding 8).
fn literal_rational(scalar: &Scalar) -> Option<BigRational> {
    match scalar {
        Scalar::Int(number) => Some(BigRational::from(BigInt::from(*number))),
        Scalar::Text(text) => literal_to_rational(text),
    }
}

/// Read a variadic operand list of at least `least` terms.
fn many(op: &str, items: Vec<TermRepr>, least: usize) -> Result<Vec<Term>, String> {
    if items.len() < least {
        return Err(format!(
            "a {op} term needs at least {least} operands, and it has {}",
            items.len()
        ));
    }
    items.into_iter().map(Term::try_from).collect()
}

/// Read an operand list of exactly two terms.
fn pair(op: &str, items: Vec<TermRepr>) -> Result<(Term, Term), String> {
    let count = items.len();
    let mut items = items.into_iter();
    let (Some(left), Some(right), None) = (items.next(), items.next(), items.next()) else {
        return Err(format!(
            "a {op} term needs exactly 2 operands, and it has {count}"
        ));
    };
    Ok((Term::try_from(left)?, Term::try_from(right)?))
}

impl From<Term> for TermRepr {
    fn from(term: Term) -> Self {
        match term {
            Term::Param(name) => Self::Param(name),
            Term::Lit(number) => Self::Op(TermOp::Lit(literal_scalar(&number))),
            Term::Add(items) => Self::Op(TermOp::Add(items.into_iter().map(Self::from).collect())),
            Term::Mul(items) => Self::Op(TermOp::Mul(items.into_iter().map(Self::from).collect())),
            Term::Sub(left, right) => {
                Self::Op(TermOp::Sub(vec![Self::from(*left), Self::from(*right)]))
            }
            Term::Mod(left, right) => {
                Self::Op(TermOp::Mod(vec![Self::from(*left), Self::from(*right)]))
            }
            Term::Abs(inner) => Self::Op(TermOp::Abs(Box::new(Self::from(*inner)))),
            Term::DigitSum(inner) => Self::Op(TermOp::DigitSum(Box::new(Self::from(*inner)))),
        }
    }
}

/// The wire scalar of a literal.
///
/// A whole number that fits an `i64` writes as a JSON number, so a document that
/// reads `{"lit": 100}` writes `{"lit": 100}` again. The round trip must hold
/// byte for byte, because `content_store.digest` covers the whole body (spec
/// section 8, trap 8) and a changed byte asks for a new human approval (C6).
fn literal_scalar(number: &BigRational) -> Scalar {
    if let Some(value) = i64_of(number) {
        return Scalar::Int(value);
    }
    Scalar::Text(write_rational(number))
}

/// Write an exact rational as the wire text of a literal.
///
/// A whole number writes its digits. Every other rational writes the decimal it
/// equals when the denominator is a power of ten, and `numerator/denominator`
/// otherwise, which the reader takes back as the same value.
pub(super) fn write_rational(number: &BigRational) -> String {
    if is_whole(number) {
        return number.numer().to_string();
    }
    let mut denominator = number.denom().clone();
    let mut scale = 0_u32;
    let ten = BigInt::from(10u8);
    while (&denominator % &ten).is_zero() && scale < 64 {
        denominator /= &ten;
        scale += 1;
    }
    if denominator.is_one() {
        let power = ten.pow(scale);
        let mantissa = number.numer() * (power / number.denom());
        let text = mantissa.magnitude().to_string();
        let scale_usize = scale as usize;
        let padded = if text.len() <= scale_usize {
            format!("{}{text}", "0".repeat(scale_usize - text.len() + 1))
        } else {
            text
        };
        let split = padded.len() - scale_usize;
        let (whole, fraction) = padded.split_at(split);
        let sign = if number.numer().is_negative() {
            "-"
        } else {
            ""
        };
        return format!("{sign}{whole}.{fraction}");
    }
    format!("{}/{}", number.numer(), number.denom())
}
