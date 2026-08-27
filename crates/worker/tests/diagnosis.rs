//! M5 U10 acceptance: the diagnosis worker (A4, D-O5, D7, T4, T5).
//!
//! The section 11 row of the spec names five checks, and each one is a test
//! here:
//!
//! 1. two workers × 100 claims never take the same row;
//! 2. a `finish_reason: "length"` reply retries with a ×4 budget and the second
//!    reply is accepted (`crates/model-client/tests/client.rs`, and end to end
//!    here);
//! 3. a 400 does not retry (`crates/model-client/tests/client.rs`);
//! 4. a tag outside the vocabulary is dropped;
//! 5. three failures dead-letter the row.
//!
//! Every expected value is a LITERAL: a literal status string, a literal tag
//! list, a literal row count, a literal NOTIFY payload. Nothing is read back
//! from the code under test.
//!
//! The endpoint is a fake OpenAI-compatible server in this file. No test reaches
//! a real provider.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use cadus_model_client::{Client, ModelConfig};
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_worker::diagnosis::{
    DiagnosisJob, MAX_JOB_ATTEMPTS, Outcome, claim, filter_tags, run_once, sweep,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use sqlx::postgres::PgListener;
use sqlx::types::Uuid;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

// --------------------------------------------------------------------------- //
// The fake OpenAI-compatible server
// --------------------------------------------------------------------------- //

/// A local endpoint that answers a fixed list of replies, in order. A call past
/// the end of the list gets `500` with an empty body.
struct FakeModel {
    base_url: String,
    calls: Arc<Mutex<Vec<Value>>>,
}

impl FakeModel {
    async fn start(replies: Vec<(u16, String)>) -> FakeModel {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let record = Arc::clone(&calls);

        tokio::spawn(async move {
            let mut index = 0_usize;
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let mut raw: Vec<u8> = Vec::new();
                let mut buffer = [0_u8; 4096];
                let request = loop {
                    let read = socket.read(&mut buffer).await.unwrap_or(0);
                    if read == 0 {
                        break String::from_utf8_lossy(&raw).to_string();
                    }
                    raw.extend_from_slice(&buffer[..read]);
                    let text = String::from_utf8_lossy(&raw).to_string();
                    if let Some(split) = text.find("\r\n\r\n") {
                        let length: usize = text[..split]
                            .to_lowercase()
                            .split("\r\n")
                            .find_map(|line| line.strip_prefix("content-length:"))
                            .and_then(|value| value.trim().parse().ok())
                            .unwrap_or(0);
                        if text.len() >= split + 4 + length {
                            break text;
                        }
                    }
                };
                let split = request.find("\r\n\r\n").unwrap_or(request.len());
                let body: Value = serde_json::from_str(request.get(split + 4..).unwrap_or(""))
                    .unwrap_or(Value::Null);
                record.lock().unwrap().push(body);

                let (status, payload) = replies
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| (500, String::new()));
                index += 1;
                let reply = format!(
                    "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{payload}",
                    payload.len()
                );
                let _ = socket.write_all(reply.as_bytes()).await;
                let _ = socket.flush().await;
            }
        });

        FakeModel {
            base_url: format!("http://127.0.0.1:{port}/v1"),
            calls,
        }
    }

    /// The request bodies the endpoint received, in order.
    fn calls(&self) -> Vec<Value> {
        self.calls.lock().unwrap().clone()
    }

    /// A diagnosis job pointed at this endpoint.
    fn job(&self, calls_per_session: u32) -> DiagnosisJob {
        let cfg = ModelConfig {
            base_url: self.base_url.clone(),
            api_key: "test-key".to_owned(),
            model: "qwen3.6".to_owned(),
            output_tokens: 600,
            reasoning_max_tokens: 600,
            provider_order: Vec::new(),
            timeout: Duration::from_secs(5),
        };
        DiagnosisJob::new(Client::new(cfg).unwrap(), calls_per_session)
    }
}

/// A reply that carries a complete forced tool call with these arguments.
fn tool_reply(arguments: &str) -> String {
    json!({
        "id": "gen-1",
        "choices": [{"finish_reason": "tool_calls", "message": {"tool_calls": [{"function": {
            "name": "emit_diagnosis", "arguments": arguments
        }}]}}],
        "usage": {"prompt_tokens": 500, "completion_tokens": 60}
    })
    .to_string()
}

// --------------------------------------------------------------------------- //
// Queue helpers
// --------------------------------------------------------------------------- //

