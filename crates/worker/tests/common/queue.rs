//! The diagnosis queue and the model-call ledger.

use cadus_model_client::{Attempt, Usage};
use cadus_store::test_support::TestDb;
use cadus_worker::diagnosis;
use serde_json::{Value, json};
use sqlx::PgPool;
use sqlx::types::Uuid;
use sqlx::types::chrono::{DateTime, Utc};

use super::{FakeModel, handle, reply};

/// A reply that carries a complete `emit_diagnosis` call with these arguments
/// and the diagnosis token counts.
pub fn diagnosis_reply(arguments: &str) -> (u16, String) {
    reply(
        "emit_diagnosis",
        arguments,
        Some(json!({"prompt_tokens": 500, "completion_tokens": 60})),
    )
}

/// The document the grade transaction writes (`cadus_store::diagnosis::JobPayload`).
pub fn payload(session: Option<&str>) -> Value {
    json!({
        "v": 1,
        "session": session,
        "task_id": "task-1",
        "topic": "subtracting-two-digits",
        "kp": "subtracting-two-digits/kp1",
        "problem": "Compute $8 - 5$.",
        "expected": "3",
        "answer_kind": "numeric",
        "given_answer": "2",
        "work": null
    })
}

/// Put one row on the queue and return its id.
pub async fn enqueue(pool: &PgPool, user: Uuid, attempt: &str, body: &Value) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO diagnosis_jobs (user_id, attempt_id, payload) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(user)
    .bind(attempt)
    .bind(body)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Put one row on the queue, then mark it with this status and attempt count.
pub async fn enqueue_as(
    pool: &PgPool,
    user: Uuid,
    attempt: &str,
    status: &str,
    attempts: i32,
) -> Uuid {
    let id = enqueue(pool, user, attempt, &payload(Some("session-1"))).await;
    sqlx::query("UPDATE diagnosis_jobs SET status = $2, attempts = $3 WHERE id = $1")
        .bind(id)
        .bind(status)
        .bind(attempts)
        .execute(pool)
        .await
        .unwrap();
    id
}

/// The status, the attempt count and the result of one row.
pub async fn row_of(pool: &PgPool, id: Uuid) -> (String, i32, Option<Value>) {
    sqlx::query_as::<_, (String, i32, Option<Value>)>(
        "SELECT status, attempts, result FROM diagnosis_jobs WHERE id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Run one pass of the queue against the fake endpoint, with this T4 cap.
pub async fn diagnose(db: &TestDb, fake: &FakeModel, calls_per_session: u32) -> diagnosis::Report {
    let mut job = fake.diagnosis_job(calls_per_session);
    diagnosis::run_once(&handle(db), &mut job).await.unwrap()
}

/// [`diagnose`], and demand this outcome of the pass and this status of the
/// row.
pub async fn diagnose_to(
    db: &TestDb,
    fake: &FakeModel,
    calls_per_session: u32,
    id: Uuid,
    outcome: diagnosis::Outcome,
    status: &str,
) -> diagnosis::Report {
    let report = diagnose(db, fake, calls_per_session).await;
    assert_eq!(report.outcome, outcome);
    assert_eq!(row_of(&db.admin, id).await.0, status);
    report
}

/// One learner of `session-1` with `spent` calls behind it and one queued row
/// in front of it: the learner and the queued row.
pub async fn session_with_spent(db: &TestDb, email: &str, spent: u32) -> (Uuid, Uuid) {
    let user = db.seed_user(email).await;
    for index in 0..spent {
        enqueue_as(&db.admin, user, &format!("old-{index}"), "done", 1).await;
    }
    let id = enqueue(&db.admin, user, "task-1", &payload(Some("session-1"))).await;
    (user, id)
}

/// One `model_call_log` row, without the two fields a clock decides.
///
/// `cost_usd` is read back as its exact TEXT, so the assertion compares money
/// and never a float. `ts` and `latency_ms` are asserted where they are the
/// subject, because a wall clock has no literal value.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Ledger {
    pub purpose: String,
    pub model_id: String,
    pub provider: Option<String>,
    pub user_id: Option<Uuid>,
    pub session_id: Option<String>,
    #[sqlx(rename = "input_tokens_cached")]
    pub cached: i32,
    #[sqlx(rename = "input_tokens_uncached")]
    pub uncached: i32,
    #[sqlx(rename = "output_tokens")]
    pub output: i32,
    #[sqlx(rename = "reasoning_tokens")]
    pub reasoning: i32,
    pub cost: Option<String>,
    pub request_id: Option<String>,
}

impl Ledger {
    /// The row of one diagnosis call of this learner and session, with these
    /// token counts and this request id.
    pub fn diagnosis(
        user: Uuid,
        session: &str,
        uncached: i32,
        output: i32,
        request_id: &str,
    ) -> Self {
        Self {
            purpose: "diagnosis".to_string(),
            model_id: "qwen3.6".to_string(),
            provider: None,
            user_id: Some(user),
            session_id: Some(session.to_string()),
            cached: 0,
            uncached,
            output,
            reasoning: 0,
            cost: None,
            request_id: Some(request_id.to_string()),
        }
    }
}

/// Every ledger row, oldest first.
pub async fn ledger(pool: &PgPool) -> Vec<Ledger> {
    sqlx::query_as::<_, Ledger>(
        "SELECT purpose, model_id, provider, user_id, session_id,
                input_tokens_cached, input_tokens_uncached, output_tokens, reasoning_tokens,
                cost_usd::text AS cost, request_id
           FROM model_call_log ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

/// The `ts` and `latency_ms` of every ledger row, oldest first.
pub async fn ledger_clock(pool: &PgPool) -> Vec<(DateTime<Utc>, i32)> {
    sqlx::query_as::<_, (DateTime<Utc>, i32)>(
        "SELECT ts, latency_ms FROM model_call_log ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

/// One HTTP attempt record with no usage and no price, for a ledger write.
pub fn one_attempt() -> Attempt {
    Attempt {
        index: 0,
        max_tokens: 600,
        status: 200,
        latency_ms: 7,
        usage: Usage::default(),
        model_id: "qwen3.6".to_owned(),
        provider: None,
        request_id: None,
        cost_usd: None,
    }
}
