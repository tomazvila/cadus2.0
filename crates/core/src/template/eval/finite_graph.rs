//! Exact parent-graph extrema on closed quarter-turn subintervals of one period.
use num_rational::BigRational;
use num_traits::ToPrimitive;

use super::exact::{as_rational, literal};
use super::{EvalError, evaluate, text_binding};
use crate::answer::ast::{Ast, Const};
use crate::template::domain::Bindings;

const NAME: &str = "quarterextremum";

pub(super) fn extremum(args: &[Ast], bindings: &Bindings) -> Result<Ast, EvalError> {
    let [family, interval] = args else {
        return Err(EvalError::Arity {
            func: NAME.to_owned(),
            want: 2,
            given: args.len(),
        });
    };
    let Ast::List(interval) = interval else {
        return Err(domain("expected [lower, upper, direction]"));
    };
    let [lower, upper, direction] = interval.as_slice() else {
        return Err(domain("expected three interval fields"));
    };
    let values = match quarter(family, bindings)? {
        0 => [0, 1, 0, -1, 0],
        1 => [1, 0, -1, 0, 1],
        _ => return Err(domain("family must be 0 (sine) or 1 (cosine)")),
    };
    let direction =
        text_binding(direction, bindings).ok_or_else(|| domain("direction must be text"))?;
    let maximize = match direction.as_str() {
        "highest" => true,
        "lowest" => false,
        _ => return Err(domain("direction must be highest or lowest")),
    };
    let lower = quarter(lower, bindings)?;
    let upper = quarter(upper, bindings)?;
    if lower >= upper {
        return Err(domain("endpoints must satisfy 0 <= lower < upper <= 4"));
    }
    let mut best = lower;
    for index in lower + 1..=upper {
        if (maximize && values[index] > values[best]) || (!maximize && values[index] < values[best])
        {
            best = index;
        }
    }
    // Both parent curves are monotone between consecutive quarter turns.
    // Keeping the first equal extremum implements the explicit leftmost tie rule.
    Ok(Ast::Tuple(vec![
        Ast::Mul(vec![
            literal(BigRational::new(best.into(), 2.into()))?,
            Ast::Const(Const::Pi),
        ]),
        literal(BigRational::from_integer(values[best].into()))?,
    ]))
}

fn quarter(ast: &Ast, bindings: &Bindings) -> Result<usize, EvalError> {
    let value = evaluate(ast, bindings)?;
    let number = as_rational(&value).ok_or_else(|| domain("endpoint must be rational"))?;
    if !number.is_integer() {
        return Err(domain("endpoint must be a whole quarter turn"));
    }
    number
        .to_integer()
        .to_usize()
        .filter(|value| *value <= 4)
        .ok_or_else(|| domain("endpoint is outside one period"))
}

fn domain(value: &str) -> EvalError {
    EvalError::Domain {
        func: NAME,
        value: value.to_owned(),
    }
}
