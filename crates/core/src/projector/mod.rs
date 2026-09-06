//! The fold: the event log replayed into the learner model (R5, D3, D4, spec section 4).
//!
//! The projector is the ONE writer of the derived learner model. It is a deterministic
//! fold: the same events, curriculum, and config always give the same model, so a full
//! rebuild and an incremental resume agree byte for byte, apart from `built_from_ts`.
//!
//! Two paths, one code path
//! ------------------------
//! [`Projector::apply`] folds one event. Its `apply_fire` flag decides whether the
//! expensive per-topic FIRe state changes:
//!
//! - [`project`] applies every event with `apply_fire = true`.
//! - [`project_incremental`] seeds the topic states from a CACHED model, replays the
//!   prior events with `apply_fire = false` (the cheap indices only), then applies the
//!   new events with `apply_fire = true`.
//!
//! Everything except the FIRe topic states — XP, streak, velocity, quiz cadence,
//! pending remediation, the mastery dates — is a cheap tally over event fields. The
//! fold always rebuilds those from the whole stream, so a cache never holds them.
//!
//! Time
//! ----
//! Velocity, "today", and the streak read `t_ref`, the LAST event's instant, never a
//! wall clock (trap T10). Only `built_from_ts` carries `now`, and the parity blob drops
//! that field.
//!
//! Iteration order
//! ---------------
//! Wherever 1.0 iterates a Python set, this port iterates the same ids SORTED (trap
//! T5): the mastery floor of `enrolled` and the peel-back candidates. Wherever 1.0
//! relies on dict insertion order, this port keeps that order (trap T6): the `balances`
//! map of `diagnostic_placed` and the ability delta map of the FIRe update.
//!
//! No handler panics. A malformed event never reaches the fold, because
//! [`crate::event::Event::from_json`] rejects it with an error value first; an event
//! that names a topic outside the curriculum is skipped exactly where 1.0 skips it.

use std::collections::{BTreeMap, BTreeSet};

use chrono_tz::Tz;

use crate::config::Config;
use crate::curriculum::Curriculum;
use crate::event::{Event, Slug};
use crate::fire::{clamp, py_max, py_min};
use crate::learner::{TopicState, UngradedAttempt};
use crate::numeric::{OutOfRangeError, TimeError, resolve_timezone};
use crate::retention::state::RetentionState;

mod entry;
mod handlers;
mod pass_rule;
mod regrade;
mod state;

pub use entry::{
    ProjectionInput, blob_digest, canonical_blob, kp_failed, kp_passed, project,
    project_incremental,
};
pub use pass_rule::PassRule;
pub use regrade::apply_regrades;

/// The projection-logic version stamped into the model (`projector.py:89`).
///
/// A stale cache is detected with it: 1 to 2 for the methodology fixes, 2 to 3 for
/// [`apply_regrades`], 3 to 4 for the third attempt outcome (D-F2). ANY change to the
/// fold bumps this number, and a bump replays every model in full (D-O6).
pub const PROJECTOR_VERSION: i64 = 5;

/// The neutral prior a placed topic's diagnostic answers fold onto (`projector.py:98`).
pub const ABILITY_SEED_PRIOR: f64 = 0.5;

/// The discount on an INFERRED placed topic's neighborhood ability (`projector.py:105`).
pub const INFERRED_SEED_BIAS: f64 = 0.9;

/// The placement rep-credit cap, `repNum = min(balance, 4)` (`diagnostic.py:71`).
pub const PLACEMENT_REPNUM_CAP: f64 = 4.0;

/// A freshly placed topic is fully remembered now (`diagnostic.py:76`).
pub const PLACEMENT_MEMORY_BASE: f64 = 1.0;

/// The daily XP goal the 1.0 entry points default to (`projector.py:779`).
pub const DEFAULT_XP_GOAL: i64 = 40;

