//! Exact values written as a normalized decimal coefficient times a power of ten.

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::Signed;

use super::{Canon, Undecidable};
use crate::answer::ast::Ast;
use crate::answer::canon::canon;
use crate::answer::lexer::{Tok, Token, lex};
use crate::answer::{normalize, parse};

pub(super) fn expected(text: &str) -> Result<Canon, Undecidable> {
    match read(text)? {
        Some(value) => Ok(value),
        None => Err(refused()),
    }
}

pub(super) fn equivalent(expected: &str, learner: &str) -> Result<bool, Undecidable> {
    let Some(expected) = read(expected)? else {
        return Err(refused());
    };
    let Some(learner) = read(learner)? else {
        return Ok(false);
    };
    Ok(expected == learner)
}

fn read(text: &str) -> Result<Option<Canon>, Undecidable> {
    let source = normalize(text).source;
    let ast = parse(&source)?;
    if !scientific_shape(&source)? {
        return Ok(None);
    }
    let Ast::Mul(factors) = &ast else {
        return Ok(None);
    };
    let [coefficient, power] = factors.as_slice() else {
        return Ok(None);
    };
    let Ast::Pow(base, _) = power else {
        return Ok(None);
    };
    if !matches!(base.as_ref(), Ast::Integer(value) if value == &BigInt::from(10_u8))
        || !decimal_literal(coefficient)
    {
        return Ok(None);
    }
    let Canon::Rational(value) = canon(coefficient)? else {
        return Ok(None);
    };
    let magnitude = value.abs();
    if magnitude < BigRational::from_integer(BigInt::from(1_u8))
        || magnitude >= BigRational::from_integer(BigInt::from(10_u8))
    {
        return Ok(None);
    }
    Ok(Some(canon(&ast)?))
}

fn scientific_shape(source: &str) -> Result<bool, Undecidable> {
    let tokens = lex(source)?;
    let tokens = strip_outer_group(&tokens);
    let Some(product) = one_top_level(tokens, is_product) else {
        return Ok(false);
    };
    let coefficient = strip_outer_group(&tokens[..product]);
    let power = strip_outer_group(&tokens[product + 1..]);
    let Some(caret) = one_top_level(power, |kind| matches!(kind, Tok::Pow)) else {
        return Ok(false);
    };
    let base = strip_outer_group(&power[..caret]);
    let exponent = strip_outer_group(&power[caret + 1..]);
    Ok(is_signed_number(coefficient, false)
        && matches!(base, [Token { kind: Tok::Num(value), .. }] if value == "10")
        && is_signed_number(exponent, true))
}

fn strip_outer_group(mut tokens: &[Token]) -> &[Token] {
    while tokens.len() >= 2
        && matches!(tokens.first().map(|token| &token.kind), Some(Tok::LParen))
        && matching_close(tokens) == Some(tokens.len() - 1)
    {
        tokens = &tokens[1..tokens.len() - 1];
    }
    tokens
}

fn matching_close(tokens: &[Token]) -> Option<usize> {
    let mut depth = 0_usize;
    for (at, token) in tokens.iter().enumerate() {
        match token.kind {
            Tok::LParen => depth += 1,
            Tok::RParen => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
    }
    None
}

fn one_top_level(tokens: &[Token], predicate: impl Fn(&Tok) -> bool) -> Option<usize> {
    let mut depth = 0_usize;
    let mut found = None;
    for (at, token) in tokens.iter().enumerate() {
        match token.kind {
            Tok::LParen => depth += 1,
            Tok::RParen => depth = depth.checked_sub(1)?,
            _ if depth == 0 && predicate(&token.kind) && found.replace(at).is_some() => {
                return None;
            }
            _ => {}
        }
    }
    (depth == 0).then_some(found).flatten()
}

fn is_product(kind: &Tok) -> bool {
    match kind {
        Tok::Star => true,
        Tok::Ident(word) => word.eq_ignore_ascii_case("x"),
        _ => false,
    }
}

fn is_signed_number(tokens: &[Token], integer: bool) -> bool {
    let number = match tokens {
        [
            Token {
                kind: Tok::Num(number),
                ..
            },
        ] => number,
        [
            Token {
                kind: Tok::Plus | Tok::Minus,
                ..
            },
            Token {
                kind: Tok::Num(number),
                ..
            },
        ] => number,
        _ => return false,
    };
    !integer || number.chars().all(|digit| digit.is_ascii_digit())
}

fn decimal_literal(ast: &Ast) -> bool {
    match ast {
        Ast::Integer(_) | Ast::Decimal { .. } => true,
        Ast::Neg(inner) => matches!(inner.as_ref(), Ast::Integer(_) | Ast::Decimal { .. }),
        _ => false,
    }
}

fn refused() -> Undecidable {
    Undecidable::new(
        "normalized scientific notation needs one decimal coefficient with magnitude in [1, 10) times 10 to an integer power",
    )
}
