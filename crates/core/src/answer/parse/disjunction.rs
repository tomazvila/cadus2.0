//! Finite solution disjunctions preserve the unknown and each exact value.

use super::{Parser, is_variable_name};
use crate::answer::lexer::{Tok, Token};
use crate::answer::{Ast, Undecidable};

/// Read `A or B` at the answer boundary. Each branch is a value or an assignment.
pub(super) fn read(tokens: &[Token], extra: &[&str]) -> Option<Result<Ast, Undecidable>> {
    let mut depth = 0_usize;
    let mut start = 0;
    let mut branches = Vec::new();
    for (at, token) in tokens.iter().enumerate() {
        match &token.kind {
            Tok::LParen | Tok::LBrack | Tok::LBrace => depth += 1,
            Tok::RParen | Tok::RBrack | Tok::RBrace => depth = depth.saturating_sub(1),
            Tok::Ident(name) if name == "or" && depth == 0 => {
                branches.push(&tokens[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    if branches.is_empty() {
        return None;
    }
    branches.push(&tokens[start..]);
    Some(solution_set(&branches, extra))
}

fn solution_set(branches: &[&[Token]], extra: &[&str]) -> Result<Ast, Undecidable> {
    if branches.len() > 16 {
        return Err(Undecidable::new("a disjunction exceeds 16 alternatives"));
    }
    let mut label: Option<String> = None;
    let mut values = Vec::with_capacity(branches.len());
    for tokens in branches {
        let mut parser = Parser {
            tokens,
            at: 0,
            depth: 0,
            extra,
        };
        let mut value = parser.parse_answer()?;
        if parser.at != tokens.len() {
            return Err(Undecidable::new("a disjunction branch has trailing text"));
        }
        if let Ast::Assign { var, value: inner } = value {
            let name = var.to_lowercase();
            if !is_variable_name(&var, extra)
                || label.as_ref().is_some_and(|previous| previous != &name)
            {
                return Err(Undecidable::new(
                    "a solution disjunction requires one unknown",
                ));
            }
            label = Some(name);
            value = *inner;
        }
        if !scalar(&value) {
            return Err(Undecidable::new(
                "a disjunction requires finite scalar solutions",
            ));
        }
        values.push(value);
    }
    let value = Ast::Set(values);
    Ok(match label {
        Some(var) => Ast::Assign {
            var,
            value: Box::new(value),
        },
        None => value,
    })
}

fn scalar(value: &Ast) -> bool {
    match value {
        Ast::Tuple(_)
        | Ast::List(_)
        | Ast::Set(_)
        | Ast::Assign { .. }
        | Ast::Interval { .. }
        | Ast::Ineq { .. }
        | Ast::Chain { .. } => false,
        Ast::Add(items) | Ast::Mul(items) | Ast::Func(_, items) => items.iter().all(scalar),
        Ast::Div(left, right) => scalar(left) && scalar(right),
        Ast::Neg(inner)
        | Ast::Sqrt(inner)
        | Ast::Pow(inner, _)
        | Ast::RationalPow { base: inner, .. }
        | Ast::Quantity { value: inner, .. } => scalar(inner),
        _ => true,
    }
}
