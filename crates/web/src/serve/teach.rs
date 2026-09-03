//! `POST /api/task/{task_id}/teach`: the authored teach page of a lesson (L4).

use super::*;

/// The authored teach page of a lesson's current knowledge point (`api.py:1064-1103`).
///
/// Only a lesson teaches. Every other task type is `409 no_instruction`, and so
/// is a lesson whose knowledge point has no APPROVED teach page: in both cases
/// the server has no worked example to give, and 2.0 never asks a model for one
/// (L4, T1).
///
/// D-O3: one `content_store` read, no state write, and the transaction is read
/// only.
pub async fn teach(
    State(state): State<AppState>,
    Tenant(user_id): Tenant,
    ApiPath(task_id): ApiPath<String>,
) -> Result<Json<Value>, ApiError> {
    let content = content(&state)?;
    let (_, now) = now_pair();
    let graph = &content.curriculum;

    let Open {
        mut tx,
        scratch,
        plan,
        ..
    } = open(&state, content, user_id, now, false).await?;
    let task = find(&plan, &task_id)?;
    if task.task_type != TaskType::Lesson {
        return Err(no_instruction());
    }
    let topic = topic_or_refuse(task)?;
    let current = scratch
        .tasks
        .get(&task_id)
        .and_then(|progress| progress.current_kp.as_deref());
    let kp = lesson_kp(current, task, graph, &topic).ok_or_else(no_instruction)?;
    let key = kp_key(&topic, &kp);

    let doc = store(&state, approved_document(&mut *tx, &key, KIND_TEACH)).await?;
    tx.rollback().await.map_err(db_failed)?;

    let page: TeachDoc = read_document(
        doc.ok_or_else(no_instruction)?,
        "teach",
        "The authored teach page is not readable.",
    )?;
    Ok(Json(json!({
        "kp": kp,
        "concept": page.concept,
        "worked_example": {
            "problem": page.worked_example.problem,
            "steps": page.worked_example.steps,
        },
    })))
}
