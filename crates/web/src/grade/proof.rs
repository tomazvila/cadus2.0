//! `POST /api/task/{task_id}/proof/continue`: a passed written proof closes
//! its knowledge point (D-PR1).
//!
//! The background grader lands the verdict in the log as a `regraded`
//! correction (outcome correct), which the fold reads. The lesson close is
//! the request tier's: this route reads the open chain of the live proof,
//! and when its head passed it closes the chain and the knowledge point in
//! ONE transaction — a later point advances the lesson, the last point
//! appends the passing `lesson_result` with its XP, which is what FIRe fires
//! from. The reply has the shape of a grade reply's advance half, so the
//! client continues the lesson the way it does after an answer.

use super::*;
use crate::proof_grading::{Phase, chain};
use cadus_store::proof_grading::{Mark, mark, open_lesson_head};

/// The `409` of a Continue on a proof that has not passed.
fn not_passed() -> ApiError {
    ApiError::new(
        StatusCode::CONFLICT,
        proof_grading::lesson::PROOF_NOT_PASSED,
        "This proof has not passed yet.",
    )
}

/// Close the knowledge point of the live written proof of `task_id`, whose
/// chain passed.
///
/// # Errors
///
/// - `404 unknown_task` — the task is not in this session's plan;
/// - `409 task_complete` — the task is closed;
/// - `409 proof_not_passed` — the task shows no written proof whose open
///   chain passed.
pub async fn proof_continue(request: TaskWithBody) -> Result<Json<Value>, ApiError> {
    let (state, user_id, task_id, _raw, now) = task_request(request);
    let content = crate::session::content(&state)?;
    let graph = &content.curriculum;
    let Open {
        mut tx,
        events,
        mut scratch,
        plan,
        readiness,
    } = open(&state, content, user_id, now, true).await?;
    let task = find(&plan, &task_id)?.clone();
    if task.task_type != TaskType::Lesson {
        return Err(not_passed());
    }
    if progress_for(&mut scratch, &task, graph).done {
        return Err(crate::state::ValidateError::TaskComplete.into());
    }
    let Some(live) = scratch
        .served
        .get(&task_id)
        .filter(|live| proof_grading::is_proof_item(live))
        .cloned()
    else {
        return Err(not_passed());
    };
    let topic = live.serving_topic().unwrap_or_default().to_owned();
    let kp = live.kp.clone().unwrap_or_default();
    let key = proof_grading::lesson::item_key(&live.text);
    let head = store(&state, open_lesson_head(&mut *tx, &topic, &kp, &key))
        .await?
        .filter(|head| chain::phase_of(head) == Phase::Passed)
        .ok_or_else(not_passed)?;
    store(&state, mark(&mut *tx, head.id, Mark::Closed)).await?;
    // One problem, one signal (D-PR1): a pass on the first graded draft earns
    // the pass tier; a pass reached through revision earns less.
    let rows = store(
        &state,
        cadus_store::proof_grading::chain_rows(&mut *tx, head.id),
    )
    .await?
    .ok_or_else(|| broken_state("the passed proof chain is missing"))?;
    let first_try = rows[..rows.len() - 1]
        .iter()
        .all(|row| row.verdict().is_none());

    let record = live
        .topic
        .as_deref()
        .and_then(|id| Slug::new(id).ok())
        .ok_or_else(|| broken_state("the served problem names no topic"))?;
    let close = CloseOf {
        session: scratch.session.clone(),
        topic: record,
        task_id: &task_id,
        quality: if first_try {
            PROOF_PASS_TIER
        } else {
            PROOF_REVISED_TIER
        },
        assisted: false,
    };
    // The point closes when EVERY written item's chain is closed; until then
    // the lesson carries on and serves the next one (D-PR1).
    let remain =
        proof_grading::lesson::items_remain(&state, &mut tx, graph, &topic, &kp, &task_id).await?;
    let mut moved = if remain {
        Advance::carry_on()
    } else {
        proof_close(graph, &content.cfg, now, &close, &kp, &events)
    };
    if matches!(
        moved.result.as_ref(),
        Some(Event::LessonResult(result)) if result.passed
    ) {
        let score = super::proof_score::completed(&state, &mut tx, &topic).await?;
        moved = super::proof_score::apply(graph, &content.cfg, moved, score);
    }
    for extra in moved.events() {
        store(&state, append_event(&mut tx, user_id, &extra, None)).await?;
    }
    let input = projection_input(content, now);
    store(&state, project_and_save(&mut tx, user_id, &input, None)).await?;

    let progress = progress_for(&mut scratch, &task, graph);
    let closed = task_moved_on(progress, TaskType::Lesson, &moved);
    let next = next_problem(
        &state,
        content,
        &mut tx,
        user_id,
        &task,
        &mut scratch,
        &readiness,
        now,
        closed,
    )
    .await;
    save_and_commit(&state, tx, user_id, &scratch).await?;
    let mut body = json!({
        "task_status": moved.status,
        "remediation": moved.remediation_view(),
        "next": next,
        "proof_grading": proof_grading::reply_field(None),
        "proof": {"context": "lesson", "phase": "closed", "job_id": head.id},
    });
    if !closed && body["next"].is_null() {
        body["next_unavailable"] = json!(true);
    }
    if let Some(xp) = moved.xp {
        body["xp"] = json!(xp);
    }
    Ok(Json(body))
}

