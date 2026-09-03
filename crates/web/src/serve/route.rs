//! `POST /api/task/{task_id}/serve`: one transaction, the `task_served` event,
//! and the one place that installs a served problem.

use super::*;

/// The task ids this session already served (`served_task_ids`,
/// `service.py:1078`).
///
/// `events` is the window of the OPEN SESSION, and the filter still reads the
/// session field of every row, because a log whose `session_start` names no
/// session gives a window over the WHOLE log.
fn served_task_ids(events: &[EventRow], session: &str) -> BTreeSet<String> {
    events
        .iter()
        .filter_map(|row| match &row.event {
            Event::TaskServed(body) if body.session.as_deref() == Some(session) => {
                Some(body.task_id.clone())
            }
            _ => None,
        })
        .collect()
}

/// The slug of one topic or knowledge point id, or `None` for a malformed id.
fn slug_of(id: &str) -> Option<Slug> {
    Slug::new(id).ok()
}

/// Append `task_served` the first time this session serves `task` (D-M5-8).
///
/// It is the ONE source of `SessionView::last_drill_at`, and so of the 3.5-day
/// drill cadence: no other 2.0 path writes the event (M5 review 1, finding
/// F17). 1.0 appends it at plan composition (`service.py:1302-1321`); 2.0
/// appends it here, because `GET /api/session/plan` stays a pure read (trap W3).
///
/// It is idempotent per task id: a re-serve, the next question of a running
/// task, and a second browser tab all find the id in `events` and write nothing.
/// The caller holds the advisory lock, so the read of the window and the append
/// cannot interleave with another append of this learner.
///
/// The event carries the fields 1.0 records. `problems` stays empty: 1.0 emits
/// the event before it draws a statement, and no 2.0 code reads the stubs.
///
/// The answer is `true` when the append went in. The caller folds and saves on a
/// `true`, so the log head and the fold cursor leave the transaction together
/// (M5 review 2, findings V1 and V8).
async fn record_first_serve(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    task: &Task,
    session: &str,
    events: &[EventRow],
    now: Timestamp,
) -> Result<bool, ApiError> {
    if served_task_ids(events, session).contains(&task.task_id) {
        return Ok(false);
    }
    let event = Event::TaskServed(TaskServed {
        ts: now,
        session: Some(session.to_owned()),
        v: SchemaVersion,
        task_id: task.task_id.clone(),
        task_type: task.task_type,
        // A quiz task spans many topics and names none of its own, so its event
        // carries no topic (`service.py:1307-1310`, trap D2).
        topic: task.topic.as_deref().and_then(slug_of),
        kp: task.start_at_kp.as_deref().and_then(slug_of),
        problems: Vec::new(),
        component_topics: task
            .component_topics
            .iter()
            .map(String::as_str)
            .filter_map(slug_of)
            .collect(),
        seed: None,
    });
    store(state, append_event(tx, user_id, &event, None)).await?;
    Ok(true)
}

