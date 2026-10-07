//! A nested radical has no exact canonical form here: `sqrt(2+sqrt(3))/2` and
//! `(sqrt(6)+sqrt(2))/4` are one number, and the exact form keeps them apart. A
//! key that holds a radical inside a radical is compared by value instead.

use std::collections::BTreeMap;

use crate::answer::evalf::{eval, free_vars};
use crate::answer::{Ast, normalize, parse};

/// The relative gap under which two nested-radical values are one number.
const GAP: f64 = 1e-12;

/// Whether the learner answer is the key, for a key with a nested radical.
///
/// `None` when the key holds no nested radical, when the learner wrote a
/// decimal (a decimal is the exact rounding rule's business), or when either
/// side has a variable or leaves the grammar.
pub(super) fn same_value(key: &str, learner: &str) -> Option<bool> {
    let key = parse(&normalize(key).source).ok()?;
    let learner = parse(&normalize(learner).source).ok()?;
    if !nested(&key, false) || holds_decimal(&learner) {
        return None;
    }
    if !free_vars(&key).is_empty() || !free_vars(&learner).is_empty() {
        return None;
    }
    let env = BTreeMap::new();
    let (a, b) = (eval(&key, &env)?, eval(&learner, &env)?);
    Some((a - b).abs() <= GAP * a.abs().max(1.0))
}

/// Whether a square root stands inside a square root.
fn nested(node: &Ast, inside: bool) -> bool {
    match node {
        Ast::Sqrt(inner) => inside || nested(inner, true) || contains_sqrt(inner),
        Ast::Neg(inner) | Ast::Pow(inner, _) => nested(inner, inside),
        Ast::RationalPow { base, .. } => nested(base, inside),
        Ast::Add(items) | Ast::Mul(items) => items.iter().any(|item| nested(item, inside)),
        Ast::Div(a, b) => nested(a, inside) || nested(b, inside),
        _ => false,
    }
}

fn contains_sqrt(node: &Ast) -> bool {
    match node {
        Ast::Sqrt(_) => true,
        Ast::Neg(inner) | Ast::Pow(inner, _) => contains_sqrt(inner),
        Ast::RationalPow { base, .. } => contains_sqrt(base),
        Ast::Add(items) | Ast::Mul(items) => items.iter().any(contains_sqrt),
        Ast::Div(a, b) => contains_sqrt(a) || contains_sqrt(b),
        _ => false,
    }
}

fn holds_decimal(node: &Ast) -> bool {
    match node {
        Ast::Decimal { .. } => true,
        Ast::Neg(inner) | Ast::Pow(inner, _) | Ast::Sqrt(inner) => holds_decimal(inner),
        Ast::RationalPow { base, .. } => holds_decimal(base),
        Ast::Add(items) | Ast::Mul(items) => items.iter().any(holds_decimal),
        Ast::Div(a, b) => holds_decimal(a) || holds_decimal(b),
        _ => false,
    }
}
