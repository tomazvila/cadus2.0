//! The numeric value of an answer tree at one point (`function` contract).
//!
//! This module is the one part of [`crate::answer`] that uses a float. The
//! `function` contract compares two expressions at sample points, and this
//! module gives the value of one expression at one point.
//!
//! [`eval`] has no random source and no clock, so two calls with the same
//! input give the same bits. It never panics. A value that is not defined, or
//! that is not finite, is `None`.

mod func;
mod number;
mod power;

use std::collections::{BTreeMap, BTreeSet};

use num_traits::ToPrimitive;

use super::ast::{Ast, Const};

/// The value of each variable at one sample point.
pub type Env = BTreeMap<String, f64>;

/// The largest nesting depth that [`eval`] reads.
///
/// The parser stops at depth 96. A tree that a caller builds by hand has no
/// such limit, and a deeper recursion risks the stack. A deeper tree is `None`.
const MAX_DEPTH: usize = 256;

/// The value of a scalar expression at one point. `None` = not defined or not finite there.
#[must_use]
pub fn eval(ast: &Ast, env: &Env) -> Option<f64> {
    eval_at(ast, env, 0)
}

/// Each variable name that occurs in the tree (`Ast::Var`), constants `pi` and `e` excluded.
///
/// The label of an `Assign` is not a variable of the value, so it stays out.
/// The variable of an `Ineq` or a `Chain` is included.
#[must_use]
pub fn free_vars(ast: &Ast) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut stack = vec![ast];
    // The loop keeps its own stack, so a deep tree does not use the call stack.
    while let Some(node) = stack.pop() {
        if let Some(name) = own_name(node) {
            names.insert(name.to_string());
        }
        push_children(node, &mut stack);
    }
    names
}

/// Return the variable name that the node itself holds.
fn own_name(node: &Ast) -> Option<&str> {
    match node {
        Ast::Var(name) => Some(name),
        Ast::Ineq { var, .. } | Ast::Chain { var, .. } => Some(var),
        _ => None,
    }
}

/// Return the items of a node that holds a list of children.
fn item_list(node: &Ast) -> &[Ast] {
    match node {
        Ast::Func(_, items) => items,
        Ast::Add(items) | Ast::Mul(items) => items,
        Ast::Tuple(items) | Ast::Set(items) | Ast::List(items) => items,
        _ => &[],
    }
}

/// Push each child of the node on the stack.
fn push_children<'a>(node: &'a Ast, stack: &mut Vec<&'a Ast>) {
    stack.extend(item_list(node));
    match node {
        Ast::Sqrt(child) | Ast::Neg(child) | Ast::Pow(child, _) => stack.push(child),
        Ast::RationalPow { base: child, .. }
        | Ast::Ineq { bound: child, .. }
        | Ast::Quantity { value: child, .. }
        | Ast::Assign { value: child, .. } => stack.push(child),
        Ast::Div(first, second)
        | Ast::Interval {
            lo: first,
            hi: second,
            ..
        }
        | Ast::Chain {
            lo: first,
            hi: second,
            ..
        } => stack.extend([&**first, &**second]),
        _ => {}
    }
}

/// Evaluate one node, and refuse a result that is not finite.
fn eval_at(ast: &Ast, env: &Env, depth: usize) -> Option<f64> {
    if depth > MAX_DEPTH {
        return None;
    }
    let value = match ast {
        Ast::Integer(value) => value.to_f64(),
        Ast::Decimal { mantissa, scale } => number::decimal(mantissa, *scale),
        Ast::Fraction {
            numerator,
            denominator,
        } => number::ratio(numerator, denominator),
        Ast::Mixed {
            whole,
            numerator,
            denominator,
        } => number::mixed(whole, numerator, denominator),
        Ast::Var(name) => env.get(name).copied(),
        Ast::Const(Const::Pi) => Some(std::f64::consts::PI),
        Ast::Const(Const::E) => Some(std::f64::consts::E),
        other => compound(other, env, depth + 1),
    }?;
    value.is_finite().then_some(value)
}

/// Evaluate an arithmetic node or an `Assign`. Each other node goes to [`applied`].
fn compound(ast: &Ast, env: &Env, depth: usize) -> Option<f64> {
    let at = |child: &Ast| eval_at(child, env, depth);
    match ast {
        Ast::Neg(child) => at(child).map(|value| -value),
        Ast::Add(items) => fold(items, 0.0, |sum, item| sum + item, at),
        Ast::Mul(items) => fold(items, 1.0, |product, item| product * item, at),
        Ast::Div(top, bottom) => Some(at(top)? / at(bottom)?),
        Ast::Assign { value, .. } => at(value),
        other => applied(other, env, depth),
    }
}

/// Evaluate a root, a power, or a function. A node that is not a scalar is `None`.
fn applied(ast: &Ast, env: &Env, depth: usize) -> Option<f64> {
    let at = |child: &Ast| eval_at(child, env, depth);
    match ast {
        Ast::Sqrt(child) => at(child).map(f64::sqrt),
        Ast::Pow(base, exponent) => Some(power::int_pow(at(base)?, *exponent)),
        Ast::RationalPow {
            base,
            numerator,
            denominator,
        } => power::rational_pow(at(base)?, *numerator, *denominator),
        Ast::Func(name, args) => match args.as_slice() {
            [arg] => func::apply(name, at(arg)?),
            _ => None,
        },
        _ => None,
    }
}

/// Combine the values of the items from left to right. One `None` item gives `None`.
fn fold(
    items: &[Ast],
    start: f64,
    combine: impl Fn(f64, f64) -> f64,
    at: impl Fn(&Ast) -> Option<f64>,
) -> Option<f64> {
    items
        .iter()
        .try_fold(start, |total, item| Some(combine(total, at(item)?)))
}
