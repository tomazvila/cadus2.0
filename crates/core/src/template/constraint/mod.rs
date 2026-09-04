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

mod eval;
mod wire;

use std::collections::BTreeSet;

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::ToPrimitive;
use serde::{Deserialize, Serialize};
use wire::TermRepr;

use super::domain::is_whole;

pub use eval::{all_hold, eval_term, holds};

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
