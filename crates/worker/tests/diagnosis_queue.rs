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
//! `diagnosis_ledger.rs` holds the U11 checks of the model-call ledger.
//!
//! Every expected value is a LITERAL: a literal status string, a literal tag
//! list, a literal row count, a literal NOTIFY payload. Nothing is read back
//! from the code under test.
//!
//! The endpoint is a fake OpenAI-compatible server (`common::FakeModel`). No
//! test reaches a real provider.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::time::Duration;

use cadus_store::test_support::TestDb;
use cadus_worker::diagnosis::{MAX_JOB_ATTEMPTS, Outcome, claim, filter_tags, sweep};
use serde_json::json;
use sqlx::PgPool;
use sqlx::postgres::PgListener;
use sqlx::types::Uuid;

use common::{
    FakeModel, diagnose, diagnose_to, diagnosis_reply, enqueue, handle, payload, row_of,
    session_with_spent,
};

/// Mark one claimed row `running` with this attempt count, claimed this many
/// minutes ago.
async fn lease(pool: &PgPool, id: Uuid, attempts: i32, minutes_ago: i32) {
    sqlx::query(
        "UPDATE diagnosis_jobs SET status = 'running', attempts = $2,
                claimed_at = now() - make_interval(mins => $3) WHERE id = $1",
    )
    .bind(id)
    .bind(attempts)
    .bind(minutes_ago)
    .execute(pool)
    .await
    .unwrap();
}

/// One learner with one queued row of this payload, and its id.
async fn one_job(db: &TestDb, email: &str, body: &serde_json::Value) -> (Uuid, Uuid) {
    let user = db.seed_user(email).await;
    let id = enqueue(&db.admin, user, "task-1", body).await;
    (user, id)
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

        let one = handle(&db);
        let two = handle(&db);
        let drain = |handle: cadus_store::Db| async move {
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

        let running = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM diagnosis_jobs WHERE status = 'running' AND attempts = 1",
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
        let (user, first) = one_job(&db, "locked@example.test", &payload(None)).await;
        let second = enqueue(&db.admin, user, "task-2", &payload(None)).await;

        // Hold the oldest pending row in an open transaction, as a worker that
        // claimed it and has not committed yet does.
        let mut held = db.admin.begin().await.unwrap();
        let locked: Uuid = sqlx::query_scalar(
            "SELECT id FROM diagnosis_jobs WHERE status = 'pending'
                ORDER BY created_at FOR UPDATE LIMIT 1",
        )
        .fetch_one(&mut *held)
        .await
        .unwrap();
        assert_eq!(locked, first);

        let claimed = tokio::time::timeout(Duration::from_secs(2), claim(&handle(&db)))
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
        assert!(claim(&handle(&db)).await.unwrap().is_none());
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
        let (_, id) = one_job(&db, "dead@example.test", &payload(None)).await;
        let server = FakeModel::start(Vec::new()).await;

        for _ in 0..2 {
            diagnose_to(&db, &server, 0, id, Outcome::Retry, "pending").await;
        }

        diagnose_to(&db, &server, 0, id, Outcome::Failed, "failed").await;
        let (_, attempts, result) = row_of(&db.admin, id).await;
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
        let (user, stale) = one_job(&db, "sweep@example.test", &payload(None)).await;
        let spent = enqueue(&db.admin, user, "spent", &payload(None)).await;
        let fresh = enqueue(&db.admin, user, "fresh", &payload(None)).await;
        lease(&db.admin, stale, 1, 6).await;
        lease(&db.admin, spent, 3, 6).await;
        lease(&db.admin, fresh, 1, 1).await;

        let (reset, dead) = sweep(&handle(&db)).await.unwrap();

        assert_eq!(reset, 1, "one stale lease goes back on the queue");
        assert_eq!(dead, 1, "one spent row dead-letters");
        assert_eq!(row_of(&db.admin, stale).await.0, "pending");
        assert_eq!(row_of(&db.admin, spent).await.0, "failed");
        assert_eq!(row_of(&db.admin, fresh).await.0, "running");
    })
    .await;
}

/// The pass runs the sweep before its claim, so a stale lease goes back on the
/// queue and the same pass takes it.
#[tokio::test]
async fn the_pass_sweeps_a_stale_lease_before_it_claims() {
    TestDb::with(|db| async move {
        let (_, stale) = one_job(&db, "swept@example.test", &payload(None)).await;
        lease(&db.admin, stale, 1, 6).await;
        let server = FakeModel::start(vec![diagnosis_reply(
            "{\"error_tags\":[],\"prose\":\"Try again.\"}",
        )])
        .await;

        let report = diagnose(&db, &server, 0).await;

        assert_eq!(report.outcome, Outcome::Done);
        assert_eq!(report.job_id, Some(stale));
        let (status, attempts, _) = row_of(&db.admin, stale).await;
        assert_eq!(status, "done");
        assert_eq!(
            attempts, 2,
            "the sweep reset the lease, and the claim raised the count"
        );
    })
    .await;
}