/// The document the grade transaction writes (`cadus_store::diagnosis::JobPayload`).
fn payload(session: Option<&str>) -> Value {
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
async fn enqueue(pool: &PgPool, user: Uuid, attempt: &str, body: &Value) -> Uuid {
    sqlx::query_scalar!(
        "INSERT INTO diagnosis_jobs (user_id, attempt_id, payload) VALUES ($1, $2, $3) RETURNING id",
        user,
        attempt,
        body,
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

/// The status, the attempt count and the result of one row.
async fn row_of(pool: &PgPool, id: Uuid) -> (String, i32, Option<Value>) {
    let row = sqlx::query!(
        r#"SELECT status AS "status!", attempts AS "attempts!", result FROM diagnosis_jobs WHERE id = $1"#,
        id
    )
    .fetch_one(pool)
    .await
    .unwrap();
    (row.status, row.attempts, row.result)
}

// --------------------------------------------------------------------------- //
// D-O5 — the SKIP LOCKED claim
// --------------------------------------------------------------------------- //

/// The acceptance literal: two workers × 100 claims never take the same row.
///
/// Both tasks run the same statement against the same queue at the same time. A
/// claim without `SKIP LOCKED` either blocks or hands one row to both.
#[tokio::test]
async fn two_workers_and_a_hundred_rows_never_take_the_same_job() {
    TestDb::with(|db| async move {
        let user = db.seed_user("claims@example.test").await;
        let body = payload(Some("session-1"));
        for index in 0..100 {
            enqueue(&db.admin, user, &format!("task-{index}"), &body).await;
        }

        let one = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
        let two = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
        let drain = |handle: Db| async move {
            let mut taken: Vec<Uuid> = Vec::new();
            while let Some(job) = claim(&handle).await.unwrap() {
                taken.push(job.id);
            }
            taken
        };

        let (first, second) = tokio::join!(drain(one), drain(two));

        let mut all = [first.clone(), second.clone()].concat();
        assert_eq!(all.len(), 100, "the two workers must claim 100 rows in all");
        all.sort();
        all.dedup();
        assert_eq!(all.len(), 100, "no row may be claimed twice");

        let running = sqlx::query_scalar!(
            r#"SELECT count(*) AS "count!" FROM diagnosis_jobs WHERE status = 'running' AND attempts = 1"#
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(running, 100, "every claimed row is running at attempt 1");
    })
    .await;
}

/// A row another worker holds NEVER blocks this one (D-O5).
///
/// This is the half of `SKIP LOCKED` that a row count cannot see. The open
/// transaction below locks the oldest pending row and keeps it. With
/// `SKIP LOCKED` the second claim steps over that row and takes the next one at
/// once; with a plain `FOR UPDATE` it waits for the lock and this test times
/// out.
#[tokio::test]
async fn a_row_another_worker_holds_does_not_block_the_claim() {
    TestDb::with(|db| async move {
        let user = db.seed_user("locked@example.test").await;
        let first = enqueue(&db.admin, user, "task-1", &payload(None)).await;
        let second = enqueue(&db.admin, user, "task-2", &payload(None)).await;

        // Hold the oldest pending row in an open transaction, as a worker that
        // claimed it and has not committed yet does.
        let mut held = db.admin.begin().await.unwrap();
        let locked: Uuid = sqlx::query_scalar!(
            r#"SELECT id AS "id!" FROM diagnosis_jobs WHERE status = 'pending'
                ORDER BY created_at FOR UPDATE LIMIT 1"#
        )
        .fetch_one(&mut *held)
        .await
        .unwrap();
        assert_eq!(locked, first);

        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
        let claimed = tokio::time::timeout(Duration::from_secs(2), claim(&handle))
            .await
            .expect("the claim must not wait for another worker's row")
            .unwrap()
            .expect("the claim must take the row that is free");

        assert_eq!(claimed.id, second);
        held.rollback().await.unwrap();
    })
    .await;
}

/// An empty queue gives `None` and costs one read.
#[tokio::test]
async fn an_empty_queue_claims_nothing() {
    TestDb::with(|db| async move {
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
        assert!(claim(&handle).await.unwrap().is_none());
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The dead letter and the lease sweep (spec section 6.1)
// --------------------------------------------------------------------------- //

/// The acceptance literal: three failures dead-letter the row.
///
/// The endpoint answers 500 to everything, so every pass spends its two HTTP
/// attempts and fails. The row goes back to `pending` twice and dead-letters on
/// the third claim.
#[tokio::test]
async fn three_failures_dead_letter_the_row() {
    TestDb::with(|db| async move {
        let user = db.seed_user("dead@example.test").await;
        let id = enqueue(&db.admin, user, "task-1", &payload(None)).await;
        let server = FakeModel::start(Vec::new()).await;
        let mut job = server.job(0);
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);

        assert_eq!(
            run_once(&handle, &mut job).await.unwrap().outcome,
            Outcome::Retry
        );
        assert_eq!(row_of(&db.admin, id).await.0, "pending");

        assert_eq!(
            run_once(&handle, &mut job).await.unwrap().outcome,
            Outcome::Retry
        );
        assert_eq!(row_of(&db.admin, id).await.0, "pending");

        assert_eq!(
            run_once(&handle, &mut job).await.unwrap().outcome,
            Outcome::Failed
        );
        let (status, attempts, result) = row_of(&db.admin, id).await;
        assert_eq!(status, "failed");
        assert_eq!(attempts, 3);
        assert_eq!(result, None, "a dead letter writes no diagnosis");
        assert_eq!(MAX_JOB_ATTEMPTS, 3);
    })
    .await;
}

/// A lease past 5 minutes goes back on the queue; a stale row that already used
/// its three attempts dead-letters instead.
#[tokio::test]
async fn the_sweep_reclaims_a_stale_lease_and_dead_letters_a_spent_row() {
    TestDb::with(|db| async move {
        let user = db.seed_user("sweep@example.test").await;
        let stale = enqueue(&db.admin, user, "stale", &payload(None)).await;
        let spent = enqueue(&db.admin, user, "spent", &payload(None)).await;
        let fresh = enqueue(&db.admin, user, "fresh", &payload(None)).await;

        sqlx::query!(
            "UPDATE diagnosis_jobs SET status = 'running', attempts = 1,
                    claimed_at = now() - interval '6 minutes' WHERE id = $1",
            stale
        )
        .execute(&db.admin)
        .await
        .unwrap();
        sqlx::query!(
            "UPDATE diagnosis_jobs SET status = 'running', attempts = 3,
                    claimed_at = now() - interval '6 minutes' WHERE id = $1",
            spent
        )
        .execute(&db.admin)
        .await
        .unwrap();
        sqlx::query!(
            "UPDATE diagnosis_jobs SET status = 'running', attempts = 1,
                    claimed_at = now() - interval '1 minute' WHERE id = $1",
            fresh
        )
        .execute(&db.admin)
        .await
        .unwrap();

        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
        let (reset, dead) = sweep(&handle).await.unwrap();

        assert_eq!(reset, 1, "one stale lease goes back on the queue");
        assert_eq!(dead, 1, "one spent row dead-letters");
        assert_eq!(row_of(&db.admin, stale).await.0, "pending");
        assert_eq!(row_of(&db.admin, spent).await.0, "failed");
        assert_eq!(row_of(&db.admin, fresh).await.0, "running");
    })
    .await;
}

/// A payload the worker cannot read dead-letters at once. Every retry would
/// reproduce it exactly, so it must not spend two more claims.
#[tokio::test]
async fn an_unreadable_payload_dead_letters_at_once() {
    TestDb::with(|db| async move {
        let user = db.seed_user("payload@example.test").await;
        let id = enqueue(
            &db.admin,
            user,
            "task-1",
            &json!({"v": 1, "nonsense": true}),
        )
        .await;
        let server = FakeModel::start(Vec::new()).await;
        let mut job = server.job(0);
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);

        let report = run_once(&handle, &mut job).await.unwrap();

        assert_eq!(report.outcome, Outcome::Failed);
        assert_eq!(row_of(&db.admin, id).await.0, "failed");
        assert_eq!(row_of(&db.admin, id).await.1, 1);
        assert!(
            server.calls().is_empty(),
            "an unreadable payload calls no model"
        );
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The vocabulary filter and the finished document (spec sections 6.3, 5.3)
// --------------------------------------------------------------------------- //

/// The acceptance literal: a tag outside the vocabulary is dropped, end to end.
#[tokio::test]
async fn a_tag_outside_the_vocabulary_is_dropped_from_the_result() {
    TestDb::with(|db| async move {
        let user = db.seed_user("tags@example.test").await;
        let id = enqueue(&db.admin, user, "task-1", &payload(None)).await;
        let server = FakeModel::start(vec![(
            200,
            tool_reply(
                "{\"error_tags\":[\"sign-error\",\"carelessness\",\"units\"],\
                 \"prose\":\"Watch the sign.\"}",
            ),
        )])
        .await;
        let mut job = server.job(0);
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);

        let report = run_once(&handle, &mut job).await.unwrap();

        assert_eq!(report.outcome, Outcome::Done);
        assert_eq!(
            report.attempts.len(),
            1,
            "T6 bills one row per HTTP attempt"
        );
        let (status, attempts, result) = row_of(&db.admin, id).await;
        assert_eq!(status, "done");
        assert_eq!(attempts, 1);
        let result = result.unwrap();
        assert_eq!(result["error_tags"], json!(["sign-error", "units"]));
        assert_eq!(result["prose"], json!("Watch the sign."));
        assert_eq!(result["model_id"], json!("qwen3.6"));
    })
    .await;
}

/// The prompt names the problem, the reference answer and the learner's answer,
/// and it never asks the model for the verdict (spec section 6.3).
#[tokio::test]
async fn the_user_message_carries_the_attempt_and_not_the_verdict() {
    TestDb::with(|db| async move {
        let user = db.seed_user("prompt@example.test").await;
        enqueue(&db.admin, user, "task-1", &payload(None)).await;
        let server = FakeModel::start(vec![(
            200,
            tool_reply("{\"error_tags\":[],\"prose\":\"Try again.\"}"),
        )])
        .await;
        let mut job = server.job(0);
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);

        run_once(&handle, &mut job).await.unwrap();

        let sent = server.calls();
        assert_eq!(sent.len(), 1);
        let user_text = sent[0]["messages"][1]["content"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(
            user_text.contains("Problem: Compute $8 - 5$."),
            "{user_text}"
        );
        assert!(
            user_text.contains("Correct final answer (reference): 3"),
            "{user_text}"
        );
        assert!(user_text.contains("Learner's answer: '2'"), "{user_text}");
        assert!(
            user_text.contains("Learner's shown work: (none provided)"),
            "{user_text}"
        );
        assert!(
            user_text.contains("The answer is WRONG; the server decided that."),
            "{user_text}"
        );
        assert_eq!(
            sent[0]["tool_choice"]["function"]["name"],
            json!("emit_diagnosis")
        );
    })
    .await;
}

/// The filter is the same function the prompt renders its vocabulary from.
#[test]
fn the_filter_drops_a_tag_the_vocabulary_lacks() {
    assert_eq!(
        filter_tags(&json!(["sign-error", "carelessness", "units"])),
        vec!["sign-error", "units"]
    );
}

// --------------------------------------------------------------------------- //
// The push (D7) and the T4 knobs (spec section 6.6)
// --------------------------------------------------------------------------- //

/// The finished row pushes `NOTIFY diagnosis_done, '<job_id>:<user_id>'`.
///
/// The payload carries ids only: the channel has no row-level security at all
/// (trap W14), so the request tier re-reads the row through `begin_tenant`.
#[tokio::test]
async fn a_finished_job_notifies_with_the_two_ids() {
    TestDb::with(|db| async move {
        let user = db.seed_user("notify@example.test").await;
        let id = enqueue(&db.admin, user, "task-1", &payload(None)).await;
        let server = FakeModel::start(vec![(
            200,
            tool_reply("{\"error_tags\":[\"units\"],\"prose\":\"Name the unit.\"}"),
        )])
        .await;
        let mut job = server.job(0);
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);

        let mut listener = PgListener::connect_with(&db.admin).await.unwrap();
        listener.listen("diagnosis_done").await.unwrap();

        run_once(&handle, &mut job).await.unwrap();

        let notice = tokio::time::timeout(Duration::from_secs(5), listener.recv())
            .await
            .expect("the NOTIFY must arrive")
            .unwrap();
        assert_eq!(notice.channel(), "diagnosis_done");
        assert_eq!(notice.payload(), format!("{id}:{user}"));
    })
    .await;
}

/// A configured per-session cap writes `capped` and calls no model. The learner
/// still holds the verdict and the stock re-solve instruction (spec 6.6).
#[tokio::test]
async fn a_session_cap_writes_capped_and_calls_no_model() {
    TestDb::with(|db| async move {
        let user = db.seed_user("cap@example.test").await;
        let spent = enqueue(&db.admin, user, "task-0", &payload(Some("session-1"))).await;
        sqlx::query!(
            "UPDATE diagnosis_jobs SET status = 'done' WHERE id = $1",
            spent
        )
        .execute(&db.admin)
        .await
        .unwrap();
        let id = enqueue(&db.admin, user, "task-1", &payload(Some("session-1"))).await;

        let server = FakeModel::start(Vec::new()).await;
        let mut job = server.job(1);
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);

        let report = run_once(&handle, &mut job).await.unwrap();

        assert_eq!(report.outcome, Outcome::Capped);
        assert_eq!(row_of(&db.admin, id).await.0, "capped");
        assert!(server.calls().is_empty(), "a capped job calls no model");
    })
    .await;
}

/// The default cap is 0 = unlimited (O2): the same queue runs the call.
#[tokio::test]
async fn the_default_cap_of_zero_refuses_nothing() {
    TestDb::with(|db| async move {
        let user = db.seed_user("uncapped@example.test").await;
        for index in 0..3 {
            let spent = enqueue(
                &db.admin,
                user,
                &format!("old-{index}"),
                &payload(Some("session-1")),
            )
            .await;
            sqlx::query!(
                "UPDATE diagnosis_jobs SET status = 'done' WHERE id = $1",
                spent
            )
            .execute(&db.admin)
            .await
            .unwrap();
        }
        let id = enqueue(&db.admin, user, "task-1", &payload(Some("session-1"))).await;

        let server = FakeModel::start(vec![(
            200,
            tool_reply("{\"error_tags\":[],\"prose\":\"Try again.\"}"),
        )])
        .await;
        let mut job = server.job(0);
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);

        let report = run_once(&handle, &mut job).await.unwrap();

        assert_eq!(report.outcome, Outcome::Done);
        assert_eq!(row_of(&db.admin, id).await.0, "done");
        assert_eq!(server.calls().len(), 1);
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The retry contract, end to end (spec section 6.5)
// --------------------------------------------------------------------------- //

/// A truncated first reply retries with a ×4 ceiling inside ONE claim, and the
/// second reply finishes the job. The row is claimed once, not twice.
#[tokio::test]
async fn a_truncated_reply_finishes_the_job_inside_one_claim() {
    TestDb::with(|db| async move {
        let user = db.seed_user("truncated@example.test").await;
        let id = enqueue(&db.admin, user, "task-1", &payload(None)).await;
        let truncated = json!({
            "choices": [{"finish_reason": "length", "message": {"content": null}}],
            "usage": {"prompt_tokens": 900, "completion_tokens": 600}
        })
        .to_string();
        let server = FakeModel::start(vec![
            (200, truncated),
            (
                200,
                tool_reply("{\"error_tags\":[\"sign-error\"],\"prose\":\"Watch the sign.\"}"),
            ),
        ])
        .await;
        let mut job = server.job(0);
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);

        let report = run_once(&handle, &mut job).await.unwrap();

        let sent = server.calls();
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0]["max_tokens"], json!(600));
        assert_eq!(sent[1]["max_tokens"], json!(2400));
        assert_eq!(report.outcome, Outcome::Done);
        assert_eq!(report.attempts.len(), 2, "two HTTP attempts are two bills");
        let (status, attempts, result) = row_of(&db.admin, id).await;
        assert_eq!(status, "done");
        assert_eq!(attempts, 1, "one claim, whatever the HTTP attempts cost");
        assert_eq!(result.unwrap()["error_tags"], json!(["sign-error"]));
    })
    .await;
}

/// A 400 spends one HTTP attempt and the row goes back on the queue. A refused
/// request body is reproduced exactly by a second copy of itself.
#[tokio::test]
async fn a_four_hundred_spends_one_attempt_and_requeues_the_row() {
    TestDb::with(|db| async move {
        let user = db.seed_user("refused@example.test").await;
        let id = enqueue(&db.admin, user, "task-1", &payload(None)).await;
        let server = FakeModel::start(vec![(
            400,
            json!({"error": {"message": "unknown field provider"}}).to_string(),
        )])
        .await;
        let mut job = server.job(0);
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);

        let report = run_once(&handle, &mut job).await.unwrap();

        assert_eq!(server.calls().len(), 1, "a 400 makes exactly one attempt");
        assert_eq!(report.outcome, Outcome::Retry);
        assert_eq!(row_of(&db.admin, id).await.0, "pending");
        assert_eq!(row_of(&db.admin, id).await.1, 1);
    })
    .await;
}
