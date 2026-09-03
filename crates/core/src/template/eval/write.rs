//! The answer writer: an evaluated tree as a string inside the M2 grammar.

use super::exact::{decimal_value, rational_node};
use super::{EXTRA_FUNCTIONS, EvalError};
use crate::answer::ast::{Ast, IneqOp};
use num_bigint::BigInt;
use num_traits::Signed;

/// Write an evaluated tree as an answer string inside the M2 grammar.
///
/// The writer brackets by precedence, so `2*(x+1)` keeps its brackets and `2*x`
/// takes none. A whole rational writes its digits, so the 1.0 float trap
/// `3.00000000000000` has no spelling here (spec section 8, trap 3).
///
/// # Errors
///
/// Returns [`EvalError`] when the tree still holds an evaluation-only function,
/// which happens only when an argument of one was not an exact number.
pub fn write(value: &Ast) -> Result<String, EvalError> {
    let mut out = String::new();
    write_at(value, Prec::Lowest, &mut out)?;
    Ok(out)
}

/// The bracketing levels of the writer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Prec {
    /// A whole answer, a bracketed group, or a collection member.
    Lowest,
    /// An operand of a sum.
    Sum,
    /// An operand of a product or a quotient.
    Product,
    /// A place where a power stands with no brackets: an operand of a product,
    /// the divisor of a quotient, and the operand of a minus sign.
    Power,
    /// The base of a power, which reads no operator of its own.
    ///
    /// The level exists because `**` groups to the right: the base of a power
    /// must be atomic, or `Pow(Pow(x, 2), 3)` writes `x**2**3`, which the M2
    /// parser refuses as a tower of powers (M4 review 1, finding 17).
    Atom,
}

/// The bracketing level one node writes at when it stands alone.
///
/// A node whose level is BELOW the level its place needs takes brackets. A
/// leading minus sign is the case that matters: `Ast::Integer(-3)` writes `-3`,
/// and `-3**2` reads as `-(3**2)`, so a negative literal takes the level of a
/// sum and the power brackets it into `(-3)**2`.
///
/// A power writes an operator of its own, so it is not atomic and the base of a
/// power brackets it. Every self-delimiting node — a non-negative literal, a
/// name, a function call, a root, a collection — is atomic.
fn level(node: &Ast) -> Prec {
    match node {
        Ast::Ineq { .. } | Ast::Assign { .. } | Ast::Chain { .. } => Prec::Lowest,
        Ast::Add(_) | Ast::Neg(_) | Ast::Mixed { .. } => Prec::Sum,
        Ast::Integer(value) if value.is_negative() => Prec::Sum,
        Ast::Decimal { mantissa, .. } if mantissa.is_negative() => Prec::Sum,
        Ast::Fraction { numerator, .. } if numerator.is_negative() => Prec::Sum,
        Ast::Fraction { .. } | Ast::Mul(_) | Ast::Div(_, _) => Prec::Product,
        Ast::Pow(_, _) => Prec::Power,
        _ => Prec::Atom,
    }
}

/// Write one node at a bracketing level.
fn write_at(node: &Ast, need: Prec, out: &mut String) -> Result<(), EvalError> {
    let bracket = level(node) < need;
    if bracket {
        out.push('(');
    }
    write_bare(node, out)?;
    if bracket {
        out.push(')');
    }
    Ok(())
}

/// Write one node without its outer brackets.
fn write_bare(node: &Ast, out: &mut String) -> Result<(), EvalError> {
    match node {
        Ast::Integer(value) => write_text(&value.to_string(), out),
        Ast::Decimal { mantissa, scale } => {
            write_bare(&rational_node(&decimal_value(mantissa, *scale)), out)
        }
        Ast::Fraction {
            numerator,
            denominator,
        } => write_fraction(numerator, denominator, out),
        Ast::Mixed {
            whole,
            numerator,
            denominator,
        } => write_mixed(whole, numerator, denominator, out),
        Ast::Var(name) => write_text(name, out),
        Ast::Const(constant) => write_text(constant.name(), out),
        Ast::Neg(inner) => write_negation(inner, out),
        Ast::Sqrt(inner) => write_root(inner, out),
        Ast::Pow(base, exponent) => write_power(base, *exponent, out),
        Ast::Add(items) => write_joined(items, " + ", Prec::Product, out),
        Ast::Mul(items) => write_joined(items, "*", Prec::Power, out),
        Ast::Div(left, right) => write_quotient(left, right, out),
        Ast::Func(name, args) => write_call(name, args, out),
        Ast::Tuple(items) => write_wrapped(items, ('(', ')'), out),
        Ast::Set(items) => write_wrapped(items, ('{', '}'), out),
        Ast::List(items) => write_wrapped(items, ('[', ']'), out),
        Ast::Interval {
            lo,
            hi,
            lo_closed,
            hi_closed,
        } => write_interval(lo, hi, *lo_closed, *hi_closed, out),
        Ast::Ineq { var, op, bound } => write_inequality(var, *op, bound, out),
        Ast::Assign { var, value } => write_assignment(var, value, out),
        Ast::Chain {
            lo,
            lo_closed,
            var,
            hi_closed,
            hi,
        } => write_chain(lo, *lo_closed, var, *hi_closed, hi, out),
    }
}

