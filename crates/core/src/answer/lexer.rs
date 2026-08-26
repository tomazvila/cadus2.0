//! The token reader for the decidable answer grammar (V1, V4).
//!
//! The reader runs on the `source` string of [`crate::answer::normalize`], which
//! holds the whole-string steps of the V4 table and nothing else. Every LaTeX
//! construct and every multi-character glyph is a token of this module, and
//! [`crate::answer::parse`] builds the tree from the tokens.
//!
//! # Why the constructs live here
//!
//! Review round 3 (`docs/reviews/M2-review-3.md`, findings #1, #2, #3, #4, #6,
//! #8) measures four C4 false positives that a string rewrite produced: a space
//! inside a `\frac` brace changed the value, a `%` re-associated under `/` and
//! under `**`, and a product sign written in front of `\sqrt` glued itself to
//! the last letter of `\cdot`. The orchestrator ruling of that round makes each
//! construct a token with its own structure:
//!
//! - `\frac{A}{B}` is one [`Tok::Frac`] token. The two brace bodies are lexed
//!   recursively, so whitespace inside a brace changes no token at all.
//! - `\sqrt{A}` is one [`Tok::Sqrt`] token, and `\sqrt A` is the name `sqrt`.
//! - `√` is [`Tok::Root`], which takes exactly one primary (1.0 `:153-157`).
//! - `%` is the postfix [`Tok::Percent`], which the parser binds to the primary
//!   in front of it and to nothing else.
//! - `^{n}` becomes `Pow LParen … RParen`, so the exponent keeps its brackets.
//! - `\cdot` and `\times` are [`Tok::Star`]; `\left` and `\right` are dropped;
//!   `°` is dropped; a vulgar glyph is a [`Tok::Frac`] of two digit runs; a run
//!   of superscript digits is `Pow` and a number.
//!
//! A character the grammar does not know ends the read with [`Undecidable`].

use super::Undecidable;

/// The largest brace nesting the reader descends into.
///
/// A deeper string is refused, so a hostile `\frac{\frac{…` cannot exhaust the
/// stack.
const MAX_LEX_DEPTH: usize = 32;

/// The vulgar-fraction glyphs, with the numerator and the denominator of each.
///
/// Every glyph is one [`Tok::Frac`] token of two digit runs, in every position.
/// The parser decides whether a whole number in front of the token makes a mixed
/// number (review round 2, findings #1, #2, #3, #5, #6, #7).
const VULGAR_FRACTIONS: [(char, &str, &str); 5] = [
    ('½', "1", "2"),
    ('⅓', "1", "3"),
    ('⅔', "2", "3"),
    ('¼', "1", "4"),
    ('¾', "3", "4"),
];

/// The LaTeX words that are one product sign (1.0 `:96-97`).
const PRODUCT_WORDS: [&str; 2] = ["\\cdot", "\\times"];

/// The LaTeX words that carry no value and leave no token (1.0 `:99-100`).
const DROPPED_WORDS: [&str; 2] = ["\\left", "\\right"];

/// One token of the grammar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tok {
    /// A number literal: digits, with at most one point.
    Num(String),
    /// A fraction that the answer writes as one construct.
    ///
    /// The two bodies are the token lists of `\frac{A}{B}`, or the two digit
    /// runs of a vulgar glyph such as `½`. One construct is one token, whatever
    /// whitespace its braces hold (review round 3, findings #1, #2).
    Frac {
        /// The tokens above the bar.
        numerator: Vec<Token>,
        /// The tokens below the bar.
        denominator: Vec<Token>,
    },
    /// A square root that carries its argument in braces: `\sqrt{A}`.
    Sqrt(Vec<Token>),
    /// The radical glyph `√`, which takes exactly one primary after it.
    Root,
    /// The postfix percent sign. It divides the primary in front of it by 100.
    Percent,
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
    /// `**` or `^`
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
/// The mixed-number production `a b/c` is the one place the grammar reads a
/// space, so the flag travels with the token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// What the token is.
    pub kind: Tok,
    /// True when at least one space comes in front of the token.
    pub space_before: bool,
}

