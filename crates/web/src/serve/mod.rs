//! The serve, teach, and hint routes of unit U7.
//!
//! Requirements: A6 (a pool miss falls back to authored exemplars and raises the
//! operator flag; it never generates), D5 (the anti-repeat ring and the per-task
//! memory), D-O1 (one transaction on the serve path), D-O3 (one `content_store`
//! read on the teach and hint paths), D-S6, L4, L5, R4, T1 (these three paths
//! spend no model token).
//!
//! Spec: `docs/reference/web-service-1.0-spec.md` section 2 (the route table),
//! section 4.1 (`served` is keyed by task id), section 5.6 (`started_at`), and
//! section 11 row U7; `docs/reference/serving-1.0-spec.md` sections 6 and 7.2.
//!
//! # Hard Rule 1
//!
//! A serve, a teach page, and a hint NEVER carry `expected` or the solution
//! sketch. Both documents stay in the D-S6 row, which the client cannot read,
//! and the payload builders below name every field they emit. Trap W7 gives the
//! test rule: scan the raw JSON, do not read the fields.
//!
//! # One transaction, and the clock
//!
//! There is no model call (T1, R4), so the whole serve is one transaction
//! (`serving-1.0-spec.md` section 7.2): the advisory lock, the projection, the
//! session-window read, the pool pop, the authored-solution read of a template
//! row, the `task_served` append of a task served for the first time, the fold
//! and save that append needs, the state write, commit. `started_at` is
//! re-stamped
//! at EVERY hand-off, a re-serve of a live problem included, because that is the
//! moment the problem goes on screen (section 5.6). A re-serve therefore returns
//! the SAME `problem_id` with a FRESH `started_at`.
//!
//! # What the serve reads (F18, V1, V8)
//!
//! A serve that appends NOTHING — the re-serve of a live problem and the second
//! to twentieth question of a task — reads no whole log: the projection answers
//! from the cached model and the cached [`cadus_store::state::SessionView`], and
//! the window read asks for the events of the OPEN SESSION only.
//!
//! The FIRST serve of a task appends `task_served`, and it then folds and saves
//! in the same transaction, so the log head and the fold cursor of
//! `learner_models` move together. That fold takes the incremental branch of
//! `cadus_store::state::project_current`, which reads the whole log ONCE, the
//! same term the grade path pays (`docs/reference/l1-budget.md` section 8). The
//! alternative is worse: a cursor left one line behind hands that whole-log read
//! to EVERY later serve, teach, hint, plan and status request of the learner,
//! until a grade or a session event repairs it.
//!
//! # The `task_served` event (D-M5-8)
//!
//! The serve appends one `task_served` event the first time a session serves a
//! task, and never a second one for that task id. The event is the only source
//! of `SessionView::last_drill_at`, and that map is the only gate of the
//! 3.5-day drill cadence, so before this append the selector re-offered every
//! drill-eligible topic in every session (M5 review 1, finding F17). 1.0 writes
//! the event at plan composition; `GET /api/session/plan` of 2.0 stays a pure
//! read (trap W3), so the serve is the append point.
//!
//! [`crate::session::view_for_open_session`] keeps the drill of the OPEN session
//! servable to its last question: the cadence gate reads the drills of the
//! EARLIER sessions. The plan route and the dashboard call that one helper too,
//! so the three routes list, gate and serve the same drill (M5 review 2,
//! findings V3 and V9).
//!
//! # The A6 fallback
//!
//! An empty pool must not generate. The handler instantiates the knowledge
//! point's authored exemplars in process (pure CPU over the arena), writes them
//! into the pool with `source = 'exemplar'`, and pops again. A pair whose whole
//! exemplar list is already claimed rotates through
//! [`reclaim_exemplar_tx`], which re-stamps `claimed_at` and so keeps
//! `last_exemplar_at` of the A6 operator view current.

