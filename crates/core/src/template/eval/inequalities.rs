//! Fail-closed writers for exact inequality solution sets.
use super::{Answer, EvalError, answer, contracted, text_binding};
use crate::answer::{AnswerContract, Canon, ast::Ast};
use crate::template::domain::Bindings;
use num_rational::BigRational;
use num_traits::{One, Zero};

pub(super) fn union_answer(
    ast: &Ast,
    bindings: &Bindings,
    contract: &AnswerContract,
) -> Result<Answer, EvalError> {
    debug_assert!(matches!(
        contract,
        AnswerContract::InequalityUnion | AnswerContract::RequiredInequalityNotation
    ));
    let Ast::Func(name, args) = ast else {
        return answer(ast, bindings);
    };
    if name == "rayunion" {
        return rays(args, bindings, contract);
    }
    if name == "convertnotation" {
        return super::notation::convert(args, bindings, contract);
    }
    if args.len() != 2 || !matches!(name.as_str(), "excludepoint" | "lowerbound" | "upperbound") {
        return answer(ast, bindings);
    }
    let Some(variable) = text_binding(&args[0], bindings) else {
        return answer(ast, bindings);
    };
    let bound = answer(&args[1], bindings)?.text;
    let text = match name.as_str() {
        "excludepoint" => format!("{variable} < {bound} or {variable} > {bound}"),
        "lowerbound" => format!("{variable} >= {bound}"),
        "upperbound" => format!("{variable} <= {bound}"),
        _ => unreachable!(),
    };
    contracted(text, contract)
}

fn rays(args: &[Ast], bindings: &Bindings, contract: &AnswerContract) -> Result<Answer, EvalError> {
    let [left, right] = args else {
        return Err(shape());
    };
    let (lo, lo_closed) = endpoint(left, bindings)?;
    let (hi, hi_closed) = endpoint(right, bindings)?;
    if lo >= hi {
        return Err(shape());
    }
    let left_op = if lo_closed { "<=" } else { "<" };
    let right_op = if hi_closed { ">=" } else { ">" };
    contracted(format!("x {left_op} {lo} or x {right_op} {hi}"), contract)
}

fn endpoint(ast: &Ast, bindings: &Bindings) -> Result<(BigRational, bool), EvalError> {
    let Ast::Tuple(parts) = ast else {
        return Err(shape());
    };
    let [bound, closed] = parts.as_slice() else {
        return Err(shape());
    };
    let bound = rational(bound, bindings)?;
    let closed = rational(closed, bindings)?;
    if !closed.is_zero() && !closed.is_one() {
        return Err(shape());
    }
    Ok((bound, closed.is_one()))
}

fn rational(ast: &Ast, bindings: &Bindings) -> Result<BigRational, EvalError> {
    match answer(ast, bindings)?.canon {
        Canon::Rational(value) => Ok(value),
        _ => Err(shape()),
    }
}

fn shape() -> EvalError {
    EvalError::NotNumber {
        func: "rayunion requires ordered rational endpoints with 0/1 inclusion flags",
    }
}

/// Classification branches have a fixed mathematical meaning and closed inputs.
pub(super) fn label_answer(
    name: &str,
    args: &[Ast],
    bindings: &Bindings,
    contract: &AnswerContract,
) -> Result<Answer, EvalError> {
    let (op, negative) = match (name, args) {
        ("boundaryincluded" | "boundarycircle" | "boundarystyle" | "raydirection", [op]) => {
            (op, false)
        }
        ("negativeabs", [rhs, op]) => {
            if rational(rhs, bindings)? >= BigRational::zero() {
                return Err(relation_shape());
            }
            (op, true)
        }
        _ => return Err(relation_shape()),
    };
    let op = comparison(op, bindings)?;
    let label = if negative {
        if matches!(op.as_str(), ">" | ">=") {
            "all real numbers"
        } else {
            "no solution"
        }
    } else if name == "boundarycircle" {
        if matches!(op.as_str(), "<=" | ">=") {
            "closed"
        } else {
            "open"
        }
    } else if name == "boundarystyle" {
        if matches!(op.as_str(), "<=" | ">=") {
            "solid"
        } else {
            "dashed"
        }
    } else if name == "raydirection" {
        if matches!(op.as_str(), ">" | ">=") {
            "right"
        } else {
            "left"
        }
    } else if matches!(op.as_str(), "<=" | ">=") {
        "yes"
    } else {
        "no"
    };
    contracted(label.to_owned(), contract)
}

// Comparison codes 0,1,2,3 mean <,<=,>,>=; text choices use those four spellings.
fn comparison(ast: &Ast, bindings: &Bindings) -> Result<String, EvalError> {
    if let Some(text) = text_binding(ast, bindings) {
        return matches!(text.as_str(), "<" | "<=" | ">" | ">=")
            .then_some(text)
            .ok_or_else(relation_shape);
    }
    let value = rational(ast, bindings)?;
    ["<", "<=", ">", ">="]
        .into_iter()
        .enumerate()
        .find(|(index, _)| value == BigRational::from_integer((*index).into()))
        .map(|(_, op)| op.to_owned())
        .ok_or_else(relation_shape)
}

fn relation_shape() -> EvalError {
    EvalError::NotNumber {
        func: "inequality writer requires its declared shape and one of <, <=, >, >=",
    }
}
