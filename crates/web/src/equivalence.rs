//! The request-tier half of Amendment K (steer note 114, the owner's design).
//!
//! The web tier never calls a model (L6, a build guard). It does three local
//! things:
//!
//! 1. **The cache lookup.** On a deterministic wrong or unparseable answer,
//!    the normalized learner text is looked up in `equivalence_cache`. A hit
//!    answers at once: an EQUIVALENT verdict counts as correct for the topic
//!    and for mastery (note 114, point 4), and a NOT verdict leaves the
//!    deterministic wrong with the model's one-line reason beside it.
//! 2. **The enqueue.** A miss with no cached verdict writes one
//!    `equivalence_jobs` row inside the grade transaction (the A4 pattern).
//!    The reply carries `equivalence: {id, status: "pending"}` and the
//!    deterministic verdict stands until the background verdict lands.
//! 3. **The poll route.** `GET /api/equivalence/{id}` answers the standing
//!    verdict of one job, tenant-scoped by the row's policy.
//!
//! The worker does the rest: one claim, one local-model call, the cache
//! write, the settle, and — for an accepted verdict — the `regraded` event
//! that folds the correct answer into the learner's log.
//!
//! # The daily cap
//!
//! The cap counts in the worker ([`DAILY_CAP`]), at claim time: the request
//! tier does no counting queries on its hot path.

use axum::Json;
use axum::extract::{Path as ApiPath, State};
use cadus_store::equivalence::{self, JOB_CAPPED, JOB_DONE, JOB_PENDING, Verdict};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::types::Uuid;

use crate::AppState;
use crate::error::ApiError;
use crate::session::{Tx, begin, store};
use crate::state::{ServedProblem, Tenant};

/// The reply key of the equivalence field.
pub const FIELD: &str = "equivalence";

/// The `status` value of a cache hit that flipped the answer to correct.
pub const STATUS_ACCEPTED: &str = "accepted";

/// The `status` value of a cache hit that kept the answer wrong.
pub const STATUS_REFUSED: &str = "refused";

/// The `status` value of a fresh enqueue (the background check is running).
pub const STATUS_PENDING: &str = "pending";

/// The `status` value of a job the worker could not decide.
pub const STATUS_FAILED: &str = "failed";

/// The one grader note an accepted verdict stamps on the attempt.
pub const ACCEPTED_NOTE: &str = "equivalence (cached background verdict)";

/// The reason field of a refused verdict's reply line.
pub const REFUSED_PREFIX: &str = "A background check of this answer read: ";

/// The normalized cache key of one learner answer (shared with the worker,
/// so the write and the lookup use one key).
pub use cadus_store::equivalence::cache_key;

/// The item digest a served problem is keyed by in the cache.
///
/// The hand-off digest is the stable semantic identity; a problem drawn
/// before hand-offs records one carries none, and the problem-text hash is
/// the fallback the attempt event itself uses.
#[must_use]
pub fn item_digest(served: &ServedProblem) -> String {
    served.handoff.as_ref().map_or_else(
        || cadus_core::learner::problem_text_hash(&served.text),
        |handoff| handoff.item_digest.clone(),
    )
}

/// The cache namespace for equivalence verdicts on structured unit answers.
/// A versioned prefix keeps model interpretations made before unit contracts
/// distinct while leaving general hand-off identity unchanged.
const UNIT_CACHE_NAMESPACE: &str = "cadus/equivalence-cache/unit/v1";

/// The identity used only by the equivalence cache and its worker jobs.
///
/// Unit policy adds meaning that the hand-off digest deliberately omits. Hash
/// a structured JSON tuple so the statement digest, expected key, and complete
/// unit contract cannot collide through ambiguous string concatenation. Other
/// contracts retain the historical text-only cache identity.
#[must_use]
pub fn cache_item_digest(served: &ServedProblem) -> String {
    let base = item_digest(served);
    if !matches!(
        served.expected.answer_contract,
        Some(cadus_core::answer::AnswerContract::Unit { .. })
    ) {
        return base;
    }
    let identity = json!([
        UNIT_CACHE_NAMESPACE,
        base,
        served.expected.answer,
        served.expected.answer_contract,
    ]);
    format!("{:x}", Sha256::digest(identity.to_string().as_bytes()))
}

/// The outcome the cached verdict turns the deterministic grade into.
pub enum Cached {
    /// The model accepted the answer: correct, with the equivalence note.
    Accepted(Verdict),
    /// The model refused it: the deterministic wrong stands, with the reason.
    Refused(Verdict),
}