/// A payload the worker cannot read dead-letters at once. Every retry would
/// reproduce it exactly, so it must not spend two more claims. So does a payload
/// of a version this worker does not know.
#[tokio::test]
async fn an_unreadable_payload_dead_letters_at_once() {
    TestDb::with(|db| async move {
        let mut future = payload(None);
        future["v"] = json!(2);
        let (user, unreadable) = one_job(
            &db,
            "payload@example.test",
            &json!({"v": 1, "nonsense": true}),
        )
        .await;
        let unknown = enqueue(&db.admin, user, "task-2", &future).await;
        let server = FakeModel::start(Vec::new()).await;

        for id in [unreadable, unknown] {
            let report = diagnose_to(&db, &server, 0, id, Outcome::Failed, "failed").await;

            assert_eq!(report.job_id, Some(id));
            assert_eq!(row_of(&db.admin, id).await.1, 1);
        }
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
        let (_, id) = one_job(&db, "tags@example.test", &payload(None)).await;
        let server = FakeModel::start(vec![diagnosis_reply(
            "{\"error_tags\":[\"sign-error\",\"carelessness\",\"units\"],\
             \"prose\":\"Watch the sign.\"}",
        )])
        .await;

        let report = diagnose_to(&db, &server, 0, id, Outcome::Done, "done").await;

        assert_eq!(
            report.attempts.len(),
            1,
            "T6 bills one row per HTTP attempt"
        );
        let (_, attempts, result) = row_of(&db.admin, id).await;
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
        one_job(&db, "prompt@example.test", &payload(None)).await;
        let server = FakeModel::start(vec![diagnosis_reply(
            "{\"error_tags\":[],\"prose\":\"Try again.\"}",
        )])
        .await;

        diagnose(&db, &server, 0).await;

        let sent = server.calls();
        assert_eq!(sent.len(), 1);
        let user_text = server.user_message(0);
        for line in [
            "Problem: Compute $8 - 5$.",
            "Correct final answer (reference): 3",
            "Learner's answer: '2'",
            "Learner's shown work: (none provided)",
            "The answer is WRONG; the server decided that.",
        ] {
            assert!(user_text.contains(line), "{user_text}");
        }
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
        let (user, id) = one_job(&db, "notify@example.test", &payload(None)).await;
        let server = FakeModel::start(vec![diagnosis_reply(
            "{\"error_tags\":[\"units\"],\"prose\":\"Name the unit.\"}",
        )])
        .await;

        let mut listener = PgListener::connect_with(&db.admin).await.unwrap();
        listener.listen("diagnosis_done").await.unwrap();

        diagnose(&db, &server, 0).await;

        let notice = tokio::time::timeout(Duration::from_secs(5), listener.recv())
            .await
            .expect("the NOTIFY must arrive")
            .unwrap();
        assert_eq!(notice.channel(), "diagnosis_done");
        assert_eq!(notice.payload(), format!("{id}:{user}"));
    })
    .await;
}

/// One pass over a learner of `session-1` with `spent` calls behind it, under
/// this cap: the report and the endpoint.
async fn cap_pass(
    db: &TestDb,
    email: &str,
    spent: u32,
    cap: u32,
) -> (cadus_worker::DiagnosisReport, FakeModel, Uuid) {
    let (_, id) = session_with_spent(db, email, spent).await;
    let server = FakeModel::start(vec![diagnosis_reply(
        "{\"error_tags\":[],\"prose\":\"Try again.\"}",
    )])
    .await;
    let report = diagnose(db, &server, cap).await;
    (report, server, id)
}

/// A configured per-session cap writes `capped` and calls no model. The learner
/// still holds the verdict and the stock re-solve instruction (spec 6.6).
#[tokio::test]
async fn a_session_cap_writes_capped_and_calls_no_model() {
    TestDb::with(|db| async move {
        let (report, server, id) = cap_pass(&db, "cap@example.test", 1, 1).await;

        assert_eq!(report.outcome, Outcome::Capped);
        assert_eq!(row_of(&db.admin, id).await.0, "capped");
        assert!(server.calls().is_empty(), "a capped job calls no model");
    })
    .await;
}

/// One pass under a cap that refuses nothing: the row is done after one call.
async fn cap_runs(db: &TestDb, email: &str, spent: u32, cap: u32) {
    let (report, server, id) = cap_pass(db, email, spent, cap).await;
    assert_eq!(report.outcome, Outcome::Done);
    assert_eq!(row_of(&db.admin, id).await.0, "done");
    assert_eq!(server.calls().len(), 1);
}

/// The default cap is 0 = unlimited (O2): the same queue runs the call.
#[tokio::test]
async fn the_default_cap_of_zero_refuses_nothing() {
    TestDb::with(|db| async move {
        cap_runs(&db, "uncapped@example.test", 3, 0).await;
    })
    .await;
}

/// A cap the session has not reached runs the call: one call was spent, the
/// cap is two, and the second call goes through.
#[tokio::test]
async fn a_cap_the_session_has_not_reached_runs_the_call() {
    TestDb::with(|db| async move {
        cap_runs(&db, "under-cap@example.test", 1, 2).await;
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
        let (_, id) = one_job(&db, "truncated@example.test", &payload(None)).await;
        let truncated = json!({
            "choices": [{"finish_reason": "length", "message": {"content": null}}],
            "usage": {"prompt_tokens": 900, "completion_tokens": 600}
        })
        .to_string();
        let server = FakeModel::start(vec![
            (200, truncated),
            diagnosis_reply("{\"error_tags\":[\"sign-error\"],\"prose\":\"Watch the sign.\"}"),
        ])
        .await;

        let report = diagnose_to(&db, &server, 0, id, Outcome::Done, "done").await;

        let sent = server.calls();
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0]["max_tokens"], json!(600));
        assert_eq!(sent[1]["max_tokens"], json!(2400));
        assert_eq!(report.attempts.len(), 2, "two HTTP attempts are two bills");
        let (_, attempts, result) = row_of(&db.admin, id).await;
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
        let (_, id) = one_job(&db, "refused@example.test", &payload(None)).await;
        let server = FakeModel::start(vec![(
            400,
            json!({"error": {"message": "unknown field provider"}}).to_string(),
        )])
        .await;

        diagnose_to(&db, &server, 0, id, Outcome::Retry, "pending").await;

        assert_eq!(server.calls().len(), 1, "a 400 makes exactly one attempt");
        assert_eq!(row_of(&db.admin, id).await.1, 1);
    })
    .await;
}
