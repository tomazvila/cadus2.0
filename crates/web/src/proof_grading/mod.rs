//! The request-tier half of Amendment K point 6 and of the proof revision
//! loop (D-PR1): background grading of a written proof or a free
//! explanation, and the revise-and-resubmit chain around it.
//!
//! The web tier never calls a model (L6). It does these local things:
//!
//! 1. **The enqueue.** An UNGRADED attempt on an item with no checkable key
//!    (answer kind `proof` with no contract, or the contract `none`) writes one
//!    `proof_grading_jobs` row inside the grade transaction. The reply carries
//!    `proof_grading: {id, status: "pending"}`. The row names its context
//!    (lesson, review, quiz, self-check) and, for a resubmission, the row it
//!    revises.
//! 2. **The poll route.** `GET /api/proof-grading/{id}` answers the standing
//!    result of one job, tenant-scoped by the row's policy: the verdict, the
//!    feedback, the per-check list, the chain the row belongs to, and the
//!    reference solution once the chain allows it ([`chain`]).
//! 3. **The lesson loop** ([`lesson`]). A written proof inside a lesson closes
//!    its knowledge point on a PASS, not on the submission.
//! 4. **The learner routes** ([`routes`]): the verdict seen, the dispute, the
//!    proofs list, and the revision of a non-lesson chain.
//!
//! A quiz answer never enqueues before the reveal: the quiz receipt returns
//! before the enqueue step, as it does for the equivalence check, so nothing is
//! revealed while a quiz is open. The placement test grades on its own route and
//! never reaches this module.

pub(crate) mod chain;
pub(crate) mod lesson;
mod routes;

use axum::Json;
use axum::extract::State;
use cadus_core::answer::AnswerContract;
use cadus_core::curriculum::AnswerKind;
use cadus_store::proof_grading::{
    self, CONTEXT_QUIZ, CONTEXT_REVIEW, CONTEXT_SELFCHECK, JobPayload, NewJob,
};
use serde_json::{Value, json};
use sqlx::types::Uuid;

use crate::AppState;
use crate::error::ApiError;
use crate::path::ApiPath;
use crate::session::{Tx, begin, store};
use crate::state::{ServedProblem, Tenant};

pub use chain::{Phase, REVISION_CAP};

/// The reply key of the proof-grading field.
pub const FIELD: &str = "proof_grading";

/// The poll `status` of a job the worker has not finished.
pub const STATUS_PENDING: &str = "pending";

/// The poll `status` of a passed proof.
pub const STATUS_PASS: &str = "pass";

/// The poll `status` of a proof that needs revision.
pub const STATUS_NEEDS_REVISION: &str = "needs_revision";

/// The poll `status` of a job the worker could not grade.
pub const STATUS_FAILED: &str = "failed";

/// The poll `status` of a job the daily cap refused.
pub const STATUS_CAPPED: &str = "capped";

/// The most rows one proofs-list read groups into chains.
pub const LIST_LIMIT: i64 = 500;

/// The statement openings of a written PROOF, as the curriculum spells them.
/// Every other no-key item is a free explanation (a self-check).
const PROOF_OPENINGS: [&str; 4] = [
    "Write the full proof",
    "Write a proof",
    "Prove",
    "Show that",
];

/// Whether the served item is a written proof or explanation: an item the
/// deterministic checker cannot decide because it carries no checkable key.
#[must_use]
pub fn is_written_proof(served: &ServedProblem, kind: AnswerKind) -> bool {
    match served.expected.answer_contract {
        Some(AnswerContract::None) => true,
        None => kind == AnswerKind::Proof,
        Some(_) => false,
    }
}

/// Whether a statement asks for a written proof.
#[must_use]
pub fn is_proof_text(text: &str) -> bool {
    let text = text.trim_start();
    PROOF_OPENINGS
        .iter()
        .any(|opening| text.starts_with(opening))
}

/// Whether the served item is a written PROOF: no checkable key, and a
/// statement that asks for a proof. A free explanation is not one.
#[must_use]
pub fn is_proof_item(served: &ServedProblem) -> bool {
    let no_key = match served.expected.answer_contract {
        Some(AnswerContract::None) => true,
        None => served.answer_kind.as_deref() == Some("proof"),
        Some(_) => false,
    };
    no_key && is_proof_text(&served.text)
}

/// The context of a non-lesson, non-quiz written item.
#[must_use]
pub fn context_of(served: &ServedProblem) -> &'static str {
    if is_proof_item(served) {
        CONTEXT_REVIEW
    } else {
        CONTEXT_SELFCHECK
    }
}

/// The stored key, when it says more than a pointer to the solution.
fn meaningful_key(answer: &str) -> Option<String> {
    let trimmed = answer.trim();
    let placeholder = trimmed.is_empty() || trimmed.to_lowercase().starts_with("see the solution");
    (!placeholder).then(|| trimmed.to_owned())
}

