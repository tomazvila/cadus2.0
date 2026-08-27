//! The inter-parameter constraint language and its evaluator (A1, D6).
//!
//! A1 names the constraints between parameters as the feature whose absence
//! blocked templates in 1.0: 1.0 could declare `a` in 1..12 and `b` in 1..12, but
//! it could not say `a > b`, and it could not say that `a + b` must carry
//! (`problem_templates.py:59-66`). The whole language of this module exists for
//! those two sentences.
//!
//! # The grammar
//!
//! ```text
//! constraint := { "op": CMP, "left": term, "right": term }
//! CMP        := "eq" | "ne" | "lt" | "le" | "gt" | "ge"
//!             | "divides" | "coprime" | "carries"
//! term       := "<param>"
//!             | { "lit": <integer|decimal string> }
//!             | { "add": [term, term, ...] }
//!             | { "sub": [term, term] }
//!             | { "mul": [term, term, ...] }
//!             | { "abs": term }
//!             | { "mod": [term, term] }
//!             | { "digit_sum": term }
//! ```
//!
//! Every predicate decides in constant time on a bound tuple, so the gate and the
//! instantiator run the same code and never disagree. The evaluator holds exact
//! rationals only and it never panics: a term it cannot decide is a
//! [`ConstraintError`], never a wrong answer (C4).
//!
//! # Borrowing
//!
//! `carries` is one predicate, and it expresses both sentences the specification
//! names (`docs/reference/serving-1.0-spec.md` section 2.3). `a + b` carries when
//! `carries(a, b)` holds. `a - b` borrows when the addition that undoes it
//! carries, which is `carries(a - b, b)`:
//!
//! ```jsonc
//! { "op": "carries", "left": {"sub": ["a", "b"]}, "right": "b" }
//! ```
//!
//! One predicate therefore covers the carry and the borrow, and the operator list
//! stays the nine names `docs/plans/M4.md` fixes.
//!
//! # A decimal literal is a string
//!
//! JSON has one number type and it reads `1.5` as a float. D6 forbids a float in
//! any value the answer computes with, so `{"lit": 3}` writes a whole number and
//! `{"lit": "1.5"}` writes a decimal. The reader turns the string into `3/2`
//! exactly, and a JSON float is refused at deserialization.

use std::collections::BTreeSet;

use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};
use serde::{Deserialize, Serialize};

use super::domain::{Bindings, Scalar, Value, gcd_of, is_whole, is_zero, literal_to_rational};

/// The largest count of decimal digits [`Term::DigitSum`] and `carries` read.
///
/// A drawn value comes from a bounded domain, so a real term is far under the
/// bound. The bound stops a `mul` chain of drawn values from building a number
/// with a million digits inside a predicate the serve path runs (L1).
pub const MAX_DIGITS: usize = 4_096;

/// The comparison of one constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cmp {
    /// The two terms are the same value.
    Eq,
    /// The two terms are different values.
    Ne,
    /// The left term is below the right term.
    Lt,
    /// The left term is below the right term or the same value.
    Le,
    /// The left term is above the right term.
    Gt,
    /// The left term is above the right term or the same value.
    Ge,
    /// The left whole number divides the right whole number.
    Divides,
    /// The two whole numbers share no factor above 1.
    Coprime,
    /// The column addition of the two whole numbers carries at least once.
    Carries,
}

impl Cmp {
    /// The wire name of the comparison.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Eq => "eq",
            Self::Ne => "ne",
            Self::Lt => "lt",
            Self::Le => "le",
            Self::Gt => "gt",
            Self::Ge => "ge",
            Self::Divides => "divides",
            Self::Coprime => "coprime",
            Self::Carries => "carries",
        }
    }

    /// Whether the comparison reads whole numbers only.
    #[must_use]
    pub const fn needs_whole_numbers(self) -> bool {
        matches!(self, Self::Divides | Self::Coprime | Self::Carries)
    }
}

/// One term of the constraint language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "TermRepr", into = "TermRepr")]
pub enum Term {
    /// The bound value of a declared parameter.
    Param(String),
    /// An exact literal.
    Lit(BigRational),
    /// The sum of two terms or more.
    Add(Vec<Term>),
    /// The left term less the right term.
    Sub(Box<Term>, Box<Term>),
    /// The product of two terms or more.
    Mul(Vec<Term>),
    /// The magnitude of a term.
    Abs(Box<Term>),
    /// The remainder of the left whole number by the right whole number.
    ///
    /// The remainder takes the sign of the divisor, so `mod(-7, 3)` is 2.
    Mod(Box<Term>, Box<Term>),
    /// The sum of the decimal digits of the magnitude of a whole number.
    DigitSum(Box<Term>),
}

/// The wire shape of a [`Term`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum TermRepr {
    /// A bare string names a parameter.
    Param(String),
    /// Every other term is a one-key object.
    Op(TermOp),
}

