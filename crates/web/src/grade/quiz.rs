//! Blind quiz closure, post-batch reveal, and independent practice.
use super::*;
use cadus_core::answer::multipart_credit;
use cadus_core::curriculum::{Curriculum, Exemplar};
use cadus_core::event::{QuizResult, QuizTopicResult, Secs};
use cadus_core::selector::{ItemPoints, item_points, parse_paper_entry, score_of};

/// The authored item that the learner saw in `attempt`, found by its topic and text.
fn exemplar_of<'g>(graph: &'g Curriculum, attempt: &Attempt) -> Option<&'g Exemplar> {
    let idx = graph.idx_of(attempt.topic.as_str())?;
    graph
        .knowledge_points(idx)
        .iter()
        .filter(|point| {
            attempt
                .kp
                .as_ref()
                .is_none_or(|kp| kp.as_str() == point.id.as_str())
        })
        .flat_map(|point| point.exemplars.iter())
        .find(|item| item.problem == attempt.problem.text)
}

/// The points of one answered quiz item (D-VBE3).
///
/// An item with no authored `points` counts 1 point. A multipart item with
/// authored `points` earns part of them for each named part it got right.
pub(super) fn attempt_points(graph: &Curriculum, attempt: &Attempt) -> ItemPoints {
    let points = exemplar_of(graph, attempt).and_then(|item| item.points);
    let parts = points.and_then(|_| {
        let contract = attempt.problem.answer_contract.as_deref()?;
        multipart_credit(contract, &attempt.problem.expected, &attempt.given_answer)
    });
    item_points(points, parts, attempt.correct)
}

/// The XP factor of a quiz of `n` questions: `n` over the due quiz length, at most 1.
#[expect(clippy::cast_precision_loss, reason = "quiz lengths are small counts")]
fn round_xp_scale(n: i64, due_length: i64) -> f64 {
    if due_length <= 0 {
        return 1.0;
    }
    (n as f64 / due_length as f64).min(1.0)
}

/// Close exactly the original quiz batch; supplemental attempts cannot reprice it.
pub(super) fn close_quiz(
    task: &Task,
    attempt: &Attempt,
    prior: &[EventRow],
    cfg: &Config,
    graph: &Curriculum,
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
    // The score is the points earned over the points asked. A plain round has
    // 1-point items only, so it scores as the share of correct items.
    let credits: Vec<ItemPoints> = decided
        .iter()
        .map(|item| attempt_points(graph, item))
        .collect();
    let score = score_of(&credits);
    let xp = if decided.len() == originals.len() {
        // A short round earns less: the XP scales by length over the due quiz.
        task_xp(TaskType::Quiz, WorkQuality::NearlyPerfect, cfg, 0, 0, false)
            * score
            * round_xp_scale(task.n_problems.unwrap_or(0), cfg.quiz.questions)
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
            per_topic: per_topic(task, &decided, score, cfg),
        })),
        xp: Some(round2(xp)),
        ..Advance::carry_on()
    }
}

/// The topic rows of a quiz result.
///
/// A paper round asks many items of one set topic. It writes one row for that
/// topic: right when the paper reaches the retake line (D-VBE3).
fn per_topic(task: &Task, decided: &[&&Attempt], score: f64, cfg: &Config) -> Vec<QuizTopicResult> {
    let paper = task
        .mix
        .first()
        .is_some_and(|entry| parse_paper_entry(entry).is_some());
    if !paper {
        return decided
            .iter()
            .map(|item| QuizTopicResult {
                topic: item.topic.clone(),
                correct: item.correct,
                secs: item.secs,
            })
            .collect();
    }
    let Some(first) = decided.first() else {
        return Vec::new();
    };
    let secs: i64 = decided.iter().map(|item| item.secs.get()).sum();
    vec![QuizTopicResult {
        topic: first.topic.clone(),
        correct: score >= cfg.quiz.retake_below,
        secs: Secs::new(secs).unwrap_or(first.secs),
    }]
}

