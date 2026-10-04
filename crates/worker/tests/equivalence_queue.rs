//! Amendment K (steer note 114): the equivalence worker end to end, against
//! the fake local-model endpoint.
//!
//! The four cases of the owner's design:
//!
//! 1. the owner's case (`not a solution, 5` against `verdict = contradicts;
//!    D = 5`) through a fake model that answers EQUIVALENT: the job settles
//!    done, the verdict lands in the cache, the `regraded` correction is
//!    appended, and the refolded model reads the attempt CORRECT;
//! 2. a cache hit skips the model: the endpoint records zero calls;
//! 3. the timeout path: an endpoint that never answers returns the row to
//!    `pending` (a later pass folds it) and appends no correction;
//! 4. a NOT verdict keeps the deterministic wrong: no correction, the reason
//!    stands in the result and in the cache.
//!
//! Every expected value is a literal of the test that reads it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::Arc;

use cadus_store::test_support::TestDb;
use cadus_worker::equivalence::{EquivalenceJob, Outcome};
use cadus_worker::run_with;
use common::FakeModel;
use serde_json::{Value, json};
use sqlx::types::Uuid;
use sqlx::types::chrono::Utc;

/// A chat-completion body carrying `EQUIVALENT` and the one-line why.
fn equivalent_reply() -> (u16, String) {
    completion("EQUIVALENT\nThe learner's words state the same verdict and the same number.")
}

/// A chat-completion body carrying `NOT` and the one-line why.
fn not_reply() -> (u16, String) {
    completion("NOT\nThe verdict part disagrees with the key.")
}

/// A non-streaming chat-completion reply of the OpenAI shape.
fn completion(content: &str) -> (u16, String) {
    (
        200,
        json!({
            "id": "gen-1",
            "choices": [{"finish_reason": "stop", "message": {
                "role": "assistant", "content": content
            }}],
            "usage": {"prompt_tokens": 120, "completion_tokens": 20}
        })
        .to_string(),
    )
}

/// The payload of the owner's case: `not a solution, 5` against
/// `verdict = contradicts; D = 5`, the item the note-114 design names.
fn owner_payload() -> Value {
    json!({
        "v": 1,
        "task_id": "task-1",
        "topic": "adding-two-digits",
        "item_digest": "abc123def456",
        "problem": "State the verdict and the number of dissenting judges.",
        "expected": "verdict = contradicts; D = 5",
        "answer_contract": "exact",
        "given_answer": "not a solution, 5"
    })
}

/// Seed the WRONG attempt the job corrects, at `attempt-1`.
///
/// The shape is the one the projector's own parity fixture pins (a minimal v2
/// attempt), on the fixture curriculum's `adding-two-digits` topic.
async fn seed_attempt(db: &TestDb, user: Uuid) {
    let now = Utc::now();
    sqlx::query!(
        r#"INSERT INTO events (user_id, seq, ts, type, attempt_id, payload)
           VALUES ($1, 1, $2, 'attempt', 'attempt-1', $3)"#,
        user,
        now,
        json!({
            "type": "attempt",
            "ts": now.to_rfc3339(),
            "attempt_id": "attempt-1",
            "task_id": "task-1",
            "topic": "adding-two-digits",
            "task_type": "lesson",
            "problem": {"text": "State the verdict and the dissenters.", "expected": "verdict = contradicts; D = 5"},
            "given_answer": "not a solution, 5",
            "correct": false,
            "outcome": "incorrect",
            "secs": 12,
            "work_quality": "nearly_passable",
            "v": 2
        }),
    )
    .execute(&db.admin)
    .await
    .unwrap();
}

/// Enqueue one job over the attempt, and return its id.
async fn enqueue(db: &TestDb, user: Uuid) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO equivalence_jobs (user_id, attempt_id, payload) \
         VALUES ($1, 'attempt-1', $2) RETURNING id",
    )
    .bind(user)
    .bind(owner_payload())
    .fetch_one(&db.admin)
    .await
    .unwrap()
}

