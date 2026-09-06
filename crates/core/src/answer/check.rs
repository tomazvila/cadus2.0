//! The deterministic answer check (A3, C4, V1, V2, V4).
//!
//! [`check`] is the one function that decides `correct` without a model. It reads
//! the authored answer and the learner answer, normalizes both (V4), parses both
//! into the decidable grammar (V1), canonicalizes both with exact arithmetic (D6),
//! and compares the two canonical forms.
//!
//! The comparison is an equality of two [`Canon`] values and nothing else. Every
//! algebraic rule lives in [`super::canon`], which is why a rational expression
//! needs no rung of its own: `2/x + 1/(x+1)` and `(3*x+2)/(x*(x+1))` reach one
//! canonical quotient there (M2 review 3, findings 9 to 13).
//!
//! # The rungs, in order
//!
//! 0. The learner answer is blank: `correct = false` (1.0 `blank_answer_grade`).
//! 1. The answer kind is not `numeric` and not `expression`: [`Outcome::Undecidable`].
//! 2. The two string keys are equal: `correct = true`.
//! 3. Both sides parse and canonicalize: the verdict is the equality of the two
//!    canonical forms, under the label rule of [`same_answer`]. If either side
//!    leaves the grammar, the outcome is [`Outcome::Undecidable`] (V2). If one
//!    side alone carries a unit, the outcome is [`Outcome::Undecidable`] with
//!    the reason `a unit is missing` (D-F3); the contract of D-F1 decides that
//!    pair later.
//! 4. The learner wrote a period-grouped integer whose value matches: `correct =
//!    true` with `notation = true` (spec section 2.4).
//! 5. The learner typed a DECIMAL, and the decimal is the exact rounding of the
//!    authored value to the digits the learner typed: `correct = true` with
//!    `notation = true` (ruling `D6-dec`). A rounding this build cannot decide
//!    exactly is [`Outcome::Undecidable`] (V2).
//! 6. Otherwise: `correct = false`.
//!
//! # Where 2.0 leaves 1.0
//!
//! - 1.0 runs its string rung BEFORE the kind gate, so `proof` and `multi-step`
//!   get a `True` on an exact string match. 2.0 puts the kind gate first: an
//!   undecidable kind is always [`Outcome::Undecidable`] (`docs/plans/M2.md`).
//! - 1.0 hands a wrong verifiable answer to the model. 2.0 decides it (A3).
//! - 1.0 has two float rungs, at 1e-9 and at 1e-6. 2.0 has none. `1/3` and
//!   `0.333333` are two different VALUES (D6, V1), and rung 5 reads the second
//!   one as the exact rounding of the first, which is a decidable question about
//!   the digits the learner typed and not a tolerance. `0.3334` is wrong for
//!   `1/3`; 1.0 accepted it for a big enough value.
//! - The check never panics and never raises. Every refusal is an
//!   [`Outcome::Undecidable`] and every miss is `correct = false`.

use num_bigint::BigInt;
use num_rational::BigRational;

use crate::curriculum::AnswerKind;

use super::Undecidable;
use super::ast::Ast;
use super::canon::{Canon, canon};
use super::normalize::{MAX_ANSWER_CHARS, is_grouped_integer, normalize};
use super::parse::parse;
use super::rounding::{Rounding, rounds_to};
use super::unit::lookup;

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
    decide(expected, learner, kind).0
}

/// The learner-facing note of a correct answer that carries the `notation` tag.
///
/// The note follows the 1.0 idiom of `deterministic_grade.py:82-91` (spec
/// section 7.1): it opens with `Correct value.`, it names ONE point of form, and
/// it cites the learner's own answer and the authored answer, not a stock
/// example. Every other outcome carries no note.
#[must_use]
pub fn notation_note(expected: &str, learner: &str, kind: AnswerKind) -> Option<String> {
    // `decide` names a form for a notation verdict and for nothing else.
    decide(expected, learner, kind)
        .1
        .map(|form| form.note(expected.trim(), learner.trim()))
}

/// The form a notation verdict names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Form {
    /// The learner grouped the thousands of an integer with periods.
    DotThousands,
    /// The learner wrote the value as a rounded decimal.
    Rounding,
}

