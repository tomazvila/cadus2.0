//! The helpers of the 2.0 production tests (D-F3, unit f2-grammar).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    unused_imports
)]

pub use super::check::*;
pub use super::parse::{ast, int, refusal};

/// Build a variable node.
pub fn v(name: &str) -> Ast {
    Ast::Var(name.to_string())
}

/// Build a rational-power node.
pub fn root_pow(base: Ast, numerator: i64, denominator: i64) -> Ast {
    Ast::RationalPow {
        base: Box::new(base),
        numerator,
        denominator,
    }
}

/// Build the canonical pair of two whole numbers.
pub fn pair(quotient: i64, remainder: i64) -> Canon {
    Canon::Tuple(vec![
        Canon::Rational(whole(quotient)),
        Canon::Rational(whole(remainder)),
    ])
}
