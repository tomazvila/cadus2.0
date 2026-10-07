//! A polynomial division with a remainder: `x + 2 remainder 3`.
//!
//! The key is the pair quotient and remainder. The contract stores the divisor,
//! so the learner may also write the mixed form `x+2+3/(x+1)`. Each spelling of
//! the pair (`x+2 r 3`, `quotient x+2, remainder 3`) reads as the pair.

use super::{Canon, Undecidable, canonical_form};
use crate::answer::{Ast, canon, normalize, parse};

/// The authored key: a pair of a quotient and a remainder, with a divisor that reads.
pub(super) fn expected(divisor: &str, key: &str) -> Result<Canon, Undecidable> {
    pair(key)?;
    divisor_ast(divisor)?;
    canonical_form(key)
}

/// Whether the learner answer is the pair, or the mixed form of the pair.
pub(super) fn equivalent(divisor: &str, key: &str, learner: &str) -> Result<bool, Undecidable> {
    let (quotient, remainder) = pair(key)?;
    let divisor = divisor_ast(divisor)?;
    let learner = canonical_form(learner)?;
    if learner == canonical_form(key)? {
        return Ok(true);
    }
    let mixed = Ast::Add(vec![
        quotient,
        Ast::Div(Box::new(remainder), Box::new(divisor)),
    ]);
    Ok(canon(&mixed)? == learner)
}

fn pair(key: &str) -> Result<(Ast, Ast), Undecidable> {
    match parse(&normalize(key).source)? {
        Ast::Tuple(mut items) if items.len() == 2 => {
            let remainder = items.pop().unwrap_or(Ast::Integer(0.into()));
            let quotient = items.pop().unwrap_or(Ast::Integer(0.into()));
            Ok((quotient, remainder))
        }
        _ => Err(Undecidable::new(
            "a polynomial division key is a quotient and a remainder",
        )),
    }
}

fn divisor_ast(divisor: &str) -> Result<Ast, Undecidable> {
    parse(&normalize(divisor).source)
}