use std::collections::{BTreeMap, BTreeSet};

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use cadus_core::curriculum::{Curriculum, KnowledgePoint};
use cadus_core::event::{Event, SchemaVersion, Slug, TaskServed, TaskType, Timestamp};
use cadus_core::pool::{Avoid, ExemplarSource, ProblemSource, Source, kp_key};
use cadus_core::selector::{SessionPlan, Task};
use cadus_core::template::{Bindings, Value as Binding, from_body, literal_to_rational, render};
use cadus_store::content::{ApprovedDoc, KIND_HINT_LADDER, KIND_TEACH, approved_document};
use cadus_store::pool::{
    NewInstance, PoolRow, approved_template, insert_batch, pop_with_ring_tx, reclaim_exemplar_tx,
};
use cadus_store::state::{
    EventRow, append_event, lock_web_state, project_and_save, project_current,
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use sqlx::types::Uuid;
use sqlx::{Postgres, Transaction};

use crate::AppState;
use crate::error::ApiError;
use crate::grade::{db_failed, route_input, store};
use crate::path::ApiPath;
use crate::session::{
    INTERNAL_ERROR, begin, compose_plan, content, now_pair, projection_input, read_state,
    readiness_of, view_for_open_session, write_state,
};
use crate::state::Content;
use crate::state::{
    INVALID_REQUEST, NO_OPEN_SESSION, ServedProblem, TASK_COMPLETE, TaskProgress, Tenant, WebState,
};

mod draw;
#[cfg(test)]
mod fixture;
mod hint;
mod payload;
mod route;
mod target;
mod teach;

use draw::*;
pub use hint::hint;
pub(crate) use payload::progress_for;
use payload::*;
pub(crate) use route::install_next;
pub use route::serve;
use target::*;
pub use teach::teach;

/// The code of a task id that is not in this session's plan.
pub const UNKNOWN_TASK: &str = "unknown_task";

/// The code of a multi-step task whose parts are all served (`api.py:449`).
pub const MULTISTEP_EXHAUSTED: &str = "multistep_exhausted";

/// The code of a quiz whose questions are all served (`api.py:244`).
pub const QUIZ_EXHAUSTED: &str = "quiz_exhausted";

/// The code of a teach request that has no worked example to give.
pub const NO_INSTRUCTION: &str = "no_instruction";

/// The code of a hint request inside a quiz (`api.py:1813`).
pub const NO_HINTS_IN_QUIZ: &str = "no_hints_in_quiz";

/// The code of a hint request whose knowledge point has no approved ladder.
///
/// New in 2.0. 1.0 always had a model to ask; L5 replaces it with authored
/// content, and M6 authors the ladders.
pub const NO_HINT_LADDER: &str = "no_hint_ladder";

/// The code of a serve whose knowledge point can produce no problem at all.
pub const POOL_UNAVAILABLE: &str = "pool_unavailable";

/// After this many hints, a review or a multi-step part points the learner back
/// at the reference lesson (`api.py:97`, `:1856-1859`).
pub const STUCK_HINT_THRESHOLD: usize = 3;

// --------------------------------------------------------------------------- //
// The authored documents (D-O3)
// --------------------------------------------------------------------------- //

// The teach page (L4) and the hint ladder (L5) are the core's documents, so the
// M6 authoring gate (`cadus_core::instruction`) and these routes read ONE shape.
// A second declaration here would let a body the gate accepts fail at the route,
// and `deny_unknown_fields` makes that failure a 500 on a page a reviewer
// approved. `TeachDoc` keeps the M5 name of `TeachPage`.
pub use cadus_core::instruction::{HintLadder, TeachPage as TeachDoc, WorkedExample};

/// The Unix instant of `now`, in seconds, as the D-S6 document spells it.
pub(crate) fn unix_seconds(micros: i64) -> f64 {
    micros as f64 / 1_000_000.0
}

/// Read one approved document as the type the route serves, or the `500` of a
/// body that does not read. `what` names the document in the log line.
fn read_document<T: DeserializeOwned>(
    doc: ApprovedDoc,
    what: &'static str,
    message: &'static str,
) -> Result<T, ApiError> {
    serde_json::from_value(doc.body).map_err(|reason| {
        tracing::error!(digest = %doc.digest, error = %reason, "{what}: the authored document did not read");
        ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, INTERNAL_ERROR, message)
    })
}

// --------------------------------------------------------------------------- //
// The shared opening of all three routes
// --------------------------------------------------------------------------- //

/// What every one of the three routes reads before it does its own work.
pub(crate) struct Open {
    /// The tenant-bound transaction. The caller commits it or rolls it back.
    pub(crate) tx: Transaction<'static, Postgres>,
    /// The events of the OPEN SESSION, in `seq` order, `session_start` first.
    ///
    /// It is NOT the whole log (M5 review 1, findings F15 and F18). A task id is
    /// `{session}-{task_type}-{topic}` (`selector::assign_ids`) and an attempt id
    /// is `{task_id}-{n}`, so every event of a task of the open session stands in
    /// this window. The grade route reads it for the attempt count of one task
    /// and for the earlier attempts of one knowledge point, and both are
    /// task-scoped. The serve route reads it for the tasks this session already
    /// served (D-M5-8).
    pub(crate) events: Vec<EventRow>,
    /// The D-S6 document, bound to the open session.
    pub(crate) scratch: WebState,
    /// The plan of the open session.
    pub(crate) plan: SessionPlan,
}

