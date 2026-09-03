//! The one-transaction grade path of unit U8: `POST /api/task/{task_id}/answer`.
//!
//! Requirements: A3 (the verdict is decided locally for a right AND a wrong
//! answer), A4 (the prose arrives later; the verdict never waits for it), C2
//! (the log is append-only), C3 (every statement runs inside `begin_tenant`),
//! C4 (`correct` comes from [`cadus_core::answer::check`] alone), D-O2 (one
//! transaction), L2, L6, R4, T1 (this path spends no model token).
//!
//! Spec: `docs/reference/web-service-1.0-spec.md` section 2.1 (the reply),
//! section 4.3 (the ten steps of the transaction), section 5 (grading, the
//! tier, the tags, H3, timing, the caps), section 10 (the pinned literals), and
//! row U8 of section 11. Rulings D-M5-2, D-M5-3, D-M5-4 and D-M5-7 of
//! `docs/plans/M5.md` are binding.
//!
//! # The steps, in one transaction
//!
//! The advisory lock, the state read, the validate, the timing, the check, the
//! tier, one `INSERT … ON CONFLICT DO NOTHING`, the fold, the pool pop for
//! `next`, the state write, commit. Nothing here is slow: there is no model call
//! (R4, L6), so the whole request holds one short transaction and the 1.0
//! three-way `tolerate_duplicate` / `tolerate_closed` recovery is gone.
//!
//! # The attempt number and idempotency
//!
//! `attempt_id` is `"{task_id}-{n}"`, `n` being the 1-based position of the
//! attempt inside its task (`docs/plans/M3.md`, trap T12). [`attempt_index`]
//! reads that position from the LOG, inside this transaction and under the
//! tenant's advisory lock. The D-S6 row is loss-tolerant — `POST /api/enroll`
//! deletes it inside an open session — so a counter that lives there restarts
//! while the task ids stay, and every later attempt of that task then repeats an
//! id the log already holds. The log does not restart.
//!
//! # The log read (M5 review 1, finding F15)
//!
//! The window this path reads is `Open::events`, the events of the OPEN SESSION
//! and not the whole log. A task id is `{session}-{task_type}-{topic}`
//! (`cadus_core::selector::assign_ids`) and an attempt id is `{task_id}-{n}`, so
//! every attempt of every task of the open session stands in that window, and
//! [`attempt_index`], [`stored_attempt`] and [`advance`] are all task-scoped.
//! The whole-log read this path carried before grew with the lifetime event
//! count and passed the 150 ms Postgres segment of L2 on its own.
//! `crates/store/tests/bench_long_log.rs` is the gate.
//!
//! The H3 unaided re-solve takes the same id with `-rework` after it.
//!
//! # The history read (M5 review 2, finding V2)
//!
//! ONE decision of this path reads more than the open session: the repeat-fail
//! peel-back. A lesson task id is `{session}-lesson-{topic}`, and the failed
//! task is `done` in the D-S6 row for the rest of its own session, so a second
//! failure of one lesson is only reachable in a LATER session and the window
//! above can never hold the earlier `lesson_result`.
//! `cadus_store::state::load_session_view` answers it from the whole-log map
//! `SessionView::lesson_failures`, which costs one `learner_models` row plus
//! the events above the fold cursor and never grows with the lifetime event
//! count.
//!
//! A log this build writes is dense, so the number of a new attempt is free. A
//! log with a gap in it — an operator repair, or a 1.0 log whose ids came from
//! the problem id (spec section 4.1) — can still put the computed id on a row
//! that stands. The partial unique index on `(user_id, attempt_id)` then makes
//! the INSERT a no-op, and the reply is [`STATUS_ALREADY_RECORDED`] with the
//! STORED verdict and nothing appended (spec section 4.3 step 6: "reply with the
//! state read").
//!
//! # What this unit does NOT do
//!
//! The explicit task close of 1.0 (`service.complete_task`: the `review_result`,
//! the `quiz_result` and the multi-step closes, their weighted scores, and their
//! XP) is not ported yet, and neither is `POST /api/task/{id}/abort`. A
//! non-lesson task therefore reaches `done` by count parity, exactly as
//! `api.py:1610-1614` does, but no close event stands behind it.
//!
//! # The `diagnosis` field (unit U9)
//!
//! [`crate::diagnosis`] owns every rule of it. This path calls
//! `diagnosis::decide` at ONE point, after the attempt is appended and before
//! the commit, so the enqueue of spec section 4.3 step 9 shares the fate of the
//! attempt: a grade that rolls back leaves no job row. A quiz reply carries no
//! `diagnosis` and starts no job — a quiz reveals nothing before its batch
//! reveal (trap W7), and the reveal unit is the one that hands the prose out.

