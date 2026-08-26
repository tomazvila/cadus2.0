//! The token reader for the decidable answer grammar (V1).
//!
//! The reader runs on the `source` string of [`crate::answer::normalize`], so every
//! LaTeX and Unicode rewrite is already done. A character the grammar does not know
//! ends the read with [`Undecidable`].

use super::Undecidable;
use super::normalize::{FRACTION_CLOSE, FRACTION_OPEN};

/// One token of the grammar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tok {
    /// A number literal: digits, with at most one point.
    Num(String),
    /// A literal fraction that the answer writes as one glyph.
    ///
    /// [`crate::answer::normalize`] writes the vulgar glyphs (`½`) and a
    /// `\frac{b}{c}` of two digit runs in this one spelling, so the parser reads
    /// all five mixed-number spellings through one production (review round 2,
    /// findings #1, #2). Both parts are runs of ASCII digits.
    Frac {
        /// The digits above the bar.
        numerator: String,
        /// The digits below the bar.
        denominator: String,
    },
    /// A run of ASCII letters.
    Ident(String),
    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `*`
    Star,
    /// `/`
    Slash,
    /// `**`
    Pow,
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `[`
    LBrack,
    /// `]`
    RBrack,
    /// `{`
    LBrace,
    /// `}`
    RBrace,
    /// `,`
    Comma,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `=`
    Eq,
}

/// A token and whether whitespace comes in front of it.
///
/// The mixed-number production `a b/c` is the one place the grammar reads a space,
/// so the flag travels with the token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// What the token is.
    pub kind: Tok,
    /// True when at least one space comes in front of the token.
    pub space_before: bool,
}

/// Read `source` into tokens, or refuse it.
///
/// # Errors
///
/// Returns [`Undecidable`] when the string holds a character outside the grammar,
/// or a malformed number such as `1.2.3`.
pub fn lex(source: &str) -> Result<Vec<Token>, Undecidable> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    let mut space_before = false;
    while i < chars.len() {
        let Some(c) = chars.get(i).copied() else {
            break;
        };
        if c.is_whitespace() {
            space_before = true;
            i += 1;
            continue;
        }
        if c.is_ascii_digit() || c == '.' {
            let (text, next) = read_number(&chars, i)?;
            tokens.push(Token {
                kind: Tok::Num(text),
                space_before,
            });
            i = next;
            space_before = false;
            continue;
        }
        if c == FRACTION_OPEN {
            let (kind, next) = read_fraction(&chars, i)?;
            tokens.push(Token { kind, space_before });
            i = next;
            space_before = false;
            continue;
        }
        if c.is_ascii_alphabetic() {
            let mut end = i;
            while matches!(chars.get(end), Some(l) if l.is_ascii_alphabetic()) {
                end += 1;
            }
            let text: String = chars.get(i..end).unwrap_or(&[]).iter().collect();
            tokens.push(Token {
                kind: Tok::Ident(text),
                space_before,
            });
            i = end;
            space_before = false;
            continue;
        }
        let (kind, width) = read_symbol(&chars, i)?;
        tokens.push(Token { kind, space_before });
        i += width;
        space_before = false;
    }
    Ok(tokens)
}

/// Read a number literal at `at` and return its text and the index after it.
fn read_number(chars: &[char], at: usize) -> Result<(String, usize), Undecidable> {
    let mut index = at;
    let mut digits = 0_usize;
    while matches!(chars.get(index), Some(c) if c.is_ascii_digit()) {
        index += 1;
        digits += 1;
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
    if digits == 0 && fraction_digits == 0 {
        return Err(Undecidable::new("a point that starts no number"));
    }
    if chars.get(index) == Some(&'.') {
        return Err(Undecidable::new("a number with two points"));
    }
    let text: String = chars.get(at..index).unwrap_or(&[]).iter().collect();
    Ok((text, index))
}

/// Read a literal-fraction token at `at` and return it and the index after it.
///
/// The shape is one open mark, a run of digits, a slash, a run of digits, and
/// one close mark. [`crate::answer::normalize`] is the only writer of the marks.
fn read_fraction(chars: &[char], at: usize) -> Result<(Tok, usize), Undecidable> {
    let malformed = Undecidable::new("a fraction mark the reader cannot read");
    let mut index = at + 1;
    let numerator = read_digits(chars, index);
    index += numerator.chars().count();
    if chars.get(index) != Some(&'/') {
        return Err(malformed);
    }
    index += 1;
    let denominator = read_digits(chars, index);
    index += denominator.chars().count();
    if chars.get(index) != Some(&FRACTION_CLOSE) {
        return Err(malformed);
    }
    if numerator.is_empty() || denominator.is_empty() {
        return Err(malformed);
    }
    Ok((
        Tok::Frac {
            numerator,
            denominator,
        },
        index + 1,
    ))
}

/// Read the run of ASCII digits that starts at `at`.
fn read_digits(chars: &[char], at: usize) -> String {
    let mut index = at;
    while matches!(chars.get(index), Some(c) if c.is_ascii_digit()) {
        index += 1;
    }
    chars.get(at..index).unwrap_or(&[]).iter().collect()
}

/// Read one operator or bracket at `at` and return it with its width.
fn read_symbol(chars: &[char], at: usize) -> Result<(Tok, usize), Undecidable> {
    let Some(c) = chars.get(at).copied() else {
        return Err(Undecidable::new("the string ends inside a token"));
    };
    let next = chars.get(at + 1).copied();
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