/// What the fold reports instead of a panic.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProjectorError {
    /// A time zone name or an instant that `chrono` does not represent.
    #[error("{0}")]
    Time(#[from] TimeError),
    /// The config or the model did not serialize.
    #[error("serialize: {0}")]
    Serialize(String),
    /// A topic decay left the finite range at one event of the stream.
    ///
    /// CPython raises `OverflowError` in `0.5 ** exponent` at the same input and the
    /// 1.0 fold then builds NO model, so the 2.0 fold builds none either.
    #[error("the decay of topic `{topic}` at event {event_index} is not a finite number")]
    NonFinite {
        /// The id of the topic whose decay left the finite range.
        topic: String,
        /// The 0-based index of the event in the sequence the fold applied. For a
        /// resume that sequence is the corrected stream of `project_incremental`.
        event_index: usize,
    },
    /// A rounded XP value that the `i64` range does not hold.
    ///
    /// Python `int()` has unbounded precision, so 1.0 keeps the exact big integer
    /// there. This is a DOCUMENTED divergence (spec section 7, trap T22).
    #[error("{0}")]
    OutOfRange(#[from] OutOfRangeError),
}

/// A refreshed topic's `repNum`: the mean of the current one and the refresh evidence
/// (`diagnostic.py:318-326`).
///
/// A positive balance implies `min(balance, 4)` reps; a non-positive balance implies
/// zero, so the mean pulls a stale topic DOWN.
#[must_use]
pub fn refreshed_repnum(old_rep: f64, balance: f64) -> f64 {
    let refresh_rep = py_min(py_max(0.0, balance), PLACEMENT_REPNUM_CAP);
    (old_rep + refresh_rep) / 2.0
}

/// `projector._clamp01`: Python `max(0.0, min(1.0, x))` (`projector.py:694-695`).
fn clamp01(x: f64) -> f64 {
    clamp(x, 0.0, 1.0)
}

// --------------------------------------------------------------------------- //
// The accumulator
// --------------------------------------------------------------------------- //

/// A mutable fold accumulator over the event log (`projector.py:131-682`).
#[derive(Debug, Clone)]
pub struct Projector<'a> {
    graph: &'a Curriculum,
    cfg: &'a Config,
    zone: Tz,
    goal: i64,

    /// The FIRe topic states: the expensive part, seeded from a cache on resume.
    topics: BTreeMap<String, TopicState>,

    // The light indices. The fold always rebuilds them from the whole stream.
    enrolled_course: Option<String>,
    xp_events: Vec<(i64, f64)>,
    completions: Vec<(i64, String)>,
    learned_at: BTreeMap<String, i64>,
    quiz_last_ts: Option<i64>,
    quiz_retake_pending: bool,
    remediation: Vec<(i64, String, Vec<Slug>)>,
    /// Count of complete tasks in the event stream.
    completed_tasks: usize,
    /// Idempotent task-close identities for delayed confirmation.
    completed_task_ids: BTreeSet<String>,
    /// Independent feedback evidence and the task count that makes its probe due.
    feedback_confirmations: Vec<(i64, String, usize)>,
    last_practice: BTreeMap<String, i64>,
    diag_answers: BTreeMap<String, Vec<(bool, f64)>>,
    ungraded: Vec<UngradedAttempt>,
    /// What the delayed probes answered (D-F11). It folds `retention_probe` and
    /// nothing else, so it is empty for every log written before 2.0.
    retention: RetentionState,
    last_ts: Option<i64>,

    /// The task ids of the OPEN confirmation items (D-F6). A `task_served` with
    /// `confirm` set opens one, and the review result of the topic closes it.
    confirm_tasks: BTreeSet<String>,
    /// The topics of the OPEN confirmation items. A `review_result` with no
    /// `task_id` binds through the topic instead.
    confirm_topics: BTreeSet<String>,

    /// The count of the events the fold applied. It is also the 0-based index of the
    /// event the fold applies now.
    applied: usize,

    /// The first error the fold met. It stops the fold and [`Projector::finalize`]
    /// reports it, the way the 1.0 exception stops the 1.0 fold.
    failure: Option<ProjectorError>,
}

