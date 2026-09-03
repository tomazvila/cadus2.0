//! The session view: the whole-log maps of one tenant, folded once and
//! cached beside the learner model (D4, second cached document).

use std::collections::{BTreeMap, BTreeSet};

use cadus_core::event::{
    Attempt, EnrollReason, Enrolled, Event, LessonResult, ReviewResult, SessionEnd, SessionStart,
    TaskServed, TaskType,
};
use serde::{Deserialize, Serialize};
use sqlx::types::chrono::{DateTime, NaiveDate, Utc};

use super::EventRow;

/// A quiz scored at or above this counts as aced (`service.py:1096`).
pub const QUIZ_HIGH_SCORE: f64 = 0.9;

/// The document version of [`SessionView`].
///
/// A bump makes every stored view stale, so the next read replays the whole log,
/// the way a `projector_version` bump does for the model (D4).
pub const SESSION_VIEW_VERSION: i64 = 1;

/// The whole-log maps of one tenant, folded once and cached beside the model.
///
/// Every field is a LEFT FOLD over the log in `seq` order, so
/// [`SessionView::fold`] over the events after a cursor gives the same document
/// as [`SessionView::of_log`] over the whole log. That property is what lets a
/// request read no event row at all, and
/// `crates/store/src/state.rs::tests::the_forward_fold_equals_the_whole_log_fold`
/// holds it.
///
/// The fold reads the RAW log. A `regraded` event changes no field here, and the
/// three replay rules of [`project_current`] still replay the view, so the view
/// and the model always describe the same `through_seq`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionView {
    /// The document version. It is [`SESSION_VIEW_VERSION`] on a fresh fold.
    pub v: i64,
    /// The open session id: a `session_start` with no later `session_end`.
    pub current_session: Option<String>,
    /// The `events.seq` of the `session_start` that opened [`Self::current_session`].
    pub session_start_seq: Option<i64>,
    /// The sessions that started and did not end. `session_xp` credits all of
    /// them, which is what the per-session scan of 1.0 does.
    pub open_sessions: BTreeSet<String>,
    /// Every session id the log ever opened. `new_session_id` reads it.
    pub session_ids: BTreeSet<String>,
    /// The XP each session credited, before the two-place rounding.
    pub session_xp: BTreeMap<String, f64>,
    /// The enrollment stack, base first and effective last (`service.py:1142`).
    pub enrollment_stack: Vec<String>,
    /// Topic id to the instant it was FIRST passed (`service.py:1086`).
    pub learned_at: BTreeMap<String, i64>,
    /// Topic id to the instant of its last served drill (`service.py:1120`).
    pub last_drill_at: BTreeMap<String, i64>,
    /// The task ids a `review_result` already closed (`service.py:1261`).
    pub closed_task_ids: BTreeSet<String>,
    /// Topic id to the knowledge points a FAILED `lesson_result` stopped at,
    /// over the whole log (`_topic_already_failed`, `service.py:495-513`).
    ///
    /// The repeat-fail peel-back of section 8 reads it. A second failure of one
    /// lesson is only reachable in a LATER session, because a lesson task id is
    /// `{session}-lesson-{topic}` and the closed task stays done for the rest of
    /// its session, so the open-session window cannot answer the question and
    /// this whole-log map is the only place the earlier failure stands (M5
    /// review 2, finding V2).
    ///
    /// A result with no `failed_at_kp` folds nothing: the peel-back names the
    /// key prerequisites of ONE knowledge point, and a topic that authors none
    /// has no prerequisite to peel back to.
    ///
    /// The field carries no `serde` default ON PURPOSE. A document written
    /// before this map does not read back, [`load_learner_model`] then treats it
    /// as absent, and the next fold rebuilds the view from the whole log. That
    /// is why migration 0010 needs no backfill.
    pub lesson_failures: BTreeMap<String, BTreeSet<String>>,
    /// The distinct UTC dates that carry an attempt (`service.py:1099`).
    pub active_study_days: BTreeSet<NaiveDate>,
    /// The trailing run of quizzes scored at or above [`QUIZ_HIGH_SCORE`].
    pub quiz_high_score_streak: i64,
    /// Whether any `diagnostic_placed` event stands in the log.
    pub has_diagnostic: bool,
}

impl Default for SessionView {
    fn default() -> Self {
        Self {
            v: SESSION_VIEW_VERSION,
            current_session: None,
            session_start_seq: None,
            open_sessions: BTreeSet::new(),
            session_ids: BTreeSet::new(),
            session_xp: BTreeMap::new(),
            enrollment_stack: Vec::new(),
            learned_at: BTreeMap::new(),
            last_drill_at: BTreeMap::new(),
            closed_task_ids: BTreeSet::new(),
            lesson_failures: BTreeMap::new(),
            active_study_days: BTreeSet::new(),
            quiz_high_score_streak: 0,
            has_diagnostic: false,
        }
    }
}

impl SessionView {
    /// Fold one event at line `seq` into the view.
    pub fn apply(&mut self, seq: i64, event: &Event) {
        match event {
            Event::SessionStart(body) => self.start_session(seq, body),
            Event::SessionEnd(body) => self.end_session(body),
            Event::Enrolled(body) => self.enroll(body),
            Event::TaskServed(body) => self.serve_task(body),
            Event::Attempt(body) => self.record_attempt(body),
            Event::LessonResult(body) => self.close_lesson(body),
            Event::ReviewResult(body) => self.close_review(body),
            Event::QuizResult(body) => {
                self.quiz_high_score_streak = if body.score >= QUIZ_HIGH_SCORE {
                    self.quiz_high_score_streak.saturating_add(1)
                } else {
                    0
                };
            }
            Event::DiagnosticPlaced(_) => self.has_diagnostic = true,
            _ => {}
        }
    }