/// The job payload of one served item and one learner text.
#[must_use]
pub fn payload(served: &ServedProblem, answer: &str) -> JobPayload {
    JobPayload {
        v: proof_grading::PAYLOAD_VERSION,
        task_id: served.task_id.clone(),
        topic: served.serving_topic().unwrap_or_default().to_owned(),
        item_digest: crate::equivalence::item_digest(served),
        problem: served.text.clone(),
        reference: served.solution_sketch.clone(),
        expected: meaningful_key(&served.expected.answer),
        // No curriculum item carries an authored rubric yet. When one does,
        // it goes here and replaces the derived checks.
        rubric: Vec::new(),
        given_answer: answer.to_owned(),
        kp: served.kp.clone(),
    }
}

/// The key of a quiz-buffer answer that holds a written proof's grading
/// payload until the quiz is revealed. The reveal reply never carries it.
pub const BUFFER_PAYLOAD: &str = "proof_payload";

/// Put one job on the queue, inside the grade transaction.
///
/// A failed enqueue never stops the grade: the attempt stays ungraded, which
/// it would be without the background grader too.
///
/// # Errors
///
/// Returns [`ApiError`] only when the payload does not serialize.
pub async fn enqueue(
    state: &AppState,
    tx: &mut Tx,
    user_id: Uuid,
    attempt_id: &str,
    served: &ServedProblem,
    answer: &str,
    job: &NewJob<'_>,
) -> Result<Option<Uuid>, ApiError> {
    enqueue_payload(
        state,
        tx,
        user_id,
        attempt_id,
        &payload(served, answer),
        job,
    )
    .await
}

/// Put one job with a ready payload on the queue, inside the caller's
/// transaction. The quiz reveal enqueues the payloads its buffer kept.
///
/// # Errors
///
/// Returns [`ApiError`] only when the payload does not serialize.
pub async fn enqueue_payload(
    state: &AppState,
    tx: &mut Tx,
    user_id: Uuid,
    attempt_id: &str,
    payload: &JobPayload,
    job: &NewJob<'_>,
) -> Result<Option<Uuid>, ApiError> {
    let document =
        serde_json::to_value(payload).map_err(|_| ApiError::internal("proof grading payload"))?;
    let write = proof_grading::enqueue(tx, user_id, attempt_id, &document, job);
    match store(state, write).await {
        Ok(id) => Ok(Some(id)),
        Err(_) => {
            tracing::warn!(attempt_id, "proof grading: the enqueue failed");
            Ok(None)
        }
    }
}

/// The quiz context of the reveal's enqueue.
pub const QUIZ_JOB: NewJob<'static> = NewJob::first(CONTEXT_QUIZ);

/// The reply field of one submission: the pending job, or `null`.
#[must_use]
pub fn reply_field(job: Option<Uuid>) -> Value {
    job.map_or(
        Value::Null,
        |id| json!({ "id": id.to_string(), "status": STATUS_PENDING }),
    )
}

/// Read one job row into the poll reply.
///
/// `chain` is the chain that holds the row. The reference solution rides
/// along only when the chain allows it ([`chain::Chain::solution_open`]).
#[must_use]
pub fn poll_view(row: &proof_grading::JobRow, chain: Option<&chain::Chain>) -> Value {
    let mut view = json!({
        "id": row.id,
        "attempt_id": row.attempt_id,
        "status": chain::status_of(row),
        "context": row.context,
        "revision": row.revision,
        "rewrite": row.rewrite,
        "disputed": row.disputed_at.is_some(),
    });
    if let Some(grading) = row.grading() {
        view["feedback"] = json!(grading.feedback);
        view["checks"] = json!(
            grading
                .checks
                .iter()
                .map(|check| json!({
                    "id": check.id,
                    "text": check.text,
                    "met": check.met,
                    "minor": check.minor,
                    "evidence": check.evidence,
                    "quote_verified": check.quote_verified,
                }))
                .collect::<Vec<_>>()
        );
        view["model"] = json!(grading.model);
        view["first_unmet"] = chain::first_unmet(&grading.checks).map_or(Value::Null, |check| {
            json!({"id": check.id, "text": check.text, "evidence": check.evidence,
                   "quote_verified": check.quote_verified})
        });
    }
    if let Some(chain) = chain {
        let head = chain.head();
        view["chain"] = json!({
            "root_id": chain.root().id,
            "head_id": head.id,
            "phase": chain.phase().as_str(),
            "revision": head.revision,
            "cap": REVISION_CAP,
            "revisions_left": (REVISION_CAP - head.revision).max(0),
        });
        if chain.solution_open()
            && let Some(solution) = chain.reference()
        {
            view["solution"] = json!(solution);
        }
    }
    view
}

