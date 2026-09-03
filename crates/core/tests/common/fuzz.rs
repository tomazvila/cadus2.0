//! The deterministic generator of the fuzz tests. The fuzz needs no crate for this.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    unused_imports
)]

use cadus_core::answer::check;
use cadus_core::curriculum::AnswerKind;

/// A xorshift generator over one 64-bit word of state.
pub struct Rng(pub u64);

impl Rng {
    /// Return the next pseudo-random word.
    pub fn next(&mut self) -> u64 {
        let mut state = self.0;
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        self.0 = state;
        state
    }

    /// Return a value below `bound`.
    pub fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            return 0;
        }
        let bound = u64::try_from(bound).unwrap_or(u64::MAX);
        usize::try_from(self.next() % bound).unwrap_or(0)
    }
}

/// Run pairs of `case` answers through the checker for ten seconds, and count them.
///
/// The checker must return on every pair. A panic fails the test of the caller.
pub fn checker_never_panics(rng: &mut Rng, mut case: impl FnMut(&mut Rng) -> String) -> u64 {
    let start = std::time::Instant::now();
    let mut cases = 0_u64;
    while start.elapsed() < std::time::Duration::from_secs(10) {
        for _ in 0..64 {
            let expected = case(rng);
            let learner = case(rng);
            let kind = if rng.next().is_multiple_of(2) {
                AnswerKind::Numeric
            } else {
                AnswerKind::Expression
            };
            let probe = (expected.clone(), learner.clone(), kind);
            let result = std::panic::catch_unwind(move || check(&probe.0, &probe.1, probe.2));
            assert!(
                result.is_ok(),
                "the checker panicked on {expected:?} against {learner:?} on {kind}"
            );
            cases += 1;
        }
    }
    cases
}
