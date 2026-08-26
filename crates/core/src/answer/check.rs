//! The deterministic answer check (A3, C4, V1, V2, V4).
//!
//! [`check`] is the one function that decides `correct` without a model. It reads
//! the authored answer and the learner answer, normalizes both (V4), parses both
//! into the decidable grammar (V1), canonicalizes both with exact arithmetic (D6),
//! and compares the two canonical forms.
//!
//! # The rungs, in order
//!
//! 0. The learner answer is blank: `correct = false` (1.0 `blank_answer_grade`).
//! 1. The answer kind is not `numeric` and not `expression`: [`Outcome::Undecidable`].
//! 2. The two string keys are equal: `correct = true`.
//! 3. Both sides parse and canonicalize: the verdict is the equality of the two
//!    canonical forms. If either side leaves the grammar, the outcome is
//!    [`Outcome::Undecidable`] (V2).
//! 4. The learner wrote a period-grouped integer whose value matches: `correct =
//!    true` with `notation = true` (spec section 2.4).
//! 5. Otherwise: `correct = false`.
//!
//! # Where 2.0 leaves 1.0
//!
//! - 1.0 runs its string rung BEFORE the kind gate, so `proof` and `multi-step`
//!   get a `True` on an exact string match. 2.0 puts the kind gate first: an
//!   undecidable kind is always [`Outcome::Undecidable`] (`docs/plans/M2.md`).
//! - 1.0 hands a wrong verifiable answer to the model. 2.0 decides it (A3).
//! - 1.0 has two float rungs, at 1e-9 and at 1e-6. 2.0 has none: `1/3` and
//!   `0.333333` are different values (D6, V1).
//! - The check never panics and never raises. Every refusal is an
//!   [`Outcome::Undecidable`] and every miss is `correct = false`.

use crate::curriculum::AnswerKind;

use super::Undecidable;
use super::canon::{Canon, canon};
use super::normalize::{MAX_ANSWER_CHARS, normalize};
use super::parse::parse;

/// The decision of the checker on one answer pair (C4).
///
/// The checker returns `correct` and `notation` and nothing else. It reports no
/// work quality and no error tag: the caller supplies the neutral tier, and the
/// asynchronous diagnosis owns the prose (A4, spec section 10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Verdict {
    /// True when the learner answer is the same value as the authored answer.
    pub correct: bool,
    /// True when the value matches only under the period-grouping reading.
    pub notation: bool,
}

/// The result of one check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The checker has a verdict.
    Decided(Verdict),
    /// The checker has no verdict, and the caller must grade the answer another
    /// way (V2, A4).
    Undecidable(Undecidable),
}

impl Outcome {
    /// Build a decided outcome with no notation note.
    const fn decided(correct: bool) -> Self {
        Self::Decided(Verdict {
            correct,
            notation: false,
        })
    }

    /// Build a decided, correct outcome that carries the notation note.
    const fn notation() -> Self {
        Self::Decided(Verdict {
            correct: true,
            notation: true,
        })
    }
}

/// Decide whether a learner answer is the authored answer.
///
/// The function never panics, on any input string. Every input it cannot read
/// deterministically becomes [`Outcome::Undecidable`].
#[must_use]
pub fn check(expected: &str, learner: &str, kind: AnswerKind) -> Outcome {
    // Rung 0. A blank answer is wrong, whatever the kind is. 1.0
    // `deterministic_grade.py:120-122` decides the same case the same way.
    if learner.trim().is_empty() {
        return Outcome::decided(false);
    }
    // The input cap of 1.0 `api.py:91` bounds the work before any rewrite runs.
    if expected.chars().count() > MAX_ANSWER_CHARS || learner.chars().count() > MAX_ANSWER_CHARS {
        return Outcome::Undecidable(Undecidable::new("the answer is longer than the input cap"));
    }
    // Rung 1. Only `numeric` and `expression` claim a deterministic verdict (V2).
    if !matches!(kind, AnswerKind::Numeric | AnswerKind::Expression) {
        return Outcome::Undecidable(Undecidable::new("the answer kind is not decidable"));
    }
    let expected_text = normalize(expected);
    let learner_text = normalize(learner);
    // Rung 2. The two answers are the same string.
    if expected_text.string_key == learner_text.string_key {
        return Outcome::decided(true);
    }
    // Rung 3. Both sides must be inside the grammar, or the checker has no
    // opinion. An authored answer outside the grammar never gets a verdict (V2).
    let expected_value = match canonical(&expected_text.source) {
        Ok(value) => value,
        Err(reason) => return Outcome::Undecidable(reason),
    };
    let learner_value = match canonical(&learner_text.source) {
        Ok(value) => value,
        Err(reason) => return Outcome::Undecidable(reason),
    };
    if expected_value == learner_value {
        return Outcome::decided(true);
    }
    // Rung 4. The learner side alone may carry the period grouping.
    if dot_thousands_variant(&expected_value, &learner_text.string_key) {
        return Outcome::notation();
    }
    Outcome::decided(false)
}

/// Normalize, parse, and canonicalize one answer string.
///
/// # Errors
///
/// Returns [`Undecidable`] when the string leaves the decidable grammar (V2).
pub fn canonical_form(text: &str) -> Result<Canon, Undecidable> {
    canonical(&normalize(text).source)
}

/// Parse and canonicalize one already-normalized source string.
fn canonical(source: &str) -> Result<Canon, Undecidable> {
    canon(&parse(source)?)
}

/// Whether the learner wrote the expected value with periods as thousands marks.
///
/// This is 1.0 `dot_thousands_variant` (`sympy_check.py:109-130`). Only the
/// learner side may carry the grouping, the whole string must be one grouped
/// integer, and the leading group must not start with a zero — that zero is the
/// 1000x hole of spec section 7.2, so `8` and `0.008` stay different answers.
fn dot_thousands_variant(expected: &Canon, learner_key: &str) -> bool {
    if !is_dot_grouped(learner_key) {
        return false;
    }
    let digits: String = learner_key.chars().filter(|c| *c != '.').collect();
    match canonical(&digits) {
        Ok(value) => value == *expected,
        Err(_) => false,
    }
}

/// Whether the whole string is `-?[1-9]\d{0,2}(\.\d{3})+` (1.0 `_DOT_GROUPS_RE`).
fn is_dot_grouped(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0;
    if chars.first() == Some(&'-') {
        index = 1;
    }
    if !matches!(chars.get(index), Some(c) if c.is_ascii_digit() && *c != '0') {
        return false;
    }
    index += 1;
    let mut lead = 1;
    while lead < 3 && matches!(chars.get(index), Some(c) if c.is_ascii_digit()) {
        index += 1;
        lead += 1;
    }
    let mut groups = 0_usize;
    while chars.get(index) == Some(&'.') {
        index += 1;
        let mut digits = 0;
        while digits < 3 && matches!(chars.get(index), Some(c) if c.is_ascii_digit()) {
            index += 1;
            digits += 1;
        }
        if digits != 3 {
            return false;
        }
        groups += 1;
    }
    groups >= 1 && index == chars.len()
}