/// Open the transaction, read the state and the session window, and compose the
/// plan.
///
/// `lock` takes the tenant's advisory lock first. A route that WRITES the D-S6
/// row takes it; the read-only teach route does not (section 4.2).
///
/// The whole read is inside ONE transaction, so the plan a route serves from and
/// the state row it validates against cannot disagree.
///
/// # What it reads (F18)
///
/// One `learner_models` row, one ONE-LINE range read of `events` when the fold
/// cursor already stands at the head of the log, one range read of the open
/// session's own events, and one `web_states` row. Nothing here grows with the
/// lifetime event count. The earlier version read the whole log twice and folded
/// it twice, which passed the 150 ms budget of L1, L4 and L5 on its own at about
/// 8,000 attempts.
pub(crate) async fn open(
    state: &AppState,
    content: &Content,
    user_id: Uuid,
    now: Timestamp,
    lock: bool,
) -> Result<Open, ApiError> {
    let input = projection_input(content, now);
    let mut tx = begin(state, user_id).await?;
    if lock {
        store(state, lock_web_state(&mut tx, user_id)).await?;
    }
    let projection = store(state, project_current(&mut tx, user_id, &input)).await?;
    let mut view = projection.view;
    let session = view.current_session.clone().ok_or_else(no_open_session)?;
    // The window of the OPEN SESSION, and the drill cadence repaired with it.
    // The plan route and the dashboard call the SAME helper (V3, V9).
    let events = view_for_open_session(state, &mut tx, user_id, &mut view, &session).await?;
    let mut scratch = read_state(&state.db, &mut tx, user_id).await?;
    scratch.bind(&session);
    // The readiness of D-F5, read in the SAME transaction: the plan these three
    // routes look a task up in is the plan `GET /api/session/plan` listed.
    let readiness = readiness_of(state, content, &mut tx).await?;
    let mut plan = compose_plan(content, &view, &projection.model, &session, now, &readiness);
    restore_quiz_practice(&mut plan, &scratch);
    Ok(Open {
        tx,
        events,
        scratch,
        plan,
    })
}

/// The task of this plan, or `404 unknown_task`.
pub(crate) fn find<'plan>(
    plan: &'plan SessionPlan,
    task_id: &str,
) -> Result<&'plan Task, ApiError> {
    plan.tasks
        .iter()
        .find(|task| task.task_id == task_id)
        .ok_or_else(|| unknown_task(task_id))
}

// --------------------------------------------------------------------------- //
// Shared refusals
// --------------------------------------------------------------------------- //

/// A `409` with `code` and `message`.
fn conflict(code: &'static str, message: impl Into<String>) -> ApiError {
    ApiError::new(StatusCode::CONFLICT, code, message)
}

/// `409 no_open_session`: these three routes all need an open session.
pub(crate) fn no_open_session() -> ApiError {
    conflict(NO_OPEN_SESSION, "No session is open.")
}

/// `404 unknown_task`: the id is not in this session's plan.
fn unknown_task(task_id: &str) -> ApiError {
    ApiError::new(
        StatusCode::NOT_FOUND,
        UNKNOWN_TASK,
        format!("No task {task_id:?} is in this session."),
    )
}

/// `409 no_instruction`: there is no worked example for this task.
fn no_instruction() -> ApiError {
    conflict(
        NO_INSTRUCTION,
        "Only a lesson with an approved teach page has a worked example.",
    )
}

/// `409 no_hint_ladder`: this knowledge point has no approved ladder.
fn no_ladder() -> ApiError {
    conflict(
        NO_HINT_LADDER,
        "This knowledge point has no approved hint ladder.",
    )
}

/// `409 pool_unavailable`: this knowledge point can produce no problem.
fn no_problem(topic_id: &str) -> ApiError {
    conflict(
        POOL_UNAVAILABLE,
        format!("Topic {topic_id:?} has no problem to serve."),
    )
}

/// A completed quiz remains addressable while its post-reveal practice is pending.
fn restore_quiz_practice(plan: &mut SessionPlan, scratch: &WebState) {
    for (id, progress) in &scratch.tasks {
        if progress.task_type == "quiz"
            && scratch.quizzes.contains_key(id)
            && !plan.tasks.iter().any(|task| task.task_id == *id)
        {
            plan.tasks.push(Task {
                task_id: id.clone(),
                task_type: TaskType::Quiz,
                n_problems: Some(progress.total),
                ..Task::default()
            });
        }
    }
}
