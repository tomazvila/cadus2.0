//! Exact sine and cosine values at the four quadrantal angles.

use num_rational::BigRational;
use num_traits::ToPrimitive;

use super::exact::{as_rational, literal};
use super::{EvalError, evaluate, text_binding};
use crate::answer::ast::Ast;
use crate::template::domain::Bindings;

const NAME: &str = "quartervalue";

pub(super) fn value(args: &[Ast], bindings: &Bindings) -> Result<Ast, EvalError> {
    let [family, quarter] = args else {
        return Err(EvalError::Arity {
            func: NAME.to_owned(),
            want: 2,
            given: args.len(),
        });
    };
    let family =
        text_binding(family, bindings).ok_or_else(|| domain("family must be text sin or cos"))?;
    let quarter = evaluate(quarter, bindings)?;
    let quarter = as_rational(&quarter)
        .filter(|value| value.is_integer())
        .and_then(|value| value.to_integer().to_usize())
        .filter(|value| *value <= 3)
        .ok_or_else(|| domain("quarter index must be a whole number from 0 through 3"))?;
    let coordinates = match family.as_str() {
        "sin" => [0, 1, 0, -1],
        "cos" => [1, 0, -1, 0],
        _ => return Err(domain("family must be exactly sin or cos")),
    };
    literal(BigRational::from_integer(coordinates[quarter].into()))
}

fn domain(value: &str) -> EvalError {
    EvalError::Domain {
        func: NAME,
        value: value.to_owned(),
    }
}
