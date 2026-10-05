//! The learner routes of the proof revision loop (D-PR1).
//!
//! | Method and path | What it does |
//! |---|---|
//! | `POST /api/proof-grading/{id}/seen` | stamps `seen_at`; at the cap, shows the solution ONCE and stamps `revealed_at` |
//! | `POST /api/proof-grading/{id}/dispute` | "this grade is wrong": lists the row in `GET /api/admin/ungraded` |
//! | `GET /api/proofs` | every revision chain, newest last (`?topic=` narrows it) |
//! | `POST /api/proofs/{id}/revise` | the next draft of a non-lesson chain |
//!
//! Every statement runs inside `begin_tenant`; the `tenant_isolation` policy
//! of `proof_grading_jobs` scopes every read and write, so another tenant's id
//! answers `404`.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use cadus_store::proof_grading::{self, CONTEXT_LESSON, Mark, NewJob};
use cadus_store::state::lock_web_state;
use serde_json::{Value, json};
use sqlx::types::Uuid;

use super::chain::{self, Chain, Phase};
use super::{LIST_LIMIT, STATUS_PENDING, poll_view, reply_field};
use crate::AppState;
use crate::error::ApiError;
use crate::path::ApiPath;
use crate::session::{Tx, begin, db_failed, store};
use crate::state::Tenant;

/// The longest dispute note the route keeps, in characters.
pub const MAX_NOTE_CHARS: usize = 2_000;

/// The code of a write against a row whose grading still runs.
pub const PROOF_GRADING_PENDING: &str = super::lesson::PROOF_GRADING_PENDING;

/// The code of a revision of a chain that takes none now.
pub const PROOF_NOT_REVISABLE: &str = "proof_not_revisable";

/// The code of a revision of a lesson chain outside its lesson.
pub const PROOF_IN_LESSON: &str = "proof_in_lesson";

/// A `409` with `code`.
fn conflict(code: &'static str, message: &'static str) -> ApiError {
    ApiError::new(StatusCode::CONFLICT, code, message)
}

/// The chain that holds `id`, read in `tx`, or `404`.
async fn chain_holding(state: &AppState, tx: &mut Tx, id: Uuid) -> Result<Chain, ApiError> {
    let rows = store(state, proof_grading::jobs(&mut **tx, LIST_LIMIT)).await?;
    chain::chain_of(rows, id).ok_or_else(ApiError::not_found)
}

/// The row `id` of `chain`.
fn row_in(chain: &Chain, id: Uuid) -> Result<&proof_grading::JobRow, ApiError> {
    chain
        .rows
        .iter()
        .find(|row| row.id == id)
        .ok_or_else(ApiError::not_found)
}

/// `POST /api/proof-grading/{id}/seen`: the learner saw the verdict.
///
/// The answer is the poll view of the row and the view of its chain. At the
/// revision cap the chain's head shows its reference solution in THIS answer
/// and in no later one: the stamp of `revealed_at` moves the chain to the
/// unaided rewrite.
pub(crate) async fn seen(
    State(state): State<AppState>,
    Tenant(user_id): Tenant,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let mut tx = begin(&state, user_id).await?;
    let chain = chain_holding(&state, &mut tx, id).await?;
    let row = row_in(&chain, id)?;
    if chain::status_of(row) != STATUS_PENDING {
        store(&state, proof_grading::mark(&mut *tx, id, Mark::Seen)).await?;
    }
    let reveal =
        row.id == chain.head().id && chain::loops(&row.context) && chain.phase() == Phase::Reveal;
    if reveal {
        store(&state, proof_grading::mark(&mut *tx, id, Mark::Revealed)).await?;
    }
    // Read the chain again, so the view names the phase the marks moved to.
    let chain = chain_holding(&state, &mut tx, id).await?;
    tx.commit().await.map_err(db_failed)?;
    let row = row_in(&chain, id)?;
    let solution = if reveal || chain.solution_open() {
        chain.reference()
    } else {
        None
    };
    Ok(Json(json!({
        "job": poll_view(row, Some(&chain)),
        "chain": chain::chain_view(&chain, solution),
    })))
}

/// `POST /api/proof-grading/{id}/dispute`: "this grade is wrong".
///
/// The row joins the human recovery list of `GET /api/admin/ungraded`, where
/// `POST /api/admin/ungraded/{attempt_id}/regrade` gives it a human verdict
/// that replaces the model's. A second dispute of the same row changes
/// nothing.
///
/// # Errors
///
/// - `409 proof_grading_pending` — the row has no verdict to dispute yet;
/// - `422 invalid_request` — the note is not a string of at most
///   [`MAX_NOTE_CHARS`] characters.
pub(crate) async fn dispute(
    State(state): State<AppState>,
    Tenant(user_id): Tenant,
    ApiPath(id): ApiPath<Uuid>,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, ApiError> {
    let note = match body.as_ref().and_then(|Json(value)| value.get("note")) {
        None | Some(Value::Null) => None,
        Some(Value::String(text)) if text.chars().count() <= MAX_NOTE_CHARS => {
            Some(text.trim().to_owned()).filter(|text| !text.is_empty())
        }
        Some(_) => {
            return Err(ApiError::invalid_request(format!(
                "The note must be text of at most {MAX_NOTE_CHARS} characters."
            )));
        }
    };
    let mut tx = begin(&state, user_id).await?;
    let chain = chain_holding(&state, &mut tx, id).await?;
    if chain::status_of(row_in(&chain, id)?) == STATUS_PENDING {
        return Err(conflict(
            PROOF_GRADING_PENDING,
            "This proof is still being checked.",
        ));
    }
    store(
        &state,
        proof_grading::dispute(&mut *tx, id, note.as_deref()),
    )
    .await?;
    let chain = chain_holding(&state, &mut tx, id).await?;
    tx.commit().await.map_err(db_failed)?;
    Ok(Json(poll_view(row_in(&chain, id)?, Some(&chain))))
}

/// The `?topic=` query of `GET /api/proofs`.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ProofsQuery {
    /// One topic id, or absent for every topic.
    pub topic: Option<String>,
}

