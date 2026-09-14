//! Authenticated reports over immutable owned questions and submissions.
use crate::route_prelude::*;
use crate::state::{Content, ServedProblem, Tenant};
use cadus_core::event::{AttemptProblem, Event};
use cadus_store::reports;
use sha2::{Digest, Sha256};

mod sources;

fn text_field<'a>(body: &'a Value, name: &str, max: usize) -> Result<&'a str, ApiError> {
    body.get(name)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= max)
        .ok_or_else(|| ApiError::invalid_request(format!("The report needs a valid {name}.")))
}
fn source(content: &Content, problem: &AttemptProblem) -> Result<Value, ApiError> {
    Ok(
        json!({"problem":problem,"curriculum_digest":content.curriculum_context_digest()?,
        "engine_digest":content.review_engine_digest()}),
    )
}
fn hash(value: &Value) -> Result<String, ApiError> {
    let bytes = serde_json::to_vec(&reports::source_identity(value))
        .map_err(|_| ApiError::internal("report source"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
fn response(row: &Value, reveal: bool) -> Value {
    let status = row["status"].as_str().unwrap_or("failed");
    let mut reply = json!({"report_id":row["id"],"status":status,"stage":row["stage"],
        "attempt":row["attempt"],"max_attempts":3,"retryable":matches!(status,"failed"|"unresolved")});
    if !row["result"].is_null() {
        reply["result"] = if reveal {
            row["result"].clone()
        } else {
            json!({"resolution":"needs_review","message":"Review finished. Details are available after you submit the answer and finish any assessment.",
                "qwen_verdict":"ambiguous","verification":"unresolved","grade_corrected":false,"content_published":false})
        };
    }
    reply
}
async fn reveal(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
    row: &Value,
) -> Result<bool, ApiError> {
    let input = &row["input"];
    let task = row["task_id"].as_str().unwrap_or("");
    let problem = row["problem_id"].as_str().unwrap_or("");
    let kind = input["report_identity"]["task_type"].as_str().unwrap_or("");
    // Assessment state is checked on the server, including direct polling.
    if kind == "quiz" || task.contains("-quiz") {
        return sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM events WHERE user_id=$1 AND type='quiz_result' AND payload->>'quiz_id'=$2)")
            .bind(user).bind(task).fetch_one(&mut **tx).await.map_err(db_failed);
    }
    if task == "diag" {
        return sqlx::query_scalar::<_,bool>(
            "SELECT EXISTS(SELECT 1 FROM events a JOIN events p ON p.user_id=a.user_id AND p.seq>a.seq
             AND p.type='diagnostic_placed' WHERE a.user_id=$1 AND a.type='diagnostic_answer' AND a.payload->>'problem_id'=$2)")
            .bind(user).bind(problem).fetch_one(&mut **tx).await.map_err(db_failed);
    }
    if input["content_only"] != true {
        return Ok(true);
    }
    sqlx::query_scalar::<_,bool>(
        "SELECT EXISTS(SELECT 1 FROM events a WHERE a.user_id=$1 AND a.payload->>'task_id'=$2
         AND ((a.type='integrated_attempt' AND a.payload->>'item_id'=$3)
          OR (a.type='attempt' AND (SELECT h.payload->>'problem_id' FROM events h
              WHERE h.user_id=a.user_id AND h.type='ordinary_problem_served' AND h.payload->>'task_id'=$2
              AND h.seq<a.seq ORDER BY h.seq DESC LIMIT 1)=$3)))")
        .bind(user).bind(task).bind(problem).fetch_one(&mut **tx).await.map_err(db_failed)
}
fn matches_request(row: &Value, task: &str, problem: &str, body: &Value) -> bool {
    row["task_id"] == task
        && row["problem_id"] == problem
        && body
            .get("attempt_id")
            .and_then(Value::as_str)
            .is_none_or(|id| row["attempt_id"] == id)
        && body
            .get("field_id")
            .and_then(Value::as_str)
            .is_none_or(|field| row["input"]["report_identity"]["field"] == field)
}
pub async fn create(request: TaskWithBody) -> Result<Json<Value>, ApiError> {
    let (state, user, task, raw, _now) = task_request(request);
    let (content, body) = route_input(&state, raw.as_ref())?;
    let body = body
        .filter(|value| value.is_object())
        .ok_or_else(|| ApiError::invalid_request("The report needs a JSON object."))?;
    let problem = text_field(body, "problem_id", 256)?;
    let request_id = Uuid::parse_str(text_field(body, "request_id", 64)?)
        .map_err(|_| ApiError::invalid_request("The report needs a UUID request_id."))?;
    for field in ["attempt_id", "item_digest", "field_id"] {
        if body.get(field).is_some() {
            text_field(body, field, 512)?;
        }
    }
    let kind = body
        .get("report_kind")
        .map(|_| text_field(body, "report_kind", 32))
        .transpose()?
        .unwrap_or("attempt");
    if !["attempt", "served", "integrated", "diagnostic"].contains(&kind) {
        return Err(ApiError::invalid_request("Unknown report kind."));
    }
    let note = match body.get("note") {
        None | Some(Value::Null) => "",
        Some(Value::String(s)) if s.chars().count() <= 2000 => s,
        _ => {
            return Err(ApiError::invalid_request(
                "The report note must be text of at most 2000 characters.",
            ));
        }
    };
    let mut tx = store(&state, cadus_store::begin_tenant(state.db.pool(), user)).await?;
    store(&state, cadus_store::state::lock_web_state(&mut tx, user)).await?;
    let initial = body.get("attempt_id").and_then(Value::as_str).unwrap_or("");
    if let Some(existing) = store(
        &state,
        reports::existing(&mut tx, user, request_id, initial),
    )
    .await?
    {
        if !matches_request(&existing, &task, problem, body) {
            return Err(ApiError::invalid_request(
                "That request ID belongs to another report.",
            ));
        }
        let visible = reveal(&mut tx, user, &existing).await?;
        return Ok(Json(response(&existing, visible)));
    }
    let snapshot = sources::snapshot(&state, content, &mut tx, user, &task, body).await?;
    if let Some(existing) = store(
        &state,
        reports::existing(&mut tx, user, request_id, &snapshot.identity),
    )
    .await?
    {
        if !matches_request(&existing, &task, problem, body) {
            return Err(ApiError::invalid_request(
                "That request ID belongs to another report.",
            ));
        }
        let visible = reveal(&mut tx, user, &existing).await?;
        return Ok(Json(response(&existing, visible)));
    }
    if !store(&state, reports::capacity(&mut tx, user, &snapshot.identity)).await? {
        return Err(ApiError::rate_limited());
    }
    let policy = sources::policy(&state, content, &mut tx, user, &snapshot).await?;
    let origin = source(content, &snapshot.problem)?;
    let source_hash = hash(&origin)?;
    let previous = store(&state, reports::published(&mut tx, &source_hash)).await?;
    let mut packet = snapshot.packet;
    packet["task_policy"] = policy;
    packet["source"] = origin;
    packet["note"] = json!(note);
    packet["problem_id"] = json!(problem);
    packet["correction_version"] = json!(
        previous
            .as_ref()
            .and_then(|p| p["version"].as_i64())
            .unwrap_or(0)
    );
    packet["previous_correction"] = previous.unwrap_or(Value::Null);
    if serde_json::to_vec(&packet)
        .map_err(|_| ApiError::internal("report packet"))?
        .len()
        > 80_000
    {
        return Err(ApiError::invalid_request(
            "This problem is too large for automatic review.",
        ));
    }
    let queued = store(
        &state,
        reports::enqueue(
            &mut tx,
            user,
            request_id,
            &task,
            problem,
            &snapshot.identity,
            &source_hash,
            &packet,
        ),
    )
    .await?;
    tx.commit().await.map_err(db_failed)?;
    Ok(Json(response(&queued, false)))
}
pub async fn get(
    State(state): State<AppState>,
    Tenant(user): Tenant,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let mut tx = store(&state, cadus_store::begin_tenant(state.db.pool(), user)).await?;
    let row = store(&state, reports::get(&mut tx, user, id))
        .await?
        .ok_or_else(ApiError::not_found)?;
    let visible = reveal(&mut tx, user, &row).await?;
    Ok(Json(response(&row, visible)))
}
fn accepted(body: &Value, answer: &str) -> bool {
    body["accepted_answers"].as_array().is_some_and(|items| {
        items
            .iter()
            .filter_map(Value::as_str)
            .any(|s| s.trim() == answer.trim())
    })
}
pub(crate) async fn apply(
    state: &AppState,
    content: &Content,
    tx: &mut Transaction<'_, Postgres>,
    served: &mut ServedProblem,
    answer: &str,
) -> Result<bool, ApiError> {
    let problem = AttemptProblem {
        text: served.text.clone(),
        expected: served.expected.answer.clone(),
        answer_contract: served.expected.answer_contract.clone().map(Box::new),
    };
    let source_hash = hash(&source(content, &problem)?)?;
    let Some(published) = store(state, reports::published(tx, &source_hash)).await? else {
        return Ok(false);
    };
    let body = &published["body"];
    served.expected.answer = body["candidate_answer"]
        .as_str()
        .ok_or_else(|| ApiError::internal("published answer"))?
        .to_owned();
    served.solution_sketch = Some(
        body["solution"]
            .as_str()
            .ok_or_else(|| ApiError::internal("published solution"))?
            .to_owned(),
    );
    Ok(accepted(body, answer))
}

/// Use the same verified content and exact-alias gate for every integrated field.
pub(crate) async fn grade_integrated(
    state: &AppState,
    content: &Content,
    tx: &mut Transaction<'_, Postgres>,
    item: &cadus_core::integrated::IntegratedItem,
    submission: &cadus_core::integrated::Submission,
) -> Result<
    (
        cadus_core::integrated::IntegratedItem,
        cadus_core::integrated::IntegratedGrade,
    ),
    ApiError,
> {
    let mut corrected = item.clone();
    let mut aliases = std::collections::BTreeSet::new();
    let ids: Vec<String> = item
        .steps
        .iter()
        .map(|s| s.id.as_str().to_owned())
        .chain(std::iter::once("final".to_owned()))
        .collect();
    for field in ids {
        let key = hash(&source(
            content,
            &sources::integrated_problem(item, &field)?,
        )?)?;
        if let Some(published) = store(state, reports::published(tx, &key)).await? {
            let body = &published["body"];
            let answer = if field == "final" {
                Some(submission.final_answer.answer.as_str())
            } else {
                submission
                    .steps
                    .iter()
                    .find(|s| s.id == field)
                    .map(|s| s.answer.as_str())
            };
            if answer.is_some_and(|answer| accepted(body, answer)) {
                aliases.insert(field.clone());
            }
            let ask = if field == "final" {
                &mut corrected.final_answer.ask
            } else {
                &mut corrected
                    .steps
                    .iter_mut()
                    .find(|s| s.id.as_str() == field)
                    .ok_or_else(ApiError::not_found)?
                    .ask
            };
            ask.answer = body["candidate_answer"]
                .as_str()
                .ok_or_else(|| ApiError::internal("published field answer"))?
                .to_owned();
            ask.accept_also.clear();
            if field == "final" {
                corrected.final_answer.interpretation =
                    body["solution"].as_str().unwrap_or("").to_owned();
            }
        }
    }
    let mut grade = cadus_core::integrated::grade(&corrected, submission);
    for field in grade
        .steps
        .iter_mut()
        .chain(std::iter::once(&mut grade.final_grade))
    {
        if aliases.contains(&field.id) {
            field.correct = true;
            field.ungraded = false;
            field.notation = false;
        }
    }
    grade.correct_steps = grade.steps.iter().filter(|field| field.correct).count();
    grade.solved = grade.final_grade.correct;
    grade.ungraded = grade
        .steps
        .iter()
        .chain(std::iter::once(&grade.final_grade))
        .any(|field| field.ungraded);
    grade.skills_credited.clear();
    for field in grade
        .steps
        .iter()
        .chain(std::iter::once(&grade.final_grade))
        .filter(|field| field.correct)
    {
        for skill in &field.skills {
            if !grade.skills_credited.contains(skill) {
                grade.skills_credited.push(skill.clone());
            }
        }
    }
    Ok((corrected, grade))
}
