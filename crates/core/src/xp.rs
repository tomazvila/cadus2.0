//! XP accounting, streak, and velocity (R5, spec sections 4.10 and 7).
//!
//! Pure functions over primitive inputs — task descriptors, work-quality tiers,
//! and time-stamped `(ts, xp)` and `(ts, topic)` records. The fold aggregates the
//! event log into these inputs and calls in here; nothing here reads an event, a
//! file, or a clock.
//!
//! ## Parity
//!
//! The port copies 1.0 `cadus/xp.py`, and the two summation algorithms of the 1.0
//! code base stay apart (trap T2):
//!
//! - [`xp_per_day`] sums with `sum()`, so it uses [`neumaier_sum`].
//! - [`daily_totals`] accumulates with `+=`, so it keeps a NAIVE loop. Unifying
//!   the two changes the fold.
//!
//! Rounding is Python rounding: [`round_dp`] is correctly-rounded decimal, never
//! scale-round-divide (trap T4). The ETA takes `ceil` of a float quotient, and the
//! quotient is computed first (trap T14).
//!
//! Time zones enter only through the local DATE of an instant (trap T9), and the
//! reference instant is always a parameter (trap T10).

use std::collections::{BTreeMap, BTreeSet};
use std::hint::black_box;

use chrono::{Days, NaiveDate, TimeDelta};
use chrono_tz::Tz;
use indexmap::IndexMap;

use crate::config::Config;
use crate::curriculum::Curriculum;
use crate::event::{TaskType, TopicStatus, WorkQuality};
use crate::learner::{TopicState, VelocityState};
use crate::numeric::{TimeError, local_day_in, neumaier_sum, round_dp};

/// The base XP of one lesson knowledge point (`xp.py:39`).
pub const LESSON_XP_PER_KP: f64 = 3.5;

/// The base XP of one review (`xp.py:40`).
pub const REVIEW_XP: f64 = 5.0;

/// The base XP of one quiz (`xp.py:41`).
pub const QUIZ_XP: f64 = 15.0;

/// The base XP of one multi-step task (`xp.py:42`).
pub const MULTISTEP_XP: f64 = 15.0;

/// The base XP of one speed drill (`xp.py:43`).
pub const DRILL_XP: f64 = 5.0;

/// The factor the blow-off penalty grows by per consecutive blow-off
/// (`xp.py:46`).
pub const BLOWOFF_ESCALATION: f64 = 1.5;

/// The fraction of the expected time below which a wrong answer is rushed
/// (`xp.py:50`).
pub const RUSH_TIME_FRACTION: f64 = 0.5;

/// The factor a rushed task's positive XP is multiplied by (`xp.py:52`).
pub const RUSH_PENALTY_MULT: f64 = 0.5;

/// The length of the velocity window, in local days (`xp.py:55`).
pub const VELOCITY_WINDOW_DAYS: i64 = 28;

/// The XP-per-topic estimate the ETA uses before any topic is done
/// (`xp.py:60`).
pub const DEFAULT_XP_PER_TOPIC: f64 = 12.0;

/// The statuses that count as mastered for course progress (`selector.py:150`).
///
/// Implicit credit alone never masters a topic; the status gate is the rule. The
/// selector of U4 reads the same predicate.
#[must_use]
pub fn is_mastered(state: &TopicState) -> bool {
    matches!(
        state.status,
        TopicStatus::Learning | TopicStatus::Placed | TopicStatus::Floor
    )
}

/// The pre-multiplier base XP of one task (`xp.py:68-86`).
///
/// `kp_count` counts only for a lesson. A diagnostic awards nothing: it measures,
/// it does not train.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "a topic holds a handful of knowledge points"
)]
pub fn base_xp(task_type: TaskType, kp_count: i64) -> f64 {
    match task_type {
        TaskType::Lesson => LESSON_XP_PER_KP * kp_count as f64,
        TaskType::Review => REVIEW_XP,
        TaskType::Quiz => QUIZ_XP,
        TaskType::MultiStep => MULTISTEP_XP,
        TaskType::Drill => DRILL_XP,
        TaskType::Diagnostic => 0.0,
    }
}

