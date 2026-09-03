//! The exact steps of the evaluator: one node at a time, on exact rationals.

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, Signed, Zero};

use super::{EvalError, MAX_VALUE_BITS, evaluate};
use crate::answer::ast::{Ast, IneqOp};
use crate::template::domain::{Bindings, is_whole};

/// The exact rational of a decimal literal: `mantissa / 10^scale`.
pub(super) fn decimal_value(mantissa: &BigInt, scale: u32) -> BigRational {
    BigRational::new(mantissa.clone(), BigInt::from(10u8).pow(scale))
}

/// Evaluate a literal fraction, or refuse a zero denominator.
pub(super) fn fraction_literal(numerator: &BigInt, denominator: &BigInt) -> Result<Ast, EvalError> {
    if denominator.is_zero() {
        return Err(EvalError::DivideByZero);
    }
    literal(BigRational::new(numerator.clone(), denominator.clone()))
}

/// Evaluate a mixed number `a b/c` as `a + b/c`, or refuse a zero denominator.
pub(super) fn mixed_literal(
    whole: &BigInt,
    numerator: &BigInt,
    denominator: &BigInt,
) -> Result<Ast, EvalError> {
    if denominator.is_zero() {
        return Err(EvalError::DivideByZero);
    }
    literal(
        BigRational::from(whole.clone()) + BigRational::new(numerator.clone(), denominator.clone()),
    )
}

/// The value a name is bound to, or the name itself when the tuple leaves it free.
pub(super) fn bound_value(name: &str, bindings: &Bindings) -> Result<Ast, EvalError> {
    let Some(value) = bindings.get(name) else {
        return Ok(Ast::Var(name.to_string()));
    };
    match value.as_rational() {
        Some(number) => literal(number.clone()),
        None => Err(EvalError::NotNumeric {
            name: name.to_string(),
            text: value.canonical_string(),
        }),
    }
}

/// Evaluate a negation: an exact operand negates, a symbolic one keeps the sign node.
pub(super) fn negate(inner: &Ast, bindings: &Bindings) -> Result<Ast, EvalError> {
    let value = evaluate(inner, bindings)?;
    match as_rational(&value) {
        Some(number) => literal(-number),
        None => Ok(Ast::Neg(Box::new(value))),
    }
}

/// Evaluate a quotient: two exact operands divide, and a zero divisor is refused.
pub(super) fn divide(left: &Ast, right: &Ast, bindings: &Bindings) -> Result<Ast, EvalError> {
    let dividend = evaluate(left, bindings)?;
    let divisor = evaluate(right, bindings)?;
    match (as_rational(&dividend), as_rational(&divisor)) {
        (Some(_), Some(second)) if second.is_zero() => Err(EvalError::DivideByZero),
        (Some(first), Some(second)) => literal(first / second),
        _ => Ok(Ast::Div(Box::new(dividend), Box::new(divisor))),
    }
}

/// Evaluate a power: an exact base is raised, a symbolic one keeps the node.
pub(super) fn raise(base: &Ast, exponent: i64, bindings: &Bindings) -> Result<Ast, EvalError> {
    let value = evaluate(base, bindings)?;
    match as_rational(&value) {
        Some(number) => literal(power(&number, exponent)?),
        None => Ok(Ast::Pow(Box::new(value), exponent)),
    }
}

/// Evaluate a square root: an exact radicand takes its root, a symbolic one keeps the node.
pub(super) fn root(inner: &Ast, bindings: &Bindings) -> Result<Ast, EvalError> {
    let value = evaluate(inner, bindings)?;
    match as_rational(&value) {
        Some(number) => square_root(&number),
        None => Ok(Ast::Sqrt(Box::new(value))),
    }
}

/// Evaluate both ends of a bracket interval.
pub(super) fn interval(
    lo: &Ast,
    hi: &Ast,
    lo_closed: bool,
    hi_closed: bool,
    bindings: &Bindings,
) -> Result<Ast, EvalError> {
    Ok(Ast::Interval {
        lo: Box::new(evaluate(lo, bindings)?),
        hi: Box::new(evaluate(hi, bindings)?),
        lo_closed,
        hi_closed,
    })
}

/// Evaluate the bound of a simple inequality.
pub(super) fn inequality(
    var: &str,
    op: IneqOp,
    bound: &Ast,
    bindings: &Bindings,
) -> Result<Ast, EvalError> {
    Ok(Ast::Ineq {
        var: var.to_string(),
        op,
        bound: Box::new(evaluate(bound, bindings)?),
    })
}

/// Evaluate the value of a labeled answer.
pub(super) fn assignment(var: &str, value: &Ast, bindings: &Bindings) -> Result<Ast, EvalError> {
    Ok(Ast::Assign {
        var: var.to_string(),
        value: Box::new(evaluate(value, bindings)?),
    })
}

/// Evaluate both ends of a chained inequality.
pub(super) fn chain(
    lo: &Ast,
    lo_closed: bool,
    var: &str,
    hi_closed: bool,
    hi: &Ast,
    bindings: &Bindings,
) -> Result<Ast, EvalError> {
    Ok(Ast::Chain {
        lo: Box::new(evaluate(lo, bindings)?),
        lo_closed,
        var: var.to_string(),
        hi_closed,
        hi: Box::new(evaluate(hi, bindings)?),
    })
}

