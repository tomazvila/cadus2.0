//! The FIRe spaced-repetition engine (R5, spec sections 4.9 and 7).
//!
//! Every function here is a deterministic function of `(state | states, attempt,
//! curriculum, config, t)` and returns new values. Nothing reads a clock, nothing
//! mutates its inputs, and nothing panics, so the event log stays re-projectable.
//!
//! ## Parity
//!
//! The port copies 1.0 `cadus/fire.py` instruction for instruction, including the
//! parts a rewrite is tempted to improve:
//!
//! - Python `max` and `min` are NOT `f64::max` and `f64::min`. Python returns the
//!   FIRST argument unless the second compares strictly better, which fixes the
//!   result for a signed zero and for `NaN`. [`py_max`] and [`py_min`] reproduce
//!   that, so `max(0.0, -0.0)` stays `0.0` and never turns a `repNum` into `-0.0`.
//! - 1.0 iterates `sorted(weights.items())`, so the propagation order is the byte
//!   order of the topic id (trap T18). The arena hands the pairs back sorted.
//! - 1.0 iterates the `neighborhood()` SET, whose order is hash-randomized
//!   (trap T5). The sorted order replaces it, and
//!   [`neumaier_sum`](crate::numeric::neumaier_sum) reproduces the
//!   CPython `sum()` that the mean divides (trap T1).
//! - The credit and the penalty legs are ASYMMETRIC on purpose: downward credit
//!   reads the RECIPIENT's memory, and an upward penalty is the attempted topic's
//!   `raw` scaled by `W`. The forced-explicit gate covers credit only, and the
//!   `t0 is None` gate covers penalties only.

use std::hint::black_box;

use crate::config::Config;
use crate::event::{Timestamp, TopicStatus, WorkQuality};
use crate::learner::TopicState;
use crate::numeric::days_between;

mod ability;
mod attempt;
#[cfg(test)]
pub(crate) mod testing;

pub use ability::{ability_update, difficulty, initial_ability};
pub use attempt::{
    AttemptResult, NonFiniteDecay, Propagation, PropagationKind, apply_attempt,
    apply_attempt_checked, grade_review, knockout,
};

/// The grade `q` of each work-quality tier, in tier order (`fire.py:40-47`).
///
/// These are the GRADING values of the FIRe raw delta. They are not the XP
/// multipliers of [`crate::config::XpTiers`], which the config carries.
pub const QUALITY_Q: [(WorkQuality, f64); 6] = [
    (WorkQuality::Perfect, 1.0),
    (WorkQuality::NearlyPerfect, 0.85),
    (WorkQuality::Passable, 0.7),
    (WorkQuality::NearlyPassable, 0.4),
    (WorkQuality::Poor, 0.15),
    (WorkQuality::Blowoff, 0.0),
];

/// The grade at or above which one attempt counts as a pass (`fire.py:52`).
pub const PASS_QUALITY_THRESHOLD: f64 = 0.7;

/// The memory level at or below which a topic enters the nearly-due band.
///
/// Test prep never raises it: it is a fixed lookahead window (`fire.py:57`).
pub const NEARLY_DUE_THRESHOLD: f64 = 0.6;

/// The raised due threshold of a topic in the test-prep set (`fire.py:73`).
pub const TEST_PREP_DUE_THRESHOLD: f64 = 0.7;

/// The largest review interval, in days (`fire.py:76`).
pub const INTERVAL_CAP_DAYS: f64 = 730.0;

/// The factor that discounts the positive credit of an assisted pass
/// (`fire.py:89`). An assisted MISS keeps its full negative raw delta.
pub const ASSISTED_CREDIT: f64 = 0.5;

/// The neutral ability prior of a topic with no touched neighbor.
pub const NEUTRAL_ABILITY: f64 = 0.5;

/// Python `max(a, b)` for two floats.
///
/// CPython returns `a` unless `b > a`, so `max(0.0, -0.0)` is `0.0` and
/// `max(0.0, NaN)` is `0.0`. `f64::max` picks either representation of zero and
/// drops a `NaN`, which is a different function. The 1.0 floors and clamps go
/// through this one, so a `repNum` floor never emits `-0.0`.
#[must_use]
pub fn py_max(a: f64, b: f64) -> f64 {
    if b > a { b } else { a }
}

