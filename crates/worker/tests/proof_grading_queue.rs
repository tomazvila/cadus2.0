//! Amendment K point 6: the proof-grading worker end to end, against the fake
//! OpenAI-compatible endpoint.
//!
//! 1. a pass: the job settles done, a `regraded` correction (outcome correct)
//!    lands and the model refolds, and the ledger bills the call;
//! 2. a needs-revision grading: done with the checks, no correction;
//! 3. an unparseable reply: the row returns to the queue, and fails on its
//!    last claim; no correction;
//! 4. the daily cap of 20: the row settles `capped` with no model call;
//! 5. a human correction already standing: the pass settles, no second fold.
//!
//! Every expected value is a literal of the test that reads it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::Arc;

use cadus_store::test_support::TestDb;
use cadus_worker::proof_grading::{MAX_ATTEMPTS, Outcome, ProofGradingJob, run_once};
use common::FakeModel;
use common::fake::reply;
use serde_json::{Value, json};
use sqlx::types::Uuid;
use sqlx::types::chrono::Utc;

/// The five general checks and five specific ones, every one met unless its
/// id is in `unmet`.
fn grading(unmet: &[&str]) -> String {
    let mut checks = Vec::new();
    for n in 1..=5 {
        checks.push(
            json!({"id": format!("G{n}"), "text": format!("general {n}"),
                           "minor": false, "met": true, "evidence": "Let a = 2k+1"}),
        );
    }
    for n in 1..=5 {
        checks.push(
            json!({"id": format!("S{n}"), "text": format!("specific {n}"),
                           "minor": false, "met": true, "evidence": "a + b = 2(k+m+1)"}),
        );
    }
    for check in &mut checks {
        if unmet.contains(&check["id"].as_str().unwrap()) {
            check["met"] = json!(false);
            check["evidence"] = json!("not found");
        }
    }
    json!({"checks": checks, "feedback": "Step 3 is not justified."}).to_string()
}

/// The payload of one written proof.
fn payload() -> Value {
    json!({
        "v": 1,
        "task_id": "task-1",
        "topic": "adding-two-digits",
        "item_digest": "abc123def456",
        "problem": "Write the full proof: the sum of two odd integers is even.",
        "reference": "Write a = 2k+1, b = 2m+1; then a+b = 2(k+m+1).",
        "given_answer": "Let a = 2k+1 and b = 2m+1. Then a + b = 2(k+m+1), which is even."
    })
}

/// Seed the UNGRADED attempt the job grades, at `attempt-1`.
async fn seed_attempt(db: &TestDb, user: Uuid) {
    let now = Utc::now();
    sqlx::query(
        r#"INSERT INTO events (user_id, seq, ts, type, attempt_id, payload)
           VALUES ($1, 1, $2, 'attempt', 'attempt-1', $3)"#,
    )
    .bind(user)
    .bind(now)
    .bind(json!({
        "type": "attempt",
        "ts": now.to_rfc3339(),
        "attempt_id": "attempt-1",
        "task_id": "task-1",
        "topic": "adding-two-digits",
        "task_type": "lesson",
        "problem": {"text": "Write the full proof.", "expected": "See the solution."},
        "given_answer": "Let a = 2k+1 and b = 2m+1.",
        "correct": false,
        "outcome": {"ungraded": {"reason": "no deterministic verdict for a proof"}},
        "secs": 120,
        "work_quality": "nearly_passable",
        "v": 2
    }))
    .execute(&db.admin)
    .await
    .unwrap();
}

/// Enqueue one job over the attempt, and return its id.
async fn enqueue(db: &TestDb, user: Uuid) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO proof_grading_jobs (user_id, attempt_id, payload) \
         VALUES ($1, 'attempt-1', $2) RETURNING id",
    )
    .bind(user)
    .bind(payload())
    .fetch_one(&db.admin)
    .await
    .unwrap()
}