/// The one-key object shapes of a term.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TermOp {
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
    if items.len() != 2 {
        return Err(format!(
            "a {op} term needs exactly 2 operands, and it has {}",
            items.len()
        ));
    }
    let mut read = Vec::with_capacity(2);
    for item in items {
        read.push(Term::try_from(item)?);
    }
    let Some(right) = read.pop() else {
        return Err(format!(
            "a {op} term needs exactly 2 operands, and it has 0"
        ));
    };
    let Some(left) = read.pop() else {
        return Err(format!(
            "a {op} term needs exactly 2 operands, and it has 1"
        ));
    };
    Ok((left, right))
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
fn write_rational(number: &BigRational) -> String {
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

/// One inter-parameter constraint (A1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Constraint {
    /// The comparison.
    pub op: Cmp,
    /// The left term.
    pub left: Term,
    /// The right term.
    pub right: Term,
}

/// A constraint the evaluator cannot decide.
///
/// Every variant is a document defect, not a learner input, so the gate of U2
/// turns it into a rejection message and the instantiator never serves the
/// instance (C4).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConstraintError {
    /// The term names a parameter the document does not declare.
    #[error("a constraint term names undeclared parameter {name:?}")]
    UnknownParam {
        /// The name the term uses.
        name: String,
    },
    /// The term names a parameter bound to a text choice.
    #[error(
        "a constraint term names {name:?}, which is bound to the text {text:?} and not to a number"
    )]
    NotNumeric {
        /// The parameter name.
        name: String,
        /// The text the parameter is bound to.
        text: String,
    },
    /// The comparison reads whole numbers, and a term is not whole.
    #[error("the {op} constraint reads whole numbers, and a term is {value}")]
    NotWhole {
        /// The comparison that refused the term.
        op: &'static str,
        /// The value the term produced.
        value: String,
    },
    /// A `divides` constraint has a zero left term.
    #[error("the divides constraint needs a non-zero left term")]
    DividesByZero,
    /// A `mod` term has a zero right term.
    #[error("a mod term needs a non-zero right term")]
    ModByZero,
    /// A term built a number with more digits than [`MAX_DIGITS`].
    #[error("a constraint term built a number of more than {MAX_DIGITS} digits")]
    TooWide,
}

/// Evaluate one term against a bound tuple.
///
/// # Errors
///
/// Returns [`ConstraintError`] for an undeclared name, a text binding, a
/// non-whole operand of a whole-number term, a zero divisor, and a number past
/// [`MAX_DIGITS`] digits.
pub fn eval_term(term: &Term, bindings: &Bindings) -> Result<BigRational, ConstraintError> {
    match term {
        Term::Param(name) => match bindings.get(name) {
            None => Err(ConstraintError::UnknownParam { name: name.clone() }),
            Some(value) => match value.as_rational() {
                Some(number) => Ok(number.clone()),
                None => Err(ConstraintError::NotNumeric {
                    name: name.clone(),
                    text: value.canonical_string(),
                }),
            },
        },
        Term::Lit(number) => Ok(number.clone()),
        Term::Add(items) => {
            let mut total = BigRational::from(BigInt::from(0u8));
            for item in items {
                total += eval_term(item, bindings)?;
                width_ok(&total)?;
            }
            Ok(total)
        }
        Term::Mul(items) => {
            let mut product = BigRational::from(BigInt::from(1u8));
            for item in items {
                product *= eval_term(item, bindings)?;
                width_ok(&product)?;
            }
            Ok(product)
        }
        Term::Sub(left, right) => {
            let value = eval_term(left, bindings)? - eval_term(right, bindings)?;
            width_ok(&value)?;
            Ok(value)
        }
        Term::Abs(inner) => Ok(eval_term(inner, bindings)?.abs()),
        Term::Mod(left, right) => {
            let dividend = whole("mod", &eval_term(left, bindings)?)?;
            let divisor = whole("mod", &eval_term(right, bindings)?)?;
            if divisor.is_zero() {
                return Err(ConstraintError::ModByZero);
            }
            Ok(BigRational::from(dividend.mod_floor(&divisor)))
        }
        Term::DigitSum(inner) => {
            let value = whole("digit_sum", &eval_term(inner, bindings)?)?;
            let digits = decimal_digits(&value)?;
            let total: u64 = digits.iter().map(|digit| u64::from(*digit)).sum();
            Ok(BigRational::from(BigInt::from(total)))
        }
    }
}

/// Whether one constraint holds on a bound tuple.
///
/// # Errors
///
/// Returns [`ConstraintError`] for every term the evaluator cannot decide, and
/// for a whole-number comparison over a fractional term.
pub fn holds(constraint: &Constraint, bindings: &Bindings) -> Result<bool, ConstraintError> {
    let left = eval_term(&constraint.left, bindings)?;
    let right = eval_term(&constraint.right, bindings)?;
    match constraint.op {
        Cmp::Eq => Ok(left == right),
        Cmp::Ne => Ok(left != right),
        Cmp::Lt => Ok(left < right),
        Cmp::Le => Ok(left <= right),
        Cmp::Gt => Ok(left > right),
        Cmp::Ge => Ok(left >= right),
        Cmp::Divides => {
            let divisor = whole("divides", &left)?;
            let dividend = whole("divides", &right)?;
            if divisor.is_zero() {
                return Err(ConstraintError::DividesByZero);
            }
            Ok((dividend % divisor).is_zero())
        }
        Cmp::Coprime => {
            let first = whole("coprime", &left)?;
            let second = whole("coprime", &right)?;
            Ok(gcd_of(&first, &second).is_one())
        }
        Cmp::Carries => {
            let first = whole("carries", &left)?;
            let second = whole("carries", &right)?;
            carries(&first, &second)
        }
    }
}

