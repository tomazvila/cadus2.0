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
pub(super) async fn save_and_commit(
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
pub async fn answer(request: TaskWithBody) -> Result<Json<Value>, ApiError> {
    let (state, user_id, task_id, raw, now) = task_request(request);
    let (content, body) = route_input(&state, raw.as_ref())?;
    let submitted = submission(body)?;
    let graph = &content.curriculum;

    let Open {
        mut tx,
        events,
        mut scratch,
        plan,
        readiness,
    } = open(&state, content, user_id, now, true).await?;
    let task = find(&plan, &task_id)?.clone();
    // The answer path MAY install the progress row (`_validate`, `api.py:1303`).
    // The plan route may not: trap W3.
    progress_for(&mut scratch, &task, graph);
    // The section 4.2 re-check, in its two refusals: a closed task is
    // `409 task_complete`, a superseded id is `404 unknown_problem`.
    let mut served = scratch.validate(&task_id, &submitted.problem_id)?.clone();
    let verified_answer =
        crate::problem_reports::apply(&state, content, &mut tx, &mut served, &submitted.answer)
            .await?;

    // The clock is measured BEFORE the grade, so no grading work inflates it.
    let (secs, timing_tags) = measure_secs(
        served.started_at,
        now.micros(),
        expected_time(graph, &served),
    );
    let kind = served_kind(&served)?;
    let mut grade = if verified_answer {
        Grade {
            correct: true,
            outcome: AttemptOutcome::Correct,
            work_quality: WorkQuality::NearlyPerfect,
            error_tags: Vec::new(),
        }
    } else {
        grade_served_item(&served, &submitted.answer, kind)
    };
    // Amendment K (note 114, the owner's design): a deterministic WRONG or an
    // unparseable answer is checked against the owner's local model, through
    // the (item digest, normalized text) cache. A cache hit answers at once —
    // EQUIVALENT counts as correct for the topic and for mastery, a NOT keeps
    // the wrong verdict and carries the model's one-line reason. A miss
    // enqueues the background check below and the deterministic verdict
    // stands until the worker lands. The web tier never calls a model (L6).
    // Amendment K point 6: an ungraded written proof (no checkable key) goes
    // to the background proof grader instead of the equivalence check: there
    // is no key to be equivalent to.
    let written_proof = !verified_answer
        && grade.outcome.is_ungraded()
        && proof_grading::is_written_proof(&served, kind);
    // D-PR1: a written PROOF inside a lesson runs the revision loop. The step
    // is decided BEFORE anything is appended, so a draft the chain refuses
    // (still grading, already passed, the solution not yet shown) records
    // nothing.
    let proof_step = if written_proof
        && task.task_type == TaskType::Lesson
        && served.rework.is_none()
        && proof_grading::is_proof_item(&served)
    {
        Some(proof_grading::lesson::lesson_step(&state, &mut tx, &served, &submitted.answer).await?)
    } else {
        None
    };
    // A REVISION of a lesson proof is recorded on its chain alone: the first
    // draft is the problem's one scheduling signal, so no attempt event and
    // no fold follow a later draft (D-PR1).
    if let Some(step) = proof_step
        .as_ref()
        .filter(|step| step.job.revision_of.is_some())
    {
        let revision = super::proof::Revision {
            task: &task,
            served: &served,
            answer: &submitted.answer,
            step: step.clone(),
            now,
        };
        return super::proof::revise_in_lesson(
            &state, content, tx, user_id, scratch, &events, &readiness, revision,
        )
        .await;
    }
    // A property item has no key to be equivalent to: its stored answer is
    // one example, and the exact predicate already decided the learner's own
    // object. A decided miss is final, and an unreadable answer stays
    // ungraded with its format guidance; neither goes to the model.
    let property_item = matches!(
        served.expected.answer_contract,
        Some(cadus_core::answer::AnswerContract::Property { .. })
    );
    let mut equivalence_hit = None;
    if !verified_answer && !grade.correct && !written_proof && !property_item {
        match equivalence::lookup(&state, &mut tx, &served, &submitted.answer).await {
            Ok(Some(equivalence::Cached::Accepted(verdict))) => {
                grade = Grade {
                    correct: true,
                    outcome: AttemptOutcome::Correct,
                    work_quality: WorkQuality::NearlyPerfect,
                    error_tags: Vec::new(),
                };
                equivalence_hit = Some(equivalence::Cached::Accepted(verdict));
            }
            Ok(hit @ Some(equivalence::Cached::Refused(_))) | Ok(hit @ None) => {
                equivalence_hit = hit
            }
            // A failed cache read never stops the grade: the deterministic
            // verdict stands, the answer simply goes unchecked.
            Err(_) => tracing::warn!("equivalence: the cache lookup failed"),
        }
    }
    // T6, spec section 7: one count per grade DECISION, taken with no model call.
    state.metrics.count_grade(metrics::grade_result(&grade));
    let mut error_tags = grade.error_tags.clone();
    error_tags.extend(timing_tags.iter().cloned());
    scratch.active_secs += elapsed_of(secs);

    // H3, section 5.4. `open` bound the row to the open session, so the session
    // stands; the field is carried as the log spells it.
    let assisted = if served.rework.is_some() {
        submitted.assisted || !served.hints_given.is_empty()
    } else {
        reference_assisted(task.task_type, submitted.assisted, served.hints_given.len())
    };
    let session = scratch.session.clone();
    let timing = timing::reading(graph, &task, &served, &grade, secs, assisted, &timing_tags);
    let graded = Graded {
        timing,
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
    let moved = advance_and_fold(
        &state,
        content,
        &mut tx,
        user_id,
        &task,
        &recorded,
        &events,
        &readiness,
        proof_step.is_some(),
    )
    .await?;
    let proof_pending = moved.status == STATUS_PROOF_PENDING;

    // Step 8 and step 10: move the task on, draw the next problem, write the row.
    if task.task_type == TaskType::Quiz && !recorded.feedback_practice {
        let closed = task_moved_on(
            progress_for(&mut scratch, &task, graph),
            task.task_type,
            &moved,
        );
        // A written proof inside a quiz is graded too, but only once the
        // quiz is revealed: its payload waits in the reveal buffer, and the
        // quiz-result route enqueues it (nothing is graded or shown while the
        // quiz is open).
        let proof = written_proof.then(|| proof_grading::payload(&served, &submitted.answer));
        return quiz_receipt(
            &state, tx, user_id, scratch, &task, &served, &recorded, closed, proof,
        )
        .await;
    }
    // Step 9. The pre-authored lookup and, on a miss with none, the enqueue.
    // Both run inside THIS transaction (spec section 4.3, D-M5-1).
    let diagnosis = diagnosis::decide(&state, &mut tx, user_id, &mut scratch, &about).await?;
    // Amendment K: on a wrong or unparseable answer with no cached verdict,
    // the background check is enqueued in the same transaction. A miss that
    // was flipped by the cache carries no job (the verdict is already here).
    let equivalence_job = if grade.correct || written_proof || property_item {
        None
    } else {
        match equivalence_hit {
            Some(equivalence::Cached::Refused(_)) | None => {
                equivalence::enqueue(
                    &state,
                    &mut tx,
                    user_id,
                    &attempt_id,
                    &served,
                    &submitted.answer,
                )
                .await?
            }
            Some(equivalence::Cached::Accepted(_)) => None,
        }
    };
    let equivalence_field = equivalence::reply_field(
        equivalence_hit.as_ref(),
        equivalence_job,
        &served,
        &submitted.answer,
    );
    // Amendment K point 6: the background proof grading, enqueued in the same
    // transaction. No model call here (L6); the verdict lands later.
    let proof_job = if written_proof {
        let job = proof_step.as_ref().map_or_else(
            || cadus_store::proof_grading::NewJob::first(proof_grading::context_of(&served)),
            |step| step.job,
        );
        let id = proof_grading::enqueue(
            &state,
            &mut tx,
            user_id,
            &attempt_id,
            &served,
            &submitted.answer,
            &job,
        )
        .await?;
        // The first draft opens the chain: the proof is no longer merely owed.
        if proof_step.is_some() {
            store(
                &state,
                cadus_store::proof_grading::settle_owed(
                    &mut *tx,
                    served.serving_topic().unwrap_or_default(),
                    served.kp.as_deref().unwrap_or_default(),
                    &proof_grading::lesson::item_key(&served.text),
                ),
            )
            .await?;
        }
        id
    } else {
        None
    };
    let pending_practice = scratch.feedback_practice.contains_key(&task_id);
    let progress = progress_for(&mut scratch, &task, graph);
    let closed = practice_progress(
        progress,
        task.task_type,
        &moved,
        &recorded,
        pending_practice,
    );
    // The mixed review block: the next question may belong to ANOTHER review of
    // the block. This review keeps its own attempts and its own result; only
    // the order of the questions moves.
    let handed_on = crate::serve::mixed::after_answer(&plan, &scratch, &task);
    let next = match handed_on.as_deref() {
        // A proof under grading keeps its problem live: the revision answers
        // the SAME problem, and nothing new is drawn (D-PR1).
        _ if proof_pending => None,
        Some(other) => {
            mixed_hand_on(
                &state,
                content,
                &mut tx,
                user_id,
                (&plan, &task, other, &events),
                &mut scratch,
                &readiness,
                now,
                closed,
            )
            .await?
        }
        None => {
            next_problem(
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
            .await
        }
    };
    let next = next.map(|mut payload| {
        let shown = payload["task_id"].as_str().unwrap_or(&task_id).to_owned();
        crate::serve::mixed::stamp(&mut payload, &plan, &scratch, &shown);
        payload
    });
    let hand_on_failed = handed_on.is_some() && next.is_none();
    save_and_commit(&state, tx, user_id, &scratch).await?;
    let mut body = reply(
        &recorded,
        &moved,
        &served,
        next,
        closed,
        diagnosis,
        equivalence_field,
        proof_grading::reply_field(proof_job),
    );
    if let Some(text) = notation_text(&served.expected, &submitted.answer, kind, &grade) {
        body["notation"] = json!(text);
    }
    // A failed hand-off leaves a question of the block owed, even when this
    // review closed: the client re-serves, and the serve route finds it.
    if hand_on_failed {
        body["next_unavailable"] = json!(true);
    }
    if proof_step.is_some() {
        if let Some(map) = body.as_object_mut() {
            map.remove("next_unavailable");
        }
        body["proof"] = super::proof::pending_field(proof_job, 0);
    }
    Ok(Json(body))
}

/// Step 8 of a mixed review block: close or park the answered review, then
/// draw the next question of the review `other` the interleaving rule picked.
///
/// The hand-off is that review's serve, so the first one appends its
/// `task_served` exactly as the serve route does (D-M5-8), and folds in the
/// same transaction. The payload carries `task_id`, which the caller's stamp
/// keeps. A failed draw is reported, never raised, as in [`next_problem`].
#[expect(
    clippy::too_many_arguments,
    reason = "the draw reads the content, the tasks, the scratch and the transaction"
)]
async fn mixed_hand_on(
    state: &AppState,
    content: &Content,
    tx: &mut Transaction<'static, Postgres>,
    user_id: Uuid,
    (plan, task, other, events): (&cadus_core::selector::SessionPlan, &Task, &str, &[EventRow]),
    scratch: &mut WebState,
    readiness: &ReadinessSet,
    now: Timestamp,
    closed: bool,
) -> Result<Option<Value>, ApiError> {
    if closed {
        clear_task_scratch(scratch, &task.task_id);
    } else {
        scratch.served.remove(&task.task_id);
    }
    let Some(next_task) = plan.tasks.iter().find(|item| item.task_id == other) else {
        return Ok(None);
    };
    let drawn = install_next(
        state, content, tx, user_id, next_task, scratch, readiness, now,
    )
    .await;
    let mut payload = match drawn {
        Ok(payload) => payload,
        Err(err) => {
            tracing::warn!(task_id = %other, code = %err.code, "answer: no next mixed review problem");
            return Ok(None);
        }
    };
    let appended =
        record_first_serve(state, tx, user_id, next_task, &plan.session, events, now).await?;
    if appended {
        let input = projection_input(content, now);
        store(state, project_and_save(tx, user_id, &input, None)).await?;
    }
    payload["task_id"] = json!(other);
    Ok(Some(payload))
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
#[expect(
    clippy::too_many_arguments,
    reason = "the advance reads the state, the content, the attempt and the whole-log view"
)]
pub(super) async fn advance_and_fold(
    state: &AppState,
    content: &Content,
    tx: &mut Transaction<'static, Postgres>,
    user_id: Uuid,
    task: &Task,
    recorded: &Attempt,
    events: &[EventRow],
    readiness: &ReadinessSet,
    lesson_proof: bool,
) -> Result<Advance, ApiError> {
    let now = recorded.ts;
    let history = store(state, load_session_view(tx, user_id)).await?;
    let mut moved = if lesson_proof {
        // D-PR1: the first draft of a lesson proof waits for its verdict.
        Advance {
            status: STATUS_PROOF_PENDING,
            ..Advance::carry_on()
        }
    } else if task.task_type == TaskType::Review {
        review::close_review(task, recorded, events, &content.cfg)
    } else if task.task_type == TaskType::Quiz {
        quiz::close_quiz(task, recorded, events, &content.cfg)
    } else if task.task_type == TaskType::Drill {
        drill::close_drill(task, recorded, events)
    } else {
        let moved = advance(
            &content.curriculum,
            &content.cfg,
            now,
            recorded,
            events,
            &history,
            Some(readiness),
        );
        // D-PR1: a proof-gated point serves its written proof before it closes.
        hold_for_proof(&content.curriculum, recorded, moved)
    };
    if matches!(
        moved.result.as_ref(),
        Some(Event::LessonResult(result)) if result.passed
    ) {
        let score = super::proof_score::completed(state, tx, recorded.topic.as_str()).await?;
        moved = super::proof_score::apply(&content.curriculum, &content.cfg, moved, score);
    }
    for extra in moved.events() {
        store(state, append_event(tx, user_id, &extra, None)).await?;
    }
    // D-PR1: the proof the held point owes outlives the D-S6 row.
    if let Some(kp) = moved.proof_due.as_deref() {
        let topic = recorded.topic.as_str();
        for item in crate::proof_grading::lesson::written_items(&content.curriculum, topic, kp) {
            let key = crate::proof_grading::lesson::item_key(&item.problem);
            store(
                state,
                cadus_store::proof_grading::owe(&mut **tx, user_id, topic, kp, &key),
            )
            .await?;
        }
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
    proof: Option<cadus_store::proof_grading::JobPayload>,
) -> Result<Json<Value>, ApiError> {
    buffer_quiz_answer(&mut scratch, &task.task_id, served, recorded, proof);
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
pub(super) async fn next_problem(
    state: &AppState,
    content: &Content,
    tx: &mut Transaction<'static, Postgres>,
    user_id: Uuid,
    task: &Task,
    scratch: &mut WebState,
    readiness: &ReadinessSet,
    now: Timestamp,
    closed: bool,
) -> Option<Value> {
    if closed {
        clear_task_scratch(scratch, &task.task_id);
        return None;
    }
    scratch.served.remove(&task.task_id);
    install_next(state, content, tx, user_id, task, scratch, readiness, now)
        .await
        .map_err(|err| {
            tracing::warn!(task_id = %task.task_id, code = %err.code, "answer: no next problem");
        })
        .ok()
}