use std::future::Future;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use cadus_core::answer::check::{Outcome, check};
use cadus_core::config::Config;
use cadus_core::curriculum::{AnswerKind, Curriculum};
use cadus_core::event::{
    Attempt, AttemptProblem, Event, LessonResult, RemediationTriggered, SchemaVersion, Secs, Slug,
    TaskType, Timestamp, WorkQuality,
};
use cadus_core::projector::{kp_failed, kp_passed};
use cadus_core::selector::{
    REMEDIATION_LESSON_FAIL, REMEDIATION_REPEAT_FAIL, Task, remediation_for_repeat_fail,
};
use cadus_core::xp::{is_rushing, task_xp};
use cadus_store::StoreError;
use cadus_store::state::{
    EventRow, SessionView, append_event, load_session_view, project_and_save,
};
use serde_json::{Map, Value, json};
use sqlx::types::Uuid;
use sqlx::{Postgres, Transaction};

use crate::AppState;
use crate::diagnosis::{self, Miss, Pending};
use crate::error::ApiError;
use crate::metrics;
use crate::path::ApiPath;
use crate::serve::{Open, find, install_next, open, progress_for, unix_seconds};
use crate::session::{bound, content, failed, now_pair, projection_input, write_state};
use crate::state::{
    Content, INVALID_REQUEST, STATE_UNAVAILABLE, ServedProblem, TaskProgress, Tenant, WebState,
};

mod advance;
mod reply;
mod route;
mod submission;
mod verdict;

use advance::*;
use reply::*;
pub use route::answer;
use submission::*;
use verdict::round2;
pub use verdict::{deterministic_grade, measure_secs, reference_assisted};

/// The code of an answer or a work field over its cap (`api.py:1292-1293`).
pub const ANSWER_TOO_LARGE: &str = "answer_too_large";

/// The code of an answer whose kind the checker never decides.
///
/// Spec section 5.1: a `multi-step` or a `proof` answer gets NO synchronous
/// verdict. V2 keeps both kinds out of the serving pool, so this refusal guards
/// a state the M5 routes cannot reach. It exists so that no path can fabricate a
/// `correct` the checker did not decide (C4), and it enqueues nothing.
pub const UNDECIDABLE_KIND: &str = "undecidable_kind";

/// The task is open and the learner owes it more problems.
pub const STATUS_CONTINUE: &str = "continue";

/// A lesson knowledge point passed and the lesson moved to the next one.
pub const STATUS_KP_ADVANCE: &str = "kp_advance";

/// The task closed with a pass.
pub const STATUS_TASK_PASSED: &str = "task_passed";

/// The task closed with a failure.
pub const STATUS_TASK_FAILED: &str = "task_failed";

/// The INSERT of step 6 met an `attempt_id` that already stands, so nothing was
/// appended (`api.py:596`, `:1592-1593`).
pub const STATUS_ALREADY_RECORDED: &str = "already_recorded";

/// The largest answer the route accepts, in characters (`api.py:91`).
pub const MAX_ANSWER_CHARS: usize = 4_000;

/// The largest work field the route accepts, in characters (`api.py:92`).
pub const MAX_WORK_CHARS: usize = 20_000;

/// `secs` is clamped at this multiple of the topic's `expected_time_secs`
/// (`api.py:737-740`).
pub const TIMING_CAP_MULTIPLIER: i64 = 10;

/// The note every grade of this path stamps (`deterministic_grade.py:68`).
pub const GRADER_NOTE: &str = "deterministic";

/// The form tag of a period-grouped integer (`deterministic_grade.py:146-149`).
pub const TAG_NOTATION: &str = "notation";

/// The tag of an elapsed time over the cap (`api.py:740`).
pub const TAG_TIMING_UNRELIABLE: &str = "timing-unreliable";

/// The tag of a blank submission (D-M5-7).
///
/// 1.0 writes `blank_answer`, the only underscore-spelled tag and the only one
/// outside its own vocabulary (trap W1). D-M5-7 adds the hyphenated spelling to
/// the vocabulary, so 2.0 ships one spelling rule and not the 1.0 exception.
pub const TAG_BLANK_ANSWER: &str = "blank-answer";

/// The stock re-solve instruction (D-M5-3, spec section 5.5).
///
/// The verdict now ships before any prose exists, so the instruction is a
/// constant and not model output. It is served on every miss and on every
/// assisted-correct `rework_required` reply (L2, L3).
pub const RE_SOLVE: &str = "Study the worked solution above until you can see why each step \
                            follows. Then close it and solve the original problem again \
                            yourself, from memory and unaided. Do that before you move on.";

/// The loaded content and the JSON body of a task route that reads one.
///
/// The content check runs first, so a deployment with no curriculum answers
/// `503` before any body is read.
pub(crate) fn route_input<'a>(
    state: &'a AppState,
    body: Option<&'a Json<Value>>,
) -> Result<(&'a Content, Option<&'a Value>), ApiError> {
    let content = content(state)?;
    Ok((content, body.map(|Json(value)| value)))
}

/// One deterministic verdict: what the log records and what the client reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grade {
    /// Whether the answer is the authored answer (C4).
    pub correct: bool,
    /// The work-quality tier (D-M5-2).
    pub work_quality: WorkQuality,
    /// The error tags the SERVER observed (D-M5-4). The diagnosis of unit U9
    /// adds its own later; it never moves the tier.
    pub error_tags: Vec<String>,
}
