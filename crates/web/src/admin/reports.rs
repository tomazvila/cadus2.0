//! `GET /api/admin/reports`: the learner reports that the owner reads.
//!
//! The route is read-only. It crosses every learner, so it reads through the
//! admin connection (`cadus_admin` holds BYPASSRLS and the grant on the step
//! table). `status=unresolved` is the default and lists the reports that the
//! check could not decide. `status=all` lists every report. The answer holds at
//! most [`LIMIT`] rows, newest first.
use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{Query, State};
use serde_json::{Value, json};

use super::{AdminUser, STATUS_PARAM, admin_path};
use crate::AppState;
use crate::error::ApiError;
use crate::path::ApiPath;
use crate::session::db_failed;
use sqlx::types::Uuid;

/// The most reports one answer holds.
pub const LIMIT: i64 = 200;

/// The message of a `status` value that the route does not know.
pub const STATUS_MESSAGE: &str = "The status must be \"unresolved\" or \"all\".";

/// One row for the owner. A string field of the packet is copied as it is.
fn row(raw: &Value, step: &Value) -> Value {
    let input = &raw["input"];
    let evidence = &step["evidence"];
    let message = ["message", "explanation"]
        .iter()
        .find_map(|key| evidence[*key].as_str());
    json!({
        "report_id": raw["id"],
        "kind": "check",
        "user_id": raw["user_id"],
        "task_id": raw["task_id"],
        "problem_id": raw["problem_id"],
        "topic": input["report_identity"]["topic"],
        "kp": input["attempt"]["kp"],
        "question": input["source"]["problem"]["text"],
        "note": input["note"],
        "status": raw["status"],
        "attempt": raw["attempt"],
        "result_message": raw["result"]["message"],
        "last_step": { "stage": step["stage"], "message": message },
        "created_at": raw["created_at"],
        "updated_at": raw["updated_at"],
    })
}

/// One row for a flag: a learner said the question is unclear or wrong.
fn flag_row(raw: &Value) -> Value {
    json!({
        "report_id": raw["id"],
        "kind": "question_unclear",
        "user_id": raw["user_id"],
        "task_id": raw["task_id"],
        "problem_id": raw["problem_id"],
        "topic": raw["topic"],
        "kp": raw["kp"],
        "question": raw["question"],
        "note": raw["note"],
        "status": raw["status"],
        "attempt": 0,
        "result_message": Value::Null,
        "last_step": { "stage": Value::Null, "message": Value::Null },
        "created_at": raw["created_at"],
        "updated_at": raw["created_at"],
    })
}

/// `GET /api/admin/reports?status=unresolved|all`.
///
/// # Errors
///
/// - `401 unauthorized`: the request carries no live session.
/// - `403 forbidden`: the account is not an admin.
/// - `422 invalid_request`: the `status` value is unknown.
/// - `503 admin_path_unavailable`: the deployment has no admin connection.
/// - `500 internal_error`: a statement failed.
pub async fn list_reports(
    AdminUser(_authed): AdminUser,
    State(state): State<AppState>,
    Query(params): Query<BTreeMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let all = match params.get(STATUS_PARAM).map(String::as_str) {
        None | Some("unresolved") => false,
        Some("all") => true,
        Some(_) => return Err(ApiError::invalid_request(STATUS_MESSAGE)),
    };
    let admin = admin_path(&state)?;
    let rows = sqlx::query_as::<_, (Value, Option<Value>)>(
        "SELECT to_jsonb(r),
                (SELECT jsonb_build_object('stage', s.stage, 'evidence', s.evidence)
                 FROM problem_report_steps s WHERE s.report_id = r.id
                 ORDER BY s.created_at DESC, s.id DESC LIMIT 1)
         FROM problem_reports r
         WHERE $1 OR r.status = 'unresolved'
         ORDER BY r.created_at DESC, r.id DESC LIMIT $2",
    )
    .bind(all)
    .bind(LIMIT)
    .fetch_all(admin.pool())
    .await
    .map_err(db_failed)?;
    let flags = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(f) FROM question_flags f
         WHERE $1 OR f.status = 'waiting_owner'
         ORDER BY f.created_at DESC, f.id DESC LIMIT $2",
    )
    .bind(all)
    .bind(LIMIT)
    .fetch_all(admin.pool())
    .await
    .map_err(db_failed)?;
    let mut reports: Vec<Value> = rows
        .iter()
        .map(|(raw, step)| row(raw, step.as_ref().unwrap_or(&Value::Null)))
        .collect();
    reports.extend(flags.iter().map(flag_row));
    reports.sort_by(|a, b| b["created_at"].as_str().cmp(&a["created_at"].as_str()));
    reports.truncate(usize::try_from(LIMIT).unwrap_or(200));
    Ok(Json(json!({ "reports": reports })))
}

/// `POST /api/admin/reports/{id}/keep`: the owner keeps the question. The learner gets
/// the question back.
///
/// # Errors
///
/// - `401`, `403`, `503` as for the list.
/// - `404 not_found`: no flag waits under that id.
pub async fn keep_question(
    AdminUser(_authed): AdminUser,
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let admin = admin_path(&state)?;
    let done = sqlx::query(
        "UPDATE question_flags SET status='kept', decided_at=now()
         WHERE id=$1 AND status='waiting_owner'",
    )
    .bind(id)
    .execute(admin.pool())
    .await
    .map_err(db_failed)?
    .rows_affected();
    if done == 0 {
        return Err(ApiError::not_found());
    }
    Ok(Json(json!({ "report_id": id, "status": "kept" })))
}

/// `POST /api/admin/reports/{id}/retire`: the owner retires the question. The serve path
/// skips it for every learner. Every waiting flag of the same question closes with it.
///
/// # Errors
///
/// - `401`, `403`, `503` as for the list.
/// - `404 not_found`: no flag waits under that id.
pub async fn retire_question(
    AdminUser(authed): AdminUser,
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let admin = admin_path(&state)?;
    let mut tx = admin.pool().begin().await.map_err(db_failed)?;
    let hash = sqlx::query_scalar::<_, String>(
        "SELECT problem_hash FROM question_flags WHERE id=$1 AND status='waiting_owner' FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_failed)?
    .ok_or_else(ApiError::not_found)?;
    sqlx::query(
        "INSERT INTO retired_items(problem_hash, reason, retired_by)
         VALUES($1, 'the owner retired the question after a learner report', $2)
         ON CONFLICT DO NOTHING",
    )
    .bind(&hash)
    .bind(authed.user.id)
    .execute(&mut *tx)
    .await
    .map_err(db_failed)?;
    sqlx::query(
        "UPDATE question_flags SET status='retired', decided_at=now()
         WHERE problem_hash=$1 AND status='waiting_owner'",
    )
    .bind(&hash)
    .execute(&mut *tx)
    .await
    .map_err(db_failed)?;
    tx.commit().await.map_err(db_failed)?;
    Ok(Json(json!({ "report_id": id, "status": "retired" })))
}
