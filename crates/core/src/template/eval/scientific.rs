//! Exact normalized scientific-notation output for an explicit answer contract.

use num_traits::{Signed, Zero};

use super::numeric_form::terminating_parts;
use super::{Answer, EvalError, answer as exact_answer, contracted};
use crate::answer::ast::Ast;
use crate::answer::{AnswerContract, Canon, Undecidable};
use crate::template::Bindings;

pub(super) fn answer(
    ast: &Ast,
    bindings: &Bindings,
    contract: &AnswerContract,
) -> Result<Answer, EvalError> {
    let value = exact_answer(ast, bindings)?;
    let Canon::Rational(rational) = &value.canon else {
        return Err(refuse(
            "scientific notation requires an exact rational value",
        ));
    };
    if rational.is_zero() {
        return Err(refuse("zero has no normalized scientific notation"));
    }
    let (mantissa, scale) = terminating_parts(rational)?;
    let digits = mantissa.abs().to_string();
    let exponent = i64::try_from(digits.len() - 1)
        .map_err(|_| refuse("scientific notation exceeds the exponent bound"))?
        - i64::from(scale);
    let significant = digits.trim_end_matches('0');
    let coefficient = if significant.len() == 1 {
        significant.to_owned()
    } else {
        format!("{}.{}", &significant[..1], &significant[1..])
    };
    let text = format!(
        "{}{} x 10^{}",
        if mantissa.is_negative() { "-" } else { "" },
        coefficient,
        exponent
    );
    let rendered = contracted(text, contract)?;
    if rendered.canon != value.canon {
        return Err(refuse("scientific rendering changed the exact value"));
    }
    Ok(rendered)
}

fn refuse(reason: &'static str) -> EvalError {
    EvalError::Grammar(Undecidable::new(reason))
}
