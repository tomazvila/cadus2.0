//! `POST /api/task/{task_id}/hint`: one rung of the authored hint ladder (L5).

use super::*;

/// One rung of the authored hint ladder (`api.py:1800-1865`).
///
/// A quiz reveals nothing until the batch reveal, so a hint inside one is
/// `409 no_hints_in_quiz`. Every other task type is allowed a hint: taking one
/// is recorded on `served.hints_given`, which flags the attempt
/// reference-assisted (H3), and does not force a miss.
///
/// The ladder comes from `content_store` (L5, T1). The reply carries the hint
/// text and nothing else from the served problem: `expected` never leaves the
/// D-S6 row (Hard Rule 1, trap W7).
pub async fn hint(
    State(state): State<AppState>,
    Tenant(user_id): Tenant,
    ApiPath(task_id): ApiPath<String>,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, ApiError> {
    let (content, body) = route_input(&state, body.as_ref())?;
    let problem_id = problem_id_of(body)?;
    let (_, now) = now_pair();

    let Open {
        mut tx,
        mut scratch,
        plan,
        ..
    } = open(&state, content, user_id, now, true).await?;
    let task = find(&plan, &task_id)?;
    if task.task_type == TaskType::Quiz {
        return Err(conflict(
            NO_HINTS_IN_QUIZ,
            "Hints are not available during a quiz.",
        ));
    }
    // The section 4.2 re-check, in its two refusals: a closed task is
    // `409 task_complete` and a superseded id is `404 unknown_problem`.
    let served = scratch.validate(&task_id, &problem_id)?;
    let (topic, key, rung) = ladder_key(served)?;

    let doc = store(&state, approved_document(&mut *tx, &key, KIND_HINT_LADDER)).await?;
    let ladder: HintLadder = read_document(
        doc.ok_or_else(no_ladder)?,
        "hint",
        "The authored hint ladder is not readable.",
    )?;
    // A ladder that runs out repeats its last rung. The learner keeps the
    // reference-lesson escalation below, and no model is asked for a new one.
    let text = ladder
        .hints
        .get(rung.min(ladder.hints.len().saturating_sub(1)))
        .cloned()
        .ok_or_else(no_ladder)?;

    let hint_number = push_hint(&mut scratch, &task_id, &text).ok_or_else(no_ladder)?;
    write_state(&state.db, &mut tx, user_id, &scratch).await?;
    tx.commit().await.map_err(db_failed)?;

    let mut payload = json!({"hint": text, "hint_number": hint_number});
    // Stuck on a review, or on a multi-step part, after three hints: point the
    // learner at the reference lesson (`api.py:1856-1864`).
    if matches!(task.task_type, TaskType::Review | TaskType::MultiStep)
        && hint_number >= STUCK_HINT_THRESHOLD
    {
        payload["reference_lesson"] = reference_lesson(&content.curriculum, &topic);
    }
    Ok(Json(payload))
}

/// The `problem_id` of the request body, or the `422` of a body without one.
fn problem_id_of(body: Option<&Value>) -> Result<String, ApiError> {
    let problem_id = body
        .and_then(|value| value.get("problem_id"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if problem_id.is_empty() {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            INVALID_REQUEST,
            "hint requires a problem_id.",
        ));
    }
    Ok(problem_id.to_string())
}

/// The record topic, the serving key of the ladder, and the rung the next hint
/// takes; or the `409` of a live problem that names no knowledge point.
///
/// The ladder belongs to the knowledge point that produced the STATEMENT, so
/// the key comes from `serve_topic`. `topic` is the topic the attempt records
/// against, and for a review that micro-interleaves a component skill the two
/// name different knowledge points (M5 review 1, findings F10 and F16). The
/// reference-lesson escalation still names the RECORD topic, which is the
/// lesson the review stands for.
fn ladder_key(served: &ServedProblem) -> Result<(String, String, usize), ApiError> {
    let kp = served.kp.clone().ok_or_else(no_ladder)?;
    let topic = served.topic.clone().ok_or_else(no_ladder)?;
    // `serving_topic` gives `topic` when the D-S6 row names no serve topic, so
    // the key is always a real pair.
    let key = kp_key(served.serving_topic().unwrap_or(&topic), &kp);
    Ok((topic, key, served.hints_given.len()))
}

/// Record one taken hint on the live problem of `task_id`, and give the count
/// of hints taken back. A task with no live problem takes none.
fn push_hint(scratch: &mut WebState, task_id: &str, text: &str) -> Option<usize> {
    let live = scratch.served.get_mut(task_id)?;
    live.hints_given.push(text.to_string());
    Some(live.hints_given.len())
}

/// The reference lesson of a stuck learner: the record topic and its name.
fn reference_lesson(graph: &Curriculum, topic: &str) -> Value {
    let name = graph
        .idx_of(topic)
        .and_then(|idx| graph.topic(idx))
        .map_or(topic, |found| found.name.as_str());
    json!({"topic": topic, "name": name})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_body_without_a_problem_id_is_invalid_request() {
        assert_eq!(problem_id_of(None).unwrap_err().code, INVALID_REQUEST);
        assert_eq!(
            problem_id_of(Some(&json!({"problem_id": ""})))
                .unwrap_err()
                .code,
            INVALID_REQUEST
        );
        assert_eq!(
            problem_id_of(Some(&json!({"problem_id": "p1"}))).unwrap(),
            "p1"
        );
    }

    #[test]
    fn a_hint_on_a_task_with_no_live_problem_is_not_recorded() {
        let mut scratch = WebState::for_session("s1");
        assert_eq!(push_hint(&mut scratch, "t1", "One."), None);
    }

    #[test]
    fn the_reference_lesson_of_an_unknown_topic_keeps_the_id_as_its_name() {
        let found = reference_lesson(&super::super::fixture::graph(), "nowhere");
        assert_eq!(found["name"], "nowhere");
    }
}
