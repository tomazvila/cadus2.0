//! The session and dashboard routes of unit U6, and the event scans they read.
//!
//! Requirements: A3 (the verdict is local), C2 (the log is append-only), C3
//! (every statement runs inside `begin_tenant`), C4, R4 (local CPU and database
//! I/O only), L6 (no model client links here), D-S6.
//!
//! Spec: `docs/reference/web-service-1.0-spec.md` section 2 (the route table),
//! section 4 (the state row and the lock), and section 11 row U6.
//!
//! # What each route touches
//!
//! | Route | Lock | Events | `web_states` | `learner_models` |
//! |---|---|---|---|---|
//! | `GET /api/status` | no | read the open session | no | read |
//! | `GET /api/graph` | no | none | no | read |
//! | `GET /api/modules` | no | none | no | read |
//! | `GET /api/export` | no | read all | no | no |
//! | `POST /api/enroll` | yes | append `enrolled` | clear | write |
//! | `POST /api/session/start` | yes | append `session_start` | bind | write |
//! | `POST /api/session/end` | yes | append `session_end` | clear | write |
//! | `GET /api/session/plan` | yes | read the open session | READ ONLY | read |
//!
//! "none" in the Events column means the route reads no event row when the
//! cursor already stands at the head of the log: the cached model and the cached
//! [`SessionView`] answer it (F15, F18). "read the open session" is the range
//! read of [`view_for_open_session`], the window of the OPEN SESSION and never
//! the whole log: the two routes need it for the drill cadence of the running
//! session (M5 review 2, findings V3 and V9). `GET /api/export` is the one route
//! that reads every row, and section 8 of `docs/reference/l1-budget.md` gives it
//! no p95 for that reason.
//!
//! `GET /api/session/plan` writes NOTHING. Trap W3 states the rule: the plan
//! reports each task through [`crate::state::WebState::plan_progress`], a plain
//! lookup, never 1.0's `_progress_for`, which installs a row per task merely
//! listed.
//!
//! # The event scans
//!
//! `current_session`, `new_session_id`, `session_xp`, `enrollment_stack`,
//! `learned_at`, `last_drill_at`, `active_study_days`, and
//! `quiz_high_score_streak` are ports of the 1.0 service-layer scans
//! (`cadus/service.py:298-334`, `:1078-1165`). Each one is a pure fold over the
//! log, so each one has its own test with literal expected values.
//!
//! The FOLD itself moved to [`cadus_store::state::SessionView`] (M5 review 1,
//! findings F15 and F18). The document is cached beside the learner model and
//! folds forward from `through_seq`, so no route on a learner path reads the
//! whole log any more. The eight functions below stay as the named entry points
//! the tests hold, and each one is now one call of that ONE fold: two copies of
//! a fold drift apart, one copy cannot.

use std::collections::{BTreeMap, BTreeSet};

use cadus_store::state::{EventRow, SessionView};
use sqlx::types::chrono::{DateTime, NaiveDate, Utc};

mod dashboard;
mod lifecycle;
mod plan;
mod store;

pub use dashboard::{GraphQuery, export, graph, modules, status};
pub use lifecycle::{enroll, session_end, session_start};
pub(crate) use plan::compose_plan;
pub use plan::session_plan;
pub(crate) use store::*;

/// The code of a failure the caller cannot fix.
pub const INTERNAL_ERROR: &str = "internal_error";

/// The media type of the JSONL export (`api.py:2264`).
pub const EXPORT_MEDIA_TYPE: &str = "application/x-ndjson";

/// A quiz scored at or above this counts as aced (`service.py:1096`).
///
/// The fold that reads it lives in the store, so the number is defined once.
pub const QUIZ_HIGH_SCORE: f64 = cadus_store::state::QUIZ_HIGH_SCORE;

// --------------------------------------------------------------------------- //
// The pure event scans
// --------------------------------------------------------------------------- //

// --------------------------------------------------------------------------- //
// The pure event scans
// --------------------------------------------------------------------------- //

/// The open session id: a `session_start` with no later `session_end`.
#[must_use]
pub fn current_session(events: &[EventRow]) -> Option<String> {
    SessionView::of_log(events).current_session
}

/// The next unused session id of the day: `s_<date><letter>`.
#[must_use]
pub fn new_session_id(events: &[EventRow], today: DateTime<Utc>) -> String {
    SessionView::of_log(events).new_session_id(today)
}

/// The XP the log credits inside one session, rounded to two places.
#[must_use]
pub fn session_xp(events: &[EventRow], session: &str) -> f64 {
    SessionView::of_log(events).xp_in_session(session)
}

/// The enrollment stack, base first and effective last (`service.py:1142-1165`).
///
/// A manual enroll resets the stack, a `gap-fill` switch pushes, and a
/// `gap-return` switch pops one level.
#[must_use]
pub fn enrollment_stack(events: &[EventRow]) -> Vec<String> {
    SessionView::of_log(events).enrollment_stack
}

/// Topic id to the instant it was FIRST passed (`service.py:1086-1091`).
#[must_use]
pub fn learned_at(events: &[EventRow]) -> BTreeMap<String, i64> {
    SessionView::of_log(events).learned_at
}

/// Topic id to the instant of its last served drill (`service.py:1120-1129`).
#[must_use]
pub fn last_drill_at(events: &[EventRow]) -> BTreeMap<String, i64> {
    SessionView::of_log(events).last_drill_at
}

/// The distinct UTC dates that carry an attempt (`service.py:1099-1103`).
#[must_use]
pub fn active_study_days(events: &[EventRow]) -> Vec<NaiveDate> {
    SessionView::of_log(events).study_days()
}

/// The trailing run of quizzes scored at or above [`QUIZ_HIGH_SCORE`].
#[must_use]
pub fn quiz_high_score_streak(events: &[EventRow]) -> i64 {
    SessionView::of_log(events).quiz_high_score_streak
}

/// The task ids a `review_result` already closed (`service.py:1261`).
#[must_use]
pub fn closed_task_ids(events: &[EventRow]) -> BTreeSet<String> {
    SessionView::of_log(events).closed_task_ids
}
