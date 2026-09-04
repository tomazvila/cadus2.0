//! The placement diagnostic: `POST /api/diag/start`, `/api/diag/answer` and
//! `/api/diag/finish` (`api.py:1896-2091`, spec section 2).
//!
//! The three rows sat in the spec table marked `keep` while no unit owned them:
//! M5 built no route, and the M6 placement screen shipped against
//! `SPEC_ROUTES_ABSENT`. This module closes that gap. The pedagogy is
//! `cadus_core::diagnostic`; the work here is the transport, the scratch and the
//! two events.
//!
//! THE VERDICT IS DETERMINISTIC AND THIS ROUTE FAMILY CALLS NO MODEL (R4, L6,
//! spec section 2 "deterministic-only verdict"). `start` therefore drops from the
//! probe set every topic whose `answer_kind` has no deterministic checker, so the
//! diagnostic never asks a question it cannot mark. 1.0 sent such a probe to its
//! grading engine; this service asks no model on a request path, so the question
//! is not asked at all.
//!
//! ONE TRANSACTION PER CALL. 1.0 splits `answer` into three phases, because it
//! held the tenant lock across a model round trip. The verdict here is a local
//! `check`, so the scratch save, the fold and the appended event stay in one
//! transaction. A crash then leaves neither a folded balance with no event nor an
//! event with no balance (1.0 finding F-10-1).
//!
//! THE SENTINEL TASK ID. The diagnostic is a single-track task: the served probe
//! lives at `served["diag"]` of the D-S6 document, so a new probe replaces the
//! one before it and a stale `problem_id` answers `404 unknown_problem`.

use axum::http::StatusCode;
use cadus_core::config::Config;
use cadus_core::curriculum::{AnswerKind, Curriculum, Topic};
use cadus_core::diagnostic::{self, DiagState};
use cadus_core::event::Timestamp;
use cadus_core::pool::PoolAnswer;
use cadus_store::state::{load_diag_state, save_diag_state};
use serde_json::{Value, json};
use sqlx::types::Uuid;

use crate::AppState;
use crate::error::ApiError;
use crate::serve::unix_seconds;
use crate::session::{Tx, store, write_state};
use crate::state::{Content, ServedProblem, WebState};

mod routes;

pub use routes::{answer, finish, start};

/// The code of a diagnostic call made with no diagnostic open.
pub const NO_DIAGNOSTIC: &str = "no_diagnostic";

/// The code of a start with no course to diagnose.
pub const NO_COURSE: &str = "no_course";

/// The task id the served probe hangs on.
const DIAG_TASK_ID: &str = "diag";

/// The text a topic with no exemplar shows. It matches 1.0 character for
/// character, because the placement screen renders whatever it is given.
const NO_EXEMPLAR: &str = "(no exemplar)";

/// Whether this answer kind has a deterministic checker.
const fn deterministic(kind: AnswerKind) -> bool {
    matches!(kind, AnswerKind::Numeric | AnswerKind::Expression)
}

/// The topic record of `topic`, or `None` when the arena does not hold it.
fn topic_record<'a>(graph: &'a Curriculum, topic: &str) -> Option<&'a Topic> {
    graph.idx_of(topic).and_then(|idx| graph.topic(idx))
}

/// The answer kind of one topic, or `None` when the arena does not hold it.
fn topic_kind(graph: &Curriculum, topic: &str) -> Option<AnswerKind> {
    topic_record(graph, topic).map(|record| record.answer_kind)
}

/// The `409 no_diagnostic` of a call with no diagnostic in progress.
fn no_diagnostic() -> ApiError {
    ApiError::new(
        StatusCode::CONFLICT,
        NO_DIAGNOSTIC,
        "No diagnostic is in progress; POST /api/diag/start.",
    )
}

/// The live problem of one probe of `topic`, dealt at `started_at`.
fn probe_problem(
    record: &Topic,
    topic: &str,
    index: usize,
    started_at: f64,
) -> (ServedProblem, Value) {
    let exemplar = record.diagnostic_exemplar.as_ref();
    let problem_id = Uuid::new_v4().simple().to_string();
    let text = exemplar.map_or_else(|| NO_EXEMPLAR.to_owned(), |item| item.problem.clone());
    let served = ServedProblem {
        problem_id: problem_id.clone(),
        task_id: DIAG_TASK_ID.to_owned(),
        topic: Some(topic.to_owned()),
        serve_topic: Some(topic.to_owned()),
        kp: None,
        answer_kind: Some(record.answer_kind.as_str().to_owned()),
        text: text.clone(),
        expected: PoolAnswer {
            v: cadus_core::pool::POOL_ROW_VERSION,
            answer: exemplar.map(|item| item.answer.clone()).unwrap_or_default(),
        },
        solution_sketch: exemplar.and_then(|item| item.solution_sketch.clone()),
        started_at,
        hints_given: Vec::new(),
        index: index as i64,
        rework: None,
    };
    (
        served,
        json!({ "topic": topic, "problem_id": problem_id, "text": text }),
    )
}

/// Serve the next probe, storing it as the live problem.
///
/// The return value is the `probe` object of the spec, or `None` when the probe
/// list is exhausted. The caller answers `{"probe": null}` or `{"done": true}`
/// from that, which the screen reads. `next_probe` deals a topic of the arena
/// only, so the record lookup finds it.
fn serve_probe(
    diag: &DiagState,
    graph: &Curriculum,
    cfg: &Config,
    scratch: &mut WebState,
    started_at: f64,
) -> Option<Value> {
    let topic = diagnostic::next_probe(diag, graph, cfg)?;
    topic_record(graph, &topic).map(|record| {
        let (served, probe) = probe_problem(record, &topic, diag.answered.len(), started_at);
        scratch.served.insert(DIAG_TASK_ID.to_owned(), served);
        probe
    })
}

/// Drop the live probe and deal the next one at `now`.
fn deal_probe(
    diag: &DiagState,
    content: &Content,
    scratch: &mut WebState,
    now: Timestamp,
) -> Option<Value> {
    scratch.served.remove(DIAG_TASK_ID);
    let started_at = unix_seconds(now.micros());
    serve_probe(diag, &content.curriculum, &content.cfg, scratch, started_at)
}

/// Read the open diagnostic, or answer `409 no_diagnostic`.
fn open_diagnostic(doc: Option<Value>) -> Result<DiagState, ApiError> {
    let Some(doc) = doc else {
        return Err(no_diagnostic());
    };
    serde_json::from_value(doc).map_err(unreadable_diagnostic)
}

/// The `409 no_diagnostic` of a diagnostic document that did not read.
fn unreadable_diagnostic(err: serde_json::Error) -> ApiError {
    tracing::error!(error = %err, "cadus-web: the diagnostic document did not read");
    no_diagnostic()
}

/// Load the open diagnostic of this tenant, inside the caller's transaction.
async fn load_diagnostic(
    state: &AppState,
    tx: &mut Tx,
    user_id: Uuid,
) -> Result<DiagState, ApiError> {
    let doc = store(state, load_diag_state(tx, user_id)).await?;
    open_diagnostic(doc)
}

/// Write the diagnostic document and the D-S6 document that holds its probe.
/// Every field of the diagnostic is a plain value, so its JSON always writes.
async fn save_diagnostic(
    state: &AppState,
    tx: &mut Tx,
    user_id: Uuid,
    diag: &DiagState,
    scratch: &WebState,
) -> Result<(), ApiError> {
    store(state, save_diag_state(tx, user_id, &json!(diag))).await?;
    write_state(&state.db, tx, user_id, scratch).await
}
