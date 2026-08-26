//! Deterministic answer checking (V1, V2, V4, D6).
//!
//! The module reads a learner answer and an authored answer into one decidable
//! grammar. It holds exact values only: big integers, exact decimals, and exact
//! fractions. No step of it uses a float, and no step of it runs a search (D6, L2).
//!
//! Unit U1 supplies the two front stages:
//!
//! 1. [`normalize`] rewrites learner notation into a parser source and a string key.
//! 2. [`parse`] reads that source into an [`Ast`], or refuses it with [`Undecidable`].
//!
//! Every refusal is an [`Undecidable`] value. The parser never panics, on any input.

pub mod ast;
pub mod lexer;
pub mod normalize;
pub mod parse;

pub use ast::{Ast, Const, IneqOp};
pub use normalize::{MAX_ANSWER_CHARS, Normalized, normalize};
pub use parse::parse;

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