impl Token {
    /// Build a token that no space comes in front of.
    #[must_use]
    pub const fn glued(kind: Tok) -> Self {
        Self {
            kind,
            space_before: false,
        }
    }
}

/// Read `source` into tokens, or refuse it.
///
/// # Errors
///
/// Returns [`Undecidable`] when the string holds a character outside the
/// grammar, a malformed number such as `1.2.3`, or braces nested past
/// [`MAX_LEX_DEPTH`].
pub fn lex(source: &str) -> Result<Vec<Token>, Undecidable> {
    let chars: Vec<char> = source.chars().collect();
    lex_run(&chars, 0)
}

/// Read one run of characters into tokens, `depth` braces deep.
fn lex_run(chars: &[char], depth: usize) -> Result<Vec<Token>, Undecidable> {
    if depth > MAX_LEX_DEPTH {
        return Err(Undecidable::new("the answer nests too deeply"));
    }
    let mut tokens: Vec<Token> = Vec::new();
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
        // The degree sign carries no value: 1.0 deletes it (`_UNICODE_SIMPLE`).
        if c == '°' {
            i += 1;
            continue;
        }
        if c == '\\' {
            i = read_backslash_word(chars, i, depth, &mut tokens, &mut space_before)?;
            continue;
        }
        if c.is_ascii_digit() || c == '.' {
            let (text, next) = read_number(chars, i)?;
            tokens.push(Token {
                kind: Tok::Num(text),
                space_before,
            });
            i = next;
            space_before = false;
            continue;
        }
        if let Some((_, numerator, denominator)) =
            VULGAR_FRACTIONS.iter().find(|(from, _, _)| *from == c)
        {
            tokens.push(Token {
                kind: Tok::Frac {
                    numerator: vec![Token::glued(Tok::Num((*numerator).to_string()))],
                    denominator: vec![Token::glued(Tok::Num((*denominator).to_string()))],
                },
                space_before,
            });
            i += 1;
            space_before = false;
            continue;
        }
        if superscript_digit(c).is_some() {
            i = read_superscripts(chars, i, &mut tokens, space_before)?;
            space_before = false;
            continue;
        }
        if c == '√' {
            tokens.push(Token {
                kind: Tok::Root,
                space_before,
            });
            i += 1;
            space_before = false;
            continue;
        }
        if c == '%' {
            tokens.push(Token {
                kind: Tok::Percent,
                space_before,
            });
            i += 1;
            space_before = false;
            continue;
        }
        if c == '^' {
            i = read_caret(chars, i, depth, &mut tokens, space_before)?;
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
        let (kind, width) = read_symbol(chars, i)?;
        tokens.push(Token { kind, space_before });
        i += width;
        space_before = false;
    }
    Ok(tokens)
}

/// Read the LaTeX word that starts at the backslash at `at`.
///
/// `\frac` and `\sqrt` with braces become one token each. `\cdot` and `\times`
/// become a product sign. `\left` and `\right` leave no token. Every other
/// backslash is deleted and the letters after it stay, which is what 1.0 does
/// (`sympy_check.py:101`).
fn read_backslash_word(
    chars: &[char],
    at: usize,
    depth: usize,
    tokens: &mut Vec<Token>,
    space_before: &mut bool,
) -> Result<usize, Undecidable> {
    if let Some((numerator, denominator, next)) = read_frac(chars, at) {
        tokens.push(Token {
            kind: Tok::Frac {
                numerator: lex_run(numerator, depth + 1)?,
                denominator: lex_run(denominator, depth + 1)?,
            },
            space_before: *space_before,
        });
        *space_before = false;
        return Ok(next);
    }
    if let Some((body, next)) = read_braced_after(chars, at, "\\sqrt") {
        tokens.push(Token {
            kind: Tok::Sqrt(lex_run(body, depth + 1)?),
            space_before: *space_before,
        });
        *space_before = false;
        return Ok(next);
    }
    for word in PRODUCT_WORDS {
        if let Some(next) = match_literal(chars, at, word) {
            tokens.push(Token {
                kind: Tok::Star,
                space_before: *space_before,
            });
            *space_before = false;
            return Ok(next);
        }
    }
    for word in DROPPED_WORDS {
        if let Some(next) = match_literal(chars, at, word) {
            return Ok(next);
        }
    }
    // 1.0 deletes every remaining backslash, so `\pi` is the name `pi` and
    // `\{2, 5\}` is the set `{2, 5}`.
    Ok(at + 1)
}

