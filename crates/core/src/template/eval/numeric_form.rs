//! Exact decimal notation for a reviewed decimal-required answer contract.

use num_bigint::BigInt;
use num_traits::{One, Signed, Zero};

use super::super::domain::Bindings;
use super::{Answer, EvalError, answer, contracted};
use crate::answer::ast::Ast;
use crate::answer::canon::MAX_SCALE;
use crate::answer::{AnswerContract, Canon, Undecidable};

pub(super) fn decimal_answer(
    ast: &Ast,
    bindings: &Bindings,
    contract: &AnswerContract,
) -> Result<Answer, EvalError> {
    let value = answer(ast, bindings)?;
    let Canon::Rational(rational) = &value.canon else {
        return contracted(value.text, contract);
    };
    let scale = decimal_scale(rational.denom())?;
    let multiplier = BigInt::from(10_u8).pow(scale) / rational.denom();
    let mantissa = rational.numer() * multiplier;
    let rendered = contracted(decimal_text(&mantissa, scale), contract)?;
    if rendered.canon != value.canon {
        return Err(refuse("decimal rendering changed the exact value"));
    }
    Ok(rendered)
}

fn decimal_scale(denominator: &BigInt) -> Result<u32, EvalError> {
    let mut remaining = denominator.clone();
    let mut scale = 0;
    for factor in [2_u8, 5_u8] {
        let mut count = 0;
        while (&remaining % factor).is_zero() {
            remaining /= factor;
            count += 1;
            if count > MAX_SCALE {
                return Err(refuse(
                    "decimal required form exceeds the decimal scale bound",
                ));
            }
        }
        scale = scale.max(count);
    }
    if !remaining.is_one() {
        return Err(refuse("decimal required form needs a terminating rational"));
    }
    Ok(scale)
}

fn decimal_text(mantissa: &BigInt, scale: u32) -> String {
    let places = scale.max(1) as usize;
    let magnitude = if scale == 0 {
        mantissa.abs() * 10_u8
    } else {
        mantissa.abs()
    };
    let width = places + 1;
    let digits = format!("{:0>width$}", magnitude.to_string());
    let split = digits.len() - places;
    format!(
        "{}{}.{}",
        if mantissa.is_negative() { "-" } else { "" },
        &digits[..split],
        &digits[split..]
    )
}

fn refuse(reason: &'static str) -> EvalError {
    EvalError::Grammar(Undecidable::new(reason))
}
