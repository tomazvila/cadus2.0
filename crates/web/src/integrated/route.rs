//! The three handlers of the integrated task (D-F10).
//!
//! Each one opens the transaction the teach route opens, finds the task in the
//! plan of the OPEN session, and resolves the item from that task. The serve and
//! the answer then APPEND one event inside that same transaction, so the
//! hand-off and the submission survive a refresh, and a repeat writes nothing.

use super::*;

/// What a route resolved: the item, the open transaction, and the session.
struct Resolved<'content> {
    item: &'content IntegratedItem,
    tx: Transaction<'static, Postgres>,
    session: String,
}

/// Resolve the item of `task_id` in the learner's own plan.
///
/// The lookup runs through the plan, so a task id of another learner, a task of
/// another session, and a task type that serves no integrated item all refuse
/// before any content is read. `lock` takes the tenant's advisory lock, which a
/// route that appends an event must hold.
async fn resolve<'content>(
    state: &AppState,
    content: &'content Content,
    user_id: Uuid,
    task_id: &str,
    now: Timestamp,
    lock: bool,
) -> Result<Resolved<'content>, ApiError> {
    let Open { tx, plan, .. } = open(state, content, user_id, now, lock).await?;
    let task = find(&plan, task_id)?;
    let item = for_task(content, task).ok_or_else(no_item)?;
    Ok(Resolved {
        item,
        tx,
        session: plan.session,
    })
}

/// `POST /api/task/{task_id}/integrated`: the whole item as ONE problem.
///
/// The reply is the client-safe view. A task with no authored item answers
/// `409 no_integrated_item`, and the client then serves the task part by part
/// through `POST /api/task/{task_id}/serve`, exactly as before D-F10.
///
/// The hand-off is recorded before the reply leaves, so an abandoned task and a
/// reloaded one are both an exposure and neither reads as unseen (D-F9).
pub async fn serve(
    State(state): State<AppState>,
    Tenant(user_id): Tenant,
    ApiPath(task_id): ApiPath<String>,
) -> Result<Json<Value>, ApiError> {
    let content = content(&state)?;
    let (_, now) = now_pair();
    let Resolved {
        item,
        mut tx,
        session,
    } = resolve(&state, content, user_id, &task_id, now, true).await?;
    let digest = item.digest();
    let event = Event::IntegratedServed(served_event(item, &task_id, &session, now));
    let key = served_key(&session, &task_id, &digest);
    store(&state, append_event(&mut tx, user_id, &event, Some(&key))).await?;
    tx.commit().await.map_err(db_failed)?;

    let payload = serde_json::to_value(view_of(item))
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
    let (_, now) = now_pair();
    let body = raw.ok_or_else(|| invalid("A hint request names a field."))?;
    let request: HintRequest = serde_json::from_value(body.0)
        .map_err(|_| invalid("A hint request names a field and a rung index."))?;
    let Resolved { item, tx, .. } = resolve(&state, content, user_id, &task_id, now, false).await?;
    let payload = hint_payload(item, &request);
    // The transaction read only, so the drop rolls it back with no round trip.
    drop(tx);
    Ok(Json(payload?))
}

/// `POST /api/task/{task_id}/integrated/answer`: grade the whole item.
///
/// One submission carries the method choice, every answered step, the final
/// answer, and the learner's own words. The grade is appended to the log in the
/// same transaction that read the plan, and the append is idempotent per
/// session, task and item digest: a second submission of the same item writes
/// nothing and the reply says `recorded: false`.
pub async fn answer(
    State(state): State<AppState>,
    Tenant(user_id): Tenant,
    ApiPath(task_id): ApiPath<String>,
    raw: Option<Json<Value>>,
) -> Result<Json<Value>, ApiError> {
    let content = content(&state)?;
    let (_, now) = now_pair();
    let body = raw.ok_or_else(|| invalid("A submission carries a final answer."))?;
    let submission: Submission = serde_json::from_value(body.0)
        .map_err(|_| invalid("A submission carries steps and a final answer."))?;
    let Resolved {
        item,
        mut tx,
        session,
    } = resolve(&state, content, user_id, &task_id, now, true).await?;

    let result = grade(item, &submission);
    let record = attempt_event(item, &result, &submission, &task_id, &session, now);
    let key = record.attempt_id.clone();
    let event = Event::IntegratedAttempt(record);
    let seq = store(&state, append_event(&mut tx, user_id, &event, Some(&key))).await?;
    tx.commit().await.map_err(db_failed)?;

    let mut payload = answer_payload(&result, submission.reasoning.as_ref());
    // `None` means the partial unique index refused a second row for this item:
    // the first submission stands, and this one credits nothing again.
    payload["recorded"] = json!(seq.is_some());
    Ok(Json(payload))
}
