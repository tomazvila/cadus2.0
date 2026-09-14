//! Reconstruct report evidence from owned events and durable served state.
use super::*;
use crate::state::WebState;
use cadus_core::diagnostic::{self, DiagState};
use cadus_core::event::{AttemptOutcome, DiagnosticAnswer};
use cadus_core::integrated::{Field, IntegratedItem};

pub(super) struct Snapshot {
    pub packet: Value,
    pub problem: AttemptProblem,
    pub identity: String,
    pub topic: String,
}

fn submission(answer: &str, kind: Value, correct: bool, outcome: Value, work: Value) -> Value {
    json!({"given_answer":answer,"answer_kind":kind,"correct":correct,"outcome":outcome,"work":work})
}

pub(super) fn integrated_problem(
    item: &IntegratedItem,
    field: &str,
) -> Result<AttemptProblem, ApiError> {
    let ask = integrated_field(item, field)?;
    let prompts: Vec<_> = item
        .steps
        .iter()
        .map(|step| json!({"id":step.id,"prompt":step.ask.prompt}))
        .collect();
    Ok(AttemptProblem {
        text: format!(
            "{}\n{}\nGiven: {}\nSteps: {}\nRequested field: {}\nQuestion: {}\nUnit: {}",
            item.title,
            item.scenario,
            json!(item.given),
            json!(prompts),
            field,
            ask.prompt,
            ask.unit.as_deref().unwrap_or("")
        ),
        expected: ask.answer.clone(),
        answer_contract: Some(Box::new(ask.contract.clone())),
    })
}

pub(super) fn integrated_field<'a>(
    item: &'a IntegratedItem,
    field: &str,
) -> Result<&'a Field, ApiError> {
    if field == "final" {
        Ok(&item.final_answer.ask)
    } else {
        item.step(field)
            .map(|step| &step.ask)
            .ok_or_else(ApiError::not_found)
    }
}

fn identity(
    task: &str,
    problem: &str,
    topic: &str,
    kind: &str,
    task_type: &str,
    field: &str,
    answer_kind: Value,
) -> Value {
    json!({"task_id":task,"problem_id":problem,"topic":topic,"report_kind":kind,
        "task_type":task_type,"field":field,"answer_kind":answer_kind})
}

