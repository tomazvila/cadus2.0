//! The `function_form` contract: a formula equal to the key that also keeps the
//! outer operation of the key.
//!
//! The `function` contract grades by value, so for "write as a product" it
//! accepts the sum the question started with. This contract needs the same value
//! and the same outer operation: a product stays a product, a sum a sum, a
//! quotient a quotient, a power a power, and a function call the same function.
//! `(x+1)(x-2)` is a product, so `x^2 - x - 2` is wrong; `cos(2x)` is a call of
//! `cos`, so `1 - 2 sin^2 x` and `sin(2x + pi/2)` are wrong.

use super::function::{self, FunctionSpec};
use crate::answer::{Ast, Outcome, Undecidable, normalize, parse};

/// The outer operation of a formula.
fn operation(ast: &Ast) -> String {
    match ast {
        Ast::Assign { value, .. } | Ast::Neg(value) => operation(value),
        Ast::Add(_) => "sum".to_owned(),
        Ast::Mul(_) => "product".to_owned(),
        Ast::Div(..) | Ast::Fraction { .. } => "quotient".to_owned(),
        Ast::Pow(..) | Ast::RationalPow { .. } => "power".to_owned(),
        Ast::Sqrt(_) => "func:sqrt".to_owned(),
        Ast::Func(name, _) => format!("func:{name}"),
        _ => "atom".to_owned(),
    }
}

/// Grade a learner formula: the value of the key and its outer operation.
pub(super) fn check(spec: &FunctionSpec, key: &str, learner: &str) -> Outcome {
    let outcome = function::check(spec, key, learner);
    let Outcome::Decided(verdict) = &outcome else {
        return outcome;
    };
    if !verdict.correct {
        return outcome;
    }
    let shapes = crate::answer::parse::with_variables(&spec.vars, || {
        let read = |text: &str| parse(&normalize(text).source).map(|tree| operation(&tree));
        read(key).and_then(|key| read(learner).map(|learner| (key, learner)))
    });
    match shapes {
        Ok((key, learner)) if key != learner => Outcome::Decided(crate::answer::Verdict {
            correct: false,
            notation: false,
        }),
        Ok(_) => outcome,
        Err(reason) => Outcome::Undecidable(Undecidable::new(reason.reason)),
    }
}