/// Read a completed quiz; `practice` starts the fresh-skill queue after studying.
pub async fn result(request: TaskWithBody) -> Result<Json<Value>, ApiError> {
    let (state, user_id, task_id, raw, now) = task_request(request);
    let content = content(&state)?;
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
    // Amendment K point 6: the quiz is revealed, so its written proofs go to
    // the background grader now (the enqueue is idempotent per attempt).
    for answer in &mut buffer.answers {
        if answer
            .get(proof_grading::FIELD)
            .is_some_and(|field| !field.is_null())
        {
            continue;
        }
        let (Some(payload), Some(attempt_id)) = (
            answer.get(proof_grading::BUFFER_PAYLOAD).cloned(),
            answer["attempt_id"].as_str().map(str::to_owned),
        ) else {
            continue;
        };
        let Ok(payload) = serde_json::from_value::<cadus_store::proof_grading::JobPayload>(payload)
        else {
            continue;
        };
        let job = proof_grading::enqueue_payload(
            &state,
            &mut tx,
            user_id,
            &attempt_id,
            &payload,
            &proof_grading::QUIZ_JOB,
        )
        .await?;
        answer[proof_grading::FIELD] = proof_grading::reply_field(job);
    }
    let answers = buffer.answers.clone();
    // The grading payload stays in the buffer; the reply shows the job only.
    let shown: Vec<Value> = answers
        .iter()
        .cloned()
        .map(|mut answer| {
            if let Some(map) = answer.as_object_mut() {
                map.remove(proof_grading::BUFFER_PAYLOAD);
                // The buffer keeps the exact points; the reply shows 2 decimals.
                if let Some(points) = map.get("points_earned").and_then(Value::as_f64) {
                    map.insert("points_earned".to_owned(), json!(round2(points)));
                }
            }
            answer
        })
        .collect();
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
    let round = scratch.rounds.get(&task_id).map(|recorded| {
        json!({"length": recorded.length, "scope": recorded.scope,
            "course_name": recorded.course_name, "n": answers.len(),
            "kind": recorded.kind(), "set": recorded.set,
            "form": recorded.set.as_deref().and_then(|set| content.curriculum.idx_of(set))
                .and_then(|idx| content.curriculum.topic(idx)).map(|topic| topic.name.clone())})
    });
    let sum = round_points(&state, &mut tx, &answers).await?;
    let response = json!({"score": result.score, "round": round, "xp": result.xp, "inconclusive": result.inconclusive, "answers": shown,
        "points_earned": round2(sum.earned), "points_total": sum.total,
        "percent": percent_of(sum),
        "practice_pending": scratch.feedback_practice.contains_key(&task_id),
        "practice_available": !scratch.quizzes[&task_id].practice_started && practice_queue(&answers).is_some(),
    });
    write_state(&state.db, &mut tx, user_id, &scratch).await?;
    tx.commit().await.map_err(db_failed)?;
    Ok(Json(response))
}

/// The percent of the points of a round, rounded to 2 decimals.
fn percent_of(sum: ItemPoints) -> f64 {
    if sum.total > 0.0 {
        round2(100.0 * sum.earned / sum.total)
    } else {
        0.0
    }
}

/// The points of a revealed round, from the buffer of its answers (D-VBE3).
///
/// An item that a model grades earns its points when its background verdict
/// is PASS. A verdict that has not come, or a verdict that is not PASS, earns
/// nothing. A buffer from before points stands for 1 point for each item.
async fn round_points(
    state: &AppState,
    tx: &mut Transaction<'static, Postgres>,
    answers: &[Value],
) -> Result<ItemPoints, ApiError> {
    let mut items = Vec::with_capacity(answers.len());
    for answer in answers {
        let total = answer["points_total"].as_f64().unwrap_or(1.0);
        let mut earned = answer["points_earned"]
            .as_f64()
            .unwrap_or(if answer["correct"] == true { 1.0 } else { 0.0 });
        let job = answer[proof_grading::FIELD]["id"]
            .as_str()
            .and_then(|id| Uuid::parse_str(id).ok());
        if let Some(job) = job
            && answer["outcome"] == "ungraded"
            && written_passed(state, tx, job).await?
        {
            earned = total;
        }
        items.push(ItemPoints { earned, total });
    }
    Ok(cadus_core::selector::sum_points(&items))
}

/// Whether the background verdict of one written item is PASS.
async fn written_passed(
    state: &AppState,
    tx: &mut Transaction<'static, Postgres>,
    job: Uuid,
) -> Result<bool, ApiError> {
    let row = store(state, cadus_store::proof_grading::job(&mut **tx, job)).await?;
    Ok(row
        .as_ref()
        .is_some_and(|row| proof_grading::chain::status_of(row) == proof_grading::STATUS_PASS))
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