impl<'a> Projector<'a> {
    /// A fold over `graph` and `cfg`, in UTC, with the 1.0 default XP goal.
    #[must_use]
    pub fn new(graph: &'a Curriculum, cfg: &'a Config) -> Self {
        Self {
            graph,
            cfg,
            zone: Tz::UTC,
            goal: DEFAULT_XP_GOAL,
            topics: BTreeMap::new(),
            enrolled_course: None,
            xp_events: Vec::new(),
            completions: Vec::new(),
            learned_at: BTreeMap::new(),
            quiz_last_ts: None,
            quiz_retake_pending: false,
            remediation: Vec::new(),
            completed_tasks: 0,
            completed_task_ids: BTreeSet::new(),
            feedback_confirmations: Vec::new(),
            last_practice: BTreeMap::new(),
            diag_answers: BTreeMap::new(),
            ungraded: Vec::new(),
            retention: RetentionState::default(),
            last_ts: None,
            confirm_tasks: BTreeSet::new(),
            confirm_topics: BTreeSet::new(),
            applied: 0,
            failure: None,
        }
    }

    /// Set the time zone of the day boundary. `None` means UTC (trap T9).
    ///
    /// # Errors
    ///
    /// Returns [`TimeError::UnknownTimezone`] when the name is not in the database.
    pub fn with_timezone(mut self, tz: Option<&str>) -> Result<Self, TimeError> {
        self.zone = resolve_timezone(tz)?;
        Ok(self)
    }

    /// Set the daily XP goal the streak compares against.
    #[must_use]
    pub const fn with_goal(mut self, goal: i64) -> Self {
        self.goal = goal;
        self
    }

    /// Seed the FIRe topic states from a cached model (the incremental resume).
    #[must_use]
    pub fn with_cached_topics(mut self, topics: BTreeMap<String, TopicState>) -> Self {
        self.topics = topics;
        self
    }

    /// The topic states as they stand.
    #[must_use]
    pub const fn topics(&self) -> &BTreeMap<String, TopicState> {
        &self.topics
    }

    /// The instant of the last event folded, or `None` before the first one.
    #[must_use]
    pub const fn last_ts(&self) -> Option<i64> {
        self.last_ts
    }

    // -- the public fold ---------------------------------------------------- //

