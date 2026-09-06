//! The speed policy of unit f15: what a clock reading means, and what it never
//! means.
//!
//! # Why the module exists
//!
//! The grade path today keeps ONE timing safeguard: it clamps an elapsed time
//! above `expected_time_secs * 10` and tags the attempt `timing-unreliable`
//! (`crates/web/src/grade/mod.rs`). The clamp keeps a corrupt number out of the
//! fold, and it answers no pedagogical question. Two questions stay open:
//!
//! 1. A learner who answers a DRILL question in three seconds shows routine
//!    fluency. A learner who answers a lesson question in three minutes may
//!    reason correctly and slowly. The two readings are different claims.
//! 2. A slow CORRECT answer is not a mathematical error. A report that prints
//!    "slow" beside "wrong" invites the reader to treat them alike.
//!
//! This module answers both with one pure reading. It decides nothing about
//! progression: [`crate::projector`] owns the state, and the grade path owns the
//! tag. The reading is evidence, and a caller displays it or records it.
//!
//! # The thresholds, and which ones need calibration
//!
//! | Constant | Value | Source |
//! |---|---|---|
//! | [`UNRELIABLE_FACTOR`] | 10 | the existing grade clamp; keep the two in step |
//! | [`FLUENT_RATIO`] | 0.5 | NEEDS CALIBRATION against real drill timings |
//! | [`SLOW_RATIO`] | 2.0 | NEEDS CALIBRATION against real lesson timings |
//!
//! The two ratios are the policy of this unit and not a port of 1.0. Unit f20
//! carries the calibration list, and both belong on it.

use serde::{Deserialize, Serialize};
use std::fmt;

/// The multiple of the expected time above which a reading means nothing.
///
/// The learner left the tab open over lunch. The number is the clamp the grade
/// path already uses, so one reading never disagrees with the other.
pub const UNRELIABLE_FACTOR: f64 = 10.0;

/// At or under this multiple of the expected time, the answer is fluent.
pub const FLUENT_RATIO: f64 = 0.5;

/// Above this multiple of the expected time, the answer is slow.
pub const SLOW_RATIO: f64 = 2.0;

/// What the clock says about one attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeedOutcome {
    /// The clock reading is unusable: zero, negative, interrupted, or above
    /// [`UNRELIABLE_FACTOR`] times the expected time.
    Unreliable,
    /// At or under [`FLUENT_RATIO`] of the expected time.
    Fluent,
    /// Between the two ratios.
    Expected,
    /// Above [`SLOW_RATIO`] of the expected time, and still readable.
    Slow,
}

impl SpeedOutcome {
    /// The wire value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unreliable => "unreliable",
            Self::Fluent => "fluent",
            Self::Expected => "expected",
            Self::Slow => "slow",
        }
    }
}

impl fmt::Display for SpeedOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The learning claim one attempt supports.
///
/// The variants separate the four claims of Phase 3: routine fluency, correct
/// reasoning that took time, an answer that leaned on help, and a mathematical
/// error. A wrong answer NEVER carries a speed claim, because the clock says
/// nothing about why the mathematics failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeedClaim {
    /// The clock is unusable, so the attempt supports no speed claim.
    NotJudged,
    /// A correct, unaided, fluent answer to a ROUTINE question.
    RoutineFluency,
    /// A correct, unaided answer inside the expected time.
    Independent,
    /// A correct, unaided answer that took longer than [`SLOW_RATIO`] allows.
    /// It is CORRECT reasoning, and it is not an error.
    SlowReasoning,
    /// A correct answer that used a hint or a solution.
    Assisted,
    /// A wrong answer. The clock adds nothing.
    Incorrect,
}

impl SpeedClaim {
    /// The wire value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotJudged => "not_judged",
            Self::RoutineFluency => "routine_fluency",
            Self::Independent => "independent",
            Self::SlowReasoning => "slow_reasoning",
            Self::Assisted => "assisted",
            Self::Incorrect => "incorrect",
        }
    }

    /// Whether the claim reports a mathematical error.
    ///
    /// [`Self::SlowReasoning`] answers `false`: the learner reached the right
    /// answer, and the time is a separate fact.
    #[must_use]
    pub const fn is_error(self) -> bool {
        matches!(self, Self::Incorrect)
    }
}

