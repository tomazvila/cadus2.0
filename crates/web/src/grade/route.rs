//! `POST /api/task/{task_id}/answer`: the ten steps of one transaction, in
//! the order of spec section 4.3.

use super::*;

/// Run one store call under the client-side query bound (L1, R4), and map its
/// failure onto the envelope.
async fn store<T>(
    state: &AppState,
    call: impl Future<Output = Result<T, StoreError>>,
) -> Result<T, ApiError> {
    bound(&state.db, call).await.map_err(store_failed)
}

/// The envelope of a store failure.
fn store_failed(err: StoreError) -> ApiError {
    failed(&err)
}

/// The envelope of a commit or a rollback that failed.
fn db_failed(err: sqlx::Error) -> ApiError {
    failed(&err.into())
}

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

/// The answer kind the checker decides, or the refusal of one it never decides.
///
/// T6, spec section 7: the `undecidable` decision of the counter is taken
/// here. This service asks no model for a verdict, so the kind ends here.
fn graded_kind(state: &AppState, served: &ServedProblem) -> Result<AnswerKind, ApiError> {
    let Some(kind) = answer_kind(served) else {
        return Err(broken_state("the served problem names no answer kind"));
    };
    if matches!(kind, AnswerKind::Numeric | AnswerKind::Expression) {
        return Ok(kind);
    }
    state.metrics.count_grade(metrics::GRADE_UNDECIDABLE);
    Err(ApiError::new(
        StatusCode::CONFLICT,
        UNDECIDABLE_KIND,
        "This answer kind has no deterministic verdict, and this service never asks a model \
         for one.",
    ))
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
    let kind = graded_kind(&state, &served)?;
    let grade = deterministic_grade(&served.expected.answer, &submitted.answer, kind);
    // T6, spec section 7: one count per grade DECISION, taken with no model call.
    state.metrics.count_grade(metrics::grade_result(&grade));
    let mut error_tags = grade.error_tags.clone();
    error_tags.extend(timing_tags);
    scratch.active_secs += elapsed_of(secs);

    // H3, section 5.4. `open` bound the row to the open session, so the session
    // stands; the field is carried as the log spells it.
    let assisted = reference_assisted(task.task_type, submitted.assisted, served.hints_given.len());
    let session = scratch.session.clone();
    let graded = Graded {
        grade: &grade,
        error_tags: &error_tags,
        secs,
        kind,
        assisted,
    };
    let (attempt, stash) = build_attempt(
        &task,
        &served,
        &submitted,
        &graded,
        session.as_deref(),
        now,
        attempt_index(&events, &task.task_id),
    )?;

    // H3 first branch: an assisted attempt that grades CORRECT is NOT recorded.
    // It is stashed, the problem stays live, and the next submission is the
    // unaided re-solve (`api.py:1520-1532`).
    if stash_required(assisted, &grade, &served) {
        scratch
            .served
            .entry(task_id)
            .and_modify(|live| live.rework = Some(stash));
        return save_and_commit(&state, tx, user_id, &scratch)
            .await
            .map(|()| Json(rework_reply(&served)));
    }

    let (recorded, attempt_id) = recorded_attempt(&served, attempt, &grade, now, session.clone())?;

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
        },
        write: true,
    };
    if appended.is_none() {
        let standing = stored_attempt(&events, &attempt_id).unwrap_or(&recorded);
        return already_recorded(&state, tx, user_id, &mut scratch, standing, about).await;
    }

    // Step 7. The lesson advance, its close event, and its remediation.
    let moved =
        advance_and_fold(&state, content, &mut tx, user_id, now, &recorded, &events).await?;

    // Step 8 and step 10: move the task on, draw the next problem, write the row.
    if task.task_type == TaskType::Quiz {
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
    let closed = task_moved_on(
        progress_for(&mut scratch, &task, graph),
        task.task_type,
        &moved,
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

/// H3 first branch: an assisted attempt that grades CORRECT is stashed and not
/// recorded, unless a stash already stands and this submission is its re-solve.
fn stash_required(assisted: bool, grade: &Grade, served: &ServedProblem) -> bool {
    assisted && grade.correct && served.rework.is_none()
}

/// The attempt the log records, and its id.
///
/// H3 second branch: when a stash stands, this submission IS the unaided
/// re-solve. The STASHED attempt is what gets recorded, under `-rework`; a
/// failed re-solve rewrites it to a miss and drops its `assisted` flag, so the
/// assisted pass does not stand (`api.py:1373-1375`). With no stash, the
/// attempt of this submission is recorded as built.
fn recorded_attempt(
    served: &ServedProblem,
    attempt: Attempt,
    grade: &Grade,
    now: Timestamp,
    session: Option<String>,
) -> Result<(Attempt, String), ApiError> {
    let Some(stash) = &served.rework else {
        let id = attempt.attempt_id.clone();
        return Ok((attempt, id));
    };
    let mut stashed: Attempt = serde_json::from_value(stash.clone())
        .map_err(|err| broken_state(&format!("the stashed attempt did not read: {err}")))?;
    stashed.ts = now;
    stashed.session = session;
    stashed.attempt_id = format!("{}-rework", attempt.attempt_id);
    if !grade.correct {
        stashed.correct = false;
        stashed.assisted = false;
    }
    let id = stashed.attempt_id.clone();
    Ok((stashed, id))
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
            ..about.miss
        },
        ..about
    };
    let replayed = diagnosis::decide(state, &mut tx, user_id, scratch, &replay).await?;
    let body = json!({
        "attempt_id": standing.attempt_id,
        "correct": standing.correct,
        "work_quality": standing.work_quality,
        "error_tags": standing.error_tags,
        "secs": standing.secs.get(),
        "task_status": STATUS_ALREADY_RECORDED,
        "remediation": Vec::<Value>::new(),
        "next": Value::Null,
        "diagnosis": replayed,
    });
    tx.rollback().await.map(|()| Json(body)).map_err(db_failed)
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
    now: Timestamp,
    recorded: &Attempt,
    events: &[EventRow],
) -> Result<Advance, ApiError> {
    let history = store(state, load_session_view(tx, user_id)).await?;
    let moved = advance(
        &content.curriculum,
        &content.cfg,
        now,
        recorded,
        events,
        &history,
    );
    for extra in moved.events() {
        store(state, append_event(tx, user_id, &extra, None)).await?;
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
