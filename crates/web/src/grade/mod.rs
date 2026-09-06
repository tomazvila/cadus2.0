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
//! # The three outcomes (D-F2)
//!
//! [`deterministic_grade`] gives one of three outcomes. `correct` and `incorrect`
//! are the two decided ones, and `correct: bool` still spells them. The third is
//! UNGRADED: the checker had no verdict, so the reply names the reason, claims no
//! correctness, reveals no solution, and hands back the NEXT task. The fold
//! ignores an ungraded attempt, the lesson does not advance on one, and the A4
//! diagnosis never fires for one (D-F4). EVERY answer kind reaches this path; a
//! `proof` takes [`PROOF_UNGRADED`] with no checker call.
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
//! D-F8 records the helped attempt, then serves a fresh same-skill problem.
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
//! # Task close
//!
//! A final review answer appends its evidence-aware review result (D-F7).
//! Quiz and multi-step result handlers remain separate work.
//!
//! # The `diagnosis` field (unit U9)
//!
//! [`crate::diagnosis`] owns every rule of it. This path calls
//! `diagnosis::decide` at ONE point, after the attempt is appended and before
//! the commit, so the enqueue of spec section 4.3 step 9 shares the fate of the
//! attempt: a grade that rolls back leaves no job row. A quiz reply carries no
//! `diagnosis` and starts no job — a quiz reveals nothing before its batch
//! reveal (trap W7), and the reveal unit is the one that hands the prose out.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use cadus_core::answer::check::{Outcome, check};
use cadus_core::config::Config;
use cadus_core::curriculum::{AnswerKind, Curriculum};
use cadus_core::event::{
    Attempt, AttemptOutcome, AttemptProblem, Event, LessonResult, RemediationTriggered,
    SchemaVersion, Secs, Slug, TaskType, Timestamp, WorkQuality,
};
use cadus_core::learner::problem_text_hash;
use cadus_core::projector::{kp_failed, kp_passed};
use cadus_core::selector::{
    REMEDIATION_LESSON_FAIL, REMEDIATION_REPEAT_FAIL, Task, remediation_for_repeat_fail,
};
use cadus_core::xp::{is_rushing, task_xp};
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
use crate::session::{content, now_pair, projection_input, write_state};
pub(crate) use crate::session::{db_failed, store};
use crate::state::{
    Content, INVALID_REQUEST, STATE_UNAVAILABLE, ServedProblem, TaskProgress, Tenant, WebState,
};

mod advance;
mod reply;
mod review;
mod route;
mod submission;
mod verdict;

use advance::*;
use reply::*;
pub use route::answer;
use submission::*;
use verdict::round2;
pub use verdict::{
    deterministic_grade, grade_item, measure_secs, reference_assisted, ungraded_grade,
};

/// The code of an answer or a work field over its cap (`api.py:1292-1293`).
pub const ANSWER_TOO_LARGE: &str = "answer_too_large";

/// The reason a `proof` answer carries no deterministic verdict (V2, D-F1).
///
/// No checker decides a proof, so the grade path spends no work on one: it names
/// this reason and records the UNGRADED outcome. The route no longer refuses the
/// kind with a `409`, because a refusal left the learner with no outcome at all
/// (audit 3a).
pub const PROOF_UNGRADED: &str = "no deterministic verdict for a proof";

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
pub const RE_SOLVE: &str = "Study the worked solution. Select Done studying to hide it, then solve a fresh problem without help.";

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
    ///
    /// It equals `outcome == AttemptOutcome::Correct`.
    pub correct: bool,
    /// The graded outcome (D-F2). An answer with no deterministic verdict is
    /// `Ungraded`, and an ungraded attempt is NOT a miss.
    pub outcome: AttemptOutcome,
    /// The work-quality tier (D-M5-2).
    pub work_quality: WorkQuality,
    /// The error tags the SERVER observed (D-M5-4). The diagnosis of unit U9
    /// adds its own later; it never moves the tier.
    pub error_tags: Vec<String>,
}
