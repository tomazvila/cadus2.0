//! `POST /api/task/{task_id}/teach`: approved preparation for lessons and integrated application.

use super::*;

/// The authored teach page of a lesson's current knowledge point (`api.py:1064-1103`).
///
/// Lessons use their current knowledge point. Integrated application uses its
/// first credited skill and persists the instruction hand-off before practice.
/// Both paths read an approved teach page; missing instruction returns a conflict.
///
/// D-O3: the ordinary lesson path reads one content document and writes no state.
pub async fn teach(
    (State(state), Tenant(user_id), ApiPath(task_id)): crate::path::TaskContext,
) -> Result<Json<Value>, ApiError> {
    let content = content(&state)?;
    let (_, now) = now_pair();
    let graph = &content.curriculum;

    let Open {
        mut tx,
        mut scratch,
        plan,
        ..
    } = open(&state, content, user_id, now, true).await?;
    let task = find(&plan, &task_id)?;
    // Only a lesson teaches, and a lesson always names a topic to teach from.
    // The one refusal covers a task of any other type; the composer never
    // builds a lesson without a topic, so the two are one decision here.
    if task.task_type == TaskType::MultiStep {
        return crate::integrated::instruction::teach(
            &state,
            content,
            user_id,
            task,
            &mut scratch,
            tx,
        )
        .await;
    }
    let (TaskType::Lesson, Some(topic)) = (task.task_type, task.topic.clone()) else {
        return Err(no_instruction());
    };
    let current = scratch
        .tasks
        .get(&task_id)
        .and_then(|progress| progress.current_kp.as_deref());
    let kp = lesson_kp(current, task, graph, &topic).ok_or_else(no_instruction)?;
    let key = kp_key(&topic, &kp);

    let doc = store(&state, approved_document(&mut *tx, &key, KIND_TEACH)).await?;
    // The transaction read one row and wrote nothing, so the drop rolls it back
    // and the read needs no second round trip.
    drop(tx);

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
