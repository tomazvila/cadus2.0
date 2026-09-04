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

mod word;

use word::{
    match_literal, raises_a_superscript, read_braced_after, read_frac, read_number, read_symbol,
    superscript_digit,
};

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
    let mut lexer = Lexer {
        chars,
        at: 0,
        depth,
        space_before: false,
        tokens: Vec::new(),
    };
    while let Some(c) = chars.get(lexer.at).copied() {
        lexer.step(c)?;
    }
    Ok(lexer.tokens)
}

/// The reader state of one run: the cursor, the brace depth, and the tokens so far.
struct Lexer<'a> {
    /// The characters of the run.
    chars: &'a [char],
    /// The index of the next character to read.
    at: usize,
    /// The brace depth of the run.
    depth: usize,
    /// True when whitespace stands between the cursor and the last token.
    space_before: bool,
    /// The tokens read so far.
    tokens: Vec<Token>,
}

impl Lexer<'_> {
    /// Read the construct that starts with `c` at the cursor.
    fn step(&mut self, c: char) -> Result<(), Undecidable> {
        match c {
            c if c.is_whitespace() => {
                self.space_before = true;
                self.at += 1;
                Ok(())
            }
            // The degree sign carries no value: 1.0 deletes it (`_UNICODE_SIMPLE`).
            '°' => {
                self.at += 1;
                Ok(())
            }
            '\\' => self.backslash_word(),
            '√' => {
                self.push(Tok::Root, 1);
                Ok(())
            }
            '%' => {
                self.push(Tok::Percent, 1);
                Ok(())
            }
            '^' => self.caret(),
            c if c.is_ascii_digit() || c == '.' => self.number(),
            c if c.is_ascii_alphabetic() => {
                self.identifier();
                Ok(())
            }
            c => self.glyph_or_symbol(c),
        }
    }

    /// Push one token of `width` characters, and clear the space flag.
    fn push(&mut self, kind: Tok, width: usize) {
        self.tokens.push(Token {
            kind,
            space_before: self.space_before,
        });
        self.space_before = false;
        self.at += width;
    }

    /// Read a number literal at the cursor.
    fn number(&mut self) -> Result<(), Undecidable> {
        let (text, next) = read_number(self.chars, self.at)?;
        let width = next - self.at;
        self.push(Tok::Num(text), width);
        Ok(())
    }

    /// Read a run of ASCII letters at the cursor.
    fn identifier(&mut self) {
        let mut end = self.at;
        while matches!(self.chars.get(end), Some(l) if l.is_ascii_alphabetic()) {
            end += 1;
        }
        let text: String = self.chars.get(self.at..end).unwrap_or(&[]).iter().collect();
        let width = end - self.at;
        self.push(Tok::Ident(text), width);
    }

    /// Read a vulgar glyph, a superscript run, or an operator at the cursor.
    fn glyph_or_symbol(&mut self, c: char) -> Result<(), Undecidable> {
        if let Some((_, numerator, denominator)) =
            VULGAR_FRACTIONS.iter().find(|(from, _, _)| *from == c)
        {
            self.push(
                Tok::Frac {
                    numerator: vec![Token::glued(Tok::Num((*numerator).to_string()))],
                    denominator: vec![Token::glued(Tok::Num((*denominator).to_string()))],
                },
                1,
            );
            return Ok(());
        }
        if superscript_digit(c).is_some() {
            return self.superscripts();
        }
        let (kind, width) = read_symbol(c, self.chars.get(self.at + 1).copied())?;
        self.push(kind, width);
        Ok(())
    }

    /// Read the LaTeX word that starts at the backslash at the cursor.
    ///
    /// `\frac` and `\sqrt` with braces become one token each. `\cdot` and `\times`
    /// become a product sign. `\left` and `\right` leave no token. Every other
    /// backslash is deleted and the letters after it stay, which is what 1.0 does
    /// (`sympy_check.py:101`).
    fn backslash_word(&mut self) -> Result<(), Undecidable> {
        let at = self.at;
        if let Some((numerator, denominator, next)) = read_frac(self.chars, at) {
            let kind = Tok::Frac {
                numerator: lex_run(numerator, self.depth + 1)?,
                denominator: lex_run(denominator, self.depth + 1)?,
            };
            self.push(kind, next - at);
            return Ok(());
        }
        if let Some((body, next)) = read_braced_after(self.chars, at, "\\sqrt") {
            let kind = Tok::Sqrt(lex_run(body, self.depth + 1)?);
            self.push(kind, next - at);
            return Ok(());
        }
        if let Some(next) = PRODUCT_WORDS
            .iter()
            .find_map(|word| match_literal(self.chars, at, word))
        {
            self.push(Tok::Star, next - at);
            return Ok(());
        }
        if let Some(next) = DROPPED_WORDS
            .iter()
            .find_map(|word| match_literal(self.chars, at, word))
        {
            self.at = next;
            return Ok(());
        }
        // 1.0 deletes every remaining backslash, so `\pi` is the name `pi` and
        // `\{2, 5\}` is the set `{2, 5}`.
        self.at += 1;
        Ok(())
    }

    /// Read a `^` and the exponent brackets it carries.
    ///
    /// `^{n}` becomes `Pow LParen … RParen`, so the exponent keeps the bracket that
    /// the parser reads. A bare `^` is one `Pow`.
    fn caret(&mut self) -> Result<(), Undecidable> {
        let at = self.at;
        self.push(Tok::Pow, 1);
        let Some((body, next)) = read_braced_after(self.chars, at, "^") else {
            return Ok(());
        };
        self.tokens.push(Token::glued(Tok::LParen));
        self.tokens.extend(lex_run(body, self.depth + 1)?);
        self.tokens.push(Token::glued(Tok::RParen));
        self.at = next;
        Ok(())
    }

    /// Read a run of superscript digits at the cursor into `Pow` and a number.
    ///
    /// The run raises the token in front of it. 1.0 leaves a detached run alone and
    /// the parser then refuses it, so a run that no token carries is refused here.
    fn superscripts(&mut self) -> Result<(), Undecidable> {
        let mut digits = String::new();
        let mut index = self.at;
        while let Some(digit) = self.chars.get(index).copied().and_then(superscript_digit) {
            digits.push(digit);
            index += 1;
        }
        if self.space_before || !raises_a_superscript(self.tokens.last()) {
            return Err(Undecidable::new("a character outside the grammar"));
        }
        self.tokens.push(Token::glued(Tok::Pow));
        self.tokens.push(Token::glued(Tok::Num(digits)));
        self.at = index;
        Ok(())
    }
}
