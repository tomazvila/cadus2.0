//! `POST /api/task/{task_id}/answer`: the ten steps of one transaction, in
//! the order of spec section 4.3.

use super::*;

/// The authored solve time of the served problem's topic, when the arena holds
/// the topic.
fn expected_time(graph: &Curriculum, served: &ServedProblem) -> Option<i64> {
    served
        .topic
        .as_deref()
        .and_then(|id| graph.idx_of(id))
        .and_then(|idx| graph.topic(idx))
        .map(|topic| topic.expected_time_secs)
}

/// The answer kind of the served problem.
///
/// EVERY kind reaches the grade path now (D-F1, D-F2). The route no longer
/// refuses a kind with `409`: a kind the checker does not decide gives the
/// UNGRADED outcome, and the learner reads a reason and takes the next task.
fn served_kind(served: &ServedProblem) -> Result<AnswerKind, ApiError> {
    answer_kind(served).ok_or_else(|| broken_state("the served problem names no answer kind"))
}

/// A solve time as the seconds the session clock adds up.
fn elapsed_of(secs: i64) -> f64 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "a solve time in seconds is far below 2**53"
    )]
    let elapsed = secs as f64;
    elapsed
}

/// Write the D-S6 row and commit the transaction.
async fn save_and_commit(
    state: &AppState,
    mut tx: Transaction<'static, Postgres>,
    user_id: Uuid,
    scratch: &WebState,
) -> Result<(), ApiError> {
    write_state(&state.db, &mut tx, user_id, scratch).await?;
    tx.commit().await.map_err(db_failed)
}

/// Grade one answer, record it, and hand back the whole verdict (A3, A4).
///
/// The route never calls a model and never waits for one. Section 4.3 gives the
/// order of the steps and this function follows it top to bottom.
pub async fn answer(
    State(state): State<AppState>,
    Tenant(user_id): Tenant,
    ApiPath(task_id): ApiPath<String>,
    raw: Option<Json<Value>>,
) -> Result<Json<Value>, ApiError> {
    let (content, body) = route_input(&state, raw.as_ref())?;
    let submitted = submission(body)?;
    let graph = &content.curriculum;
    let (_, now) = now_pair();

    let Open {
        mut tx,
        events,
        mut scratch,
        plan,
    } = open(&state, content, user_id, now, true).await?;
    let task = find(&plan, &task_id)?.clone();
    // The answer path MAY install the progress row (`_validate`, `api.py:1303`).
    // The plan route may not: trap W3.
    progress_for(&mut scratch, &task, graph);
    // The section 4.2 re-check, in its two refusals: a closed task is
    // `409 task_complete`, a superseded id is `404 unknown_problem`.
    let served = scratch.validate(&task_id, &submitted.problem_id)?.clone();

    // The clock is measured BEFORE the grade, so no grading work inflates it.
    let (secs, timing_tags) = measure_secs(
        served.started_at,
        now.micros(),
        expected_time(graph, &served),
    );
    let kind = served_kind(&served)?;
    let grade = grade_item(&served.expected, &submitted.answer, kind);
    // T6, spec section 7: one count per grade DECISION, taken with no model call.
    state.metrics.count_grade(metrics::grade_result(&grade));
    let mut error_tags = grade.error_tags.clone();
    error_tags.extend(timing_tags);
    scratch.active_secs += elapsed_of(secs);

    // H3, section 5.4. `open` bound the row to the open session, so the session
    // stands; the field is carried as the log spells it.
    let assisted = if served.rework.is_some() {
        submitted.assisted || !served.hints_given.is_empty()
    } else {
        reference_assisted(task.task_type, submitted.assisted, served.hints_given.len())
    };
    let session = scratch.session.clone();
    let graded = Graded {
        grade: &grade,
        error_tags: &error_tags,
        secs,
        kind,
        assisted,
    };
    let (attempt, _stash) = build_attempt(
        &task,
        &served,
        &submitted,
        &graded,
        session.as_deref(),
        now,
        attempt_index(&events, &task.task_id),
    )?;

    let recorded = attempt;
    let attempt_id = recorded.attempt_id.clone();
    feedback::update_practice(&mut scratch, &task_id, &served, &recorded);

    // Step 6. One INSERT. Zero rows back means the attempt already stands, so
    // the fold, the advance and the state write are all skipped and the whole
    // transaction rolls back with nothing appended (FR-14).
    let event = Event::Attempt(recorded.clone());
    let appended = store(
        &state,
        append_event(&mut tx, user_id, &event, Some(&attempt_id)),
    )
    .await?;
    let about = Pending {
        cfg: &content.cfg,
        served: &served,
        kind,
        miss: Miss {
            attempt_id: &attempt_id,
            session: session.as_deref(),
            task_id: &task_id,
            answer: &submitted.answer,
            work: submitted.work.as_deref(),
            correct: grade.correct,
            ungraded: grade.outcome.is_ungraded(),
        },
        write: true,
    };
    if appended.is_none() {
        let standing = stored_attempt(&events, &attempt_id).unwrap_or(&recorded);
        return already_recorded(&state, tx, user_id, &mut scratch, standing, about).await;
    }

    // Step 7. The lesson advance, its close event, and its remediation.
    let moved =
        advance_and_fold(&state, content, &mut tx, user_id, &task, &recorded, &events).await?;

    // Step 8 and step 10: move the task on, draw the next problem, write the row.
    if task.task_type == TaskType::Quiz && !recorded.feedback_practice {
        let closed = task_moved_on(
            progress_for(&mut scratch, &task, graph),
            task.task_type,
            &moved,
        );
        return quiz_receipt(
            &state, tx, user_id, scratch, &task, &served, &recorded, closed,
        )
        .await;
    }
    // Step 9. The pre-authored lookup and, on a miss with none, the enqueue.
    // Both run inside THIS transaction (spec section 4.3, D-M5-1).
    let diagnosis = diagnosis::decide(&state, &mut tx, user_id, &mut scratch, &about).await?;
    let pending_practice = scratch.feedback_practice.contains_key(&task_id);
    let progress = progress_for(&mut scratch, &task, graph);
    let closed = practice_progress(
        progress,
        task.task_type,
        &moved,
        &recorded,
        pending_practice,
    );
    let next = next_problem(
        &state,
        content,
        &mut tx,
        user_id,
        &task,
        &mut scratch,
        now,
        closed,
    )
    .await;
    save_and_commit(&state, tx, user_id, &scratch).await?;
    Ok(Json(reply(
        &recorded, &moved, &served, next, closed, diagnosis,
    )))
}