pub(super) async fn snapshot(
    state: &AppState,
    content: &Content,
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
    task: &str,
    body: &Value,
) -> Result<Snapshot, ApiError> {
    let problem_id = text_field(body, "problem_id", 256)?;
    let kind = body
        .get("report_kind")
        .and_then(Value::as_str)
        .unwrap_or("attempt");
    let field = body
        .get("field_id")
        .and_then(Value::as_str)
        .unwrap_or("final");
    let explicit = body.get("attempt_id").and_then(Value::as_str).unwrap_or("");
    if let Some(digest) = body.get("item_digest").and_then(Value::as_str) {
        let item = content
            .integrated
            .get(problem_id)
            .filter(|item| item.digest() == digest)
            .ok_or_else(ApiError::not_found)?;
        let event_kind = if kind == "served" {
            "integrated_served"
        } else {
            "integrated_attempt"
        };
        let row = sqlx::query_scalar::<_, Value>(
            "SELECT jsonb_build_object('event_seq',seq,'attempt',payload) FROM events
             WHERE user_id=$1 AND type=$2 AND payload->>'task_id'=$3 AND payload->>'item_id'=$4
             AND payload->>'item_digest'=$5 ORDER BY seq DESC LIMIT 1",
        )
        .bind(user)
        .bind(event_kind)
        .bind(task)
        .bind(problem_id)
        .bind(digest)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db_failed)?
        .ok_or_else(ApiError::not_found)?;
        let event = &row["attempt"];
        let topic = item.topic.as_str().to_owned();
        let problem = integrated_problem(item, field)?;
        let id = if kind == "served" {
            format!("served:{task}:{problem_id}:{digest}:{field}")
        } else {
            format!(
                "{}:{field}",
                event["attempt_id"]
                    .as_str()
                    .ok_or_else(|| ApiError::internal("integrated identity"))?
            )
        };
        let mut packet = row.clone();
        packet["content_only"] = json!(kind == "served");
        packet["report_identity"] = identity(
            task,
            problem_id,
            &topic,
            kind,
            "multi-step",
            field,
            json!("expression"),
        );
        if kind == "served" {
            packet["attempt"] = Value::Null;
            packet["event_seq"] = json!(0);
        } else {
            let submitted = if field == "final" {
                &event["final_field"]
            } else {
                event["steps"]
                    .as_array()
                    .and_then(|steps| steps.iter().find(|step| step["id"] == field))
                    .ok_or_else(ApiError::not_found)?
            };
            let correct = submitted["outcome"] == "correct";
            packet["submission"] = submission(
                submitted["answer"].as_str().unwrap_or(""),
                json!("expression"),
                correct,
                submitted["outcome"].clone(),
                event["reasoning_ungraded"].clone(),
            );
        }
        packet["report_identity"]["attempt_id"] = json!(id);
        return Ok(Snapshot {
            packet,
            problem,
            identity: id,
            topic,
        });
    }
    if kind == "integrated" {
        return Err(ApiError::invalid_request(
            "An integrated report needs item_digest.",
        ));
    }
    if kind == "served" {
        let raw = store(state, cadus_store::state::load_web_state(tx, user))
            .await?
            .ok_or_else(ApiError::not_found)?;
        let scratch: WebState =
            serde_json::from_value(raw).map_err(|_| ApiError::internal("served report state"))?;
        let served = scratch
            .served
            .get(task)
            .filter(|served| served.problem_id == problem_id)
            .ok_or_else(ApiError::not_found)?;
        let topic = served.topic.clone().unwrap_or_default();
        let task_type = if task == "diag" {
            "diagnostic".to_owned()
        } else {
            scratch.tasks.get(task).map(|progress| progress.task_type.clone()).unwrap_or_default()
        };
        let id = format!("served:{task}:{problem_id}");
        let mut packet = json!({"attempt":null,"event_seq":0,"content_only":true,
            "report_identity":identity(task,problem_id,&topic,kind,&task_type,"",json!(served.answer_kind))});
        packet["report_identity"]["attempt_id"] = json!(id);
        return Ok(Snapshot {
            packet,
            identity: id,
            topic,
            problem: AttemptProblem {
                text: served.text.clone(),
                expected: served.expected.answer.clone(),
                answer_contract: served.expected.answer_contract.clone().map(Box::new),
            },
        });
    }
    if kind == "diagnostic" {
        if task != "diag" {
            return Err(ApiError::not_found());
        }
        let mut packet = sqlx::query_scalar::<_, Value>(
            "SELECT jsonb_build_object('event_seq',seq,'attempt',payload) FROM events
             WHERE user_id=$1 AND type='diagnostic_answer' AND payload->>'problem_id'=$2
             ORDER BY seq DESC LIMIT 1",
        )
        .bind(user)
        .bind(problem_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db_failed)?
        .ok_or_else(ApiError::not_found)?;
        let event: Event = serde_json::from_value(packet["attempt"].clone())
            .map_err(|_| ApiError::internal("diagnostic report"))?;
        let Event::DiagnosticAnswer(answer) = event else {
            return Err(ApiError::not_found());
        };
        let problem = answer.problem.clone().ok_or_else(ApiError::not_found)?;
        let topic = answer.topic.as_str().to_owned();
        let answer_kind = content
            .curriculum
            .idx_of(&topic)
            .and_then(|idx| content.curriculum.topic(idx))
            .map(|topic| json!(topic.answer_kind))
            .unwrap_or(Value::Null);
        let id = format!("diagnostic:{problem_id}:{}", packet["event_seq"]);
        let outcome = answer.outcome.clone().unwrap_or(if answer.correct {
            AttemptOutcome::Correct
        } else {
            AttemptOutcome::Incorrect
        });
        packet["submission"] = submission(
            answer.submitted.as_deref().unwrap_or(""),
            answer_kind.clone(),
            answer.correct,
            json!(outcome),
            Value::Null,
        );
        packet["content_only"] = json!(false);
        packet["report_identity"] = identity(
            task,
            problem_id,
            &topic,
            kind,
            "diagnostic",
            "",
            answer_kind,
        );
        packet["report_identity"]["attempt_id"] = json!(id);
        return Ok(Snapshot {
            packet,
            problem,
            identity: id,
            topic,
        });
    }
    let mut packet = store(
        state,
        reports::attempt(tx, user, task, problem_id, explicit),
    )
    .await?
    .ok_or_else(ApiError::not_found)?;
    let event: Event = serde_json::from_value(packet["attempt"].clone())
        .map_err(|_| ApiError::internal("report attempt"))?;
    let Event::Attempt(attempt) = event else {
        return Err(ApiError::not_found());
    };
    let topic = attempt.topic.as_str().to_owned();
    let id = attempt.attempt_id.clone();
    packet["content_only"] = json!(false);
    packet["report_identity"] = identity(
        task,
        problem_id,
        &topic,
        kind,
        attempt.task_type.as_str(),
        "",
        json!(attempt.answer_kind),
    );
    packet["report_identity"]["attempt_id"] = json!(id);
    Ok(Snapshot {
        packet,
        problem: attempt.problem,
        identity: id,
        topic,
    })
}

