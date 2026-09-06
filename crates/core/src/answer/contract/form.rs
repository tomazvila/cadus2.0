//! Required numeric notation, distinct from approximate value acceptance.

use num_integer::Integer;
use num_traits::{One, Signed};
use serde::{Deserialize, Serialize};

use crate::answer::{Ast, normalize, parse};

/// An explicit numeric output form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NumericForm {
    Integer,
    Decimal,
    ReducedFraction,
}

pub(super) fn accepts(form: NumericForm, text: &str) -> bool {
    let Ok(tree) = parse(&normalize(text).source) else {
        return false;
    };
    let mut node = &tree;
    while let Ast::Neg(inner) = node {
        node = inner;
    }
    match (form, node) {
        (NumericForm::Integer, Ast::Integer(_)) | (NumericForm::Decimal, Ast::Decimal { .. }) => {
            true
        }
        (
            NumericForm::ReducedFraction,
            Ast::Fraction {
                numerator,
                denominator,
            },
        ) => denominator.is_positive() && numerator.gcd(denominator).is_one(),
        _ => false,
    }
}