/// Python `min(a, b)` for two floats. CPython returns `a` unless `b < a`.
#[must_use]
pub fn py_min(a: f64, b: f64) -> f64 {
    if b < a { b } else { a }
}

/// Python `max(lo, min(hi, x))`, the 1.0 `_clamp` (`fire.py:139-140`).
#[must_use]
pub fn clamp(x: f64, lo: f64, hi: f64) -> f64 {
    py_max(lo, py_min(hi, x))
}

/// The FIRe grade of a work-quality tier (`fire.py:150-152`).
#[must_use]
pub const fn quality_q(quality: WorkQuality) -> f64 {
    match quality {
        WorkQuality::Perfect => 1.0,
        WorkQuality::NearlyPerfect => 0.85,
        WorkQuality::Passable => 0.7,
        WorkQuality::NearlyPassable => 0.4,
        WorkQuality::Poor => 0.15,
        WorkQuality::Blowoff => 0.0,
    }
}

/// Whether one attempt's tier counts as a pass, `q >= 0.7` (`fire.py:155-157`).
#[must_use]
pub fn is_pass_quality(quality: WorkQuality) -> bool {
    quality_q(quality) >= PASS_QUALITY_THRESHOLD
}

/// Whether a state carries a review history, `status in {learning, placed}`
/// (`fire.py:186-195`).
///
/// This is the precondition that makes the memory bands mean anything: an
/// untouched topic has `memoryBase == 0`, so a bare `memory <= 0.5` test would
/// call it due forever, and a mastery-floor topic is assumed known.
#[must_use]
pub fn has_review_history(state: &TopicState) -> bool {
    matches!(state.status, TopicStatus::Learning | TopicStatus::Placed)
}

/// The review band of a topic (`fire.py:198-211`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReviewState {
    /// Not on the review schedule at all. The memory number carries no meaning.
    OffSchedule,
    /// At or below the due threshold. Serve an explicit review.
    Due,
    /// Above the due threshold and at or below the nearly-due threshold.
    NearlyDue,
    /// On the schedule with memory still fresh. Nothing is owed.
    OnSchedule,
}

/// `0.5 ** exponent`, through the platform `pow` (trap T21).
///
/// The base is a literal, so LLVM rewrites `0.5_f64.powf(x)` into `exp2(-x)` in an
/// optimized build. glibc `exp2` and glibc `pow` differ by one unit in the last
/// place, and CPython `0.5 ** x` calls `pow`, so the rewrite makes the release fold
/// diverge from the 1.0 fold while the debug fold agrees with it.
/// [`black_box`] hides the base from the optimizer and keeps the real `pow` call.
/// Do NOT remove it, and do not write a literal base at any other `powf` site.
#[must_use]
fn pow_half(exponent: f64) -> f64 {
    black_box(0.5_f64).powf(exponent)
}

/// The memory of a topic at `t_us`: `memoryBase * 0.5 ** (days / interval)`
/// (`fire.py:169-179`).
///
/// A topic with no `t0`, or with a non-positive interval, has no decay reference,
/// so its undecayed `memoryBase` comes back.
///
/// The result is not finite where CPython raises `OverflowError` in the same
/// expression. [`apply_attempt_checked`] reports that state instead of folding it.
#[must_use]
pub fn memory_at(state: &TopicState, t_us: i64) -> f64 {
    let Some(t0) = state.t0 else {
        return state.memory_base;
    };
    if state.interval_days <= 0.0 {
        return state.memory_base;
    }
    let exponent = days_between(t0.micros(), t_us) / state.interval_days;
    state.memory_base * pow_half(exponent)
}