/// The configured XP multiplier of a work-quality tier (`xp.py:89-98`).
#[must_use]
pub fn tier_multiplier(quality: WorkQuality, cfg: &Config) -> f64 {
    let tiers = &cfg.xp.tiers;
    match quality {
        WorkQuality::Perfect => tiers.perfect,
        WorkQuality::NearlyPerfect => tiers.nearly_perfect,
        WorkQuality::Passable => tiers.passable,
        WorkQuality::NearlyPassable => tiers.nearly_passable,
        WorkQuality::Poor => tiers.poor,
        WorkQuality::Blowoff => tiers.blowoff,
    }
}

/// The work-quality XP multiplier, with the blow-off run escalation
/// (`xp.py:101-114`).
///
/// `consecutive_blowoffs` is THIS blow-off's 1-based index in the run, so 1 gives
/// the plain penalty, 2 gives 1.5 times it, and 3 gives 2.25 times it. A value at
/// or below 1 gives the plain penalty. The escalation touches no other tier.
///
/// CPython raises a float to an integer power through `pow`, so the exponent goes
/// through [`f64::powf`] here and not through repeated multiplication.
///
/// The base is a constant, so it goes through [`black_box`] (trap T21): a literal
/// base lets LLVM rewrite the `pow` call in an optimized build, and a rewritten call
/// is a different number in the last bit from the one CPython computes.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "a blow-off run length is far below 2**53"
)]
pub fn quality_multiplier(quality: WorkQuality, cfg: &Config, consecutive_blowoffs: i64) -> f64 {
    let mult = tier_multiplier(quality, cfg);
    if quality == WorkQuality::Blowoff {
        let run = consecutive_blowoffs.max(1);
        return mult * black_box(BLOWOFF_ESCALATION).powf((run - 1) as f64);
    }
    mult
}

/// Whether an answer sacrificed accuracy for speed (`xp.py:117-123`).
///
/// It is rushed when it is WRONG and it came back in under
/// [`RUSH_TIME_FRACTION`] of the authored expected time. A fast CORRECT answer is
/// fluency, not rushing.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "a task time in seconds is far below 2**53"
)]
pub fn is_rushing(correct: bool, secs: i64, expected_time_secs: i64) -> bool {
    if correct || secs <= 0 || expected_time_secs <= 0 {
        return false;
    }
    (secs as f64) < RUSH_TIME_FRACTION * expected_time_secs as f64
}

/// The XP of one completed task (`xp.py:126-143`).
///
/// Base times the quality multiplier, with the blow-off escalation and the
/// rushing penalty. The rushing penalty applies only to POSITIVE XP, so rushing
/// never rewards a blow-off. A diagnostic short-circuits to `0.0`.
#[must_use]
pub fn task_xp(
    task_type: TaskType,
    quality: WorkQuality,
    cfg: &Config,
    kp_count: i64,
    consecutive_blowoffs: i64,
    rushing: bool,
) -> f64 {
    if task_type == TaskType::Diagnostic {
        return 0.0;
    }
    let base = base_xp(task_type, kp_count);
    let mut xp = base * quality_multiplier(quality, cfg, consecutive_blowoffs);
    // A zero XP keeps its value under the multiply, so the sign test alone
    // selects the positive values.
    if rushing && xp.is_sign_positive() {
        xp *= RUSH_PENALTY_MULT;
    }
    xp
}

/// The XP of each local day (`xp.py:163-169`).
///
/// The accumulation is NAIVE (`out[day] = out.get(day, 0.0) + xp`), which is a
/// different algorithm from the compensated `sum()` of [`xp_per_day`]. Trap T2
/// keeps them apart, so this loop must stay naive.
///
/// The map keeps insertion order, the order of the 1.0 dict. Nothing reads that
/// order today; every caller does a lookup.
///
/// # Errors
///
/// Returns [`TimeError::TimestampOutOfRange`] when an entry's instant is outside
/// the range `chrono` represents.
pub fn daily_totals(
    entries: &[(i64, f64)],
    zone: Tz,
) -> Result<IndexMap<NaiveDate, f64>, TimeError> {
    let mut out: IndexMap<NaiveDate, f64> = IndexMap::new();
    for &(ts_us, xp) in entries {
        let day = local_day_in(ts_us, zone)?;
        let total = out.entry(day).or_insert(0.0);
        *total += xp;
    }
    Ok(out)
}

