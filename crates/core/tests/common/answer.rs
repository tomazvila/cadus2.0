//! The helpers the checker tests share: the two decidable kinds, the decided
//! outcome, and the refusal assertion.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    unused_imports
)]

pub use cadus_core::answer::{Outcome, Verdict, check};
pub use cadus_core::curriculum::AnswerKind;

/// The `numeric` answer kind.
pub const N: AnswerKind = AnswerKind::Numeric;
/// The `expression` answer kind.
pub const E: AnswerKind = AnswerKind::Expression;

/// The outcome of a decided check.
pub fn decided(correct: bool, notation: bool) -> Outcome {
    Outcome::Decided(Verdict { correct, notation })
}

/// The outcome of a correct answer that carries the notation tag.
pub fn rounded() -> Outcome {
    decided(true, true)
}

/// Assert that 2.0 refuses a verdict, and name the refusal.
#[track_caller]
pub fn assert_undecidable(expected: &str, learner: &str, kind: AnswerKind, reason: &str) {
    match check(expected, learner, kind) {
        Outcome::Undecidable(refusal) => assert_eq!(
            refusal.reason, reason,
            "{expected:?} against {learner:?} on {kind}"
        ),
        other => panic!("{expected:?} against {learner:?} on {kind} gave {other:?}"),
    }
}