/// `GET /api/proof-grading/{id}`: the standing result of one job.
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
    let row = store(&state, proof_grading::job(&mut *tx, id)).await?;
    let Some(row) = row else {
        return Err(ApiError::not_found());
    };
    let rows = store(&state, proof_grading::chain_rows(&mut *tx, id)).await?;
    drop(tx);
    let chain = rows.map(|rows| chain::Chain { rows });
    Ok(Json(poll_view(&row, chain.as_ref())))
}

/// The router of the proof routes. `POST /api/task/{task_id}/proof/continue`
/// sits with the task routes ([`crate::grade::proof_continue`]).
pub(crate) fn router() -> axum::Router<AppState> {
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/api/proof-grading/{id}", get(poll))
        .route("/api/proof-grading/{id}/seen", post(routes::seen))
        .route("/api/proof-grading/{id}/dispute", post(routes::dispute))
        .route("/api/proofs", get(routes::list))
        .route("/api/proofs/{id}/revise", post(routes::revise))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::{is_proof_text, meaningful_key, poll_view};
    use crate::proof_grading::chain::Chain;
    use cadus_store::proof_grading::JobRow;
    use serde_json::json;
    use sqlx::types::Uuid;
    use sqlx::types::chrono::Utc;

    fn row(status: &str, result: Option<serde_json::Value>, context: &str) -> JobRow {
        JobRow {
            id: Uuid::nil(),
            attempt_id: "a-1".to_owned(),
            status: status.to_owned(),
            payload: json!({"reference": "The worked proof."}),
            result,
            created_at: Utc::now(),
            revision_of: None,
            revision: 0,
            context: context.to_owned(),
            rewrite: false,
            seen_at: None,
            revealed_at: None,
            closed_at: None,
            disputed_at: None,
            dispute_note: None,
            override_verdict: None,
        }
    }

    fn poll(row: &JobRow) -> serde_json::Value {
        let chain = Chain {
            rows: vec![row.clone()],
        };
        poll_view(row, Some(&chain))
    }

    fn needs_revision() -> serde_json::Value {
        json!({
            "v": 1, "verdict": "needs_revision", "model": "m",
            "feedback": "Step 2 assumes the conclusion.",
            "checks": [{"id": "G3", "text": "Not circular.", "minor": false, "met": false, "evidence": "not found"}]
        })
    }

    /// The placeholder key never reaches the prompt; a real key does.
    #[test]
    fn the_placeholder_key_is_dropped() {
        assert_eq!(meaningful_key("See the solution."), None);
        assert_eq!(meaningful_key("  "), None);
        assert_eq!(meaningful_key("e = 4"), Some("e = 4".to_owned()));
    }

    /// The proof statements of the curriculum read as proofs; an explanation
    /// does not.
    #[test]
    fn a_proof_statement_reads_as_a_proof() {
        assert!(is_proof_text(
            "Write the full proof: for all integers m and n, ..."
        ));
        assert!(is_proof_text(
            "Prove that every group of order 4 is abelian."
        ));
        assert!(!is_proof_text("Explain why the limit does not exist."));
    }

    /// A running job polls pending and reveals nothing.
    #[test]
    fn a_running_job_polls_pending() {
        let view = poll(&row("running", None, "lesson"));
        assert_eq!(view["status"], json!("pending"));
        assert!(view.get("solution").is_none());
        assert!(view.get("checks").is_none());
    }

    /// A lesson proof that needs revision polls its checks and the first
    /// unmet one, and HIDES the solution.
    #[test]
    fn a_needs_revision_lesson_proof_hides_the_solution() {
        let view = poll(&row("done", Some(needs_revision()), "lesson"));
        assert_eq!(view["status"], json!("needs_revision"));
        assert_eq!(view["feedback"], json!("Step 2 assumes the conclusion."));
        assert_eq!(view["checks"][0]["met"], json!(false));
        assert_eq!(view["first_unmet"]["id"], json!("G3"));
        assert_eq!(view["chain"]["phase"], json!("revise"));
        assert!(view.get("solution").is_none(), "{view}");
    }

    /// A quiz proof keeps the rule it had: the solution follows the grading.
    #[test]
    fn a_graded_quiz_proof_polls_the_solution() {
        let view = poll(&row("done", Some(needs_revision()), "quiz"));
        assert_eq!(view["solution"], json!("The worked proof."));
    }

    /// A capped or failed job polls its own status.
    #[test]
    fn a_capped_or_failed_job_polls_its_status() {
        assert_eq!(
            poll(&row("capped", None, "lesson"))["status"],
            json!("capped")
        );
        assert_eq!(
            poll(&row("failed", Some(json!({"error": "x"})), "lesson"))["status"],
            json!("failed")
        );
    }
}
