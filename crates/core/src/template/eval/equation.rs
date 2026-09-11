//! Numeric logarithm/exponential equations under a reviewed closed vocabulary.
use num_traits::{One, Signed, ToPrimitive};

use super::{Answer, EvalError, answer, contracted};
use crate::answer::{AnswerContract, Canon, Undecidable, ast::Ast};
use crate::template::{domain::Bindings, gate::MAX_EXPONENT};

pub(super) fn write(
    name: &str,
    args: &[Ast],
    bindings: &Bindings,
    contract: Option<&AnswerContract>,
) -> Result<Answer, EvalError> {
    let Some(contract @ AnswerContract::Label { .. }) = contract else {
        return Err(refuse("equation writer requires a reviewed label contract"));
    };
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
    let Canon::Rational(b) = &base.canon else {
        unreachable!()
    };
    let Canon::Rational(e) = &exponent.canon else {
        unreachable!()
    };
    let Canon::Rational(y) = &result.canon else {
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
