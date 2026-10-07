//! The pure readers of the lexer: a number, a braced body, a literal, and a symbol.

use super::{Tok, Token};
use crate::answer::Undecidable;

/// Whether a superscript run raises the token in front of it.
///
/// 1.0 attaches the run to a word character or a `)`, which is a number, a name,
/// a closing bracket, or a fraction glyph (`sympy_check.py:159-166`).
pub(super) fn raises_a_superscript(previous: Option<&Token>) -> bool {
    matches!(
        previous.map(|token| &token.kind),
        Some(
            Tok::Num(_)
                | Tok::Ident(_)
                | Tok::RParen
                | Tok::Sqrt(_)
                | Tok::NthRoot(..)
                | Tok::Frac { .. }
        )
    )
}

/// Read a number literal at `at` and return its text and the index after it.
pub(super) fn read_number(chars: &[char], at: usize) -> Result<(String, usize), Undecidable> {
    let mut index = at;
    while matches!(chars.get(index), Some(c) if c.is_ascii_digit()) {
        index += 1;
    }
    let mut fraction_digits = 0_usize;
    if chars.get(index) == Some(&'.') {
        index += 1;
        while matches!(chars.get(index), Some(c) if c.is_ascii_digit()) {
            index += 1;
            fraction_digits += 1;
        }
        if fraction_digits == 0 {
            return Err(Undecidable::new("a point with no digit after it"));
        }
    }
    if chars.get(index) == Some(&'.') {
        return Err(Undecidable::new("a number with two points"));
    }
    let text: String = chars.get(at..index).unwrap_or(&[]).iter().collect();
    Ok((text, index))
}

/// Match `\frac{a}{b}` at `at` and return the two bodies and the index after it.
pub(super) fn read_frac(chars: &[char], at: usize) -> Option<(&[char], &[char], usize)> {
    let (numerator, after_first) = read_braced_after(chars, at, "\\frac")?;
    if chars.get(after_first) != Some(&'{') {
        return None;
    }
    let close = matching_delimiter(chars, after_first, '{', '}')?;
    let denominator = &chars[after_first + 1..close];
    Some((numerator, denominator, close + 1))
}

/// Match `\sqrt[n]{a}` at `at` and return the index digits, the body, and the
/// index after the closing brace.
///
/// The brackets hold one run of digits and nothing else; the parser checks the
/// range of the index.
pub(super) fn read_nth_root(chars: &[char], at: usize) -> Option<(String, &[char], usize)> {
    let mut index = match_literal(chars, at, "\\sqrt[")?;
    let start = index;
    while matches!(chars.get(index), Some(c) if c.is_ascii_digit()) {
        index += 1;
    }
    if index == start || chars.get(index) != Some(&']') {
        return None;
    }
    let digits: String = chars.get(start..index)?.iter().collect();
    if chars.get(index + 1) != Some(&'{') {
        return None;
    }
    let close = matching_delimiter(chars, index + 1, '{', '}')?;
    Some((digits, &chars[index + 2..close], close + 1))
}

/// Match `keyword` at `at`, immediately followed by a balanced `{…}` group.
///
/// Return the group body and the index after the closing brace.
pub(super) fn read_braced_after<'a>(
    chars: &'a [char],
    at: usize,
    keyword: &str,
) -> Option<(&'a [char], usize)> {
    let after_keyword = match_literal(chars, at, keyword)?;
    if chars.get(after_keyword) != Some(&'{') {
        return None;
    }
    let close = matching_delimiter(chars, after_keyword, '{', '}')?;
    Some((&chars[after_keyword + 1..close], close + 1))
}

/// Match the characters of `literal` at `at` and return the index after them.
pub(super) fn match_literal(chars: &[char], at: usize, literal: &str) -> Option<usize> {
    let mut index = at;
    for want in literal.chars() {
        if chars.get(index) != Some(&want) {
            return None;
        }
        index += 1;
    }
    Some(index)
}

/// Find the delimiter that closes the `open` at `at`. Nesting is counted.
///
/// The caller found `open` at `at`, so the walk starts one level deep and the
/// depth never goes below zero.
pub(super) fn matching_delimiter(
    chars: &[char],
    at: usize,
    open: char,
    close: char,
) -> Option<usize> {
    let mut depth = 0_usize;
    for (offset, c) in chars[at..].iter().enumerate() {
        if *c == open {
            depth += 1;
        } else if *c == close {
            depth -= 1;
            if depth == 0 {
                return Some(at + offset);
            }
        }
    }
    None
}

/// Map a superscript digit to its ASCII digit.
pub(super) const fn superscript_digit(c: char) -> Option<char> {
    match c {
        '⁰' => Some('0'),
        '¹' => Some('1'),
        '²' => Some('2'),
        '³' => Some('3'),
        '⁴' => Some('4'),
        '⁵' => Some('5'),
        '⁶' => Some('6'),
        '⁷' => Some('7'),
        '⁸' => Some('8'),
        '⁹' => Some('9'),
        _ => None,
    }
}

/// Read one operator or bracket from its character and the one after it.
///
/// The answer is the token and its width in characters.
pub(super) fn read_symbol(c: char, next: Option<char>) -> Result<(Tok, usize), Undecidable> {
    let token = match (c, next) {
        ('*', Some('*')) => return Ok((Tok::Pow, 2)),
        ('<', Some('=')) => return Ok((Tok::Le, 2)),
        ('>', Some('=')) => return Ok((Tok::Ge, 2)),
        ('+', _) => Tok::Plus,
        ('-', _) => Tok::Minus,
        ('*', _) => Tok::Star,
        ('/', _) => Tok::Slash,
        ('(', _) => Tok::LParen,
        (')', _) => Tok::RParen,
        ('[', _) => Tok::LBrack,
        (']', _) => Tok::RBrack,
        ('{', _) => Tok::LBrace,
        ('}', _) => Tok::RBrace,
        (',', _) => Tok::Comma,
        ('<', _) => Tok::Lt,
        ('>', _) => Tok::Gt,
        // The value label `x =` reaches the parser now (review finding #2). Every
        // other `=` is a relation, and the parser refuses it.
        ('=', _) => Tok::Eq,
        _ => return Err(Undecidable::new("a character outside the grammar")),
    };
    Ok((token, 1))
}

/// Read the subscript that follows an `_` at `at - 1`: a braced body or one run
/// of letters and digits. The answer is the subscript text and the index after it.
///
/// A braced body holds letters, digits, `+`, and `-` only, so `a_{n-1}` is the
/// name `a_n-1` and no other construct hides in a subscript.
pub(super) fn read_subscript(chars: &[char], at: usize) -> Option<(String, usize)> {
    if chars.get(at) == Some(&'{') {
        let close = matching_delimiter(chars, at, '{', '}')?;
        let body: String = chars[at + 1..close]
            .iter()
            .filter(|c| !c.is_whitespace())
            .collect();
        let clean = !body.is_empty()
            && body
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-');
        return clean.then_some((body, close + 1));
    }
    let run: String = chars[at.min(chars.len())..]
        .iter()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect();
    // A letter run keeps one letter: `a_n x` is a name and a product.
    let run: String = if run.chars().next()?.is_ascii_digit() {
        run.chars().take_while(char::is_ascii_digit).collect()
    } else {
        run.chars().take(1).collect()
    };
    let width = run.chars().count();
    Some((run, at + width))
}
