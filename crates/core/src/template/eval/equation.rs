//! Numeric and setup-preserving equations under reviewed contracts.
use num_traits::{One, Signed, ToPrimitive};

use super::{Answer, EvalError, answer, contracted, exact, write as tree_writer};
use crate::answer::{AnswerContract, Canon, Undecidable, ast::Ast};
use crate::template::{domain::Bindings, gate::MAX_EXPONENT};

pub(super) fn write(
    name: &str,
    args: &[Ast],
    bindings: &Bindings,
    contract: Option<&AnswerContract>,
) -> Result<Answer, EvalError> {
    if name == "relationform" {
        let Some(contract @ AnswerContract::RelationSetup) = contract else {
            return Err(refuse("relationform requires a setup-preserving contract"));
        };
        return relation_form(args, bindings, contract);
    }
    let Some(contract @ AnswerContract::Label { .. }) = contract else {
        return Err(refuse("equation writer requires a reviewed label contract"));
    };
    exponential(name, args, bindings, contract)
}

fn exponential(
    name: &str,
    args: &[Ast],
    bindings: &Bindings,
    contract: &AnswerContract,
) -> Result<Answer, EvalError> {
    let [base, Ast::List(parts)] = args else {
        return Err(refuse(
            "equation writer requires base and [exponent, result]",
        ));
    };
    let [exponent, result] = parts.as_slice() else {
        return Err(refuse(
            "equation writer requires two numeric equation parts",
        ));
    };
    let base = rational(base, bindings)?;
    let exponent = rational(exponent, bindings)?;
    let result = rational(result, bindings)?;
    let (Canon::Rational(b), Canon::Rational(e), Canon::Rational(y)) =
        (&base.canon, &exponent.canon, &result.canon)
    else {
        unreachable!()
    };
    if !b.is_positive() || b.is_one() || !y.is_positive() || !e.is_integer() {
        return Err(refuse(
            "equation writer requires a positive base other than one, positive result, and integer exponent",
        ));
    }
    let power = e
        .to_integer()
        .to_i64()
        .filter(|p| p.unsigned_abs() <= MAX_EXPONENT as u64)
        .ok_or_else(|| refuse("equation exponent is outside the existing bound"))?;
    let computed = answer(
        &Ast::Pow(Box::new(crate::answer::parse(&base.text)?), power),
        &Bindings::new(),
    )?;
    if computed.canon != result.canon {
        return Err(refuse(
            "equation components do not satisfy base^exponent = result",
        ));
    }
    let base_text = if base.text.contains('/') {
        format!("({})", base.text)
    } else {
        base.text.clone()
    };
    let text = match name {
        "logequation" => format!("log_{}({}) = {}", base_text, result.text, exponent.text),
        "expequation" => format!("({})^({}) = {}", base.text, exponent.text, result.text),
        _ => return Err(refuse("unknown equation writer")),
    };
    contracted(text, contract)
}

fn relation_form(
    args: &[Ast],
    bindings: &Bindings,
    contract: &AnswerContract,
) -> Result<Answer, EvalError> {
    let [Ast::Tuple(sides), comparison] = args else {
        return Err(refuse("relationform requires [(left, right), comparison]"));
    };
    let [left, right] = sides.as_slice() else {
        return Err(refuse("relationform requires exactly two relation sides"));
    };
    let comparison = rational(comparison, bindings)?;
    let Canon::Rational(comparison) = comparison.canon else {
        unreachable!()
    };
    let code = comparison
        .to_integer()
        .to_i8()
        .filter(|_| comparison.is_integer())
        .ok_or_else(|| refuse("relationform comparison must be an integer from -2 to 2"))?;
    let operator = match code {
        -2 => "<",
        -1 => "<=",
        0 => "=",
        1 => ">=",
        2 => ">",
        _ => {
            return Err(refuse(
                "relationform comparison must be an integer from -2 to 2",
            ));
        }
    };
    let left = write_setup(&bind_structure(left, bindings)?)?;
    let right = write_setup(&bind_structure(right, bindings)?)?;
    contracted(format!("{left} {operator} {right}"), contract)
}

fn bind_structure(ast: &Ast, bindings: &Bindings) -> Result<Ast, EvalError> {
    match ast {
        Ast::Var(name) => match bindings.get(name) {
            Some(value) => exact::literal(
                value
                    .as_rational()
                    .ok_or_else(|| EvalError::NotNumeric {
                        name: name.clone(),
                        text: value.canonical_string(),
                    })?
                    .clone(),
            ),
            None => Ok(ast.clone()),
        },
        Ast::Neg(inner) => Ok(Ast::Neg(Box::new(bind_structure(inner, bindings)?))),
        Ast::Add(items) => Ok(Ast::Add(bind_all(items, bindings)?)),
        Ast::Mul(items) => Ok(Ast::Mul(bind_all(items, bindings)?)),
        Ast::Div(left, right) => Ok(Ast::Div(
            Box::new(bind_structure(left, bindings)?),
            Box::new(bind_structure(right, bindings)?),
        )),
        Ast::Integer(_) | Ast::Decimal { .. } | Ast::Fraction { .. } => Ok(ast.clone()),
        _ => Err(refuse(
            "relationform supports bounded arithmetic structure only",
        )),
    }
}

fn write_setup(ast: &Ast) -> Result<String, EvalError> {
    let Ast::Add(items) = ast else {
        return tree_writer::write(ast);
    };
    let mut text = String::new();
    for (index, item) in items.iter().enumerate() {
        if let Ast::Neg(inner) = item {
            text.push_str(if index == 0 { "-" } else { " - " });
            text.push_str(&tree_writer::write(inner)?);
        } else {
            if index > 0 {
                text.push_str(" + ");
            }
            text.push_str(&tree_writer::write(item)?);
        }
    }
    Ok(text)
}

fn bind_all(items: &[Ast], bindings: &Bindings) -> Result<Vec<Ast>, EvalError> {
    items
        .iter()
        .map(|item| bind_structure(item, bindings))
        .collect()
}

fn rational(ast: &Ast, bindings: &Bindings) -> Result<Answer, EvalError> {
    let value = answer(ast, bindings)?;
    if !matches!(value.canon, Canon::Rational(_)) {
        return Err(refuse("equation components must be exact rationals"));
    }
    Ok(value)
}

fn refuse(reason: &'static str) -> EvalError {
    EvalError::Grammar(Undecidable::new(reason))
}