impl fmt::Display for SpeedClaim {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One attempt, as the clock and the grader saw it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimingFacts {
    /// The seconds the learner spent, as the server measured them.
    pub elapsed_secs: i64,
    /// The authored `expected_time_secs` of the topic.
    pub expected_secs: i64,
    /// The grader marked the answer correct.
    pub correct: bool,
    /// The learner read a hint or a solution before the answer.
    pub assisted: bool,
    /// The session recorded a pause, a hidden tab, or a reconnect.
    pub interrupted: bool,
    /// The question asks for ROUTINE fluency: a drill question, or a topic the
    /// curriculum marks `drill`.
    pub routine: bool,
}

impl TimingFacts {
    /// One unaided, uninterrupted attempt of a non-routine question.
    #[must_use]
    pub const fn new(elapsed_secs: i64, expected_secs: i64, correct: bool) -> Self {
        Self {
            elapsed_secs,
            expected_secs,
            correct,
            assisted: false,
            interrupted: false,
            routine: false,
        }
    }

    /// The same attempt, marked as a routine-fluency question.
    #[must_use]
    pub const fn routine(mut self) -> Self {
        self.routine = true;
        self
    }

    /// The same attempt, marked as interrupted.
    #[must_use]
    pub const fn interrupted(mut self) -> Self {
        self.interrupted = true;
        self
    }

    /// The same attempt, marked as assisted.
    #[must_use]
    pub const fn assisted(mut self) -> Self {
        self.assisted = true;
        self
    }
}

/// What one attempt says about speed and about learning.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpeedReading {
    /// What the clock says.
    pub outcome: SpeedOutcome,
    /// The elapsed time over the expected time, or `None` for a reading the
    /// audit refuses.
    pub ratio: Option<f64>,
    /// The learning claim the attempt supports.
    pub claim: SpeedClaim,
}

/// Read one attempt.
///
/// The order is fixed. The reliability question comes first, because an
/// unusable clock supports no claim at all. The correctness question comes
/// next, because a wrong answer carries no speed claim. The ratio decides last.
#[must_use]
pub fn read(facts: &TimingFacts) -> SpeedReading {
    let ratio = ratio_of(facts);
    let Some(ratio) = ratio else {
        return SpeedReading {
            outcome: SpeedOutcome::Unreliable,
            ratio: None,
            claim: SpeedClaim::NotJudged,
        };
    };
    let outcome = if ratio <= FLUENT_RATIO {
        SpeedOutcome::Fluent
    } else if ratio > SLOW_RATIO {
        SpeedOutcome::Slow
    } else {
        SpeedOutcome::Expected
    };
    let claim = claim_of(facts, outcome);
    SpeedReading {
        outcome,
        ratio: Some(ratio),
        claim,
    }
}

/// The elapsed time over the expected time, or `None` for an unusable reading.
fn ratio_of(facts: &TimingFacts) -> Option<f64> {
    if facts.interrupted || facts.elapsed_secs <= 0 || facts.expected_secs <= 0 {
        return None;
    }
    let ratio = facts.elapsed_secs as f64 / facts.expected_secs as f64;
    if !ratio.is_finite() || ratio > UNRELIABLE_FACTOR {
        return None;
    }
    Some(ratio)
}

