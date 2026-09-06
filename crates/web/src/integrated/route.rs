//! The three handlers of the integrated task (D-F10).
//!
//! Each one opens the same read-only transaction the teach route opens, finds
//! the task in the plan of the OPEN session, and resolves the item from that
//! task. The transaction writes nothing, so the drop rolls it back.

use super::*;

/// Resolve the item of `task_id` in the learner's own plan.
///
/// The lookup runs through the plan, so a task id of another learner, a task of
/// another session, and a task type that serves no integrated item all refuse
/// before any content is read.
async fn item_of<'content>(
    state: &AppState,
    content: &'content Content,
    user_id: sqlx::types::Uuid,
    task_id: &str,
) -> Result<&'content IntegratedItem, ApiError> {
    let (_, now) = now_pair();
    let Open { tx, plan, .. } = open(state, content, user_id, now, false).await?;
    let task = find(&plan, task_id)?;
    let item = for_task(content, task).ok_or_else(no_item);
    // The transaction read only, so the drop rolls it back with no round trip.
    drop(tx);
    item
}

/// `POST /api/task/{task_id}/integrated`: the whole item as ONE problem.
///
/// The reply is the client-safe view. A task with no authored item answers
/// `409 no_integrated_item`, and the client then serves the task part by part
/// through `POST /api/task/{task_id}/serve`, exactly as before D-F10.
pub async fn serve(
    State(state): State<AppState>,
    Tenant(user_id): Tenant,
    ApiPath(task_id): ApiPath<String>,
) -> Result<Json<Value>, ApiError> {
    let content = content(&state)?;
    let item = item_of(&state, content, user_id, &task_id).await?;
    let view = view_of(item);
    let payload = serde_json::to_value(&view)
        .map_err(|_| ApiError::internal("The integrated problem did not serialize."))?;
    Ok(Json(payload))
}

/// `POST /api/task/{task_id}/integrated/hint`: ONE rung of one field's ladder.
///
/// The rungs stay on the server. The reply carries the rung the learner asked
/// for, the count of rungs the field holds, and the count they have now opened,
/// which the submission reports back as the assistance of that field.
pub async fn hint_rung(
    State(state): State<AppState>,
    Tenant(user_id): Tenant,
    ApiPath(task_id): ApiPath<String>,
    raw: Option<Json<Value>>,
) -> Result<Json<Value>, ApiError> {
    let content = content(&state)?;
    let body = raw.ok_or_else(|| invalid("A hint request names a field."))?;
    let request: HintRequest = serde_json::from_value(body.0)
        .map_err(|_| invalid("A hint request names a field and a rung index."))?;
    let item = item_of(&state, content, user_id, &task_id).await?;
    Ok(Json(hint_payload(item, &request)?))
}

/// `POST /api/task/{task_id}/integrated/answer`: grade the whole item.
///
/// One submission carries the method choice, every answered step, the final
/// answer, and the learner's own words. The reply grades the steps and the
/// final answer with the authored contracts, releases the interpretation, and
/// repeats the prose under a `graded: false` flag.
pub async fn answer(
    State(state): State<AppState>,
    Tenant(user_id): Tenant,
    ApiPath(task_id): ApiPath<String>,
    raw: Option<Json<Value>>,
) -> Result<Json<Value>, ApiError> {
    let content = content(&state)?;
    let body = raw.ok_or_else(|| invalid("A submission carries a final answer."))?;
    let submission: Submission = serde_json::from_value(body.0)
        .map_err(|_| invalid("A submission carries steps and a final answer."))?;
    let item = item_of(&state, content, user_id, &task_id).await?;
    Ok(Json(answer_payload(item, &submission)))
}
