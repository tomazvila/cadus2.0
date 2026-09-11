//! Exact values written as one power of a numeric literal.

use num_integer::Integer;
use num_traits::One;

use super::{Canon, Undecidable};
use crate::answer::ast::Ast;
use crate::answer::canon::canon;
use crate::answer::parse;

struct Power {
    value: Canon,
    base: Canon,
    exponent: i64,
}

pub(super) fn expected(text: &str) -> Result<Canon, Undecidable> {
    match read(text)? {
        Some(power) => Ok(power.value),
        None => Err(refused()),
    }
}

pub(super) fn equivalent(expected: &str, learner: &str) -> Result<bool, Undecidable> {
    let Some(expected) = read(expected)? else {
        return Err(refused());
    };
    let Some(learner) = read(learner)? else {
        return Ok(false);
    };
    Ok(expected.value == learner.value
        && expected.base == learner.base
        && expected.exponent == learner.exponent)
}

fn read(text: &str) -> Result<Option<Power>, Undecidable> {
    let ast = parse(text)?;
    let Ast::Pow(base, exponent) = &ast else {
        return Ok(None);
    };
    if !literal(base) {
        return Ok(None);
    }
    let base = canon(base)?;
    if !matches!(base, Canon::Rational(_)) {
        return Ok(None);
    }
    Ok(Some(Power {
        value: canon(&ast)?,
        base,
        exponent: *exponent,
    }))
}

fn literal(ast: &Ast) -> bool {
    match ast {
        Ast::Integer(_) | Ast::Decimal { .. } => true,
        Ast::Fraction {
            numerator,
            denominator,
        } => numerator.gcd(denominator).is_one(),
        Ast::Neg(inner) => {
            matches!(
                inner.as_ref(),
                Ast::Integer(_) | Ast::Decimal { .. } | Ast::Fraction { .. }
            ) && literal(inner)
        }
        _ => false,
    }
}

fn refused() -> Undecidable {
    Undecidable::new("a required single power needs one reduced numeric literal base")
}