/// The claim one attempt supports, once the clock reads.
const fn claim_of(facts: &TimingFacts, outcome: SpeedOutcome) -> SpeedClaim {
    if !facts.correct {
        return SpeedClaim::Incorrect;
    }
    if facts.assisted {
        return SpeedClaim::Assisted;
    }
    match outcome {
        SpeedOutcome::Fluent if facts.routine => SpeedClaim::RoutineFluency,
        SpeedOutcome::Slow => SpeedClaim::SlowReasoning,
        _ => SpeedClaim::Independent,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FLUENT_RATIO, SLOW_RATIO, SpeedClaim, SpeedOutcome, TimingFacts, UNRELIABLE_FACTOR, read,
    };

    #[test]
    fn the_three_bands_read_off_the_expected_time() {
        assert_eq!(
            read(&TimingFacts::new(30, 60, true)).outcome,
            SpeedOutcome::Fluent
        );
        assert_eq!(
            read(&TimingFacts::new(31, 60, true)).outcome,
            SpeedOutcome::Expected
        );
        assert_eq!(
            read(&TimingFacts::new(120, 60, true)).outcome,
            SpeedOutcome::Expected
        );
        assert_eq!(
            read(&TimingFacts::new(121, 60, true)).outcome,
            SpeedOutcome::Slow
        );
        assert!((FLUENT_RATIO - 0.5).abs() < f64::EPSILON);
        assert!((SLOW_RATIO - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn an_unusable_clock_supports_no_claim_at_all() {
        let unusable = [
            TimingFacts::new(0, 60, true),
            TimingFacts::new(-5, 60, true),
            TimingFacts::new(30, 0, true),
            TimingFacts::new(601, 60, true),
            TimingFacts::new(30, 60, true).interrupted(),
        ];
        for facts in unusable {
            let reading = read(&facts);
            assert_eq!(reading.outcome, SpeedOutcome::Unreliable, "{facts:?}");
            assert_eq!(reading.claim, SpeedClaim::NotJudged, "{facts:?}");
            assert_eq!(reading.ratio, None, "{facts:?}");
            assert!(!reading.claim.is_error(), "{facts:?}");
        }
        // The clamp of the grade path is the same multiple, so the last readable
        // second reads and the first unreadable one does not.
        let edge = TimingFacts::new(600, 60, true);
        assert_eq!(read(&edge).outcome, SpeedOutcome::Slow);
        assert!((UNRELIABLE_FACTOR - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_slow_correct_answer_is_reasoning_and_never_an_error() {
        let reading = read(&TimingFacts::new(300, 60, true));
        assert_eq!(reading.outcome, SpeedOutcome::Slow);
        assert_eq!(reading.claim, SpeedClaim::SlowReasoning);
        assert!(!reading.claim.is_error());
        assert_eq!(reading.claim.as_str(), "slow_reasoning");
    }

    #[test]
    fn a_wrong_answer_carries_no_speed_claim_however_fast_it_arrives() {
        for elapsed in [5, 60, 300] {
            let reading = read(&TimingFacts::new(elapsed, 60, false));
            assert_eq!(reading.claim, SpeedClaim::Incorrect);
            assert!(reading.claim.is_error());
        }
        // The clock still reports its band, for the display.
        assert_eq!(
            read(&TimingFacts::new(5, 60, false)).outcome,
            SpeedOutcome::Fluent
        );
    }

    #[test]
    fn routine_fluency_needs_a_routine_question_and_an_unaided_answer() {
        let drill = TimingFacts::new(3, 6, true).routine();
        assert_eq!(read(&drill).claim, SpeedClaim::RoutineFluency);

        // The same speed on a lesson question is independence, not fluency.
        let lesson = TimingFacts::new(3, 6, true);
        assert_eq!(read(&lesson).claim, SpeedClaim::Independent);

        // Help changes the claim, whatever the clock says.
        assert_eq!(read(&drill.assisted()).claim, SpeedClaim::Assisted);
        assert_eq!(
            read(&TimingFacts::new(300, 60, true).assisted()).claim,
            SpeedClaim::Assisted
        );
    }

    #[test]
    fn every_wire_value_is_stable() {
        assert_eq!(SpeedOutcome::Unreliable.to_string(), "unreliable");
        assert_eq!(SpeedOutcome::Fluent.to_string(), "fluent");
        assert_eq!(SpeedOutcome::Expected.to_string(), "expected");
        assert_eq!(SpeedOutcome::Slow.to_string(), "slow");
        assert_eq!(SpeedClaim::NotJudged.to_string(), "not_judged");
        assert_eq!(SpeedClaim::RoutineFluency.to_string(), "routine_fluency");
        assert_eq!(SpeedClaim::Independent.to_string(), "independent");
        assert_eq!(SpeedClaim::Assisted.to_string(), "assisted");
        assert_eq!(SpeedClaim::Incorrect.to_string(), "incorrect");
    }
}

#[cfg(test)]
mod wire_tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    #[test]
    fn persisted_readings_round_trip_without_reclassifying_reasoning() {
        for facts in [
            TimingFacts::new(3, 30, true).routine(),
            TimingFacts::new(90, 30, true),
            TimingFacts::new(3, 30, true).interrupted(),
        ] {
            let reading = read(&facts);
            let value = serde_json::to_value(reading).unwrap();
            let restored: SpeedReading = serde_json::from_value(value).unwrap();
            assert_eq!(restored, reading);
            assert!(!restored.claim.is_error());
        }
    }
}
