//! A learner says a question is unclear or wrong (D-RP2).
//!
//! No model can judge an unclear question, so this report starts no review. The row
//! in `question_flags` sets the question aside for this learner at once. The serve path
//! skips a set-aside question ([`is_set_aside`]). The owner decides: keep the question or
//! retire it.
use super::*;
use cadus_core::learner::problem_text_hash;

/// The status of a flag that waits for the owner.
pub const WAITING: &str = "waiting_owner";

/// The most flags one learner sends in one day.
const DAILY_LIMIT: i64 = 20;

/// The public shape of one flag, in the shape of a report receipt.
pub(super) fn receipt(row: &Value) -> Value {
    json!({"report_id": row["id"], "status": row["status"], "stage": row["status"],
        "attempt": 0, "max_attempts": 3, "retryable": false,
        "set_aside": row["status"] == WAITING})
}

/// One flag by its id, for the learner who owns it.
pub(super) async fn get(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
    id: Uuid,
) -> Result<Option<Value>, ApiError> {
    sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(f) FROM question_flags f WHERE user_id=$1 AND id=$2",
    )
    .bind(user)
    .bind(id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(db_failed)
}

/// Create the flag for an ordinary question, or answer the earlier flag of the same request.
#[expect(
    clippy::too_many_arguments,
    reason = "The flag needs the request identity, the task, and the body next to its transaction."
)]
pub(super) async fn create(
    state: &AppState,
    content: &Content,
    mut tx: Transaction<'_, Postgres>,
    user: Uuid,
    request_id: Uuid,
    task: &str,
    problem: &str,
    note: &str,
    body: &Value,
) -> Result<Json<Value>, ApiError> {
    let earlier = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(f) FROM question_flags f WHERE user_id=$1 AND request_id=$2",
    )
    .bind(user)
    .bind(request_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_failed)?;
    if let Some(row) = earlier {
        if row["task_id"] != task || row["problem_id"] != problem {
            return Err(ApiError::invalid_request(
                "That request ID belongs to another report.",
            ));
        }
        return Ok(Json(receipt(&row)));
    }
    let kind = body
        .get("report_kind")
        .and_then(Value::as_str)
        .unwrap_or("attempt");
    if !["attempt", "served"].contains(&kind) {
        return Err(ApiError::invalid_request(
            "This kind of question cannot be set aside.",
        ));
    }
    let snapshot = super::sources::snapshot(state, content, &mut tx, user, task, body).await?;
    let recent = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM question_flags WHERE user_id=$1 AND created_at>now()-interval '1 day'",
    )
    .bind(user)
    .fetch_one(&mut *tx)
    .await
    .map_err(db_failed)?;
    if recent >= DAILY_LIMIT {
        return Err(ApiError::rate_limited());
    }
    let hash = problem_text_hash(&snapshot.problem.text);
    let kp = snapshot.packet["attempt"]["kp"].as_str().unwrap_or("");
    let row = sqlx::query_scalar::<_, Value>(
        "INSERT INTO question_flags(user_id,request_id,task_id,problem_id,problem_hash,topic,kp,question,note)
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING to_jsonb(question_flags)",
    )
    .bind(user)
    .bind(request_id)
    .bind(task)
    .bind(problem)
    .bind(&hash)
    .bind(&snapshot.topic)
    .bind(kp)
    .bind(&snapshot.problem.text)
    .bind(note)
    .fetch_one(&mut *tx)
    .await
    .map_err(db_failed)?;
    tx.commit().await.map_err(db_failed)?;
    Ok(Json(receipt(&row)))
}

/// True when the learner set the question aside and the owner has not decided, or when
/// the owner retired the question for everyone.
pub(crate) async fn is_set_aside(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
    hash: &str,
) -> Result<bool, ApiError> {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM question_flags WHERE user_id=$1 AND problem_hash=$2 AND status='waiting_owner')
             OR EXISTS(SELECT 1 FROM retired_items WHERE problem_hash=$2)",
    )
    .bind(user)
    .bind(hash)
    .fetch_one(&mut **tx)
    .await
    .map_err(db_failed)
}
