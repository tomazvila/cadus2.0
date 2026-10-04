//! The request-tier half of Amendment K point 6: background grading of a
//! written proof or a free explanation.
//!
//! The web tier never calls a model (L6). It does two local things:
//!
//! 1. **The enqueue.** An UNGRADED attempt on an item with no checkable key
//!    (answer kind `proof` with no contract, or the contract `none`) writes one
//!    `proof_grading_jobs` row inside the grade transaction. The reply carries
//!    `proof_grading: {id, status: "pending"}`; the attempt stays ungraded
//!    until the worker lands a pass.
//! 2. **The poll route.** `GET /api/proof-grading/{id}` answers the standing
//!    result of one job, tenant-scoped by the row's policy: the verdict, the
//!    feedback, the per-check list, and (once graded) the reference solution
//!    the learner compares against.
//!
//! A quiz answer never enqueues: the quiz receipt returns before the enqueue
//! step, as it does for the equivalence check, so nothing is revealed while
//! a quiz is open. The placement test grades on its own route and never
//! reaches this module.

use axum::Json;
use axum::extract::{Path as ApiPath, State};
use cadus_core::answer::AnswerContract;
use cadus_core::curriculum::AnswerKind;
use cadus_store::proof_grading::{
    self, Grading, JOB_CAPPED, JOB_DONE, JOB_FAILED, JOB_PENDING, JOB_RUNNING, JobPayload,
};
use serde_json::{Value, json};
use sqlx::types::Uuid;

use crate::AppState;
use crate::error::ApiError;
use crate::session::{Tx, begin, store};
use crate::state::{ServedProblem, Tenant};

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
    }
}

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
) -> Result<Option<Uuid>, ApiError> {
    let document = serde_json::to_value(payload(served, answer))
        .map_err(|_| ApiError::internal("proof grading payload"))?;
    let write = proof_grading::enqueue(tx, user_id, attempt_id, &document);
    match store(state, write).await {
        Ok(id) => Ok(Some(id)),
        Err(_) => {
            tracing::warn!(attempt_id, "proof grading: the enqueue failed");
            Ok(None)
        }
    }
}

/// The reply field of one submission: the pending job, or `null`.
#[must_use]
pub fn reply_field(job: Option<Uuid>) -> Value {
    job.map_or(
        Value::Null,
        |id| json!({ "id": id.to_string(), "status": STATUS_PENDING }),
    )
}

/// Read one job row into the poll reply.
#[must_use]
pub fn poll_view(row: &proof_grading::JobRow) -> Value {
    let mut view = json!({
        "id": row.id,
        "attempt_id": row.attempt_id,
        "status": STATUS_PENDING,
    });
    match row.status.as_str() {
        JOB_DONE => {
            let grading: Option<Grading> = row
                .result
                .as_ref()
                .and_then(|doc| serde_json::from_value(doc.clone()).ok());
            match grading {
                Some(grading) => {
                    view["status"] = json!(if grading.passed() {
                        STATUS_PASS
                    } else {
                        STATUS_NEEDS_REVISION
                    });
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
                            }))
                            .collect::<Vec<_>>()
                    );
                    view["model"] = json!(grading.model);
                    // The learner attempted the problem and the grading is in,
                    // so the reference solution may be compared against now.
                    if let Some(solution) = row.payload.get("reference").and_then(Value::as_str) {
                        view["solution"] = json!(solution);
                    }
                }
                None => view["status"] = json!(STATUS_FAILED),
            }
        }
        JOB_PENDING | JOB_RUNNING => {}
        JOB_CAPPED => view["status"] = json!(STATUS_CAPPED),
        JOB_FAILED => view["status"] = json!(STATUS_FAILED),
        other => view["status"] = json!(other),
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
    drop(tx);
    let Some(row) = row else {
        return Err(ApiError::not_found());
    };
    Ok(Json(poll_view(&row)))
}

/// The router of the poll route.
pub(crate) fn router() -> axum::Router<AppState> {
    axum::Router::new().route("/api/proof-grading/{id}", axum::routing::get(poll))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::{meaningful_key, poll_view};
    use cadus_store::proof_grading::JobRow;
    use serde_json::json;
    use sqlx::types::Uuid;
    use sqlx::types::chrono::Utc;

    fn row(status: &str, result: Option<serde_json::Value>) -> JobRow {
        JobRow {
            id: Uuid::nil(),
            attempt_id: "a-1".to_owned(),
            status: status.to_owned(),
            payload: json!({"reference": "The worked proof."}),
            result,
            created_at: Utc::now(),
        }
    }

    /// The placeholder key never reaches the prompt; a real key does.
    #[test]
    fn the_placeholder_key_is_dropped() {
        assert_eq!(meaningful_key("See the solution."), None);
        assert_eq!(meaningful_key("  "), None);
        assert_eq!(meaningful_key("e = 4"), Some("e = 4".to_owned()));
    }

    /// A running job polls pending and reveals nothing.
    #[test]
    fn a_running_job_polls_pending() {
        let view = poll_view(&row("running", None));
        assert_eq!(view["status"], json!("pending"));
        assert!(view.get("solution").is_none());
        assert!(view.get("checks").is_none());
    }

    /// A graded job polls its verdict, checks, feedback and the solution.
    #[test]
    fn a_graded_job_polls_the_checks_and_the_solution() {
        let result = json!({
            "v": 1, "verdict": "needs_revision", "model": "m",
            "feedback": "Step 2 assumes the conclusion.",
            "checks": [{"id": "G3", "text": "Not circular.", "minor": false, "met": false, "evidence": "not found"}]
        });
        let view = poll_view(&row("done", Some(result)));
        assert_eq!(view["status"], json!("needs_revision"));
        assert_eq!(view["feedback"], json!("Step 2 assumes the conclusion."));
        assert_eq!(view["checks"][0]["met"], json!(false));
        assert_eq!(view["solution"], json!("The worked proof."));
    }

    /// A capped or failed job polls its own status.
    #[test]
    fn a_capped_or_failed_job_polls_its_status() {
        assert_eq!(poll_view(&row("capped", None))["status"], json!("capped"));
        assert_eq!(
            poll_view(&row("failed", Some(json!({"error": "x"}))))["status"],
            json!("failed")
        );
    }
}
