//! Deterministic answer checking (V1, V2, V4, D6).
//!
//! The module reads a learner answer and an authored answer into one decidable
//! grammar. It holds exact values only: big integers, exact decimals, and exact
//! fractions. No step of it uses a float, and no step of it runs a search (D6, L2).
//!
//! The module has four stages:
//!
//! 1. [`normalize`] applies the whole-string steps of the V4 table and writes a
//!    reader source and a string key.
//! 2. [`parse`] lexes that source into tokens and reads the tokens into an
//!    [`Ast`], or refuses it with [`Undecidable`].
//! 3. [`canon`] reads an [`Ast`] into the canonical form [`Canon`].
//! 4. [`check`] compares two answers and returns an [`Outcome`]. [`rounding`]
//!    owns one rung of it: a learner decimal that is the exact rounding of the
//!    authored value (D6, ruling `D6-dec`).
//!
//! # A construct is a token, not a string rewrite
//!
//! Review round 3 (`docs/reviews/M2-review-3.md`) moves every LaTeX and glyph
//! construct of the V4 table out of [`normalize`] and into
//! [`lexer`]: `\frac{A}{B}`, `\sqrt{A}`, `\sqrt A`, `^{n}`, `\cdot`, `\times`,
//! `\left`, `\right`, `%`, the vulgar glyphs, `√`, the superscript digits, and
//! `°`. A string rewrite carries no structure, so a later pass re-associated it
//! and four C4 false positives came out of that (findings #1, #2, #3, #4, #6,
//! #8). A token carries its own structure, and the parser builds the node.
//!
//! Every refusal is an [`Undecidable`] value. No stage panics, on any input.

pub mod ast;
pub mod canon;
pub mod check;
pub mod lexer;
pub mod normalize;
pub mod parse;
pub mod rounding;
pub mod unit;

pub use ast::{Ast, Const, IneqOp};
pub use canon::{Atom, Basis, Canon, Monomial, Poly, canon};
pub use check::{Outcome, Verdict, canonical_form, check, notation_note, same_answer};
pub use normalize::{MAX_ANSWER_CHARS, Normalized, normalize};
pub use parse::{parse, parse_with_functions};
pub use rounding::{Rounding, rounds_to};
pub use unit::Quantity;

/// The answer is outside the decidable grammar, so the checker has no verdict (V2).
///
/// The reason is a fixed string. It names the production that refused the answer and
/// carries no learner text, so it is safe to log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("undecidable answer: {reason}")]
pub struct Undecidable {
    /// Why the grammar refused the answer.
    pub reason: &'static str,
}

impl Undecidable {
    /// Build a refusal with the given reason.
    #[must_use]
    pub const fn new(reason: &'static str) -> Self {
        Self { reason }
    }
}
