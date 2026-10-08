//! The second spellings of the lexer: the `arc` function names and the bar pair.
//!
//! A second spelling becomes the tokens of the first spelling, so the parser and
//! each later step get the same tokens for `arctan(x)` and `atan(x)`, and for
//! `|x|` and `abs(x)`.

use super::{Tok, Token};

/// The `arc` spellings of the inverse functions, with the name that the grammar knows.
const ARC_NAMES: [(&str, &str); 6] = [
    ("arcsec", "asec"),
    ("arccsc", "acsc"),
    ("arctan", "atan"),
    ("arcsin", "asin"),
    ("arccos", "acos"),
    ("lg", "log"),
];

/// Give the name that the grammar knows for a letter run.
///
/// `arctan`, `arcsin`, and `arccos` become `atan`, `asin`, and `acos`. Each other
/// letter run stays as it is, so `arctanh` and `arcsec` stay outside the grammar.
pub(super) fn grammar_name(text: String) -> String {
    match ARC_NAMES.iter().find(|(arc, _)| *arc == text) {
        Some((_, name)) => (*name).to_string(),
        None => text,
    }
}

/// Count the bars of the full answer.
pub(super) fn count_bars(chars: &[char]) -> usize {
    chars.iter().filter(|c| **c == '|').count()
}

/// Read the body of the bar pair that starts with the bar at `at`.
///
/// The answer is the characters between the two bars and the index after the
/// second bar. The answer is `None` when no second bar stands in this run.
pub(super) fn bar_body(chars: &[char], at: usize) -> Option<(Vec<char>, usize)> {
    let length = chars.iter().skip(at + 1).position(|c| *c == '|')?;
    let body = chars.iter().skip(at + 1).take(length).copied().collect();
    Some((body, at + length + 2))
}

/// Whether each bracket of `tokens` closes inside `tokens`.
///
/// The bar pair becomes `abs(` … `)`. A body such as `(x` takes the bracket that
/// the lexer adds, and `|(x|)` then reads as `abs((x))`. This function refuses
/// such a body. The three bracket shapes count as one, because an interval opens
/// with one shape and closes with a different shape.
pub(super) fn brackets_close(tokens: &[Token]) -> bool {
    let mut open = 0_usize;
    for token in tokens {
        match token.kind {
            Tok::LParen | Tok::LBrack | Tok::LBrace => open += 1,
            Tok::RParen | Tok::RBrack | Tok::RBrace => match open.checked_sub(1) {
                Some(less) => open = less,
                None => return false,
            },
            _ => {}
        }
    }
    open == 0
}
