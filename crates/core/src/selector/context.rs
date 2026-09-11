//! The keyword context of `compose_session` (`selector.py:1240-1257`).

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;

use crate::learner::{PendingRemediation, QuizState};
use crate::readiness::ReadinessGate;
use crate::retention::RetentionState;

use super::plan::SessionPlan;

/// The empty topic-id set a [`SessionContext`] defaults to.
static NO_IDS: BTreeSet<String> = BTreeSet::new();

/// The empty remediation queue a [`SessionContext`] defaults to.
static NO_REMEDIATION: [PendingRemediation; 0] = [];

/// The keyword context of `compose_session` (`selector.py:1240-1257`).
///
/// Every field has the 1.0 default, so a caller sets only what it knows.
#[derive(Debug, Clone)]
pub struct SessionContext<'a> {
    /// A still-open plan to re-serve instead of composing afresh.
    pub open_plan: Option<&'a SessionPlan>,
    /// The session id the task ids are keyed on.
    pub session_id: &'a str,
    /// The enrolled course, or `None` for the whole curriculum.
    pub course_id: Option<&'a str>,
    /// The remediation queue, in trigger order.
    pub pending_remediation: &'a [PendingRemediation],
    /// The quiz cadence state.
    pub quiz_state: Option<&'a QuizState>,
    /// Topic id to the UTC microseconds it was first mastered.
    pub learned_at: Option<&'a BTreeMap<String, i64>>,
    /// Topic id to the UTC microseconds of its last drill.
    pub last_drill_at: Option<&'a BTreeMap<String, i64>>,
    /// The blocking chain to serve while switched down into a gap course.
    pub gap_fill_chain: Option<&'a BTreeSet<String>>,
    /// The course a gap-fill lesson returns to.
    pub gap_return_to: Option<&'a str>,
    /// The distinct dates the learner studied, for the quiz cadence.
    pub active_study_days: Option<&'a [NaiveDate]>,
    /// The consecutive high-scoring quizzes, for the difficulty band.
    pub quiz_high_score_streak: i64,
    /// The topics classified at the raised test-prep due threshold.
    pub test_prep_topics: &'a BTreeSet<String>,
    /// The lifetime count of closed multi-step tasks.
    pub multistep_closed: i64,
    /// The task ids already closed this session.
    pub closed_task_ids: &'a BTreeSet<String>,
    /// The components of a multi-step task already served and still open.
    pub open_multistep_components: Option<&'a [String]>,
    /// The cap on the number of served tasks.
    pub n: Option<usize>,
    /// The readiness of the knowledge points (D-F5).
    ///
    /// `None` turns the eligibility rule off, and so does
    /// `Config::readiness::enforce` set to `false`. A caller that reads no
    /// content store therefore plans as it did before this rule.
    pub readiness: Option<&'a dyn ReadinessGate>,
    /// What the delayed probes answered so far (D-F11).
    ///
    /// `None` turns the probe schedule off, so a caller that never reads the
    /// retention state plans as it did before this rule.
    pub retention: Option<&'a RetentionState>,
}

impl Default for SessionContext<'_> {
    fn default() -> Self {
        Self {
            open_plan: None,
            session_id: "s",
            course_id: None,
            pending_remediation: &NO_REMEDIATION,
            quiz_state: None,
            learned_at: None,
            last_drill_at: None,
            gap_fill_chain: None,
            gap_return_to: None,
            active_study_days: None,
            quiz_high_score_streak: 0,
            test_prep_topics: &NO_IDS,
            multistep_closed: 0,
            closed_task_ids: &NO_IDS,
            open_multistep_components: None,
            n: None,
            readiness: None,
            retention: None,
        }
    }
}

impl<'a> SessionContext<'a> {
    /// Set the session id.
    #[must_use]
    pub const fn with_session_id(mut self, session_id: &'a str) -> Self {
        self.session_id = session_id;
        self
    }

    /// Set the enrolled course.
    #[must_use]
    pub const fn with_course(mut self, course_id: Option<&'a str>) -> Self {
        self.course_id = course_id;
        self
    }

