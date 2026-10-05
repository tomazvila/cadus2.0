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
    let head = store(&state, open_lesson_head(&mut *tx, &topic, &kp))
        .await?
        .filter(|head| chain::phase_of(head) == Phase::Passed)
        .ok_or_else(not_passed)?;
    store(&state, mark(&mut *tx, head.id, Mark::Closed)).await?;

    let record = live
        .topic
        .as_deref()
        .and_then(|id| Slug::new(id).ok())
        .ok_or_else(|| broken_state("the served problem names no topic"))?;
    let close = CloseOf {
        session: scratch.session.clone(),
        topic: record,
        task_id: &task_id,
        quality: PROOF_PASS_TIER,
        assisted: false,
    };
    let moved = proof_close(graph, &content.cfg, now, &close, &kp, &events);
    for extra in moved.events() {
        store(&state, append_event(&mut tx, user_id, &extra, None)).await?;
    }
    let input = projection_input(content, now);
    store(&state, project_and_save(&mut tx, user_id, &input, None)).await?;

    let progress = progress_for(&mut scratch, &task, graph);
    let closed = task_moved_on(progress, TaskType::Lesson, &moved);
    progress.proof_kp = None;
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