    /// Fold one event (`projector.py:161-194`).
    ///
    /// `apply_fire` gates every per-topic FIRe write, so a resume replays history for
    /// its light indices without running FIRe again.
    ///
    /// A `regraded` event never arrives here: [`apply_regrades`] consumes it ahead of
    /// the fold and hands the corrected events over instead. `task_served`,
    /// `session_start`, `session_end`, `anki_card_created`, `config_changed`,
    /// and `curriculum_changed` carry no derived state, so they are no-ops.
    ///
    /// `retention_probe` DOES carry derived state (D-F11): it tallies into
    /// [`crate::retention::RetentionState`]. The event type is new in the 2.0
    /// schema and no committed 1.0 log holds one, so every cached model of an
    /// earlier log still equals a full replay and [`PROJECTOR_VERSION`] stands.
    /// The `task_served` confirmation marker of D-F6 makes the same argument.
    ///
    /// `task_served` is a no-op too, EXCEPT for the D-F6 confirmation marker: a
    /// served confirmation opens the item that the topic's review result closes.
    /// The path is unreachable for every log written before D-F6, because no
    /// such log carries the field. [`PROJECTOR_VERSION`] therefore stands: every
    /// cached model of an earlier log still equals a full replay.
    ///
    /// After a failed event the fold applies NOTHING more, and
    /// [`Projector::finalize`] reports the failure. A 1.0 exception ends the 1.0 fold
    /// at the same event.
    pub fn apply(&mut self, event: &Event, apply_fire: bool) {
        if self.failure.is_some() {
            return;
        }
        let ts = event.ts().micros();
        self.last_ts = Some(self.last_ts.map_or(ts, |last| last.max(ts)));

        match event {
            Event::Enrolled(body) => self.on_enrolled(body, apply_fire),
            Event::Attempt(body) => self.on_attempt(body, apply_fire),
            Event::LessonResult(body) => self.on_lesson_result(body, ts, apply_fire),
            Event::ReviewResult(body) => self.on_review_result(body, ts, apply_fire),
            Event::QuizResult(body) => self.on_quiz_result(body, ts, apply_fire),
            Event::RemediationTriggered(body) => self.on_remediation(body, ts),
            Event::DiagnosticAnswer(body) => self.on_diagnostic_answer(body),
            Event::DiagnosticPlaced(body) => self.on_diagnostic_placed(body, ts, apply_fire),
            Event::ProfileReset(body) => self.on_profile_reset(body, apply_fire),
            Event::TaskServed(body) => self.on_task_served(body),
            Event::SessionStart(_)
            | Event::SessionEnd(_)
            | Event::Regraded(_)
            | Event::AnkiCardCreated(_)
            | Event::ConfigChanged(_)
            | Event::CurriculumChanged(_)
            // D-F10. The integrated events carry their own evidence: the serve
            // records the exposure and the attempt records every field with the
            // contract that decided it. The fold credits no skill from them
            // here, because only a DECIDED field may credit one and the
            // progression path of the web tier owns that write. An undecided
            // field must never move a topic state (D-F2).
            | Event::IntegratedServed(_)
            | Event::IntegratedAttempt(_) => {}
            Event::RetentionProbe(body) => self.retention.apply(body, &self.cfg.retention),
        }
        self.applied += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Timestamp, TopicStatus};
    use crate::fire::testing::{ladder, topic};

    /// Course `c` with the floor `base`; `p` needs `base`, `q` needs `p`, and
    /// `r` and `s` stand alone.
    pub(super) fn tree() -> Curriculum {
        ladder(&[(
            "c",
            &["base"],
            vec![
                topic("base", &[]),
                topic("p", &[("base", 0.8, true)]),
                topic("q", &[("p", 0.8, true)]),
                topic("r", &[]),
                topic("s", &[]),
            ],
        )])
    }

    /// One event from its wire JSON.
    pub(super) fn ev(json: &str) -> Event {
        Event::from_json(json).expect("the test event reads")
    }