impl Form {
    /// Write the note of this form.
    fn note(self, expected: &str, learner: &str) -> String {
        match self {
            Self::DotThousands => format!(
                "Correct value. One note on form: here a period is a decimal point, \
                 so ${expected}$ is the way to write it \u{2014} ${learner}$ reads as a decimal."
            ),
            Self::Rounding => format!(
                "Correct value. One note on form: the exact value is ${expected}$; \
                 ${learner}$ is a rounding of it."
            ),
        }
    }
}

/// Decide one pair, and name the form a notation verdict carries.
fn decide(expected: &str, learner: &str, kind: AnswerKind) -> (Outcome, Option<Form>) {
    // Rung 0. A blank answer is wrong, whatever the kind is. 1.0
    // `deterministic_grade.py:120-122` decides the same case the same way.
    if learner.trim().is_empty() {
        return (Outcome::decided(false), None);
    }
    // The input cap of 1.0 `api.py:91` bounds the work before any rewrite runs.
    if expected.chars().count() > MAX_ANSWER_CHARS || learner.chars().count() > MAX_ANSWER_CHARS {
        return (
            Outcome::Undecidable(Undecidable::new("the answer is longer than the input cap")),
            None,
        );
    }
    // Rung 1. Only `numeric` and `expression` claim a deterministic verdict (V2).
    if !matches!(kind, AnswerKind::Numeric | AnswerKind::Expression) {
        return (
            Outcome::Undecidable(Undecidable::new("the answer kind is not decidable")),
            None,
        );
    }
    let expected_text = normalize(expected);
    let learner_text = normalize(learner);
    // Rung 2. The two answers are the same string.
    if expected_text.string_key == learner_text.string_key {
        return (Outcome::decided(true), None);
    }
    // Rung 3. Both sides must be inside the grammar, or the checker has no
    // opinion. An authored answer outside the grammar never gets a verdict (V2).
    // The learner TREE stays in hand for rung 5: the canonical form holds the
    // learner value, and the tree holds the digits the learner typed.
    let expected_value = match canonical(&expected_text.source) {
        Ok(value) => value,
        Err(reason) => return (Outcome::Undecidable(reason), None),
    };
    let learner_tree = match parse(&learner_text.source) {
        Ok(tree) => tree,
        Err(reason) => return (Outcome::Undecidable(reason), None),
    };
    let learner_value = match canon(&learner_tree) {
        Ok(value) => value,
        Err(reason) => return (Outcome::Undecidable(reason), None),
    };
    if let Some(reason) = unit_gap(&expected_value, &learner_value) {
        return (Outcome::Undecidable(Undecidable::new(reason)), None);
    }
    if same_answer(&expected_value, &learner_value) {
        return (Outcome::decided(true), None);
    }
    // Rung 4. The learner side alone may carry the period grouping.
    if dot_thousands_variant(&expected_value, &learner_text.string_key) {
        return (Outcome::notation(), Some(Form::DotThousands));
    }
    // Rung 5. The learner side alone may carry a rounding (ruling `D6-dec`).
    match rounding_variant(&expected_value, &learner_tree) {
        Rounding::Same => return (Outcome::notation(), Some(Form::Rounding)),
        Rounding::Refused(reason) => {
            return (Outcome::Undecidable(Undecidable::new(reason)), None);
        }
        Rounding::Different | Rounding::NotANumber => {}
    }
    (Outcome::decided(false), None)
}

/// The refusal of a pair where one side alone carries a unit (D-F3).
///
/// The contract of D-F1 decides later whether a bare number is acceptable for
/// a measured answer; the grammar does not, so the pair gets no verdict (V2).
fn unit_gap(expected: &Canon, learner: &Canon) -> Option<&'static str> {
    let expected = matches!(unlabeled(expected), Canon::Quantity { .. });
    let learner = matches!(unlabeled(learner), Canon::Quantity { .. });
    match (expected, learner) {
        (true, false) => Some("a unit is missing"),
        (false, true) => Some("a unit on the learner side only"),
        _ => None,
    }
}