/// The review band of `state` at `t_us` (`fire.py:214-248`).
///
/// `due` is tested BEFORE `nearly_due` and the order is load-bearing in both
/// threshold regimes. Under the default pair (0.5 < 0.6) due-first is what keeps
/// the bands disjoint. Under test prep (0.7 > 0.6) the implication inverts: a
/// test-prep topic at memory 0.65 is due and is NOT nearly due.
#[must_use]
pub fn review_state(state: &TopicState, t_us: i64, cfg: &Config, test_prep: bool) -> ReviewState {
    if !has_review_history(state) {
        return ReviewState::OffSchedule;
    }
    let memory = memory_at(state, t_us);
    let due_threshold = if test_prep {
        TEST_PREP_DUE_THRESHOLD
    } else {
        cfg.fire.due_threshold
    };
    if memory <= due_threshold {
        return ReviewState::Due;
    }
    if memory <= NEARLY_DUE_THRESHOLD {
        return ReviewState::NearlyDue;
    }
    ReviewState::OnSchedule
}

/// The review interval, in days, of a possibly fractional `rep_num`
/// (`fire.py:256-275`).
///
/// The table interpolates linearly between adjacent indices. A `rep_num` at or
/// beyond the last index clamps to the last entry, with no extrapolation, and a
/// `rep_num` at or below zero clamps to the first. Every result caps at
/// [`INTERVAL_CAP_DAYS`]. An empty table gives `0.0`, the 1.0 defensive branch.
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the floor is finite and at least 0.0; a cast past the table takes the tail"
)]
pub fn interval_for(rep_num: f64, cfg: &Config) -> f64 {
    let table = &cfg.fire.interval_table;
    let Some(&tail) = table.last() else {
        return 0.0;
    };
    let r = py_max(0.0, rep_num);
    let floor = r.floor();
    // A rep number at or past the last index has no pair to interpolate, so it
    // takes the last entry.
    let value = match table.windows(2).nth(floor as usize) {
        Some(&[low, high]) => low + (r - floor) * (high - low),
        _ => tail,
    };
    py_min(value, INTERVAL_CAP_DAYS)
}

/// The learning speed of a topic: `clamp((0.5 + ability) / (0.5 + difficulty))`
/// (`fire.py:278-283`). A speed-2 topic accrues twice the repetitions of speed-1.
#[must_use]
pub fn speed_for(ability: f64, difficulty: f64, cfg: &Config) -> f64 {
    let (lo, hi) = cfg.fire.speed_clamp;
    let raw = (0.5 + ability) / (0.5 + difficulty);
    clamp(raw, lo, hi)
}

/// The overdue decay multiplier of a state (`fire.py:286-296`).
///
/// `min(decay_cap, 1 + max(0, days / interval - 1))`. It never drops below 1, so
/// failing an overdue topic pushes `repNum` back harder.
#[must_use]
pub fn decay_for(state: &TopicState, t_us: i64, cfg: &Config) -> f64 {
    let Some(t0) = state.t0 else {
        return 1.0;
    };
    if state.interval_days <= 0.0 {
        return 1.0;
    }
    let overdue = days_between(t0.micros(), t_us) / state.interval_days - 1.0;
    py_min(cfg.fire.decay_cap, 1.0 + py_max(0.0, overdue))
}

/// The raw credit of one attempt at grade `quality_grade` and memory `memory_now`
/// (`fire.py:304-335`).
///
/// A pass earns `q * earlyFactor`, where the early-repetition discount is
/// `clamp((1 - memory) / (1 - due_threshold), early_floor, 1)`. An `assisted`
/// pass scales that credit by [`ASSISTED_CREDIT`]. A miss returns `-(1 - q)`,
/// with no early discount and NO assisted discount.
#[must_use]
pub fn raw_delta(
    quality_grade: f64,
    memory_now: f64,
    passed: bool,
    cfg: &Config,
    assisted: bool,
) -> f64 {
    if !passed {
        return -(1.0 - quality_grade);
    }
    let span = 1.0 - cfg.fire.due_threshold;
    let early = if span <= 0.0 {
        1.0
    } else {
        clamp((1.0 - memory_now) / span, cfg.fire.early_floor, 1.0)
    };
    let credit = quality_grade * early;
    if assisted {
        credit * ASSISTED_CREDIT
    } else {
        credit
    }
}

