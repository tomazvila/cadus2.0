//! Blind quiz closure, post-batch reveal, and independent practice.
use super::*;
use cadus_core::event::{QuizResult, QuizTopicResult};

/// Close exactly the original quiz batch; supplemental attempts cannot reprice it.
pub(super) fn close_quiz(
    task: &Task,
    attempt: &Attempt,
    prior: &[EventRow],
    cfg: &Config,
) -> Advance {
    if attempt.feedback_practice {
        return Advance::carry_on();
    }
    let mut originals: Vec<&Attempt> = prior
        .iter()
        .filter_map(|row| match &row.event {
            Event::Attempt(item) if item.task_id == task.task_id && !item.feedback_practice => {
                Some(item)
            }
            _ => None,
        })
        .collect();
    originals.push(attempt);
    if i64::try_from(originals.len()).unwrap_or(i64::MAX) < task.n_problems.unwrap_or(i64::MAX) {
        return Advance::carry_on();
    }
    let decided: Vec<_> = originals
        .iter()
        .filter(|item| !item.outcome.is_ungraded())
        .collect();
    #[expect(
        clippy::cast_precision_loss,
        reason = "quiz batches contain a bounded handful of items"
    )]
    let score =
        decided.iter().filter(|item| item.correct).count() as f64 / decided.len().max(1) as f64;
    let xp = if decided.len() == originals.len() {
        task_xp(TaskType::Quiz, WorkQuality::NearlyPerfect, cfg, 0, 0, false) * score
    } else {
        0.0
    };
    Advance {
        result: Some(Event::QuizResult(QuizResult {
            inconclusive: decided.len() != originals.len(),
            ts: attempt.ts,
            session: attempt.session.clone(),
            v: SchemaVersion::current(),
            quiz_id: task.task_id.clone(),
            score,
            xp,
            per_topic: decided
                .iter()
                .map(|item| QuizTopicResult {
                    topic: item.topic.clone(),
                    correct: item.correct,
                    secs: item.secs,
                })
                .collect(),
        })),
        xp: Some(round2(xp)),
        ..Advance::carry_on()
    }
}

/// Read a completed quiz; `practice` starts the fresh-skill queue after studying.
pub async fn result(
    Tenant(user_id): Tenant,
    State(state): State<AppState>,
    ApiPath(task_id): ApiPath<String>,
    raw: Option<Json<Value>>,
) -> Result<Json<Value>, ApiError> {
    let content = content(&state)?;
    let (_, now) = now_pair();
    let Open {
        mut tx,
        events,
        mut scratch,
        ..
    } = open(&state, content, user_id, now, true).await?;
    let result = events
        .iter()
        .find_map(|row| match &row.event {
            Event::QuizResult(result) if result.quiz_id == task_id => Some(result),
            _ => None,
        })
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::CONFLICT,
                "quiz_not_complete",
                "Finish every quiz question before revealing answers.",
            )
        })?;
    let buffer = scratch
        .quizzes
        .get_mut(&task_id)
        .ok_or_else(|| broken_state("the quiz reveal buffer is unavailable"))?;
    let answers = buffer.answers.clone();
    let requested = raw.as_ref().is_some_and(|body| body["practice"] == true);
    if requested && !buffer.practice_started {
        buffer.practice_started = true;
        if let Some(pending) = practice_queue(&answers) {
            scratch.feedback_practice.insert(task_id.clone(), pending);
            if let Some(progress) = scratch.tasks.get_mut(&task_id) {
                progress.done = false;
            }
        }
    }
    let response = json!({"score": result.score, "xp": result.xp, "inconclusive": result.inconclusive, "answers": answers,
        "practice_pending": scratch.feedback_practice.contains_key(&task_id),
        "practice_available": !scratch.quizzes[&task_id].practice_started && practice_queue(&answers).is_some(),
    });
    write_state(&state.db, &mut tx, user_id, &scratch).await?;
    tx.commit().await.map_err(db_failed)?;
    Ok(Json(response))
}

/// One fresh item per missed skill, avoiding every original item on that skill.
fn practice_queue(answers: &[Value]) -> Option<Value> {
    let mut seen = std::collections::BTreeSet::new();
    let mut queue: Vec<Value> = answers.iter().filter(|answer| answer["outcome"] != "ungraded" && answer["correct"] == false)
        .filter(|answer| seen.insert((answer["topic"].to_string(), answer["kp"].to_string())))
        .map(|answer| {
            let digests: Vec<_> = answers.iter().filter(|item| item["topic"] == answer["topic"] && item["kp"] == answer["kp"])
                .filter_map(|item| item["text"].as_str().map(problem_text_hash)).collect();
            json!({"topic":answer["topic"], "record_topic":answer["topic"], "kp":answer["kp"],
                "digest":problem_text_hash(answer["text"].as_str().unwrap_or_default()), "digests":digests})
        }).collect();
    if queue.is_empty() {
        return None;
    }
    let mut first = queue.remove(0);
    first["queue"] = json!(queue);
    Some(first)
}