/// The status and result of one row.
async fn row_of(db: &TestDb, id: Uuid) -> (String, Option<Value>) {
    sqlx::query_as::<_, (String, Option<Value>)>(
        "SELECT status, result FROM proof_grading_jobs WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&db.admin)
    .await
    .unwrap()
}

/// The count of `regraded` events of one learner.
async fn corrections(db: &TestDb, user: Uuid) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM events WHERE user_id = $1 AND type = 'regraded'",
    )
    .bind(user)
    .fetch_one(&db.admin)
    .await
    .unwrap()
}

/// The job over the fake endpoint and the fixture curriculum.
fn job_of(fake: &FakeModel) -> ProofGradingJob {
    ProofGradingJob::new(fake.client(4000, 2000), Arc::new(common::pool::arena()))
}

/// (1) A pass folds the attempt correct and bills the call.
#[tokio::test]
async fn a_passed_proof_folds_the_attempt_correct() {
    TestDb::with(|db| async move {
        let db: &TestDb = db.as_ref();
        let server = FakeModel::start(vec![reply("grade_proof", &grading(&[]), None)]).await;
        let alice = db.seed_user("proof-pass@example.test").await;
        seed_attempt(db, alice).await;
        let id = enqueue(db, alice).await;

        let report = run_once(&common::handle(db), &job_of(&server))
            .await
            .unwrap();
        assert_eq!(report.outcome, Outcome::Passed, "{report:?}");

        let (status, result) = row_of(db, id).await;
        assert_eq!(status, "done");
        let result = result.unwrap();
        assert_eq!(result["verdict"], json!("pass"));
        assert_eq!(result["checks"].as_array().unwrap().len(), 10);
        assert_eq!(result["model"], json!("qwen3.6"));

        let correction = sqlx::query_scalar::<_, Value>(
            "SELECT payload FROM events WHERE user_id = $1 AND type = 'regraded'",
        )
        .bind(alice)
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(correction["attempts"][0]["attempt_id"], json!("attempt-1"));
        assert_eq!(correction["attempts"][0]["outcome"], json!("correct"));

        // The refold wrote the learner model.
        let models =
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM learner_models WHERE user_id = $1")
                .bind(alice)
                .fetch_one(&db.admin)
                .await
                .unwrap();
        assert_eq!(models, 1);

        // The ledger holds the call; the request forced the grading tool and
        // carried the learner text.
        let logged = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM model_call_log WHERE purpose = 'proof_grading'",
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(logged, 1);
        let sent = server.calls();
        assert_eq!(
            sent[0]["tool_choice"]["function"]["name"],
            json!("grade_proof")
        );
        assert!(server.user_message(0).contains("<<<LEARNER\nLet a = 2k+1"));
    })
    .await;
}

/// (2) A needs-revision grading stores the checks and leaves the attempt.
#[tokio::test]
async fn a_needs_revision_grading_keeps_the_attempt_ungraded() {
    TestDb::with(|db| async move {
        let db: &TestDb = db.as_ref();
        let server =
            FakeModel::start(vec![reply("grade_proof", &grading(&["G2", "S3"]), None)]).await;
        let alice = db.seed_user("proof-revise@example.test").await;
        seed_attempt(db, alice).await;
        let id = enqueue(db, alice).await;

        let report = run_once(&common::handle(db), &job_of(&server))
            .await
            .unwrap();
        assert_eq!(report.outcome, Outcome::NeedsRevision, "{report:?}");
        let (status, result) = row_of(db, id).await;
        assert_eq!(status, "done");
        let result = result.unwrap();
        assert_eq!(result["verdict"], json!("needs_revision"));
        assert_eq!(result["feedback"], json!("Step 3 is not justified."));
        assert_eq!(corrections(db, alice).await, 0);
    })
    .await;
}