/// Apply one raw delta to a single state (`fire.py:409-431`), and report whether
/// every decay value it read was finite.
///
/// ```text
/// repNum     <- max(0, repNum + speed * decay^failed * raw)
/// memoryBase <- max(0, memory(t) + raw)
/// t0         <- t
/// interval   <- interval_for(repNum)
/// ```
///
/// The decay factor is the state's OWN overdueness and applies on a miss only.
/// Every other field of the state carries through unchanged.
///
/// The flag is `false` where the memory or the decay factor left the finite range.
/// CPython raises `OverflowError` at that input in `0.5 ** exponent`, and the 1.0
/// fold builds no model. For an infinite exponent CPython returns `inf` without an
/// error, and the port stops there too: a non-finite `memoryBase` writes JSON
/// `null`, and the stored model then reads back as an error.
pub(crate) fn apply_update(
    state: &TopicState,
    raw: f64,
    t_us: i64,
    failed: bool,
    cfg: &Config,
) -> (TopicState, bool) {
    let factor = if failed {
        decay_for(state, t_us, cfg)
    } else {
        1.0
    };
    let memory = memory_at(state, t_us);
    let new_rep = py_max(0.0, state.rep_num + state.speed * factor * raw);
    let new_base = py_max(0.0, memory + raw);
    let mut next = state.clone();
    next.rep_num = new_rep;
    next.memory_base = new_base;
    next.t0 = Some(Timestamp::from_micros(t_us));
    next.interval_days = interval_for(new_rep, cfg);
    (next, factor.is_finite() && memory.is_finite())
}

#[cfg(test)]
mod tests {
    use super::testing::{DAY_US, T_US, learned};
    use super::*;

    #[test]
    fn the_grades_and_the_bands_follow_the_1_0_table() {
        let cfg = Config::default();
        for (tier, q) in QUALITY_Q {
            assert_eq!(quality_q(tier), q);
            assert_eq!(is_pass_quality(tier), q >= PASS_QUALITY_THRESHOLD);
        }
        assert_eq!(QUALITY_Q.map(|(_, q)| q), [1.0, 0.85, 0.7, 0.4, 0.15, 0.0]);
        let bands = [
            (TopicState::default(), false, ReviewState::OffSchedule),
            (learned(0.5), false, ReviewState::Due),
            (learned(0.55), false, ReviewState::NearlyDue),
            (learned(0.9), false, ReviewState::OnSchedule),
            (learned(0.65), true, ReviewState::Due),
        ];
        for (state, test_prep, expected) in bands {
            assert_eq!(review_state(&state, T_US, &cfg, test_prep), expected);
        }
        assert_eq!(clamp(5.0, 0.0, 1.0), 1.0);
        assert_eq!(interval_for(3.5, &cfg), 33.0);
        assert_eq!(interval_for(20.0, &cfg), 480.0);
        assert_eq!(speed_for(0.5, 0.0, &cfg), 2.0);
        assert_eq!(decay_for(&learned(1.0), T_US + 20 * DAY_US, &cfg), 2.0);
        assert_eq!(decay_for(&TopicState::default(), T_US, &cfg), 1.0);
        assert_eq!(raw_delta(0.15, 0.5, false, &cfg, false), -0.85);
        assert_eq!(raw_delta(1.0, 0.5, true, &cfg, true), 0.5);
        let mut bare = Config::default();
        bare.fire.interval_table.clear();
        bare.fire.due_threshold = 1.0;
        assert_eq!(interval_for(1.0, &bare), 0.0);
        assert_eq!(raw_delta(1.0, 0.9, true, &bare, false), 1.0);
        assert_eq!(memory_at(&learned(0.5), T_US), 0.5);
    }

    #[test]
    fn apply_update_reports_an_infinite_decay_factor() {
        // A subnormal interval makes the overdue ratio infinite, and a config with
        // no decay cap keeps the factor infinite. The memory itself is 0.0 there,
        // so the flag comes from the factor alone.
        let mut cfg = Config::default();
        cfg.fire.decay_cap = f64::INFINITY;
        let mut state = learned(0.5);
        state.interval_days = 5e-324;
        let (next, finite) = apply_update(&state, -0.5, T_US + 20 * DAY_US, true, &cfg);
        assert!(!finite);
        assert_eq!(next.rep_num, 0.0);
        assert_eq!(next.memory_base, 0.0);
    }
}