/// Look one answer up in the cache, inside the caller's transaction.
///
/// # Errors
///
/// Returns [`ApiError`] when the statement fails or the bound expires.
pub async fn lookup(
    state: &AppState,
    tx: &mut Tx,
    served: &ServedProblem,
    answer: &str,
) -> Result<Option<Cached>, ApiError> {
    let hit = store(
        state,
        equivalence::cache_hit(&mut **tx, &cache_item_digest(served), &cache_key(answer)),
    )
    .await?;
    Ok(hit.map(|verdict| {
        if verdict.equivalent {
            Cached::Accepted(verdict)
        } else {
            Cached::Refused(verdict)
        }
    }))
}

/// Put one job on the background queue, inside the grade transaction.
///
/// The insert is idempotent per attempt, so a retried request names one job.
///
/// # Errors
///
/// Returns [`ApiError`] when the statement fails; the caller decides whether
/// a failed enqueue stops the grade (it must not: the deterministic verdict
/// stands without the background check).
pub async fn enqueue(
    state: &AppState,
    tx: &mut Tx,
    user_id: Uuid,
    attempt_id: &str,
    served: &ServedProblem,
    answer: &str,
) -> Result<Option<Uuid>, ApiError> {
    let payload = equivalence::JobPayload {
        v: equivalence::PAYLOAD_VERSION,
        task_id: served.task_id.clone(),
        topic: served.serving_topic().unwrap_or_default().to_owned(),
        item_digest: cache_item_digest(served),
        problem: served.text.clone(),
        expected: served.expected.answer.clone(),
        answer_contract: served
            .expected
            .answer_contract
            .as_ref()
            .map(|contract| serde_json::to_string(contract).unwrap_or_default()),
        given_answer: answer.to_owned(),
    };
    let document =
        serde_json::to_value(&payload).map_err(|_| ApiError::internal("equivalence payload"))?;
    let write = equivalence::enqueue(tx, user_id, attempt_id, &document);
    match store(state, write).await {
        Ok(id) => Ok(Some(id)),
        // A failed enqueue never stops the grade: the deterministic verdict
        // stands, and the next attempt of the same answer re-enqueues.
        Err(_) => {
            tracing::warn!(attempt_id, "equivalence: the enqueue failed");
            Ok(None)
        }
    }
}

/// The reply field of one submission: the cache verdict, or the pending job.
#[must_use]
pub fn reply_field(hit: Option<&Cached>, job: Option<Uuid>) -> Value {
    match (hit, job) {
        (Some(Cached::Accepted(verdict)), _) => json!({
            "status": STATUS_ACCEPTED,
            "reason": verdict.reason,
            "model": verdict.model,
        }),
        (Some(Cached::Refused(verdict)), _) => json!({
            "status": STATUS_REFUSED,
            "reason": verdict.reason,
            "model": verdict.model,
        }),
        (None, Some(id)) => json!({ "id": id.to_string(), "status": STATUS_PENDING }),
        (None, None) => Value::Null,
    }
}

/// Read one job row into the poll reply (the tenant policy scopes the read).
#[must_use]
pub fn poll_view(row: &equivalence::JobRow) -> Value {
    let status = match row.status.as_str() {
        JOB_DONE => {
            let verdict: Option<Verdict> = row
                .result
                .as_ref()
                .and_then(|doc| serde_json::from_value(doc.clone()).ok());
            match verdict {
                Some(verdict) if verdict.equivalent => json!({
                    "status": STATUS_ACCEPTED,
                    "reason": verdict.reason,
                    "model": verdict.model,
                }),
                Some(verdict) => json!({
                    "status": STATUS_REFUSED,
                    "reason": verdict.reason,
                    "model": verdict.model,
                }),
                None => json!({ "status": STATUS_FAILED }),
            }
        }
        JOB_PENDING => json!({ "status": STATUS_PENDING }),
        "running" => json!({ "status": STATUS_PENDING }),
        equivalence::JOB_FAILED => json!({ "status": STATUS_FAILED }),
        JOB_CAPPED => json!({ "status": STATUS_FAILED }),
        other => json!({ "status": other }),
    };
    json!({
        "id": row.id,
        "attempt_id": row.attempt_id,
        "verdict": status,
    })
}

/// `GET /api/equivalence/{id}`: the standing verdict of one job.
///
/// The read runs inside `begin_tenant`, and the statement names no `user_id`:
/// the `tenant_isolation` policy scopes it, so another tenant's id answers
/// `404`.
pub(crate) async fn poll(
    State(state): State<AppState>,
    Tenant(user_id): Tenant,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let mut tx = begin(&state, user_id).await?;
    let row = store(&state, equivalence::job(&mut *tx, id)).await?;
    drop(tx);
    let Some(row) = row else {
        return Err(ApiError::not_found());
    };
    Ok(Json(poll_view(&row)))
}

/// The router of the poll route.
pub(crate) fn router() -> axum::Router<AppState> {
    axum::Router::new().route("/api/equivalence/{id}", axum::routing::get(poll))
}