    /// Set the remediation queue.
    #[must_use]
    pub const fn with_pending_remediation(mut self, pending: &'a [PendingRemediation]) -> Self {
        self.pending_remediation = pending;
        self
    }

    /// Set the quiz cadence state.
    #[must_use]
    pub const fn with_quiz_state(mut self, quiz_state: Option<&'a QuizState>) -> Self {
        self.quiz_state = quiz_state;
        self
    }

    /// Set the first-mastered times that date the quiz strata.
    #[must_use]
    pub const fn with_learned_at(mut self, learned_at: Option<&'a BTreeMap<String, i64>>) -> Self {
        self.learned_at = learned_at;
        self
    }

    /// Set the last-drill times that rate-limit the drills.
    #[must_use]
    pub const fn with_last_drill_at(
        mut self,
        last_drill_at: Option<&'a BTreeMap<String, i64>>,
    ) -> Self {
        self.last_drill_at = last_drill_at;
        self
    }

    /// Set the active study days of the quiz cadence.
    #[must_use]
    pub const fn with_active_study_days(mut self, days: Option<&'a [NaiveDate]>) -> Self {
        self.active_study_days = days;
        self
    }

    /// Set the consecutive high-scoring quizzes.
    #[must_use]
    pub const fn with_quiz_streak(mut self, streak: i64) -> Self {
        self.quiz_high_score_streak = streak;
        self
    }

    /// Set the test-prep topics.
    #[must_use]
    pub const fn with_test_prep(mut self, topics: &'a BTreeSet<String>) -> Self {
        self.test_prep_topics = topics;
        self
    }

    /// Set the multi-step counters and the closed task ids.
    #[must_use]
    pub const fn with_multistep(
        mut self,
        closed: i64,
        closed_task_ids: &'a BTreeSet<String>,
    ) -> Self {
        self.multistep_closed = closed;
        self.closed_task_ids = closed_task_ids;
        self
    }

    /// Cap the number of served tasks.
    #[must_use]
    pub const fn with_limit(mut self, n: Option<usize>) -> Self {
        self.n = n;
        self
    }

    /// Set the delayed-probe state of D-F11.
    #[must_use]
    pub const fn with_retention(mut self, retention: Option<&'a RetentionState>) -> Self {
        self.retention = retention;
        self
    }

    /// Set the readiness rule of D-F5.
    #[must_use]
    pub const fn with_readiness(mut self, readiness: Option<&'a dyn ReadinessGate>) -> Self {
        self.readiness = readiness;
        self
    }

    /// Re-serve an open plan instead of composing afresh.
    #[must_use]
    pub const fn with_open_plan(mut self, open_plan: Option<&'a SessionPlan>) -> Self {
        self.open_plan = open_plan;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builder_sets_its_field() {
        let closed: BTreeSet<String> = ["s-quiz".to_owned()].into();
        let learned_at: BTreeMap<String, i64> = BTreeMap::new();
        let days: [NaiveDate; 0] = [];
        let plan = SessionPlan::default();
        let ctx = SessionContext::default()
            .with_session_id("s")
            .with_course(Some("c"))
            .with_pending_remediation(&NO_REMEDIATION)
            .with_quiz_state(None)
            .with_learned_at(Some(&learned_at))
            .with_last_drill_at(Some(&learned_at))
            .with_active_study_days(Some(&days))
            .with_quiz_streak(2)
            .with_test_prep(&closed)
            .with_multistep(1, &closed)
            .with_limit(Some(3))
            .with_open_plan(Some(&plan));
        assert_eq!(ctx.course_id, Some("c"));
        assert_eq!(ctx.quiz_high_score_streak, 2);
        assert_eq!(ctx.multistep_closed, 1);
        assert_eq!(ctx.n, Some(3));
        assert!(ctx.open_plan.is_some() && ctx.learned_at.is_some());
        assert!(ctx.active_study_days.is_some() && ctx.last_drill_at.is_some());
        assert!(ctx.test_prep_topics.contains("s-quiz"));
    }
}