/// Whether the learner wrote the expected value as a rounded decimal.
///
/// The rule reads the learner TREE and not the learner value alone, because the
/// digit count is half the question: `0.33` and `0.3300` are one value and two
/// roundings, and `33/100` is neither. A learner integer and a learner fraction
/// carry no digit count, so the rule leaves them to the next rung.
///
/// A label falls away on both sides, which is the rule [`same_answer`] holds for
/// a one-sided label. A unit stands on both sides or on neither, because
/// [`unit_gap`] refused the one-sided pair: the rounding then reads the
/// expected value in the unit the learner typed, so `1.33 h` is the rounding of
/// `80 min` (D-F3).
fn rounding_variant(expected: &Canon, learner_tree: &Ast) -> Rounding {
    let expected = unlabeled(expected);
    let (Canon::Quantity { quantity, value }, Ast::Quantity { value: tree, unit }) =
        (expected, learner_tree)
    else {
        let Some((value, scale)) = typed_decimal(learner_tree) else {
            return Rounding::NotANumber;
        };
        return rounds_to(expected, &value, scale);
    };
    let Some((decimal, scale)) = typed_decimal(tree) else {
        return Rounding::NotANumber;
    };
    let Some(unit) = lookup(unit) else {
        return Rounding::NotANumber;
    };
    if unit.quantity != *quantity {
        return Rounding::Different;
    }
    match in_unit(value, &unit.factor()) {
        Some(expected) => rounds_to(&expected, &decimal, scale),
        None => Rounding::NotANumber,
    }
}

/// Write a number of base units in a unit of `factor` base units.
fn in_unit(value: &Canon, factor: &BigRational) -> Option<Canon> {
    match value {
        Canon::Rational(number) => Some(Canon::Rational(number / factor)),
        Canon::Radical(parts) => Some(Canon::Radical(
            parts
                .iter()
                .map(|(basis, coefficient)| (basis.clone(), coefficient / factor))
                .collect(),
        )),
        _ => None,
    }
}

/// The exact value and the count of digits after the point the learner typed,
/// when the learner answer is one decimal literal.
///
/// A scale of zero is not a decimal: the normalizer strips a trailing period
/// (V4), so `2.` is the integer 2 and it names no digit after the point. The
/// walk is a loop and not a recursion, so a run of sign tokens costs no stack.
fn typed_decimal(tree: &Ast) -> Option<(BigRational, u32)> {
    let mut node = tree;
    let mut negative = false;
    loop {
        match node {
            Ast::Decimal { mantissa, scale } => {
                let magnitude =
                    BigRational::new(mantissa.clone(), BigInt::from(10_u32).pow(*scale));
                let value = if negative { -magnitude } else { magnitude };
                return (*scale >= 1).then_some((value, *scale));
            }
            Ast::Neg(inner) => {
                negative = !negative;
                node = inner;
            }
            _ => return None,
        }
    }
}

/// The value under a label, or the value itself.
fn unlabeled(value: &Canon) -> &Canon {
    let mut node = value;
    while let Canon::Assign { value, .. } = node {
        node = value;
    }
    node
}

/// Whether the learner value is the authored value, under the label rule.
///
/// A leading `x =` on an answer is a label, and a label is a tolerance and not a
/// value. The rule follows `docs/reviews/M2-review-1.md`:
///
/// - If one side carries a label and the other side carries none, the label falls
///   away and the two values compare.
/// - If both sides carry a label, the two variable names must be the same name,
///   casefolded. `x = 4` and `y = 4` are therefore two different answers, which
///   is the 1.0 verdict.
///
/// Every other pair of canonical forms compares by equality.
#[must_use]
pub fn same_answer(expected: &Canon, learner: &Canon) -> bool {
    match (expected, learner) {
        (
            Canon::Assign {
                var: expected_var,
                value: expected_value,
            },
            Canon::Assign {
                var: learner_var,
                value: learner_value,
            },
        ) => {
            expected_var.to_lowercase() == learner_var.to_lowercase()
                && same_answer(expected_value, learner_value)
        }
        (Canon::Assign { value, .. }, other) => same_answer(value, other),
        (other, Canon::Assign { value, .. }) => same_answer(other, value),
        (left, right) => left == right,
    }
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
    canonical(&digits).is_ok_and(|value| same_answer(expected, &value))
}

/// Whether the whole string is `-?[1-9]\d{0,2}(\.\d{3})+` (1.0 `_DOT_GROUPS_RE`).
///
/// The shape is the comma-grouped integer of the normalizer with a period as the
/// separator, plus one rule of its own: the leading group starts with no zero.
fn is_dot_grouped(text: &str) -> bool {
    let lead = text.strip_prefix('-').unwrap_or(text).chars().next();
    is_grouped_integer(text, &['.']) && lead.is_some_and(|c| c != '0')
}