/// The consecutive days that met the daily XP goal, counting back from `today`
/// (`xp.py:172-184`).
///
/// A `today` still below the goal does not break the streak — it is in progress —
/// so counting then starts at yesterday. A past day strictly below the goal ends
/// the streak.
///
/// The walk stops at the first representable date, so a non-positive `goal`
/// terminates instead of looping. 1.0 raises `OverflowError` there.
#[must_use]
pub fn current_streak(daily: &IndexMap<NaiveDate, f64>, goal: f64, today: NaiveDate) -> i64 {
    let total_of = |day: &NaiveDate| daily.get(day).copied().unwrap_or(0.0);
    let mut day = today;
    if total_of(&today) < goal {
        let Some(previous) = day.checked_sub_days(Days::new(1)) else {
            return 0;
        };
        day = previous;
    }
    let mut streak: i64 = 0;
    while total_of(&day) >= goal {
        streak += 1;
        let Some(previous) = day.checked_sub_days(Days::new(1)) else {
            break;
        };
        day = previous;
    }
    streak
}

/// The first local day still inside the trailing window (`xp.py:192-194`).
///
/// The membership test of the window is `day >= start`, so the window holds
/// `window_days` days including the day of `t_us`.
///
/// A window so long that the start date underflows gives [`NaiveDate::MIN`],
/// which keeps every entry inside the window. 1.0 raises `OverflowError` there.
///
/// # Errors
///
/// Returns [`TimeError::TimestampOutOfRange`] for an unrepresentable instant.
pub fn window_start(t_us: i64, zone: Tz, window_days: i64) -> Result<NaiveDate, TimeError> {
    let today = local_day_in(t_us, zone)?;
    let back = TimeDelta::try_days(window_days - 1);
    Ok(back
        .and_then(|delta| today.checked_sub_signed(delta))
        .unwrap_or(NaiveDate::MIN))
}

/// The mean XP per day over the trailing window (`xp.py:197-208`).
///
/// The total is a 1.0 `sum()`, so it goes through [`neumaier_sum`] (trap T1).
///
/// # Errors
///
/// Returns [`TimeError::TimestampOutOfRange`] for an unrepresentable instant.
#[expect(
    clippy::cast_precision_loss,
    reason = "a window length in days is far below 2**53"
)]
pub fn xp_per_day(
    entries: &[(i64, f64)],
    t_us: i64,
    zone: Tz,
    window_days: i64,
) -> Result<f64, TimeError> {
    let start = window_start(t_us, zone, window_days)?;
    let mut inside: Vec<f64> = Vec::new();
    for &(ts_us, xp) in entries {
        if local_day_in(ts_us, zone)? >= start {
            inside.push(xp);
        }
    }
    Ok(neumaier_sum(&inside) / window_days as f64)
}

/// The distinct topics mastered per week over the trailing window
/// (`xp.py:211-226`).
///
/// Each completion is `(mastered_at, topic_id)`; a repeated topic counts once.
///
/// # Errors
///
/// Returns [`TimeError::TimestampOutOfRange`] for an unrepresentable instant.
#[expect(
    clippy::cast_precision_loss,
    reason = "a topic count and a window length are far below 2**53"
)]
pub fn topics_per_week(
    completions: &[(i64, String)],
    t_us: i64,
    zone: Tz,
    window_days: i64,
) -> Result<f64, TimeError> {
    let start = window_start(t_us, zone, window_days)?;
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for (ts_us, topic) in completions {
        if local_day_in(*ts_us, zone)? >= start {
            seen.insert(topic.as_str());
        }
    }
    let weeks = window_days as f64 / 7.0;
    Ok(seen.len() as f64 / weeks)
}

/// The mastered and total topic counts of a course (`xp.py:229-238`).
///
/// A topic absent from `states` counts as a default, untouched state.
#[must_use]
pub fn course_counts(
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    course_id: &str,
) -> (i64, i64) {
    let course = graph.topics_in_course(course_id);
    let total = i64::try_from(course.len()).unwrap_or(i64::MAX);
    let default = TopicState::default();
    let mut done: i64 = 0;
    for &idx in course {
        let id = graph.id_of(idx);
        if is_mastered(states.get(id).unwrap_or(&default)) {
            done += 1;
        }
    }
    (done, total)
}