/// (3) An unparseable reply returns the row to the queue; the last claim
/// fails it. Nothing folds.
#[tokio::test]
async fn an_unparseable_reply_retries_then_fails() {
    TestDb::with(|db| async move {
        let db: &TestDb = db.as_ref();
        // The tool call parses, but the grading misses its general checks.
        let broken = json!({"checks": [{"id": "S1", "text": "x", "minor": false, "met": true, "evidence": "q"}],
                            "feedback": "fine"})
        .to_string();
        let server = FakeModel::start(vec![
            reply("grade_proof", &broken, None),
            reply("grade_proof", &broken, None),
            (200, "not json".to_owned()),
        ])
        .await;
        let alice = db.seed_user("proof-broken@example.test").await;
        seed_attempt(db, alice).await;
        let id = enqueue(db, alice).await;
        let job = job_of(&server);

        let report = run_once(&common::handle(db), &job).await.unwrap();
        assert_eq!(report.outcome, Outcome::Failed, "{report:?}");
        assert_eq!(row_of(db, id).await.0, "pending", "the row waits for a later pass");
        for _ in 1..MAX_ATTEMPTS {
            let report = run_once(&common::handle(db), &job).await.unwrap();
            assert_eq!(report.outcome, Outcome::Failed, "{report:?}");
        }
        let (status, result) = row_of(db, id).await;
        assert_eq!(status, "failed");
        assert!(result.unwrap()["error"].is_string());
        assert_eq!(corrections(db, alice).await, 0);
        // Every HTTP attempt is billed.
        let logged = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM model_call_log WHERE purpose = 'proof_grading'",
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(logged, i64::try_from(server.call_count()).unwrap());
    })
    .await;
}

/// (4) Past 20 jobs a day the row settles `capped` and no call runs.
#[tokio::test]
async fn the_daily_cap_refuses_the_call() {
    TestDb::with(|db| async move {
        let db: &TestDb = db.as_ref();
        let server = FakeModel::start(Vec::new()).await;
        let alice = db.seed_user("proof-cap@example.test").await;
        seed_attempt(db, alice).await;
        let id = enqueue(db, alice).await;
        for _ in 0..20 {
            sqlx::query(
                "INSERT INTO proof_grading_jobs (user_id, attempt_id, payload, status, finished_at)
                 VALUES ($1, gen_random_uuid()::text, '{}'::jsonb, 'done', now())",
            )
            .bind(alice)
            .execute(&db.admin)
            .await
            .unwrap();
        }

        let report = run_once(&common::handle(db), &job_of(&server))
            .await
            .unwrap();
        assert_eq!(report.outcome, Outcome::Capped, "{report:?}");
        assert_eq!(server.call_count(), 0, "the cap answered before the model");
        assert_eq!(row_of(db, id).await.0, "capped");
        assert_eq!(corrections(db, alice).await, 0);
    })
    .await;
}

/// (5) A human correction already stands: the pass settles and adds none.
#[tokio::test]
async fn a_standing_correction_is_never_superseded() {
    TestDb::with(|db| async move {
        let db: &TestDb = db.as_ref();
        let server = FakeModel::start(vec![reply("grade_proof", &grading(&[]), None)]).await;
        let alice = db.seed_user("proof-human@example.test").await;
        seed_attempt(db, alice).await;
        let now = Utc::now();
        sqlx::query(
            r#"INSERT INTO events (user_id, seq, ts, type, payload)
               VALUES ($1, 2, $2, 'regraded', $3)"#,
        )
        .bind(alice)
        .bind(now)
        .bind(json!({
            "type": "regraded", "ts": now.to_rfc3339(), "v": 2,
            "task_id": "task-1", "topic": "adding-two-digits",
            "attempts": [{"attempt_id": "attempt-1", "outcome": "incorrect",
                          "work_quality": "nearly_passable", "error_tags": []}],
            "reason": "a human read the proof"
        }))
        .execute(&db.admin)
        .await
        .unwrap();
        let id = enqueue(db, alice).await;

        let report = run_once(&common::handle(db), &job_of(&server))
            .await
            .unwrap();
        assert_eq!(report.outcome, Outcome::Passed, "{report:?}");
        assert_eq!(row_of(db, id).await.0, "done");
        assert_eq!(
            corrections(db, alice).await,
            1,
            "the human correction stands alone"
        );
    })
    .await;
}

