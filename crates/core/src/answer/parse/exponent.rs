//! The exponent production: a whole number, or a rational `p/q` in brackets (D-F3).

use super::{MAX_EXPONENT, Parser};
use crate::answer::Undecidable;
use crate::answer::ast::Ast;
use crate::answer::lexer::Tok;

/// The largest denominator of a rational exponent, as the answer writes it.
const MAX_ROOT_INDEX: i64 = 6;

/// The largest magnitude of the numerator of a rational exponent, as the answer
/// writes it.
const MAX_ROOT_NUMERATOR: i64 = 12;

/// The refusal of an exponent that is not one number literal.
const NOT_WHOLE: &str = "an exponent that is not a whole number";

/// The refusal of a rational exponent outside the written bounds.
const OUT_OF_BOUND: &str = "a rational exponent outside the bound";

/// The exponent of a power.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Exponent {
    /// A whole exponent, inside the evaluation bound.
    Whole(i64),
    /// A rational exponent in lowest terms, with a denominator of 2 or more.
    Rational {
        /// The numerator, with its sign.
        numerator: i64,
        /// The denominator. Always 2 or more.
        denominator: i64,
    },
}

/// Build the power node of a base and an exponent.
pub(super) fn raise(base: Ast, exponent: Exponent) -> Ast {
    match exponent {
        Exponent::Whole(value) => Ast::Pow(Box::new(base), value),
        Exponent::Rational {
            numerator,
            denominator,
        } => Ast::RationalPow {
            base: Box::new(base),
            numerator,
            denominator,
        },
    }
}

/// Reduce a written rational exponent, or refuse it.
///
/// The bounds read the written numbers: the denominator is 2 to 6 and the
/// magnitude of the numerator is at most 12. A denominator of 1 is a whole
/// exponent. The reduced exponent then keeps a denominator of 2 or more, or it
/// is a whole exponent.
fn rational_exponent(numerator: i64, denominator: i64) -> Result<Exponent, Undecidable> {
    if denominator == 1 {
        return whole_exponent(numerator);
    }
    if !(2..=MAX_ROOT_INDEX).contains(&denominator) || numerator.abs() > MAX_ROOT_NUMERATOR {
        return Err(Undecidable::new(OUT_OF_BOUND));
    }
    let divisor = gcd(numerator.abs(), denominator);
    let numerator = numerator / divisor;
    let denominator = denominator / divisor;
    if denominator == 1 {
        return Ok(Exponent::Whole(numerator));
    }
    Ok(Exponent::Rational {
        numerator,
        denominator,
    })
}

/// Bound a whole exponent (1.0 `_MAX_EXPONENT`).
fn whole_exponent(value: i64) -> Result<Exponent, Undecidable> {
    if value.abs() > MAX_EXPONENT {
        return Err(Undecidable::new("an exponent outside the evaluation bound"));
    }
    Ok(Exponent::Whole(value))
}

/// The greatest common divisor of two non-negative numbers, the second one positive.
fn gcd(mut a: i64, mut b: i64) -> i64 {
    while b != 0 {
        let rest = a % b;
        a = b;
        b = rest;
    }
    a
}

impl Parser<'_> {
    /// Parse the exponent of a power: a whole number, or `(p/q)` in brackets.
    ///
    /// The rational shape needs its brackets: `x^1/2` is `(x^1)/2`, as it is in
    /// 1.0, and `x^(1/2)` and `x^{1/2}` are the root. A `%` inside an exponent
    /// stays refused: `2^50%` writes the exponent 50/100 through a postfix, and
    /// the grammar picks neither of its two readings (review round 3, #3, #4).
    pub(super) fn parse_exponent(&mut self) -> Result<Exponent, Undecidable> {
        let parenthesized = self.eat(&Tok::LParen);
        let negative = self.read_sign_chain();
        let magnitude = self.read_exponent_number()?;
        let numerator = if negative { -magnitude } else { magnitude };
        let denominator = if parenthesized && self.eat(&Tok::Slash) {
            Some(self.read_exponent_number()?)
        } else {
            None
        };
        if parenthesized && !self.eat(&Tok::RParen) {
            return Err(Undecidable::new(NOT_WHOLE));
        }
        match denominator {
            None => whole_exponent(numerator),
            Some(denominator) => rational_exponent(numerator, denominator),
        }
    }

    /// Read a run of sign tokens, and report whether the run is negative.
    fn read_sign_chain(&mut self) -> bool {
        let mut negative = false;
        loop {
            if self.eat(&Tok::Minus) {
                negative = !negative;
            } else if !self.eat(&Tok::Plus) {
                return negative;
            }
        }
    }

    /// Read one whole-number literal of an exponent.
    fn read_exponent_number(&mut self) -> Result<i64, Undecidable> {
        let Some(Tok::Num(text)) = self.peek() else {
            return Err(Undecidable::new(NOT_WHOLE));
        };
        if text.contains('.') {
            return Err(Undecidable::new(NOT_WHOLE));
        }
        let value: i64 = text
            .parse()
            .map_err(|_| Undecidable::new("an exponent outside the evaluation bound"))?;
        self.bump();
        if self.peek() == Some(&Tok::Percent) {
            return Err(Undecidable::new(NOT_WHOLE));
        }
        Ok(value)
    }
}
