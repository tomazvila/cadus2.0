//! The pure grade (spec section 5): the tier, the assisted rule, and the clock.

use super::*;

/// Grade one submission with no model call at all (A3, spec section 5.1).
///
/// The tiers are the D-M5-2 ruling:
///
/// - correct: `nearly_perfect`, the NEUTRAL tier. `perfect` is a bonus for
///   method the checker never reads, so it is never awarded, and every tier
///   below is a penalty for a flaw the checker never observes.
/// - a decided miss: `nearly_passable` (XP ×0.3, FIRe q 0.4), below the pass
///   threshold of 0.7, so a wrong answer is never priced as a pass.
/// - a blank: `poor`, which is the tier of 1.0's `blank_answer_grade`.
///
/// An answer the checker refuses ([`Outcome::Undecidable`]: the input cap, an
/// exit from the grammar, or a kind no checker decides) is UNGRADED (D-F2). It is
/// not a miss: the outcome carries the refusal reason, the fold ignores the
/// attempt, and the tier stays `nearly_passable` on the row while no grade reads
/// it. It is never a model verdict and never a pass.
#[must_use]
pub fn deterministic_grade(expected: &str, answer: &str, kind: AnswerKind) -> Grade {
    grade_outcome(answer, check(expected, answer, kind))
}

/// Grade the policy captured in the served item (D-F1, C2).
#[must_use]
pub fn grade_item(
    expected: &cadus_core::pool::PoolAnswer,
    answer: &str,
    kind: AnswerKind,
) -> Grade {
    if kind == AnswerKind::Proof {
        return ungraded_grade(PROOF_UNGRADED);
    }
    let Some(contract) = expected.answer_contract.clone() else {
        return deterministic_grade(&expected.answer, answer, kind);
    };
    grade_outcome(
        answer,
        cadus_core::answer::check_contract(&expected.answer, answer, contract),
    )
}

fn grade_outcome(answer: &str, outcome: Outcome) -> Grade {
    if answer.trim().is_empty() && matches!(outcome, Outcome::Decided(_)) {
        return Grade {
            correct: false,
            outcome: AttemptOutcome::Incorrect,
            work_quality: WorkQuality::Poor,
            error_tags: vec![TAG_BLANK_ANSWER.to_string()],
        };
    }
    match outcome {
        Outcome::Decided(verdict) if verdict.correct => Grade {
            correct: true,
            outcome: AttemptOutcome::Correct,
            work_quality: WorkQuality::NearlyPerfect,
            error_tags: if verdict.notation {
                vec![TAG_NOTATION.to_string()]
            } else {
                Vec::new()
            },
        },
        Outcome::Decided(_) => Grade {
            correct: false,
            outcome: AttemptOutcome::Incorrect,
            work_quality: WorkQuality::NearlyPassable,
            error_tags: Vec::new(),
        },
        Outcome::Undecidable(refusal) => ungraded_grade(refusal.reason),
    }
}

/// The grade of an answer with no deterministic verdict (D-F2).
///
/// `reason` names why in one phrase, and the client shows it. The tier stays
/// `nearly_passable` on the row, and the fold reads neither the tier nor
/// `correct` of an ungraded attempt.
#[must_use]
pub fn ungraded_grade(reason: &str) -> Grade {
    Grade {
        correct: false,
        outcome: AttemptOutcome::Ungraded {
            reason: reason.to_owned(),
        },
        work_quality: WorkQuality::NearlyPassable,
        error_tags: Vec::new(),
    }
}

/// Whether one submission is reference-assisted (H3, spec section 5.4).
///
/// An attempt is assisted when the learner took a hint on this problem or the
/// client flags it (`api.py:1360`).
///
/// A QUIZ attempt is never assisted. 1.0 answers a quiz in `_quiz_answer` and
/// returns from it BEFORE the assisted rule runs (`api.py:1349-1358`), so no
/// quiz answer of 1.0 carries the flag and no quiz answer reaches the H3 reply.
/// The order matters here and not only for parity: the H3 reply names
/// `expected` and `solution`, and a quiz reveals NOTHING before its batch reveal
/// (trap W7). Without this rule a client that sends `"assisted": true` with a
/// correct quiz answer reads the authored answer of every remaining question.
#[must_use]
pub fn reference_assisted(task_type: TaskType, client_flag: bool, hints_given: usize) -> bool {
    if task_type == TaskType::Quiz {
        return false;
    }
    client_flag || hints_given > 0
}

/// The server-measured solve time and its timing tags (`_measure_secs`).
///
/// `expected_time_secs` is `None` when the topic is not in the arena. 1.0 skips
/// the clamp in that case, so this port skips it too: no cap, and no tag.
#[must_use]
pub fn measure_secs(
    started_at: f64,
    now_micros: i64,
    expected_time_secs: Option<i64>,
) -> (i64, Vec<String>) {
    let elapsed = (unix_seconds(now_micros) - started_at).max(0.0).round();
    let Some(expected) = expected_time_secs else {
        return (whole_secs(elapsed), Vec::new());
    };
    let cap = expected.saturating_mul(TIMING_CAP_MULTIPLIER);
    if whole_secs(elapsed) > cap {
        return (cap, vec![TAG_TIMING_UNRELIABLE.to_string()]);
    }
    (whole_secs(elapsed), Vec::new())
}

/// A rounded, non-negative elapsed time as whole seconds.
///
/// The value is bounded BEFORE the cast, so no instant can truncate and the
/// function cannot panic.
fn whole_secs(elapsed: f64) -> i64 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "the ceiling only has to be safe, not exact"
    )]
    let ceiling = i64::MAX as f64;
    if elapsed <= 0.0 {
        return 0;
    }
    if elapsed >= ceiling {
        return i64::MAX;
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the two guards above bound the value inside the i64 range"
    )]
    let secs = elapsed as i64;
    secs
}

/// Round an XP total to two places, as `service.py` does.
pub(super) fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One Unix microsecond instant, 4,000 seconds after `started_at` 0.
    const LATER_US: i64 = 4_000_000_000;

    /// A topic outside the arena has no authored solve time, so 1.0 skips the
    /// clamp: the whole elapsed time is recorded and no tag is set.
    #[test]
    fn a_topic_with_no_expected_time_is_never_clamped() {
        assert_eq!(measure_secs(0.0, LATER_US, None), (4_000, Vec::new()));
    }

    /// A hand-off stamped in the future measures zero, never a negative time.
    #[test]
    fn a_future_hand_off_measures_zero() {
        assert_eq!(measure_secs(9_000.0, LATER_US, Some(30)), (0, Vec::new()));
    }

    /// The cast is guarded at both ends: a time past the `i64` range saturates
    /// at the maximum, and a time at or below zero is zero.
    #[test]
    fn whole_secs_is_bounded_at_both_ends() {
        assert_eq!(whole_secs(f64::MAX), i64::MAX);
        assert_eq!(whole_secs(f64::INFINITY), i64::MAX);
        assert_eq!(whole_secs(-1.0), 0);
        assert_eq!(whole_secs(0.0), 0);
        assert_eq!(whole_secs(12.0), 12);
    }

    /// The XP rounding is two places, half away from zero, as `round()` is.
    #[test]
    fn round2_keeps_two_places() {
        assert!((round2(1.049) - 1.05).abs() < f64::EPSILON);
        assert!((round2(2.004) - 2.0).abs() < f64::EPSILON);
        assert!((round2(7.0) - 7.0).abs() < f64::EPSILON);
    }
}