/// (6) A reply with no usable grading is asked again inside the same claim.
#[tokio::test]
async fn an_unusable_reply_is_asked_again_in_the_same_claim() {
    TestDb::with(|db| async move {
        let db: &TestDb = db.as_ref();
        let broken = json!({"checks": [], "feedback": "fine"}).to_string();
        let server = FakeModel::start(vec![
            reply("grade_proof", &broken, None),
            reply("grade_proof", &grading(&[]), None),
        ])
        .await;
        let alice = db.seed_user("proof-reask@example.test").await;
        seed_attempt(db, alice).await;
        let id = enqueue(db, alice).await;

        let report = run_once(&common::handle(db), &job_of(&server))
            .await
            .unwrap();
        assert_eq!(report.outcome, Outcome::Passed, "{report:?}");
        assert_eq!(server.call_count(), 2);
        assert_eq!(report.attempts.len(), 2);
        assert_eq!(row_of(db, id).await.0, "done");
    })
    .await;
}

/// (7) A hanging proof call never stalls the tick loop: the equivalence pass
/// finishes its row while the proof call is still waiting, and the shutdown
/// ends both loops at once.
#[tokio::test]
async fn a_hanging_proof_call_does_not_stall_the_other_passes() {
    TestDb::with(|db| async move {
        let db: &TestDb = db.as_ref();
        let slow = FakeModel::start_with_delay(
            vec![reply("grade_proof", &grading(&[]), None)],
            std::time::Duration::from_secs(60),
        )
        .await;
        let fast = FakeModel::start(vec![(
            200,
            json!({"choices": [{"finish_reason": "stop", "message": {"role": "assistant",
                    "content": "EQUIVALENT\nThe same value."}}],
                   "usage": {"prompt_tokens": 10, "completion_tokens": 2}})
            .to_string(),
        )])
        .await;
        let alice = db.seed_user("proof-concurrent@example.test").await;
        seed_attempt(db, alice).await;
        let proof_id = enqueue(db, alice).await;
        let equivalence_id = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO equivalence_jobs (user_id, attempt_id, payload) VALUES ($1, 'attempt-1', $2) RETURNING id",
        )
        .bind(alice)
        .bind(json!({
            "v": 1, "task_id": "task-1", "topic": "adding-two-digits",
            "item_digest": "abc123def456", "problem": "Compute 8 + 5.",
            "expected": "13", "given_answer": "thirteen"
        }))
        .fetch_one(&db.admin)
        .await
        .unwrap();

        let proofs = ProofGradingJob::new(
            {
                let mut client = slow.client(4000, 2000).config().clone();
                client.timeout = std::time::Duration::from_secs(120);
                cadus_model_client::Client::new(client).unwrap()
            },
            Arc::new(common::pool::arena()),
        );
        let equivalence = cadus_worker::EquivalenceJob::new(
            cadus_model_client::EquivalenceClient::new(&fast.base_url, "qwen-general-8bit").unwrap(),
            Arc::new(common::pool::arena()),
        );
        let cfg = cadus_worker::WorkerConfig {
            tick: std::time::Duration::from_millis(50),
        };
        let started = std::time::Instant::now();
        cadus_worker::run_with_proofs(
            &common::handle(db),
            &cfg,
            None,
            None,
            Some(&equivalence),
            Some(&proofs),
            tokio::time::sleep(std::time::Duration::from_millis(1500)),
        )
        .await
        .unwrap();
        assert!(started.elapsed() < std::time::Duration::from_secs(10), "the shutdown waited on the proof call");
        assert_eq!(slow.call_count(), 1, "the proof call was in flight");
        let equivalence_status = sqlx::query_scalar::<_, String>(
            "SELECT status FROM equivalence_jobs WHERE id = $1",
        )
        .bind(equivalence_id)
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(equivalence_status, "done", "the equivalence pass ran beside the hanging proof call");
        assert_eq!(row_of(db, proof_id).await.0, "running", "the proof row waits for its lease");
    })
    .await;
}