/// Read a `^` and the exponent brackets it carries.
///
/// `^{n}` becomes `Pow LParen … RParen`, so the exponent keeps the bracket that
/// the parser reads. A bare `^` is one `Pow`.
fn read_caret(
    chars: &[char],
    at: usize,
    depth: usize,
    tokens: &mut Vec<Token>,
    space_before: bool,
) -> Result<usize, Undecidable> {
    tokens.push(Token {
        kind: Tok::Pow,
        space_before,
    });
    let Some((body, next)) = read_braced_after(chars, at, "^") else {
        return Ok(at + 1);
    };
    tokens.push(Token::glued(Tok::LParen));
    tokens.extend(lex_run(body, depth + 1)?);
    tokens.push(Token::glued(Tok::RParen));
    Ok(next)
}

/// Read a run of superscript digits at `at` into `Pow` and a number.
///
/// The run raises the token in front of it. 1.0 leaves a detached run alone and
/// the parser then refuses it, so a run that no token carries is refused here.
fn read_superscripts(
    chars: &[char],
    at: usize,
    tokens: &mut Vec<Token>,
    space_before: bool,
) -> Result<usize, Undecidable> {
    let mut digits = String::new();
    let mut index = at;
    while let Some(digit) = chars.get(index).copied().and_then(superscript_digit) {
        digits.push(digit);
        index += 1;
    }
    if space_before || !raises_a_superscript(tokens.last()) {
        return Err(Undecidable::new("a character outside the grammar"));
    }
    tokens.push(Token::glued(Tok::Pow));
    tokens.push(Token::glued(Tok::Num(digits)));
    Ok(index)
}

/// Whether a superscript run raises the token in front of it.
///
/// 1.0 attaches the run to a word character or a `)`, which is a number, a name,
/// a closing bracket, or a fraction glyph (`sympy_check.py:159-166`).
fn raises_a_superscript(previous: Option<&Token>) -> bool {
    matches!(
        previous.map(|token| &token.kind),
        Some(Tok::Num(_) | Tok::Ident(_) | Tok::RParen | Tok::Sqrt(_) | Tok::Frac { .. })
    )
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

/// Match `\frac{a}{b}` at `at` and return the two bodies and the index after it.
fn read_frac(chars: &[char], at: usize) -> Option<(&[char], &[char], usize)> {
    let (numerator, after_first) = read_braced_after(chars, at, "\\frac")?;
    if chars.get(after_first) != Some(&'{') {
        return None;
    }
    let close = matching_delimiter(chars, after_first, '{', '}')?;
    let denominator = chars.get(after_first + 1..close)?;
    Some((numerator, denominator, close + 1))
}

/// Match `keyword` at `at`, immediately followed by a balanced `{…}` group.
///
/// Return the group body and the index after the closing brace.
fn read_braced_after<'a>(
    chars: &'a [char],
    at: usize,
    keyword: &str,
) -> Option<(&'a [char], usize)> {
    let after_keyword = match_literal(chars, at, keyword)?;
    if chars.get(after_keyword) != Some(&'{') {
        return None;
    }
    let close = matching_delimiter(chars, after_keyword, '{', '}')?;
    Some((chars.get(after_keyword + 1..close)?, close + 1))
}

/// Match the characters of `literal` at `at` and return the index after them.
fn match_literal(chars: &[char], at: usize, literal: &str) -> Option<usize> {
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
fn matching_delimiter(chars: &[char], at: usize, open: char, close: char) -> Option<usize> {
    if chars.get(at) != Some(&open) {
        return None;
    }
    let mut depth = 0_usize;
    for (offset, c) in chars.get(at..)?.iter().enumerate() {
        if *c == open {
            depth += 1;
        } else if *c == close {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(at + offset);
            }
        }
    }
    None
}

/// Map a superscript digit to its ASCII digit.
const fn superscript_digit(c: char) -> Option<char> {
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