/// The `proof` field of a reply whose lesson proof is being graded.
pub(super) fn pending_field(job: Option<Uuid>, revision: i32) -> Value {
    json!({
        "context": cadus_store::proof_grading::CONTEXT_LESSON,
        "phase": "grading",
        "job_id": job,
        "revision": revision,
        "cap": proof_grading::REVISION_CAP,
        "revisions_left": (proof_grading::REVISION_CAP - revision).max(0),
    })
}

/// One revision of a lesson proof: the draft that revises the chain's head.
pub(super) struct Revision<'a> {
    pub(super) task: &'a Task,
    pub(super) served: &'a ServedProblem,
    pub(super) answer: &'a str,
    pub(super) step: proof_grading::lesson::LessonStep,
    pub(super) now: Timestamp,
}

/// Record one revision of a lesson proof on its chain (D-PR1).
///
/// The FIRST draft is the problem's one scheduling signal, so a revision
/// appends no `attempt` event and folds nothing: it is a new grading job
/// linked to the draft it revises, under the attempt id the step names. The
/// unaided rewrite after the cap closes the knowledge point as assisted, which
/// appends the close (a `kp_advance` writes nothing; the last point appends
/// its `lesson_result`).
#[expect(
    clippy::too_many_arguments,
    reason = "the close reads the content, the scratch, the window and the transaction"
)]
pub(super) async fn revise_in_lesson(
    state: &AppState,
    content: &Content,
    mut tx: Transaction<'static, Postgres>,
    user_id: Uuid,
    mut scratch: WebState,
    events: &[EventRow],
    readiness: &ReadinessSet,
    revision: Revision<'_>,
) -> Result<Json<Value>, ApiError> {
    let Revision {
        task,
        served,
        answer,
        step,
        now,
    } = revision;
    let graph = &content.curriculum;
    let attempt_id = step.revision_attempt.clone().unwrap_or_default();
    let job = proof_grading::enqueue(
        state,
        &mut tx,
        user_id,
        &attempt_id,
        served,
        answer,
        &step.job,
    )
    .await?;
    let mut body = json!({
        "attempt_id": attempt_id,
        "outcome": "ungraded",
        "reason": PROOF_UNGRADED,
        "error_tags": [],
        "remediation": [],
        "diagnosis": {"status": "not_offered"},
        "proof_grading": proof_grading::reply_field(job),
    });
    if !step.rewrite() {
        save_and_commit(state, tx, user_id, &scratch).await?;
        body["task_status"] = json!(STATUS_PROOF_PENDING);
        body["next"] = Value::Null;
        body["proof"] = pending_field(job, step.job.revision);
        return Ok(Json(body));
    }
    if let Some(id) = job {
        store(state, mark(&mut *tx, id, Mark::Closed)).await?;
    }
    let record = served
        .topic
        .as_deref()
        .and_then(|id| Slug::new(id).ok())
        .ok_or_else(|| broken_state("the served problem names no topic"))?;
    let close = CloseOf {
        session: scratch.session.clone(),
        topic: record,
        task_id: &task.task_id,
        quality: PROOF_ASSISTED_TIER,
        assisted: true,
    };
    let kp = served.kp.clone().unwrap_or_default();
    let remain = proof_grading::lesson::items_remain(
        state,
        &mut tx,
        graph,
        close.topic.as_str(),
        &kp,
        &task.task_id,
    )
    .await?;
    let mut moved = if remain {
        Advance::carry_on()
    } else {
        proof_close(graph, &content.cfg, now, &close, &kp, events)
    };
    if matches!(
        moved.result.as_ref(),
        Some(Event::LessonResult(result)) if result.passed
    ) {
        let score = super::proof_score::completed(state, &mut tx, close.topic.as_str()).await?;
        moved = super::proof_score::apply(graph, &content.cfg, moved, score);
    }
    for extra in moved.events() {
        store(state, append_event(&mut tx, user_id, &extra, None)).await?;
    }
    let input = projection_input(content, now);
    store(state, project_and_save(&mut tx, user_id, &input, None)).await?;
    let progress = progress_for(&mut scratch, task, graph);
    let closed = task_moved_on(progress, TaskType::Lesson, &moved);
    let next = next_problem(
        state,
        content,
        &mut tx,
        user_id,
        task,
        &mut scratch,
        readiness,
        now,
        closed,
    )
    .await;
    save_and_commit(state, tx, user_id, &scratch).await?;
    body["task_status"] = json!(moved.status);
    body["remediation"] = json!(moved.remediation_view());
    body["proof"] = json!({"context": "lesson", "phase": "closed", "job_id": job, "rewrite": true});
    if !closed && next.is_none() {
        body["next_unavailable"] = json!(true);
    }
    body["next"] = json!(next);
    if let Some(xp) = moved.xp {
        body["xp"] = json!(xp);
    }
    Ok(Json(body))
}
