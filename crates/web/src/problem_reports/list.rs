//! `GET /api/reports`: the reports of the signed-in learner, newest first.
//!
//! Row-level security binds the read to the learner. The answer holds at most
//! [`LIMIT`] rows. A report that belongs to an open quiz or placement keeps its
//! details hidden, by the same rule as `GET /api/reports/{id}`.
use super::reveal;
use crate::route_prelude::*;
use crate::state::Tenant;

/// The most reports one answer holds.
pub const LIMIT: i64 = 50;

/// The outcome word of a finished report, or null while the check runs.
fn outcome(status: &str, result: &Value) -> Value {
    match status {
        "completed" => result["resolution"].clone(),
        "unresolved" => json!("needs_review"),
        "failed" => json!("failed"),
        _ => Value::Null,
    }
}

/// One public row. The learner sees no lease, no packet, and no evidence.
fn row(raw: &Value, visible: bool) -> Value {
    let status = raw["status"].as_str().unwrap_or("failed");
    let result = &raw["result"];
    let input = &raw["input"];
    let mut reply = json!({
        "report_id": raw["id"],
        "kind": "check",
        "task_id": raw["task_id"],
        "problem_id": raw["problem_id"],
        "topic": input["report_identity"]["topic"],
        "question": input["source"]["problem"]["text"],
        "note": input["note"],
        "status": status,
        "stage": raw["stage"],
        "outcome": Value::Null,
        "grade_corrected": false,
        "details_hidden": !visible && !result.is_null(),
        "created_at": raw["created_at"],
        "updated_at": raw["updated_at"],
    });
    if visible {
        reply["outcome"] = outcome(status, result);
        reply["grade_corrected"] = json!(result["grade_corrected"] == true);
        reply["explanation"] = result["explanation"].clone();
        reply["corrected_answer"] = result["corrected_answer"].clone();
    }
    reply
}

/// One public row of a flag: the learner said the question is unclear or wrong.
fn flag_row(raw: &Value) -> Value {
    let waiting = raw["status"] == "waiting_owner";
    let outcome = match raw["status"].as_str() {
        Some("kept") => json!("question_kept"),
        Some("retired") => json!("question_retired"),
        _ => Value::Null,
    };
    json!({
        "report_id": raw["id"],
        "kind": "question_unclear",
        "task_id": raw["task_id"],
        "problem_id": raw["problem_id"],
        "topic": raw["topic"],
        "question": raw["question"],
        "note": raw["note"],
        "status": if waiting { json!("waiting_owner") } else { json!("completed") },
        "stage": raw["status"],
        "outcome": outcome,
        "grade_corrected": false,
        "details_hidden": false,
        "created_at": raw["created_at"],
        "updated_at": raw["decided_at"].as_str().map_or_else(|| raw["created_at"].clone(), |_| raw["decided_at"].clone()),
    })
}

/// `GET /api/reports`: the own reports of the learner.
///
/// # Errors
///
/// - `401 unauthorized`: the request carries no live session.
/// - `500 internal_error`: a statement failed.
pub async fn mine(
    State(state): State<AppState>,
    Tenant(user): Tenant,
) -> Result<Json<Value>, ApiError> {
    let mut tx = store(&state, cadus_store::begin_tenant(state.db.pool(), user)).await?;
    let raws = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(r) FROM problem_reports r WHERE user_id=$1
         ORDER BY created_at DESC, id DESC LIMIT $2",
    )
    .bind(user)
    .bind(LIMIT)
    .fetch_all(&mut *tx)
    .await
    .map_err(db_failed)?;
    let mut reports = Vec::with_capacity(raws.len());
    for raw in &raws {
        let visible = reveal(&mut tx, user, raw).await?;
        reports.push(row(raw, visible));
    }
    let flags = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(f) FROM question_flags f WHERE user_id=$1
         ORDER BY created_at DESC, id DESC LIMIT $2",
    )
    .bind(user)
    .bind(LIMIT)
    .fetch_all(&mut *tx)
    .await
    .map_err(db_failed)?;
    reports.extend(flags.iter().map(flag_row));
    reports.sort_by(|a, b| b["created_at"].as_str().cmp(&a["created_at"].as_str()));
    reports.truncate(usize::try_from(LIMIT).unwrap_or(50));
    Ok(Json(json!({ "reports": reports })))
}