pub(super) async fn policy(
    state: &AppState,
    content: &Content,
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
    snapshot: &Snapshot,
) -> Result<Value, ApiError> {
    let topic = content
        .curriculum
        .idx_of(&snapshot.topic)
        .and_then(|idx| content.curriculum.topic(idx));
    let points: Vec<_> = topic
        .map(|topic| {
            topic
                .knowledge_points
                .iter()
                .map(|kp| kp.id.as_str())
                .collect()
        })
        .unwrap_or_default();
    let mut policy = json!({"config":content.cfg,"knowledge_points":points,
        "expected_time_secs":topic.map_or(0,|topic|topic.expected_time_secs)});
    if snapshot.packet["attempt"]["type"] != "diagnostic_answer" {
        return Ok(policy);
    }
    let seq = snapshot.packet["event_seq"]
        .as_i64()
        .ok_or_else(|| ApiError::internal("diagnostic sequence"))?;
    let Some(run) = store(state, reports::diagnostics::for_answer(tx, user, seq)).await? else {
        // Legacy answers have no reliable run boundary. Preserve the report,
        // but the worker must not reconstruct a placement by guessing.
        return Ok(policy);
    };
    if run["curriculum_digest"] != content.curriculum_context_digest()? {
        return Ok(policy);
    }
    let mut diagnostic: DiagState = serde_json::from_value(run["state"].clone())
        .map_err(|_| ApiError::internal("diagnostic report context"))?;
    let config: cadus_core::config::Config = serde_json::from_value(run["config"].clone())
        .map_err(|_| ApiError::internal("diagnostic report policy"))?;
    let answer: DiagnosticAnswer =
        match serde_json::from_value::<Event>(snapshot.packet["attempt"].clone())
            .map_err(|_| ApiError::internal("diagnostic event"))?
        {
            Event::DiagnosticAnswer(answer) => answer,
            _ => return Err(ApiError::internal("diagnostic event")),
        };
    for balance in diagnostic.balances.values_mut() {
        *balance = 0.0;
    }
    diagnostic.answered.clear();
    let mut before = diagnostic.clone();
    if !answer
        .outcome
        .as_ref()
        .is_some_and(AttemptOutcome::is_ungraded)
    {
        diagnostic::apply_answer(
            &mut before,
            &content.curriculum,
            &snapshot.topic,
            answer.correct,
            answer.weight.get(),
            &config,
        );
    }
    let weight = diagnostic::answer_weight(
        true,
        topic.map_or(0, |topic| topic.expected_time_secs) as f64,
        answer.secs.get() as f64,
    );
    diagnostic::apply_answer(
        &mut diagnostic,
        &content.curriculum,
        &snapshot.topic,
        true,
        weight,
        &config,
    );
    let delta: serde_json::Map<String, Value> = diagnostic
        .balances
        .iter()
        .map(|(id, value)| {
            (
                id.clone(),
                json!(value - before.balances.get(id).copied().unwrap_or(0.0)),
            )
        })
        .collect();
    policy["diagnostic_run_id"] = run["run_id"].clone();
    policy["diagnostic_delta"] = Value::Object(delta);
    policy["diagnostic_weight"] = json!(weight);
    policy["diagnostic_curriculum_digest"] = run["curriculum_digest"].clone();
    Ok(policy)
}