/// `GET /api/proofs`: every revision chain of the learner, oldest first.
///
/// Each chain lists its versions with their verdicts and feedback. The
/// reference solution rides along only once the chain allows it.
pub(crate) async fn list(
    State(state): State<AppState>,
    Tenant(user_id): Tenant,
    Query(query): Query<ProofsQuery>,
) -> Result<Json<Value>, ApiError> {
    let mut tx = begin(&state, user_id).await?;
    let rows = store(&state, proof_grading::jobs(&mut *tx, LIST_LIMIT)).await?;
    drop(tx);
    let graph = state.content.as_deref().map(|content| &content.curriculum);
    let chains: Vec<Value> = chain::chains(rows)
        .iter()
        .filter(|chain| {
            query
                .topic
                .as_deref()
                .is_none_or(|topic| chain.root().payload_str("topic") == Some(topic))
        })
        .map(|chain| {
            let solution = chain.solution_open().then(|| chain.reference()).flatten();
            let mut view = chain::chain_view(chain, solution);
            let name = graph
                .zip(chain.root().payload_str("topic"))
                .and_then(|(graph, topic)| graph.idx_of(topic).and_then(|idx| graph.topic(idx)))
                .map(|topic| topic.name.clone());
            view["topic_name"] = json!(name);
            view
        })
        .collect();
    Ok(Json(json!({ "chains": chains })))
}

/// `POST /api/proofs/{id}/revise`: the next draft of a non-lesson chain.
///
/// `id` is the chain's HEAD. The draft is a new job linked to it
/// (`revision_of`) and graded again; it records no attempt, because the task
/// it came from is closed. The unaided rewrite after the cap closes the
/// chain. A lesson chain is revised inside its lesson.
///
/// # Errors
///
/// - `404 not_found` — no chain holds `id`, or `id` is not its head;
/// - `409 proof_in_lesson` — a lesson chain;
/// - `409 proof_grading_pending`, `409 proof_solution_unseen`,
///   `409 proof_not_revisable` — the chain takes no draft now;
/// - `422 invalid_request` — the answer is blank or too long.
pub(crate) async fn revise(
    State(state): State<AppState>,
    Tenant(user_id): Tenant,
    ApiPath(id): ApiPath<Uuid>,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, ApiError> {
    let answer = body
        .as_ref()
        .and_then(|Json(value)| value.get("answer"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty() && text.chars().count() <= crate::grade::MAX_ANSWER_CHARS)
        .ok_or_else(|| {
            ApiError::invalid_request("The body needs the revised proof as a non-empty `answer`.")
        })?
        .to_owned();
    let mut tx = begin(&state, user_id).await?;
    store(&state, lock_web_state(&mut tx, user_id)).await?;
    let chain = chain_holding(&state, &mut tx, id).await?;
    let head = chain.head();
    if head.id != id {
        return Err(ApiError::not_found());
    }
    if head.context == CONTEXT_LESSON {
        return Err(conflict(
            PROOF_IN_LESSON,
            "Revise this proof in its lesson.",
        ));
    }
    if !chain::loops(&head.context) {
        return Err(conflict(
            PROOF_NOT_REVISABLE,
            "This proof takes no revision.",
        ));
    }
    let (revision, rewrite) = match chain.phase() {
        Phase::Revise => (head.revision.saturating_add(1), false),
        Phase::Unavailable => (head.revision, false),
        Phase::Rewrite => (head.revision, true),
        Phase::Grading => {
            return Err(conflict(
                PROOF_GRADING_PENDING,
                "Your last draft is still being checked.",
            ));
        }
        Phase::Reveal => {
            return Err(conflict(
                super::lesson::PROOF_SOLUTION_UNSEEN,
                "Read the solution once before you rewrite the proof.",
            ));
        }
        Phase::Passed | Phase::Closed => {
            return Err(conflict(
                PROOF_NOT_REVISABLE,
                "This proof takes no revision.",
            ));
        }
    };
    let Some(mut payload) = chain.root().job_payload() else {
        return Err(ApiError::internal("proof revision payload"));
    };
    payload.given_answer = answer;
    let attempt_id = format!("{}-r{}", chain.root().attempt_id, chain.rows.len());
    let job = NewJob {
        context: head.context.as_str(),
        revision_of: Some(head.id),
        revision,
        rewrite,
    };
    let document =
        serde_json::to_value(&payload).map_err(|_| ApiError::internal("proof grading payload"))?;
    let new_id = store(
        &state,
        proof_grading::enqueue(&mut tx, user_id, &attempt_id, &document, &job),
    )
    .await?;
    if rewrite {
        store(&state, proof_grading::mark(&mut *tx, new_id, Mark::Closed)).await?;
    }
    let chain = chain_holding(&state, &mut tx, new_id).await?;
    tx.commit().await.map_err(db_failed)?;
    let solution = chain.solution_open().then(|| chain.reference()).flatten();
    Ok(Json(json!({
        "proof_grading": reply_field(Some(new_id)),
        "chain": chain::chain_view(&chain, solution),
    })))
}
