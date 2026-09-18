//! The answer mutator of the self-check (C3).
//!
//! A mutation is "+1 on one numeric component": the first numeric leaf of the
//! authored answer tree gets one added, and the tree is written back as a
//! learner string. A mutated answer must grade WRONG for the check to pass, so
//! the mutation only has to be a near miss, never a plausible mistake.
//!
//! The tree path is the primary one: parse the answer into the M2 [`Ast`], bump
//! one numeric leaf, and let the answer writer produce a string inside the
//! grammar. When the tree path refuses (an answer outside the decidable
//! grammar, or a spelling the writer cannot put back), a text fallback bumps
//! the first digit run of the raw answer, which keeps the check available for
//! multipart and prose-shaped answers.

use num_bigint::BigInt;

use cadus_core::answer::ast::Ast;
use cadus_core::answer::{normalize, parse};
use cadus_core::template::eval::write;

/// Build one wrong variant of an authored answer, if any mutation applies.
///
/// `None` means the answer holds no numeric component the two paths can reach;
/// the caller then reports the wrong-answer rung as unavailable instead of a
/// failure.
#[must_use]
pub fn mutate_plus_one(answer: &str) -> Option<String> {
    mutate_via_tree(answer).or_else(|| mutate_via_text(answer))
}

/// Parse the answer, bump its first numeric leaf, and write the tree back.
fn mutate_via_tree(answer: &str) -> Option<String> {
    let source = normalize(answer).source;
    let mut tree = parse(&source).ok()?;
    if bump(&mut tree) {
        write(&tree).ok()
    } else {
        None
    }
}

/// Add one to the first numeric leaf of a tree, depth first.
fn bump(node: &mut Ast) -> bool {
    match node {
        Ast::Integer(value) => {
            *value += BigInt::from(1);
            true
        }
        Ast::Decimal { mantissa, .. } => {
            *mantissa += BigInt::from(1);
            true
        }
        Ast::Fraction { numerator, .. } => {
            *numerator += BigInt::from(1);
            true
        }
        Ast::Mixed { whole, .. } => {
            *whole += BigInt::from(1);
            true
        }
        _ => children_mut(node).into_iter().any(bump),
    }
}

/// The mutable children of one node, in reading order.
fn children_mut(node: &mut Ast) -> Vec<&mut Ast> {
    match node {
        Ast::Neg(inner) | Ast::Sqrt(inner) => vec![inner],
        Ast::Pow(base, _) => vec![base],
        Ast::RationalPow { base, .. } => vec![base],
        Ast::Div(left, right) => vec![left, right],
        Ast::Add(items)
        | Ast::Mul(items)
        | Ast::Tuple(items)
        | Ast::Set(items)
        | Ast::List(items)
        | Ast::Func(_, items) => items.iter_mut().collect(),
        Ast::Interval { lo, hi, .. } | Ast::Chain { lo, hi, .. } => vec![lo, hi],
        Ast::Ineq { bound, .. }
        | Ast::Quantity { value: bound, .. }
        | Ast::Assign { value: bound, .. } => vec![bound],
        Ast::Integer(_)
        | Ast::Decimal { .. }
        | Ast::Fraction { .. }
        | Ast::Mixed { .. }
        | Ast::Var(_)
        | Ast::Const(_) => Vec::new(),
    }
}

/// Bump the first digit run of the raw answer text.
///
/// The fallback serves answers the grammar does not read as one expression,
/// such as a multipart string (`amplitude = 3; period = pi/2`). A run keeps a
/// minus sign that stands right in front of it, so `-9` becomes `-10`.
fn mutate_via_text(answer: &str) -> Option<String> {
    let chars: Vec<char> = answer.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        if !chars[index].is_ascii_digit() {
            index += 1;
            continue;
        }
        let first_digit = index;
        let mut last_digit = index;
        while last_digit + 1 < chars.len() && chars[last_digit + 1].is_ascii_digit() {
            last_digit += 1;
        }
        let digits: String = chars[first_digit..=last_digit].iter().collect();
        let value: i128 = digits.parse().ok()?;
        // A minus directly in front of the run belongs to the number.
        let signed = first_digit > 0 && chars[first_digit - 1] == '-';
        let head = if signed { first_digit - 1 } else { first_digit };
        let mut out: String = chars[..head].iter().collect();
        if signed {
            out.push('-');
        }
        out.push_str(&value.checked_add(1)?.to_string());
        out.extend(chars[last_digit + 1..].iter());
        return Some(out);
    }
    None
}