/// Whether every constraint holds on a bound tuple.
///
/// # Errors
///
/// Returns the first [`ConstraintError`] a constraint raises.
pub fn all_hold(constraints: &[Constraint], bindings: &Bindings) -> Result<bool, ConstraintError> {
    for constraint in constraints {
        if !holds(constraint, bindings)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Every parameter name the term reads.
#[must_use]
pub fn term_params(term: &Term) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    collect_params(term, &mut names);
    names
}

/// Every parameter name the constraint list reads.
#[must_use]
pub fn constraint_params(constraints: &[Constraint]) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for constraint in constraints {
        collect_params(&constraint.left, &mut names);
        collect_params(&constraint.right, &mut names);
    }
    names
}

/// Walk a term and collect the parameter names it reads.
fn collect_params(term: &Term, names: &mut BTreeSet<String>) {
    match term {
        Term::Param(name) => {
            names.insert(name.clone());
        }
        Term::Lit(_) => {}
        Term::Add(items) | Term::Mul(items) => {
            for item in items {
                collect_params(item, names);
            }
        }
        Term::Sub(left, right) | Term::Mod(left, right) => {
            collect_params(left, names);
            collect_params(right, names);
        }
        Term::Abs(inner) | Term::DigitSum(inner) => collect_params(inner, names),
    }
}

/// Whether the column addition of the two magnitudes carries at least once.
///
/// The predicate reads the decimal digit runs of the two magnitudes, from the
/// ones column upward, and it reports a carry when a column sum reaches ten. The
/// incoming carry needs no state: the first column that carries ends the walk,
/// so every column the walk reads has an incoming carry of zero.
///
/// The predicate reads the magnitudes, because a decimal digit run carries no
/// sign: `carries(-59, 63)` reads the columns of 59 and 63 and reports the carry
/// of `9 + 3`.
fn carries(left: &BigInt, right: &BigInt) -> Result<bool, ConstraintError> {
    let first = decimal_digits(left)?;
    let second = decimal_digits(right)?;
    let width = first.len().max(second.len());
    for column in 0..width {
        let a = column_digit(&first, column);
        let b = column_digit(&second, column);
        if u16::from(a) + u16::from(b) >= 10 {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The digit of one decimal column, counting the ones column as column zero.
///
/// A column past the leading digit reads zero, the way column addition pads the
/// shorter number with leading zeros.
fn column_digit(digits: &[u8], column: usize) -> u8 {
    digits
        .len()
        .checked_sub(column + 1)
        .and_then(|index| digits.get(index).copied())
        .unwrap_or(0)
}

/// The decimal digits of the magnitude of a whole number, most significant first.
fn decimal_digits(value: &BigInt) -> Result<Vec<u8>, ConstraintError> {
    let text = value.magnitude().to_string();
    if text.len() > MAX_DIGITS {
        return Err(ConstraintError::TooWide);
    }
    let mut digits = Vec::with_capacity(text.len());
    for character in text.chars() {
        match character
            .to_digit(10)
            .and_then(|digit| u8::try_from(digit).ok())
        {
            Some(digit) => digits.push(digit),
            None => return Err(ConstraintError::TooWide),
        }
    }
    Ok(digits)
}

/// Read a whole number out of an exact rational, or refuse it.
fn whole(op: &'static str, number: &BigRational) -> Result<BigInt, ConstraintError> {
    if !is_whole(number) {
        return Err(ConstraintError::NotWhole {
            op,
            value: write_rational(number),
        });
    }
    width_ok(number)?;
    Ok(number.numer().clone())
}

/// Refuse a number of more than [`MAX_DIGITS`] decimal digits on either side.
fn width_ok(number: &BigRational) -> Result<(), ConstraintError> {
    let bits = number.numer().bits().max(number.denom().bits());
    // A decimal digit is more than three bits, so this bound is never tighter
    // than MAX_DIGITS digits and it costs no decimal conversion.
    let limit = u64::try_from(MAX_DIGITS)
        .unwrap_or(u64::MAX)
        .saturating_mul(4);
    if bits > limit {
        return Err(ConstraintError::TooWide);
    }
    Ok(())
}

/// Whether the value is the exact zero rational.
#[must_use]
pub fn value_is_zero(value: &Value) -> bool {
    value.as_rational().is_some_and(is_zero)
}

/// The whole number an exact rational names, when it names one.
#[must_use]
pub fn whole_of(number: &BigRational) -> Option<BigInt> {
    is_whole(number).then(|| number.numer().clone())
}

/// The `i64` a rational names, when it names one.
#[must_use]
pub fn i64_of(number: &BigRational) -> Option<i64> {
    whole_of(number).and_then(|value| value.to_i64())
}
