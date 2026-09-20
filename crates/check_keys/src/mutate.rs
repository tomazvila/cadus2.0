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
use cadus_core::answer::contract::function::CONSTANT_NAMES;
use cadus_core::answer::{AnswerContract, normalize, parse};
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

/// Build the wrong variant that the contract of the item needs.
///
/// A `function` key gets `2*(E) + v`: `v` is the first variable, and `E` is the
/// key with no leading `name =` label and, with `up_to_constant`, with no
/// trailing constant term. The `+1` variant of a formula stays correct with
/// `up_to_constant`, so it is not in use for this kind. Each other contract
/// gets [`mutate_plus_one`].
#[must_use]
pub fn mutant_for(answer: &str, contract: Option<&AnswerContract>) -> Option<String> {
    let Some(AnswerContract::Function {
        vars,
        up_to_constant,
        ..
    }) = contract
    else {
        return mutate_plus_one(answer);
    };
    let formula = answer
        .split_once('=')
        .map_or(answer, |(_, formula)| formula)
        .trim();
    let formula = if *up_to_constant {
        without_constant(formula)
    } else {
        formula
    };
    vars.first()
        .map(|variable| format!("2*({formula}) + {variable}"))
}

/// Remove one trailing `+ C`, `+ c`, `+ K` or `+ k` term.
fn without_constant(formula: &str) -> &str {
    CONSTANT_NAMES
        .iter()
        .find_map(|name| formula.strip_suffix(name))
        .map(str::trim_end)
        .and_then(|rest| rest.strip_suffix('+'))
        .map_or(formula, str::trim_end)
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

#[cfg(test)]
mod tests {
    use super::*;
    use cadus_core::answer::{Outcome, check_contract};

    fn function(up_to_constant: bool) -> AnswerContract {
        let tail = if up_to_constant {
            r#","up_to_constant":true"#
        } else {
            ""
        };
        let doc = format!(r#"{{"kind":"function","vars":["x","y"]{tail}}}"#);
        serde_json::from_str(&doc)
            .ok()
            .unwrap_or(AnswerContract::None)
    }

    #[test]
    fn a_function_key_gets_the_frozen_mutant() {
        let plain = function(false);
        let constant = function(true);
        for (key, contract, mutant) in [
            ("x/sqrt(x^2+9)", &plain, "2*(x/sqrt(x^2+9)) + x"),
            ("y = x^2/2 + C", &constant, "2*(x^2/2) + x"),
            ("x^2/2 +k", &constant, "2*(x^2/2) + x"),
            ("f = x*y + K ", &constant, "2*(x*y) + x"),
            ("x^2/2 + C", &plain, "2*(x^2/2 + C) + x"),
            ("x*c", &constant, "2*(x*c) + x"),
            ("x - c", &constant, "2*(x - c) + x"),
        ] {
            assert_eq!(mutant_for(key, Some(contract)).as_deref(), Some(mutant));
        }
    }

    #[test]
    fn each_other_contract_gets_the_plus_one_mutant() {
        assert_eq!(mutant_for("41", None).as_deref(), Some("42"));
        assert_eq!(
            mutant_for("41", Some(&AnswerContract::Exact)),
            mutate_plus_one("41")
        );
        assert_eq!(mutant_for("yes", Some(&AnswerContract::Exact)), None);
    }

    #[test]
    fn the_function_mutant_grades_wrong() {
        let x: AnswerContract = serde_json::from_str(r#"{"kind":"function","vars":["x"]}"#)
            .ok()
            .unwrap_or(AnswerContract::None);
        let constant: AnswerContract =
            serde_json::from_str(r#"{"kind":"function","vars":["x"],"up_to_constant":true}"#)
                .ok()
                .unwrap_or(AnswerContract::None);
        // The first two keys are the `function` items of the golden row
        // `FLOW/spec/golden/calc-chain-rule.row.json`.
        for (key, contract) in [
            ("x/sqrt(x^2+9)", &x),
            ("-3/(3x+1)^2", &x),
            ("y = x^2/2 + C", &constant),
            ("ln(x) + C", &constant),
        ] {
            let mutant = mutant_for(key, Some(contract)).unwrap_or_default();
            assert!(matches!(
                check_contract(key, key, contract.clone()),
                Outcome::Decided(verdict) if verdict.correct
            ));
            assert!(
                matches!(
                    check_contract(key, &mutant, contract.clone()),
                    Outcome::Decided(verdict) if !verdict.correct
                ),
                "{key}: {mutant}"
            );
        }
    }
}
