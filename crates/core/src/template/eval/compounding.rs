//! Exact repeated-compounding factors for a bounded whole-number frequency.
use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::ToPrimitive;

use super::exact::{as_rational, literal};
use super::{EvalError, evaluate};
use crate::answer::ast::Ast;
use crate::template::domain::Bindings;

const NAME: &str = "compounding";
const MAX_COUNT: u32 = 64;

pub(super) fn factor(args: &[Ast], bindings: &Bindings) -> Result<Ast, EvalError> {
    let [count] = args else {
        return Err(EvalError::Arity {
            func: NAME.to_owned(),
            want: 1,
            given: args.len(),
        });
    };
    let value = evaluate(count, bindings)?;
    let count = as_rational(&value).ok_or_else(|| domain("count must be rational"))?;
    if !count.is_integer() {
        return Err(domain("count must be a whole number"));
    }
    let count = count
        .to_integer()
        .to_u32()
        .filter(|count| (1..=MAX_COUNT).contains(count))
        .ok_or_else(|| domain("count must be between 1 and 64"))?;
    literal(BigRational::new(
        BigInt::from(count + 1).pow(count),
        BigInt::from(count).pow(count),
    ))
}

fn domain(value: &str) -> EvalError {
    EvalError::Domain {
        func: NAME,
        value: value.to_owned(),
    }
}