/// Serve this task's problem, taking one from the D-S5 pool (`api.py:1014-1060`).
///
/// The whole route is ONE transaction. A problem that is already live is handed
/// straight back with a fresh `started_at` and the same `problem_id`.
pub async fn serve(
    State(state): State<AppState>,
    Tenant(user_id): Tenant,
    ApiPath(task_id): ApiPath<String>,
) -> Result<Json<Value>, ApiError> {
    let (_, now) = now_pair();
    let content = content(&state)?;
    let graph = &content.curriculum;
    let started_at = unix_seconds(now.micros());

    let Open {
        mut tx,
        mut scratch,
        plan,
        events,
    } = open(&state, content, user_id, now, true).await?;
    let task = find(&plan, &task_id)?;
    if progress_for(&mut scratch, task, graph).done {
        return Err(conflict(TASK_COMPLETE, "This task is already complete."));
    }

    // The WHOLE-quiz clock, stamped by the first serve of the quiz and read by
    // every later one (QUIZ-budget, V6). It stands before the branch below,
    // because a re-serve of a live question is exactly the reload that must
    // resume the running clock.
    let elapsed = quiz_elapsed(&mut scratch, &task_id, task.task_type, started_at);

    // A problem is already live for this task: a reload, or the problem the last
    // answer installed. Re-stamp the PER-PROBLEM clock and hand the SAME one
    // back (`_serve_live`, section 5.6).
    let payload = match scratch.served.get_mut(&task_id) {
        Some(live) => {
            live.started_at = started_at;
            serve_payload(live, task, graph, content.cfg.drill.target_secs, elapsed)
        }
        None => {
            install_next(
                &state,
                content,
                &mut tx,
                user_id,
                task,
                &mut scratch,
                started_at,
            )
            .await?
        }
    };
    // The hand-off happened, so the task is served. The event goes in once per
    // task and per session, and it is what fills the drill cadence (D-M5-8).
    let appended =
        record_first_serve(&state, &mut tx, user_id, task, &plan.session, &events, now).await?;
    // An append moves the head of the log, so the fold cursor moves with it in
    // the SAME transaction (V1, V8). A cursor one line behind takes every later
    // request of this learner out of the "nothing new" branch of
    // `project_current` and into the incremental branch, which reads the WHOLE
    // log. A serve that appends nothing leaves the cursor where it stands, so
    // the common re-serve writes no `learner_models` row at all.
    if appended {
        let input = projection_input(content, now);
        store(&state, project_and_save(&mut tx, user_id, &input, None)).await?;
    }
    write_state(&state.db, &mut tx, user_id, &scratch).await?;
    tx.commit().await.map_err(db_failed)?;
    Ok(Json(payload))
}

/// Draw the next problem of `task`, install it in the D-S6 row, and give back
/// its client-safe payload.
///
/// It is the ONE place that installs a served problem. `serve` calls it when no
/// problem is live, and the grade path of unit U8 calls it after the attempt
/// commits, so the `next` of a grade reply and a later serve cannot disagree.
///
/// The caller owns the transaction: this writes into `scratch` and into the
/// pool, and it commits nothing.
pub(crate) async fn install_next(
    state: &AppState,
    content: &Content,
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    task: &Task,
    scratch: &mut WebState,
    started_at: f64,
) -> Result<Value, ApiError> {
    let graph = &content.curriculum;
    let task_id = task.task_id.clone();
    let progress = progress_for(scratch, task, graph).clone();
    let index = next_serve_index(task.task_type, &progress);
    // The grade path installs the next quiz question through here, so the clock
    // of a quiz whose first question arrives on this path is stamped too. The
    // stamp is written ONCE per task, so this call cannot restart a clock the
    // serve route already started (V6).
    let elapsed = quiz_elapsed(scratch, &task_id, task.task_type, started_at);

    let target = target_of(task, index, &progress, graph)?;
    let (ring, memory) = (scratch.ring(&target.serve), scratch.memory(&task_id));
    let avoid = Avoid::new(&ring, &memory);
    let row = draw(state, tx, user_id, graph, &target, &avoid).await?;
    let solution_sketch = solution_of(state, tx, graph, &target, &row).await?;

    let served = ServedProblem {
        problem_id: Uuid::new_v4().simple().to_string(),
        task_id: task_id.clone(),
        topic: Some(target.record.clone()),
        serve_topic: Some(target.serve.clone()),
        kp: Some(target.kp.clone()),
        answer_kind: answer_kind_of(graph, &target.serve),
        text: row.problem.text.clone(),
        expected: row.expected_answer.clone(),
        solution_sketch,
        started_at,
        hints_given: Vec::new(),
        index,
        rework: None,
    };
    let payload = serve_payload(&served, task, graph, content.cfg.drill.target_secs, elapsed);
    scratch.record_served(&target.serve, &task_id, &row.instance_hash);
    scratch.served.insert(task_id, served);
    let row = progress_for(scratch, task, graph);
    row.served = row.served.saturating_add(1);
    Ok(payload)
}
