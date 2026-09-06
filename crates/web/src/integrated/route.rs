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
    scratch: crate::state::WebState,
    task: Task,
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
    let Open {
        tx, plan, scratch, ..
    } = open(state, content, user_id, now, lock).await?;
    let task = find(&plan, task_id)?.clone();
    let item = for_task(content, &task).ok_or_else(no_item)?;
    Ok(Resolved {
        item,
        tx,
        session: plan.session,
        scratch,
        task,
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
        mut scratch,
        ..
    } = resolve(&state, content, user_id, &task_id, now, true).await?;
    timing::start(&mut scratch, item, &task_id, now);
    crate::session::write_state(&state.db, &mut tx, user_id, &scratch).await?;
    let digest = item.digest();
    let event = Event::IntegratedServed(served_event(item, &task_id, &session, now));
    let key = served_key(&session, &task_id, &digest);
    store(&state, append_event(&mut tx, user_id, &event, Some(&key))).await?;
    let used = store(
        &state,
        cadus_store::integrated::hints_used(&mut tx, user_id, &session, &task_id, &digest),
    )
    .await?;
    tx.commit().await.map_err(db_failed)?;

    let mut payload = serde_json::to_value(view_of(item))
        .map_err(|_| ApiError::internal("The integrated problem did not serialize."))?;
    payload["hints_used"] = json!(used);
    Ok(Json(payload))
}

/// `POST /api/task/{task_id}/integrated/hint`: ONE rung of one field's ladder.
///
/// The rungs stay on the server. The reply carries the rung the learner asked
/// for, the count of rungs the field holds, and the count they have now opened,
/// persisted before the reply and read again when the answer is graded.
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
    let Resolved {
        item,
        mut tx,
        session,
        ..
    } = resolve(&state, content, user_id, &task_id, now, true).await?;
    let mut payload = hint_payload(item, &request)?;
    let digest = item.digest();
    if payload["hint"].is_string() {
        let event = Event::IntegratedHintRevealed(cadus_core::event::IntegratedHintRevealed {
            ts: now,
            session: Some(session.clone()),
            v: cadus_core::event::SchemaVersion::current(),
            task_id: task_id.clone(),
            item_digest: digest.clone(),
            field: request.field.clone(),
            index: request.index,
        });
        let key = format!(
            "{session}:{task_id}:integrated-hint:{digest}:{}:{}",
            request.field, request.index
        );
        store(&state, append_event(&mut tx, user_id, &event, Some(&key))).await?;
    }
    let used = store(
        &state,
        cadus_store::integrated::hints_used(&mut tx, user_id, &session, &task_id, &digest),
    )
    .await?;
    payload["hints_used"] = json!(used.get(&request.field).copied().unwrap_or(0));
    tx.commit().await.map_err(db_failed)?;
    Ok(Json(payload))
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
    let mut submission: Submission = serde_json::from_value(body.0)
        .map_err(|_| invalid("A submission carries steps and a final answer."))?;
    let Resolved {
        item,
        mut tx,
        session,
        mut scratch,
        task,
    } = resolve(&state, content, user_id, &task_id, now, true).await?;

    let key = attempt_key(&session, &task_id, &item.digest());
    if let Some(record) = store(
        &state,
        cadus_store::integrated::attempt(&mut tx, user_id, &key),
    )
    .await?
    {
        let result = assistance::replay(&record, item);
        let mut payload = answer_payload(&result, record.reasoning_ungraded.as_ref());
        timing::payload(&mut payload, &record);
        payload["recorded"] = json!(false);
        tx.commit().await.map_err(db_failed)?;
        return Ok(Json(payload));
    }
    let used = store(
        &state,
        cadus_store::integrated::hints_used(&mut tx, user_id, &session, &task_id, &item.digest()),
    )
    .await?;
    assistance::submission(&mut submission, &used);
    let mut result = grade(item, &submission);
    assistance::verdict(&mut result, &used);
    let mut record = attempt_event(item, &result, &submission, &task_id, &session, now);
    timing::record(
        &mut record,
        item,
        &content.curriculum,
        &scratch,
        &task_id,
        now,
    );
    let key = record.attempt_id.clone();
    let note = record.reasoning_ungraded.clone();
    let timing = record.timing;
    let reliable = record.timing_reliable;
    let event = Event::IntegratedAttempt(record);
    let seq = store(&state, append_event(&mut tx, user_id, &event, Some(&key))).await?;
    if seq.is_some() {
        let progress = crate::serve::progress_for(&mut scratch, &task, &content.curriculum);
        progress.total = 1;
        progress.served = progress.served.max(1);
        progress.answered = 1;
        progress.done = true;
        crate::session::write_state(&state.db, &mut tx, user_id, &scratch).await?;
    }
    tx.commit().await.map_err(db_failed)?;

    let mut payload = answer_payload(&result, note.as_ref());
    payload["timing"] = json!(timing);
    payload["timing_reliable"] = json!(reliable);
    // `None` means the partial unique index refused a second row for this item:
    // the first submission stands, and this one credits nothing again.
    payload["recorded"] = json!(seq.is_some());
    Ok(Json(payload))
}
