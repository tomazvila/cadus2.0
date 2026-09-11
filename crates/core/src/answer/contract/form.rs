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
    /// A greatest integer factor multiplying a primitive linear sum.
    FactoredLinear,
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
        (NumericForm::FactoredLinear, node) => factored_linear(node),
        _ => false,
    }
}

fn factored_linear(node: &Ast) -> bool {
    let node = match node {
        Ast::Neg(inner) => inner.as_ref(),
        other => other,
    };
    let Ast::Mul(factors) = node else {
        return false;
    };
    let [outer_node, Ast::Add(terms)] = factors.as_slice() else {
        return false;
    };
    let Some(outer) = signed_integer(outer_node) else {
        return false;
    };
    if outer.abs() < 2.into() || terms.len() < 2 {
        return false;
    }
    let Some(coefficients) = terms
        .iter()
        .map(linear_coefficient)
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };
    coefficients
        .iter()
        .find(|(_, symbolic)| *symbolic)
        .is_some_and(|(coefficient, _)| coefficient.is_positive())
        && coefficients.iter().any(|(_, symbolic)| *symbolic)
        && coefficients
            .iter()
            .map(|(coefficient, _)| coefficient.abs())
            .reduce(|left, right| left.gcd(&right))
            .is_some_and(|gcd| gcd.is_one())
}

fn linear_coefficient(node: &Ast) -> Option<(num_bigint::BigInt, bool)> {
    match node {
        Ast::Integer(value) => Some((value.clone(), false)),
        Ast::Var(_) | Ast::Pow(_, 1) => Some((1.into(), true)),
        Ast::Neg(inner) => linear_coefficient(inner).map(|(value, symbolic)| (-value, symbolic)),
        Ast::Mul(factors) => {
            let mut coefficient = num_bigint::BigInt::one();
            let mut symbolic_count = 0_u8;
            for factor in factors {
                match factor {
                    Ast::Integer(value) => coefficient *= value,
                    Ast::Var(_) | Ast::Pow(_, 1) => symbolic_count += 1,
                    _ => return None,
                }
            }
            (symbolic_count == 1).then_some((coefficient, true))
        }
        _ => None,
    }
}

fn signed_integer(node: &Ast) -> Option<num_bigint::BigInt> {
    match node {
        Ast::Integer(value) => Some(value.clone()),
        Ast::Neg(inner) => match inner.as_ref() {
            Ast::Integer(value) => Some(-value),
            _ => None,
        },
        _ => None,
    }
}