/// The reply of a request whose attempt already stands (spec section 4.3
/// step 6): the state READ, and a rollback that appends nothing.
///
/// The verdict this request graded is NOT what the log holds, so the log's own
/// verdict is what the client reads. `standing` falls back to the graded
/// attempt only when the standing row is outside this transaction's read, which
/// the advisory lock rules out.
///
/// The replay reads the pre-authored answer and the job id the FIRST request
/// wrote, and writes neither. A retried request therefore names one job, not
/// two, and the rollback leaves the queue as it was.
async fn already_recorded(
    state: &AppState,
    mut tx: Transaction<'static, Postgres>,
    user_id: Uuid,
    scratch: &mut WebState,
    standing: &Attempt,
    about: Pending<'_>,
) -> Result<Json<Value>, ApiError> {
    let replay = Pending {
        write: false,
        miss: Miss {
            correct: standing.correct,
            ungraded: standing.outcome.is_ungraded(),
            ..about.miss
        },
        ..about
    };
    let replayed = diagnosis::decide(state, &mut tx, user_id, scratch, &replay).await?;
    let mut body = outcome_fields(standing);
    body.insert("attempt_id".to_string(), json!(standing.attempt_id));
    body.insert("work_quality".to_string(), json!(standing.work_quality));
    body.insert("error_tags".to_string(), json!(standing.error_tags));
    body.insert("secs".to_string(), json!(standing.secs.get()));
    body.insert("task_status".to_string(), json!(STATUS_ALREADY_RECORDED));
    body.insert("remediation".to_string(), json!(Vec::<Value>::new()));
    body.insert("next".to_string(), Value::Null);
    body.insert("diagnosis".to_string(), replayed);
    tx.rollback()
        .await
        .map(|()| Json(Value::Object(body)))
        .map_err(db_failed)
}