/// The status and result of one row.
async fn row_of(db: &TestDb, id: Uuid) -> (String, Option<Value>) {
    sqlx::query_as::<_, (String, Option<Value>)>(
        "SELECT status, result FROM equivalence_jobs WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&db.admin)
    .await
    .unwrap()
}

/// The job over the fake endpoint and the fixture curriculum.
fn job_of(fake: &FakeModel) -> EquivalenceJob {
    EquivalenceJob::new(
        cadus_model_client::EquivalenceClient::new(&fake.base_url, "qwen-general-8bit").unwrap(),
        Arc::new(common::pool::arena()),
    )
}

/// (1) The owner's case: EQUIVALENT → done, cached, corrected, logged.
#[tokio::test]
async fn an_equivalent_verdict_folds_the_attempt_correct() {
    TestDb::with(|db| async move {
        let db: &TestDb = db.as_ref();
        let server = FakeModel::start(vec![equivalent_reply()]).await;
        let alice = db.seed_user("owner-case@example.test").await;
        seed_attempt(db, alice).await;
        let id = enqueue(db, alice).await;

        let report = cadus_worker::equivalence::run_once(&common::handle(db), &job_of(&server))
            .await
            .unwrap();
        assert_eq!(report.outcome, Outcome::Accepted, "{report:?}");

        // The row is done with the verdict in its result.
        let (status, result) = row_of(db, id).await;
        assert_eq!(status, "done");
        assert_eq!(result.unwrap()["equivalent"], json!(true));

        // The correction is in the log: the attempt now reads CORRECT.
        let correction = sqlx::query_scalar::<_, Value>(
            "SELECT payload FROM events WHERE user_id = $1 AND type = 'regraded'",
        )
        .bind(alice)
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(correction["attempts"][0]["attempt_id"], json!("attempt-1"));
        assert_eq!(correction["attempts"][0]["outcome"], json!("correct"));

        // The verdict is in the cache: a repeat answers at once.
        let cached = sqlx::query_scalar::<_, bool>(
            "SELECT equivalent FROM equivalence_cache WHERE item_digest = 'abc123def456'",
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert!(cached);
        // The worker writes the NORMALIZED key, the one the web lookup reads.
        let key = sqlx::query_scalar::<_, String>(
            "SELECT answer_key FROM equivalence_cache WHERE item_digest = 'abc123def456'",
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(
            key,
            cadus_store::equivalence::cache_key("not a solution, 5")
        );
        assert_eq!(key, "not a solution. 5");

        // The ledger holds the call (note 114, point 5).
        let logged = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM model_call_log WHERE purpose = 'equivalence'",
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(logged, 1);
    })
    .await;
}

/// (2) A cache hit skips the model: the endpoint records zero calls.
#[tokio::test]
async fn a_cache_hit_skips_the_model() {
    TestDb::with(|db| async move {
        let db: &TestDb = db.as_ref();
        let server = FakeModel::start(Vec::new()).await;
        let alice = db.seed_user("cache-hit@example.test").await;
        seed_attempt(db, alice).await;
        sqlx::query!(
            "INSERT INTO equivalence_cache (item_digest, answer_key, equivalent, reason, model)
             VALUES ('abc123def456', 'not a solution. 5', true, 'same value', 'qwen-general-8bit')"
        )
        .execute(&db.admin)
        .await
        .unwrap();
        let id = enqueue(db, alice).await;

        let report = cadus_worker::equivalence::run_once(&common::handle(db), &job_of(&server))
            .await
            .unwrap();
        assert_eq!(report.outcome, Outcome::Accepted, "{report:?}");
        assert_eq!(
            server.call_count(),
            0,
            "the cache answered, the model never ran"
        );
        let (status, _) = row_of(db, id).await;
        assert_eq!(status, "done");
    })
    .await;
}

/// (3) The timeout path: an endpoint that answers nothing returns the row to
/// `pending`, so a later pass folds the verdict — nothing lands now.
#[tokio::test]
async fn the_timeout_path_returns_the_row_and_lands_nothing() {
    TestDb::with(|db| async move {
        let db: &TestDb = db.as_ref();
        // A 500 on every call: the transport answer is a refusal, the call
        // fails, and the row goes back to the queue.
        let server = FakeModel::start(vec![(500, String::new())]).await;
        let alice = db.seed_user("timeout-path@example.test").await;
        seed_attempt(db, alice).await;
        let id = enqueue(db, alice).await;

        let report = cadus_worker::equivalence::run_once(&common::handle(db), &job_of(&server))
            .await
            .unwrap();
        assert_eq!(report.outcome, Outcome::Failed, "{report:?}");
        let (status, _) = row_of(db, id).await;
        assert_eq!(status, "pending", "the row waits for a later pass");
        let corrections = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM events WHERE user_id = $1 AND type = 'regraded'",
        )
        .bind(alice)
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(corrections, 0, "no fold lands before a verdict");
    })
    .await;
}

/// (4) A NOT verdict keeps the deterministic wrong: no correction, and the
/// reason stands in the result.
#[tokio::test]
async fn a_not_verdict_keeps_the_deterministic_wrong() {
    TestDb::with(|db| async move {
        let db: &TestDb = db.as_ref();
        let server = FakeModel::start(vec![not_reply()]).await;
        let alice = db.seed_user("not-verdict@example.test").await;
        seed_attempt(db, alice).await;
        let id = enqueue(db, alice).await;

        let report = cadus_worker::equivalence::run_once(&common::handle(db), &job_of(&server))
            .await
            .unwrap();
        assert_eq!(report.outcome, Outcome::Refused, "{report:?}");
        let (status, result) = row_of(db, id).await;
        assert_eq!(status, "done");
        let result = result.expect("the refused verdict stands in the result");
        assert_eq!(result["equivalent"], json!(false));
        assert_eq!(
            result["reason"],
            json!("The verdict part disagrees with the key")
        );
        let corrections = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM events WHERE user_id = $1 AND type = 'regraded'",
        )
        .bind(alice)
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(corrections, 0, "the deterministic wrong stands");
    })
    .await;
}

/// The daily cap: past 200 jobs a day the row settles `capped` and no call
/// runs (the model is free; the cap bounds abuse).
#[tokio::test]
async fn the_daily_cap_refuses_the_call() {
    TestDb::with(|db| async move {
        let db: &TestDb = db.as_ref();
        let server = FakeModel::start(Vec::new()).await;
        let alice = db.seed_user("daily-cap@example.test").await;
        seed_attempt(db, alice).await;
        enqueue(db, alice).await;
        for _ in 0..200 {
            sqlx::query!(
                "INSERT INTO equivalence_jobs (user_id, attempt_id, payload, status, finished_at)
                 VALUES ($1, gen_random_uuid()::text, '{}'::jsonb, 'done', now())",
                alice
            )
            .execute(&db.admin)
            .await
            .unwrap();
        }

        let report = cadus_worker::equivalence::run_once(&common::handle(db), &job_of(&server))
            .await
            .unwrap();
        assert_eq!(report.outcome, Outcome::Capped, "{report:?}");
        assert_eq!(server.call_count(), 0, "the cap answered before the model");
    })
    .await;
}

/// The tick loop drains the queue (`run_with` step 6).
#[tokio::test]
async fn the_tick_loop_drains_the_equivalence_queue() {
    TestDb::with(|db| async move {
        let db: &TestDb = db.as_ref();
        let server = FakeModel::start(vec![equivalent_reply()]).await;
        let alice = db.seed_user("tick-loop@example.test").await;
        seed_attempt(db, alice).await;
        enqueue(db, alice).await;

        let job = Arc::new(job_of(&server));
        let cfg = cadus_worker::WorkerConfig {
            tick: std::time::Duration::from_millis(50),
        };
        run_with(
            &common::handle(db),
            &cfg,
            None,
            None,
            Some(&job),
            tokio::time::sleep(std::time::Duration::from_millis(400)),
        )
        .await
        .unwrap();
        let (_, status, _) = sqlx::query_as::<_, (Uuid, String, Option<Value>)>(
            "SELECT id, status, result FROM equivalence_jobs WHERE user_id = $1",
        )
        .bind(alice)
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(status, "done", "the loop finished the queued row");
    })
    .await;
}
/// (7) The local model is unreachable: the hosted fallback answers, the
/// attempt folds correct, and the ledger bills both calls under their models.
#[tokio::test]
async fn an_unreachable_local_model_falls_back_to_the_hosted_model() {
    TestDb::with(|db| async move {
        let db: &TestDb = db.as_ref();
        let hosted = FakeModel::start(vec![equivalent_reply()]).await;
        let alice = db.seed_user("fallback@example.test").await;
        seed_attempt(db, alice).await;
        let id = enqueue(db, alice).await;
        // Port 9 (discard) refuses the connection at once.
        let local = cadus_model_client::EquivalenceClient::new(
            "http://127.0.0.1:9/v1",
            "qwen-general-8bit",
        )
        .unwrap();
        let fallback =
            cadus_model_client::EquivalenceClient::new(&hosted.base_url, "deepseek/deepseek-chat")
                .unwrap()
                .with_key("test-key", vec!["deepinfra".to_owned()]);
        let job =
            EquivalenceJob::new(local, Arc::new(common::pool::arena())).with_fallback(fallback);

        let report = cadus_worker::equivalence::run_once(&common::handle(db), &job)
            .await
            .unwrap();
        assert_eq!(report.outcome, Outcome::Accepted, "{report:?}");
        let (status, result) = row_of(db, id).await;
        assert_eq!(status, "done");
        assert_eq!(result.unwrap()["model"], json!("deepseek/deepseek-chat"));

        // The hosted call carries the provider order and no local template switch.
        let sent = hosted.calls();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0]["provider"]["order"], json!(["deepinfra"]));
        assert!(sent[0].get("chat_template_kwargs").is_none());

        let models = sqlx::query_scalar::<_, String>(
            "SELECT model_id FROM model_call_log WHERE purpose = 'equivalence' ORDER BY id",
        )
        .fetch_all(&db.admin)
        .await
        .unwrap();
        assert_eq!(models, ["qwen-general-8bit", "deepseek/deepseek-chat"]);
    })
    .await;
}