/// The fraction of the course that is mastered (`xp.py:241-247`).
///
/// It advances only when a NEW topic becomes mastered — a lesson completion or a
/// diagnostic placement — never on a review or a quiz. An empty course is `0.0`.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "a course topic count is far below 2**53"
)]
pub fn course_progress(
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    course_id: &str,
) -> f64 {
    let (done, total) = course_counts(states, graph, course_id);
    if total == 0 {
        return 0.0;
    }
    done as f64 / total as f64
}

/// The projected completion date of a course (`xp.py:250-274`).
///
/// The remaining XP is the remaining topics times the average XP already spent
/// per completed topic, falling back to [`DEFAULT_XP_PER_TOPIC`] before any topic
/// is done. A complete course gives `today`, and no recent velocity gives `None`.
///
/// The day count is `ceil` of the float quotient, computed in that order
/// (trap T14). A projected date outside the representable range gives `None`,
/// which is the same "ETA undefined" answer; 1.0 raises `OverflowError` there.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "a course topic count is far below 2**53"
)]
#[expect(
    clippy::cast_possible_truncation,
    reason = "the saturating cast keeps an unrepresentable day count out of the date math"
)]
pub fn estimate_eta(
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    course_id: &str,
    total_xp: f64,
    xp_per_day_recent: f64,
    today: NaiveDate,
) -> Option<NaiveDate> {
    let (done, total) = course_counts(states, graph, course_id);
    let remaining = total - done;
    if remaining <= 0 {
        return Some(today);
    }
    if xp_per_day_recent <= 0.0 {
        return None;
    }
    let per_topic = if done > 0 {
        total_xp / done as f64
    } else {
        DEFAULT_XP_PER_TOPIC
    };
    let xp_remaining = remaining as f64 * per_topic;
    let days = (xp_remaining / xp_per_day_recent).ceil();
    if !days.is_finite() {
        return None;
    }
    TimeDelta::try_days(days as i64).and_then(|delta| today.checked_add_signed(delta))
}

/// The inputs of [`compute_velocity_state`].
///
/// The 1.0 signature is one call with nine keyword arguments
/// (`xp.py:277-314`); this struct carries the same nine.
#[derive(Debug, Clone, Copy)]
pub struct VelocityInput<'a> {
    /// The topic states of the learner.
    pub states: &'a BTreeMap<String, TopicState>,
    /// The curriculum the course lives in.
    pub graph: &'a Curriculum,
    /// The enrolled course. `None` gives a default velocity with no ETA.
    pub course_id: Option<&'a str>,
    /// Every `(instant, xp)` record of the log.
    pub xp_entries: &'a [(i64, f64)],
    /// Every `(instant, topic)` mastery record of the log.
    pub completions: &'a [(i64, String)],
    /// The whole-log XP total the per-topic average divides.
    pub total_xp: f64,
    /// The reference instant: the last event's `ts`, never a wall clock.
    pub t_us: i64,
    /// The resolved time zone of the day boundary.
    pub zone: Tz,
    /// The length of the trailing window, in local days.
    pub window_days: i64,
}

/// Assemble the derived [`VelocityState`] of the learner model (`xp.py:277-314`).
///
/// Each of the three rates is rounded to 4 decimal places with Python rounding
/// (trap T4). A learner with no enrolled course gets `0.0` progress and no ETA.
///
/// # Errors
///
/// Returns [`TimeError::TimestampOutOfRange`] for an unrepresentable instant.
pub fn compute_velocity_state(input: &VelocityInput<'_>) -> Result<VelocityState, TimeError> {
    let today = local_day_in(input.t_us, input.zone)?;
    let rate = xp_per_day(input.xp_entries, input.t_us, input.zone, input.window_days)?;
    let progress = match input.course_id {
        Some(course_id) => course_progress(input.states, input.graph, course_id),
        None => 0.0,
    };
    let eta = match input.course_id {
        Some(course_id) => estimate_eta(
            input.states,
            input.graph,
            course_id,
            input.total_xp,
            rate,
            today,
        ),
        None => None,
    };
    let topics = topics_per_week(input.completions, input.t_us, input.zone, input.window_days)?;
    Ok(VelocityState {
        xp_per_day_28d: round_dp(rate, 4),
        topics_per_week_28d: round_dp(topics, 4),
        course_progress: round_dp(progress, 4),
        eta,
    })
}