    /// A stream that reaches every handler of the fold at least once.
    pub(super) fn stream() -> Vec<Event> {
        [
            r#"{"type":"enrolled","ts":"2026-07-14T12:00:00Z","course":"c"}"#,
            r#"{"type":"diagnostic_answer","ts":"2026-07-14T12:01:00Z","topic":"q","correct":true,"secs":10,"weight":1.0}"#,
            r#"{"type":"diagnostic_placed","ts":"2026-07-14T12:02:00Z","balances":{"q":2.0,"p":0.5,"ghost":1.0,"r":0.0},"conditional":["p"]}"#,
            r#"{"type":"attempt","ts":"2026-07-14T12:03:00Z","attempt_id":"a1","task_id":"t1","topic":"p","task_type":"lesson","problem":{"text":"1+1","expected":"2"},"given_answer":"3","correct":false,"secs":5,"work_quality":"poor"}"#,
            r#"{"type":"lesson_result","ts":"2026-07-14T12:04:00Z","topic":"p","passed":false,"failed_at_kp":"kp1","quality_tier":"poor"}"#,
            r#"{"type":"lesson_result","ts":"2026-07-14T12:05:00Z","topic":"p","passed":false,"failed_at_kp":"kp1","quality_tier":"poor"}"#,
            r#"{"type":"lesson_result","ts":"2026-07-14T12:06:00Z","topic":"r","passed":true,"xp":10.0,"quality_tier":"perfect"}"#,
            r#"{"type":"lesson_result","ts":"2026-07-14T12:07:00Z","topic":"r","passed":true,"xp":10.0,"quality_tier":"perfect"}"#,
            r#"{"type":"review_result","ts":"2026-07-14T12:08:00Z","topic":"q","passed":true,"weighted_score":1.0,"xp":5.0,"quality_tier":"perfect"}"#,
            r#"{"type":"quiz_result","ts":"2026-07-14T12:09:00Z","quiz_id":"z","score":0.5,"xp":2.0,"per_topic":[{"topic":"q","correct":true,"secs":1},{"topic":"ghost","correct":true,"secs":1},{"topic":"r","correct":false,"secs":1}]}"#,
            r#"{"type":"remediation_triggered","ts":"2026-07-14T12:10:00Z","kind":"quiz_miss","source_topic":"r","targets":["q","ghost"]}"#,
            r#"{"type":"enrolled","ts":"2026-07-14T12:11:00Z","course":"c"}"#,
            r#"{"type":"diagnostic_answer","ts":"2026-07-14T12:12:00Z","topic":"q","correct":false,"secs":10,"weight":0.5}"#,
            r#"{"type":"diagnostic_placed","ts":"2026-07-14T12:13:00Z","balances":{"q":1.0,"ghost":1.0,"s":-1.0,"r":-1.0},"refresh":true}"#,
            r#"{"type":"profile_reset","ts":"2026-07-14T12:14:00Z","topics":["r","ghost"]}"#,
            r#"{"type":"attempt","ts":"2026-07-14T12:15:00Z","attempt_id":"a2","task_id":"t2","topic":"ghost","task_type":"lesson","problem":{"text":"2+2","expected":"4"},"given_answer":"5","correct":false,"secs":5,"work_quality":"poor"}"#,
            r#"{"type":"lesson_result","ts":"2026-07-14T12:16:00Z","topic":"ghost","passed":false,"failed_at_kp":"kp9","quality_tier":"poor"}"#,
            r#"{"type":"lesson_result","ts":"2026-07-14T12:17:00Z","topic":"ghost","passed":false,"quality_tier":"poor"}"#,
            r#"{"type":"lesson_result","ts":"2026-07-14T12:18:00Z","topic":"ghost","passed":true,"quality_tier":"perfect"}"#,
            r#"{"type":"session_start","ts":"2026-07-14T12:19:00Z"}"#,
        ]
        .map(ev)
        .to_vec()
    }

    #[test]
    fn the_fold_reaches_every_handler_and_assembles_the_model() {
        let tree = tree();
        let cfg = Config::default();
        let mut proj = Projector::new(&tree, &cfg).with_goal(40);
        for event in stream() {
            proj.apply(&event, true);
        }
        assert_eq!(proj.last_ts(), Some(1_784_031_540_000_000));
        let model = proj
            .finalize(Timestamp::from_micros(1_784_031_400_000_000))
            .expect("the fold assembles");
        assert_eq!(model.topics["base"].status, TopicStatus::Floor);
        assert_eq!(model.topics["q"].status, TopicStatus::Placed);
        assert!(!model.topics["p"].conditional);
        assert!(!model.topics.contains_key("r"));
        assert!(!model.topics.contains_key("s"));
        assert_eq!(model.xp.total, 27);
        assert!(model.quiz.retake_pending);
        assert_eq!(model.pending_remediation.len(), 1);
        assert_eq!(model.projector_version, Some(PROJECTOR_VERSION));
    }

    #[test]
    fn a_light_replay_tallies_xp_and_writes_no_state() {
        let tree = tree();
        let cfg = Config::default();
        let mut proj = Projector::new(&tree, &cfg).with_cached_topics(BTreeMap::new());
        for event in stream() {
            proj.apply(&event, false);
        }
        assert!(proj.topics().is_empty());
        let model = proj
            .finalize(Timestamp::from_micros(1_784_031_400_000_000))
            .expect("the fold assembles");
        assert_eq!(model.xp.total, 27);
        assert_eq!(refreshed_repnum(3.0, -1.0), 1.5);
    }
}