/// Which variadic fold one node takes.
#[derive(Debug, Clone, Copy)]
pub(super) enum Fold {
    /// A sum.
    Add,
    /// A product.
    Mul,
}

/// Evaluate a variadic node, folding the exact operands into one rational.
///
/// Every exact operand folds into one number, and every symbolic operand stays.
/// A node with no symbolic operand left is one literal, which is the whole point:
/// a numeric answer reduces to a number and never to a sum of numbers.
pub(super) fn fold(items: &[Ast], bindings: &Bindings, kind: Fold) -> Result<Ast, EvalError> {
    let identity = match kind {
        Fold::Add => BigRational::from(BigInt::from(0u8)),
        Fold::Mul => BigRational::from(BigInt::from(1u8)),
    };
    let mut number = identity.clone();
    let mut rest: Vec<Ast> = Vec::new();
    for item in items {
        let value = evaluate(item, bindings)?;
        match as_rational(&value) {
            Some(operand) => {
                match kind {
                    Fold::Add => number += operand,
                    Fold::Mul => number *= operand,
                }
                width_ok(&number)?;
            }
            None => rest.push(value),
        }
    }
    if matches!(kind, Fold::Mul) && number.is_zero() {
        return literal(number);
    }
    let redundant = match kind {
        Fold::Add => number.is_zero(),
        Fold::Mul => number.is_one(),
    };
    if !redundant {
        rest.insert(0, rational_node(&number));
    }
    match rest.pop() {
        None => literal(identity),
        Some(last) if rest.is_empty() => Ok(last),
        Some(last) => {
            rest.push(last);
            Ok(match kind {
                Fold::Add => Ast::Add(rest),
                Fold::Mul => Ast::Mul(rest),
            })
        }
    }
}

/// Evaluate every member of a collection.
pub(super) fn evaluate_all(items: &[Ast], bindings: &Bindings) -> Result<Vec<Ast>, EvalError> {
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        out.push(evaluate(item, bindings)?);
    }
    Ok(out)
}

/// The exact square root of a non-negative rational, or the root node.
///
/// `sqrt(4)` is 2 and `sqrt(4/9)` is `2/3`. A radicand with no exact root stays
/// [`Ast::Sqrt`] over a literal, which is inside the M2 grammar: the
/// canonicalizer reduces `sqrt(8)` to `2*sqrt(2)` on its own.
pub(super) fn square_root(value: &BigRational) -> Result<Ast, EvalError> {
    if value.numer().is_negative() {
        return Err(EvalError::NegativeRoot {
            value: value.to_string(),
        });
    }
    let numerator = value.numer().sqrt();
    let denominator = value.denom().sqrt();
    if &(&numerator * &numerator) == value.numer()
        && &(&denominator * &denominator) == value.denom()
    {
        return literal(BigRational::new(numerator, denominator));
    }
    Ok(Ast::Sqrt(Box::new(rational_node(value))))
}

/// Raise an exact rational to a whole power, inside the width bound.
pub(super) fn power(base: &BigRational, exponent: i64) -> Result<BigRational, EvalError> {
    if exponent == 0 {
        return Ok(BigRational::from(BigInt::from(1u8)));
    }
    if base.is_zero() && exponent < 0 {
        return Err(EvalError::DivideByZero);
    }
    let magnitude = exponent.unsigned_abs();
    let steps = u32::try_from(magnitude).map_err(|_| EvalError::TooWide)?;
    let mut result = BigRational::from(BigInt::from(1u8));
    for _ in 0..steps {
        result *= base;
        width_ok(&result)?;
    }
    if exponent < 0 {
        if result.is_zero() {
            return Err(EvalError::DivideByZero);
        }
        return Ok(result.recip());
    }
    Ok(result)
}

/// Build a literal node from an exact rational, inside the width bound.
pub(super) fn literal(value: BigRational) -> Result<Ast, EvalError> {
    width_ok(&value)?;
    Ok(rational_node(&value))
}

/// Build the node that holds an exact rational.
pub(super) fn rational_node(value: &BigRational) -> Ast {
    if is_whole(value) {
        return Ast::Integer(value.numer().clone());
    }
    Ast::Fraction {
        numerator: value.numer().clone(),
        denominator: value.denom().clone(),
    }
}

/// Read the exact rational a node holds, when it holds one.
pub(super) fn as_rational(node: &Ast) -> Option<BigRational> {
    match node {
        Ast::Integer(value) => Some(BigRational::from(value.clone())),
        Ast::Fraction {
            numerator,
            denominator,
        } if !denominator.is_zero() => {
            Some(BigRational::new(numerator.clone(), denominator.clone()))
        }
        _ => None,
    }
}

/// Refuse a value wider than [`MAX_VALUE_BITS`] on either side.
pub(super) fn width_ok(value: &BigRational) -> Result<(), EvalError> {
    if value.numer().bits().max(value.denom().bits()) > MAX_VALUE_BITS {
        return Err(EvalError::TooWide);
    }
    Ok(())
}