/// Write one text as it stands.
fn write_text(text: &str, out: &mut String) -> Result<(), EvalError> {
    out.push_str(text);
    Ok(())
}

/// Write `numerator/denominator`.
fn write_fraction(
    numerator: &BigInt,
    denominator: &BigInt,
    out: &mut String,
) -> Result<(), EvalError> {
    out.push_str(&numerator.to_string());
    out.push('/');
    out.push_str(&denominator.to_string());
    Ok(())
}

/// Write `whole numerator/denominator`.
fn write_mixed(
    whole: &BigInt,
    numerator: &BigInt,
    denominator: &BigInt,
    out: &mut String,
) -> Result<(), EvalError> {
    out.push_str(&whole.to_string());
    out.push(' ');
    write_fraction(numerator, denominator, out)
}

/// Write a minus sign and its operand.
fn write_negation(inner: &Ast, out: &mut String) -> Result<(), EvalError> {
    out.push('-');
    write_at(inner, Prec::Power, out)
}

/// Write `sqrt(…)`.
fn write_root(inner: &Ast, out: &mut String) -> Result<(), EvalError> {
    out.push_str("sqrt(");
    write_at(inner, Prec::Lowest, out)?;
    out.push(')');
    Ok(())
}

/// Write a power, with a negative exponent in brackets.
fn write_power(base: &Ast, exponent: i64, out: &mut String) -> Result<(), EvalError> {
    write_at(base, Prec::Atom, out)?;
    out.push_str("**");
    if exponent < 0 {
        out.push('(');
        out.push_str(&exponent.to_string());
        out.push(')');
    } else {
        out.push_str(&exponent.to_string());
    }
    Ok(())
}

/// Write a quotient.
fn write_quotient(left: &Ast, right: &Ast, out: &mut String) -> Result<(), EvalError> {
    write_at(left, Prec::Product, out)?;
    out.push('/');
    write_at(right, Prec::Power, out)
}

/// Write a function call, or refuse an evaluation-only name.
fn write_call(name: &str, args: &[Ast], out: &mut String) -> Result<(), EvalError> {
    if !is_writable_function(name) {
        return Err(EvalError::NotNumber {
            func: "an evaluation-only function",
        });
    }
    out.push_str(name);
    write_wrapped(args, ('(', ')'), out)
}

/// Write a collection between its two delimiters.
fn write_wrapped(
    items: &[Ast],
    delimiters: (char, char),
    out: &mut String,
) -> Result<(), EvalError> {
    out.push(delimiters.0);
    write_joined(items, ", ", Prec::Lowest, out)?;
    out.push(delimiters.1);
    Ok(())
}

/// Write a bracket interval.
fn write_interval(
    lo: &Ast,
    hi: &Ast,
    lo_closed: bool,
    hi_closed: bool,
    out: &mut String,
) -> Result<(), EvalError> {
    out.push(if lo_closed { '[' } else { '(' });
    write_at(lo, Prec::Lowest, out)?;
    out.push_str(", ");
    write_at(hi, Prec::Lowest, out)?;
    out.push(if hi_closed { ']' } else { ')' });
    Ok(())
}

/// Write a simple inequality.
fn write_inequality(var: &str, op: IneqOp, bound: &Ast, out: &mut String) -> Result<(), EvalError> {
    out.push_str(var);
    out.push(' ');
    out.push_str(op.symbol());
    out.push(' ');
    write_at(bound, Prec::Lowest, out)
}

/// Write a labeled value.
fn write_assignment(var: &str, value: &Ast, out: &mut String) -> Result<(), EvalError> {
    out.push_str(var);
    out.push_str(" = ");
    write_at(value, Prec::Lowest, out)
}

/// Write a chained inequality.
fn write_chain(
    lo: &Ast,
    lo_closed: bool,
    var: &str,
    hi_closed: bool,
    hi: &Ast,
    out: &mut String,
) -> Result<(), EvalError> {
    write_at(lo, Prec::Lowest, out)?;
    out.push_str(if lo_closed { " <= " } else { " < " });
    out.push_str(var);
    out.push_str(if hi_closed { " <= " } else { " < " });
    write_at(hi, Prec::Lowest, out)
}

/// Write a list of nodes, separated by one string.
fn write_joined(
    items: &[Ast],
    separator: &str,
    need: Prec,
    out: &mut String,
) -> Result<(), EvalError> {
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            out.push_str(separator);
        }
        write_at(item, need, out)?;
    }
    Ok(())
}

/// Whether the writer may put a function name back into the answer string.
///
/// An evaluation-only name is never writable: it must be erased, or the answer
/// string would leave the grammar the M2 checker reads (V2). `abs` and `sqrt`
/// are in both sets, and the M2 grammar holds them, so they are writable.
fn is_writable_function(name: &str) -> bool {
    if name == "abs" || name == "sqrt" {
        return true;
    }
    !EXTRA_FUNCTIONS.contains(&name)
}
