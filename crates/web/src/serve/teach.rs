//! `POST /api/task/{task_id}/teach`: approved preparation for lessons and integrated application.

use super::choices::label_choices;
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
    let lesson = lesson_point(&scratch, task, graph, &topic, &task_id)?;
    let page = teach_page(&state, content, &mut tx, &lesson.key).await?;
    // The transaction read one row and wrote nothing, so the drop rolls it back
    // and the read needs no second round trip.
    drop(tx);

    let mut reply = json!({
        "kp": lesson.kp,
        "concept": page.concept,
        "worked_example": {
            "problem": page.worked_example.problem,
            "steps": page.worked_example.steps,
        },
    });
    // The try-first block carries what the learner needs to act and nothing
    // that grades the act: `answer` and `reveal` stay on the server until the
    // check route (Hard Rule 1).
    if let Some(point) = lesson.point(graph)
        && let Some(first) = &point.try_first
    {
        let mut block = json!({"problem": first.problem});
        if let Some(choices) = label_choices(Some(&first.answer_contract), &task_id) {
            block["choices"] = json!(choices);
        }
        reply["try_first"] = block;
    }
    Ok(Json(reply))
}

/// `POST /api/task/{task_id}/teach/check`: grade one active-example act.
///
/// The body is `{"part": "try_first", "answer": "..."}`. The reply carries the
/// verdict and the material the teach payload withheld: the authored answer
/// and `reveal`. Any other `part` is refused; `step_check` is `400`.
///
/// The route writes nothing: no event, no D-S6 row, no XP. The acts are
/// formative, so the fold, the lesson pass rule, and the schedule never see
/// them, and a learner may check as often as the page allows.
pub async fn teach_check(
    ((State(state), Tenant(user_id), ApiPath(task_id)), body): crate::path::TaskWithBody,
) -> Result<Json<Value>, ApiError> {
    let content = content(&state)?;
    let act = teach_act(body.as_ref().map(|Json(value)| value))?;
    let (_, now) = now_pair();
    let graph = &content.curriculum;

    let Open {
        tx, scratch, plan, ..
    } = open(&state, content, user_id, now, false).await?;
    let task = find(&plan, &task_id)?;
    let (TaskType::Lesson, Some(topic)) = (task.task_type, task.topic.clone()) else {
        return Err(no_active_example());
    };
    let lesson = lesson_point(&scratch, task, graph, &topic, &task_id)?;
    let point = lesson.point(graph).ok_or_else(no_active_example)?;
    let reply = match act {
        TeachAct::TryFirst(answer) => {
            let first = point.try_first.as_ref().ok_or_else(no_active_example)?;
            let expected = cadus_core::pool::PoolAnswer {
                v: cadus_core::pool::POOL_ROW_VERSION,
                answer_contract: Some(first.answer_contract.clone()),
                answer: first.answer.clone(),
            };
            let topic_kind = graph
                .idx_of(&topic)
                .and_then(|idx| graph.topic(idx))
                .map(|found| found.answer_kind)
                .ok_or_else(no_active_example)?;
            let grade = crate::grade::grade_item(&expected, &answer, topic_kind);
            json!({
                "part": "try_first",
                "outcome": grade.outcome.as_str(),
                "correct": grade.correct,
                "answer": first.answer,
                "reveal": first.reveal,
            })
        }
    };
    drop(tx);
    Ok(Json(reply))
}

/// The current knowledge point of a lesson task.
struct LessonPoint {
    topic: String,
    kp: String,
    key: String,
}

impl LessonPoint {
    /// The authored knowledge point, when the curriculum still holds it.
    fn point<'g>(&self, graph: &'g Curriculum) -> Option<&'g KnowledgePoint> {
        let idx = graph.idx_of(&self.topic)?;
        graph
            .knowledge_points(idx)
            .iter()
            .find(|point| point.id.as_str() == self.kp)
    }
}

/// The knowledge point a lesson teaches now, or `409 no_instruction`.
fn lesson_point(
    scratch: &WebState,
    task: &Task,
    graph: &Curriculum,
    topic: &str,
    task_id: &str,
) -> Result<LessonPoint, ApiError> {
    let current = scratch
        .tasks
        .get(task_id)
        .and_then(|progress| progress.current_kp.as_deref());
    let kp = lesson_kp(current, task, graph, topic).ok_or_else(no_instruction)?;
    let key = kp_key(topic, &kp);
    Ok(LessonPoint {
        topic: topic.to_owned(),
        kp,
        key,
    })
}

/// The current approved teach page of `key`, or `409 no_instruction`.
async fn teach_page(
    state: &AppState,
    content: &Content,
    tx: &mut Transaction<'static, Postgres>,
    key: &str,
) -> Result<TeachDoc, ApiError> {
    let policy = content.policy_digest(key)?;
    let doc = store(
        state,
        cadus_store::content::approved_document_current(
            &mut **tx,
            key,
            KIND_TEACH,
            content.review_context(policy.as_deref())?,
        ),
    )
    .await?;
    read_document(
        doc.ok_or_else(no_instruction)?,
        "teach",
        "The authored teach page is not readable.",
    )
}

/// One learner act on an active example.
enum TeachAct {
    /// The learner's attempt at the try-first problem.
    TryFirst(String),
}

/// Read the act from the request body, or `422 invalid_request`.
fn teach_act(body: Option<&Value>) -> Result<TeachAct, ApiError> {
    let text = |field: &str| {
        body.and_then(|value| value.get(field))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    match body
        .and_then(|value| value.get("part"))
        .and_then(Value::as_str)
    {
        Some("step_check") => Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            INVALID_REQUEST,
            "Lessons no longer have step questions; part must be try_first.",
        )),
        Some("try_first") => text("answer")
            .filter(|answer| answer.chars().count() <= cadus_core::answer::MAX_ANSWER_CHARS)
            .map(TeachAct::TryFirst)
            .ok_or_else(|| invalid_act("try_first requires an answer.")),
        _ => Err(invalid_act("part must be try_first.")),
    }
}

/// `422 invalid_request` with `message`.
fn invalid_act(message: &'static str) -> ApiError {
    ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, INVALID_REQUEST, message)
}

/// `409 no_active_example`: this lesson's knowledge point authors no such block.
fn no_active_example() -> ApiError {
    conflict(
        NO_ACTIVE_EXAMPLE,
        "This lesson's knowledge point has no such active example.",
    )
}
