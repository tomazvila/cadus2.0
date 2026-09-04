//! The helpers of the `template_eval_nodes` tests.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    unused_imports
)]

pub use super::gate::bind;
pub use cadus_core::answer::{Ast, Const, IneqOp};
pub use cadus_core::template::{
    Answer, Bindings, EvalError, Scalar, Value, answer, evaluate, parse_answer_expr, write,
};
pub use num_bigint::BigInt;
pub use num_rational::BigRational;
pub use std::collections::BTreeMap;

/// An integer literal node.
pub fn int(value: i64) -> Ast {
    Ast::Integer(BigInt::from(value))
}

/// A fraction literal node.
pub fn frac(numerator: i64, denominator: i64) -> Ast {
    Ast::Fraction {
        numerator: BigInt::from(numerator),
        denominator: BigInt::from(denominator),
    }
}

/// A variable node.
pub fn var(name: &str) -> Ast {
    Ast::Var(name.to_string())
}

/// The tree of one answer expression.
pub fn expr(source: &str) -> Ast {
    parse_answer_expr(source).unwrap_or_else(|e| panic!("{source:?}: {}", e.reason))
}

/// Evaluate one expression with no binding.
pub fn eval(source: &str) -> Result<Ast, EvalError> {
    evaluate(&expr(source), &Bindings::new())
}

/// Evaluate one expression and write the result.
pub fn written(source: &str) -> String {
    write(&eval(source).expect("the expression evaluates")).expect("the value writes")
}