    /// Open the session of a `session_start` at line `seq`.
    fn start_session(&mut self, seq: i64, body: &SessionStart) {
        self.current_session.clone_from(&body.session);
        self.session_start_seq = body.session.as_ref().map(|_| seq);
        if let Some(session) = body.session.as_ref() {
            self.session_ids.insert(session.clone());
            self.open_sessions.insert(session.clone());
        }
    }

    /// Close the session of a `session_end`.
    fn end_session(&mut self, body: &SessionEnd) {
        self.current_session = None;
        self.session_start_seq = None;
        if let Some(session) = body.session.as_ref() {
            self.open_sessions.remove(session);
        }
    }

    /// Push, pop, or reset the enrollment stack for an `enrolled`.
    fn enroll(&mut self, body: &Enrolled) {
        let course = body.course.as_str().to_string();
        match body.reason {
            Some(EnrollReason::GapFill) => self.enrollment_stack.push(course),
            Some(EnrollReason::GapReturn) => {
                if self.enrollment_stack.len() > 1 {
                    self.enrollment_stack.pop();
                }
            }
            None => self.enrollment_stack = vec![course],
        }
    }

    /// Stamp `last_drill_at` for a served drill.
    fn serve_task(&mut self, body: &TaskServed) {
        if body.task_type == TaskType::Drill
            && let Some(topic) = body.topic.as_ref()
        {
            self.last_drill_at
                .insert(topic.as_str().to_string(), body.ts.micros());
        }
    }

    /// Add the UTC date of an attempt to the active study days.
    fn record_attempt(&mut self, body: &Attempt) {
        if let Some(stamp) = DateTime::<Utc>::from_timestamp_micros(body.ts.micros()) {
            self.active_study_days.insert(stamp.date_naive());
        }
    }

    /// Fold a `lesson_result`: the first pass, the failed knowledge point, and
    /// the XP.
    fn close_lesson(&mut self, body: &LessonResult) {
        if body.passed {
            self.learned_at
                .entry(body.topic.as_str().to_string())
                .or_insert_with(|| body.ts.micros());
        } else if let Some(kp) = body.failed_at_kp.as_ref() {
            self.lesson_failures
                .entry(body.topic.as_str().to_string())
                .or_default()
                .insert(kp.as_str().to_string());
        }
        self.credit(body.xp);
    }

    /// Fold a `review_result`: the closed task and the XP.
    fn close_review(&mut self, body: &ReviewResult) {
        if let Some(task_id) = body.task_id.as_ref() {
            self.closed_task_ids.insert(task_id.clone());
        }
        self.credit(body.xp);
    }

    /// Credit `xp` to every session that is open at this point of the log.
    fn credit(&mut self, xp: f64) {
        for session in &self.open_sessions {
            *self.session_xp.entry(session.clone()).or_insert(0.0) += xp;
        }
    }

    /// Fold the events of `rows` into the view, in `seq` order.
    pub fn fold(&mut self, rows: &[EventRow]) {
        for row in rows {
            self.apply(row.seq, &row.event);
        }
    }

    /// The view of a whole log.
    #[must_use]
    pub fn of_log(rows: &[EventRow]) -> Self {
        let mut view = Self::default();
        view.fold(rows);
        view
    }

    /// Whether a FAILED `lesson_result` already stopped `topic` at `kp`.
    ///
    /// This is the repeat test of the peel-back (`_topic_already_failed`,
    /// `service.py:495-513`). A caller that names no knowledge point gets
    /// `false`: [`Self::lesson_failures`] folds no result without one.
    #[must_use]
    pub fn already_failed(&self, topic: &str, kp: Option<&str>) -> bool {
        let Some(kp) = kp else { return false };
        self.lesson_failures
            .get(topic)
            .is_some_and(|points| points.contains(kp))
    }

    /// The XP the log credits inside one session, rounded to two places.
    #[must_use]
    pub fn xp_in_session(&self, session: &str) -> f64 {
        let total = self.session_xp.get(session).copied().unwrap_or(0.0);
        (total * 100.0).round() / 100.0
    }

    /// The active study days, oldest first.
    #[must_use]
    pub fn study_days(&self) -> Vec<NaiveDate> {
        self.active_study_days.iter().copied().collect()
    }

    /// The next unused session id of `today`: `s_<date><letter>`.
    ///
    /// The letters run `a` to `z` (`service.py:316`). A day that used all 26
    /// gives `z` again, which is what the 1.0 loop does when it falls off.
    #[must_use]
    pub fn new_session_id(&self, today: DateTime<Utc>) -> String {
        let prefix = format!("s_{}", today.date_naive());
        for letter in SESSION_LETTERS.chars() {
            let candidate = format!("{prefix}{letter}");
            if !self.session_ids.contains(&candidate) {
                return candidate;
            }
        }
        format!("{prefix}z")
    }
}

/// The letters a same-day session id takes, in order (`service.py:316`).
const SESSION_LETTERS: &str = "abcdefghijklmnopqrstuvwxyz";

#[cfg(test)]
mod tests;
