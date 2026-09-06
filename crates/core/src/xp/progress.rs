//! Course progress, the completion estimate, and the velocity state
//! (`xp.py:229-314`, D-F6).
//!
//! The counts of a course stand apart from the XP accounting, because D-F6 gave
//! them a second rule: a placement gives CREDIT, and the progress percentage
//! counts PRACTICE. [`CourseCounts`] holds the two numbers side by side, and
//! `mastery.confirm_inferred` picks the one the percentage divides.

use std::collections::BTreeMap;

use chrono::{NaiveDate, TimeDelta};
use chrono_tz::Tz;

use crate::config::Config;
use crate::curriculum::Curriculum;
use crate::learner::{TopicState, VelocityState};
use crate::numeric::{TimeError, local_day_in, round_dp};

use super::{DEFAULT_XP_PER_TOPIC, is_inferred, is_practiced, topics_per_week, xp_per_day};

/// The practiced, inferred and total topic counts of a course (D-F6).
///
/// The three numbers the dashboard shows. `practiced` counts the topics the
/// learner passed, `inferred` counts the placed and floor topics that carry no
/// direct answer, and `total` counts the course.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CourseCounts {
    /// The topics the learner practiced.
    pub practiced: i64,
    /// The known topics the course inferred and never confirmed.
    pub inferred: i64,
    /// The topics of the course.
    pub total: i64,
}

impl CourseCounts {
    /// The count the progress percentage divides by the total.
    ///
    /// D-F6 counts the practiced topics alone. With
    /// `mastery.confirm_inferred` off the count returns to the 1.0 rule, which
    /// adds the inferred topics.
    #[must_use]
    pub const fn done(&self, cfg: &Config) -> i64 {
        if cfg.mastery.confirm_inferred {
            self.practiced
        } else {
            self.practiced + self.inferred
        }
    }
}

/// The per-status topic counts of a course (`xp.py:229-238`, D-F6).
///
/// A topic absent from `states` counts as a default, untouched state.
#[must_use]
pub fn course_counts(
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    course_id: &str,
) -> CourseCounts {
    let course = graph.topics_in_course(course_id);
    let default = TopicState::default();
    let mut counts = CourseCounts {
        total: i64::try_from(course.len()).unwrap_or(i64::MAX),
        ..CourseCounts::default()
    };
    for &idx in course {
        let state = states.get(graph.id_of(idx)).unwrap_or(&default);
        if is_practiced(state) {
            counts.practiced += 1;
        } else if is_inferred(state) {
            counts.inferred += 1;
        }
    }
    counts
}

/// The fraction of the course the learner practiced (`xp.py:241-247`, D-F6).
///
/// It advances only when a NEW topic becomes practiced — a passed lesson or a
/// passed confirmation item — never on a review or a quiz, and never on a
/// placement alone. An empty course is `0.0`.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "a course topic count is far below 2**53"
)]
pub fn course_progress(
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    course_id: &str,
    cfg: &Config,
) -> f64 {
    let counts = course_counts(states, graph, course_id);
    if counts.total == 0 {
        return 0.0;
    }
    counts.done(cfg) as f64 / counts.total as f64
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
    cfg: &Config,
) -> Option<NaiveDate> {
    let counts = course_counts(states, graph, course_id);
    let done = counts.done(cfg);
    let remaining = counts.total - done;
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
/// (`xp.py:277-314`); this struct carries the same nine, plus the config the
/// D-F6 progress rule reads.
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
    /// The scheduler config. `mastery.confirm_inferred` picks the progress rule.
    pub cfg: &'a Config,
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
        Some(course_id) => course_progress(input.states, input.graph, course_id, input.cfg),
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
            input.cfg,
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