/// Step 7: the lesson advance, the close events it appends, and the fold.
///
/// The repeat-fail peel-back reads HISTORY, not the open session (V2), so the
/// advance takes the whole-log session view beside the session window.
async fn advance_and_fold(
    state: &AppState,
    content: &Content,
    tx: &mut Transaction<'static, Postgres>,
    user_id: Uuid,
    task: &Task,
    recorded: &Attempt,
    events: &[EventRow],
) -> Result<Advance, ApiError> {
    let now = recorded.ts;
    let history = store(state, load_session_view(tx, user_id)).await?;
    let moved = if task.task_type == TaskType::Review {
        review::close_review(task, recorded, events, &content.cfg)
    } else if task.task_type == TaskType::Quiz {
        quiz::close_quiz(task, recorded, events, &content.cfg)
    } else {
        advance(
            &content.curriculum,
            &content.cfg,
            now,
            recorded,
            events,
            &history,
        )
    };
    for extra in moved.events() {
        store(state, append_event(tx, user_id, &extra, None)).await?;
    }
    // f19-retention: the attempt on a probe task is the delayed measurement, so it
    // writes its own event with the provenance (D-F11). A plain task writes none.
    if let Some(probe) = crate::report::probe::probe_event(&content.cfg, recorded, events, now) {
        store(state, append_event(tx, user_id, &probe, None)).await?;
    }
    let input = projection_input(content, now);
    store(state, project_and_save(tx, user_id, &input, None)).await?;
    Ok(moved)
}

/// The bare receipt of a quiz answer (trap W7).
///
/// A quiz reveals NOTHING until its batch reveal: no verdict, no solution, and
/// no expected answer in any pre-reveal body. The answer goes into the buffer
/// the close reads, and the buffer outlives the close, because the close unit
/// builds the reveal from it (`_quiz_answer`, `api.py:1748-1793`).
#[expect(
    clippy::too_many_arguments,
    reason = "the receipt reads the task, the problem, the attempt and the transaction"
)]
async fn quiz_receipt(
    state: &AppState,
    tx: Transaction<'static, Postgres>,
    user_id: Uuid,
    mut scratch: WebState,
    task: &Task,
    served: &ServedProblem,
    recorded: &Attempt,
    closed: bool,
) -> Result<Json<Value>, ApiError> {
    buffer_quiz_answer(&mut scratch, &task.task_id, served, recorded);
    scratch.served.remove(&task.task_id);
    let answered = scratch.plan_progress(&task.task_id).0;
    let receipt = json!({
        "accepted": true,
        "remaining": task.n_problems.unwrap_or(answered).saturating_sub(answered).max(0),
        "quiz_complete": closed,
    });
    save_and_commit(state, tx, user_id, &scratch)
        .await
        .map(|()| Json(receipt))
}

/// Step 8: the next problem of an open task, or nothing for a closed one.
///
/// A closed task drops its whole scratch. An open task drops its answered
/// problem and draws the next one.
///
/// Trap W6: the attempt is already recorded, so a failed draw is reported,
/// never raised. A bare `next: null` on an open task reads as "task over"
/// (trap W5), so the caller says `next_unavailable` instead.
#[expect(
    clippy::too_many_arguments,
    reason = "the draw reads the content, the task, the scratch and the transaction"
)]
async fn next_problem(
    state: &AppState,
    content: &Content,
    tx: &mut Transaction<'static, Postgres>,
    user_id: Uuid,
    task: &Task,
    scratch: &mut WebState,
    now: Timestamp,
    closed: bool,
) -> Option<Value> {
    if closed {
        clear_task_scratch(scratch, &task.task_id);
        return None;
    }
    scratch.served.remove(&task.task_id);
    install_next(
        state,
        content,
        tx,
        user_id,
        task,
        scratch,
        unix_seconds(now.micros()),
    )
    .await
    .map_err(|err| {
        tracing::warn!(task_id = %task.task_id, code = %err.code, "answer: no next problem");
    })
    .ok()
}
