//! M5 U11 acceptance: the T6 model-call ledger of the diagnosis worker (T6).
//!
//! Row U11 names three checks, and each one is a test here:
//!
//! 6. a reply with no `usage` block writes a zeros row with a NULL cost;
//! 7. a truncation retry writes two rows;
//! 8. `cadus_app` cannot read, write, or `nextval` the table or its sequence.
//!
//! The last section holds one more check, from review round 2: a `None` refill
//! job still runs the diagnosis pass (finding V7).
//!
//! Every expected value is a LITERAL: a literal row, a literal token count, a
//! literal SQLSTATE. Nothing is read back from the code under test.
//!
//! Every endpoint is `common::FakeModel`; no test reaches a real provider.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_store::test_support::TestDb;
use cadus_worker::diagnosis::Outcome;
use cadus_worker::{WorkerConfig, run_with};
use serde_json::{Value, json};
use sqlx::PgPool;
use sqlx::types::Uuid;

use common::{
    FakeModel, Ledger, diagnose, diagnose_to, diagnosis_reply, enqueue, handle, ledger,
    ledger_clock, payload, row_of, session_with_spent,
};

/// The arguments of one accepted diagnosis: the sign.
const SIGN_ERROR: &str = "{\"error_tags\":[\"sign-error\"],\"prose\":\"Watch the sign.\"}";

/// A `200` reply that carries the sign-error diagnosis and this `usage` block,
/// under this request id.
fn full_reply(id: &str, provider: Option<&str>, usage: Option<Value>) -> (u16, String) {
    let mut body = json!({
        "id": id,
        "choices": [{"finish_reason": "tool_calls", "message": {"tool_calls": [{"function": {
            "name": "emit_diagnosis",
            "arguments": SIGN_ERROR
        }}]}}]
    });
    if let Some(provider) = provider {
        body["provider"] = json!(provider);
    }
    if let Some(usage) = usage {
        body["usage"] = usage;
    }
    (200, body.to_string())
}

/// A `usage` block of 500 prompt tokens with 200 cached, and 60 completion
/// tokens with 40 reasoning tokens inside them.
fn detailed_usage() -> Value {
    json!({
        "prompt_tokens": 500,
        "prompt_tokens_details": {"cached_tokens": 200},
        "completion_tokens": 60,
        "completion_tokens_details": {"reasoning_tokens": 40}
    })
}

/// One learner with one queued row of session `session-1`, and the row's id.
async fn one_job(db: &TestDb, email: &str) -> (Uuid, Uuid) {
    let user = db.seed_user(email).await;
    let id = enqueue(&db.admin, user, "task-1", &payload(Some("session-1"))).await;
    (user, id)
}

/// The SQLSTATE of one refused statement, run as `cadus_app`.
async fn refused_code(pool: &PgPool, statement: &str) -> Option<String> {
    let err = sqlx::query(sqlx::AssertSqlSafe(statement.to_owned()))
        .execute(pool)
        .await
        .unwrap_err();
    err.as_database_error()
        .and_then(|e| e.code())
        .map(|code| code.into_owned())
}

/// The U11 acceptance literal: a reply with NO `usage` block writes a zeros row
/// with a NULL cost.
///
/// An unmeasured call must be visible AS unmeasured, so the row is written and
/// the money column is NULL. A dropped row hides a call the operator paid
/// for.
#[tokio::test]
async fn a_reply_with_no_usage_block_writes_a_zeros_row_with_a_null_cost() {
    TestDb::with(|db| async move {
        let (user, id) = one_job(&db, "nousage@example.test").await;
        let server = FakeModel::start(vec![full_reply("gen-bare", None, None)]).await;

        diagnose_to(&db, &server, 0, id, Outcome::Done, "done").await;

        assert_eq!(
            ledger(&db.admin).await,
            vec![Ledger::diagnosis(user, "session-1", 0, 0, "gen-bare")]
        );
    })
    .await;
}

/// The U11 acceptance literal: a truncation retry writes TWO rows.
///
/// A truncation retry is two calls and two bills. The first row carries the
/// truncated attempt's own tokens, and its `ts` is the OLDER one: the client
/// waited 500 ms between the two attempts, so the two stamps are at least that
/// far apart and they stand in the order the attempts ran.
#[tokio::test]
async fn a_truncation_retry_writes_two_ledger_rows() {
    TestDb::with(|db| async move {
        let (user, id) = one_job(&db, "twobills@example.test").await;
        let truncated = json!({
            "id": "gen-cut",
            "choices": [{"finish_reason": "length", "message": {"content": null}}],
            "usage": {"prompt_tokens": 900, "completion_tokens": 600}
        })
        .to_string();
        let server = FakeModel::start(vec![(200, truncated), diagnosis_reply(SIGN_ERROR)]).await;

        diagnose_to(&db, &server, 0, id, Outcome::Done, "done").await;

        let rows = ledger(&db.admin).await;
        assert_eq!(rows.len(), 2, "two HTTP attempts are two rows");
        assert_eq!(
            rows,
            vec![
                Ledger::diagnosis(user, "session-1", 900, 600, "gen-cut"),
                Ledger::diagnosis(user, "session-1", 500, 60, "gen-1"),
            ]
        );

        let clock = ledger_clock(&db.admin).await;
        let gap = clock[1].0 - clock[0].0;
        assert!(
            gap.num_milliseconds() >= 500,
            "the 500 ms backoff sits between the two starts, not {gap:?}"
        );
    })
    .await;
}

/// The token reader: cached, uncached, output and reasoning, from one reply.
///
/// `prompt_tokens` 500 with 200 cached leaves 300 uncached. `completion_tokens`
/// 60 holds 40 reasoning tokens inside it, so the visible output is 20 and the
/// two columns count disjoint tokens. `usage.cost` reaches `numeric(12,6)` as
/// its own text, so the money column reads back exactly.
#[tokio::test]
async fn the_ledger_reads_every_token_field_of_one_reply() {
    TestDb::with(|db| async move {
        let user = db.seed_user("tokens@example.test").await;
        enqueue(&db.admin, user, "task-1", &payload(Some("session-7"))).await;
        let mut usage = detailed_usage();
        usage["cost"] = json!(0.001234);
        let server =
            FakeModel::start(vec![full_reply("gen-full", Some("DeepInfra"), Some(usage))]).await;

        let report = diagnose(&db, &server, 0).await;

        assert_eq!(report.outcome, Outcome::Done);
        assert_eq!(
            ledger(&db.admin).await,
            vec![Ledger {
                provider: Some("DeepInfra".to_string()),
                cached: 200,
                uncached: 300,
                output: 20,
                reasoning: 40,
                cost: Some("0.001234".to_string()),
                ..Ledger::diagnosis(user, "session-7", 300, 20, "gen-full")
            }]
        );
        let clock = ledger_clock(&db.admin).await;
        assert_eq!(clock.len(), 1);
        assert!(
            clock[0].1 >= 0,
            "the row carries the wall clock of its call"
        );
    })
    .await;
}

/// One row per HTTP ATTEMPT, not per job that finished.
///
/// Two 500s are two calls the operator pays for and no diagnosis at all, so the
/// ledger holds two rows while the queue holds one pending row.
#[tokio::test]
async fn two_failed_attempts_write_two_ledger_rows() {
    TestDb::with(|db| async move {
        let user = db.seed_user("failedbills@example.test").await;
        let id = enqueue(&db.admin, user, "task-1", &payload(None)).await;
        let upstream = || (500, json!({"error": "upstream"}).to_string());
        let server = FakeModel::start(vec![upstream(), upstream()]).await;

        diagnose_to(&db, &server, 0, id, Outcome::Retry, "pending").await;

        let rows = ledger(&db.admin).await;
        assert_eq!(rows.len(), 2, "two paid attempts are two rows");
        for row in &rows {
            assert_eq!(row.purpose, "diagnosis");
            assert_eq!(row.user_id, Some(user));
            assert_eq!(row.session_id, None, "this payload names no session");
            assert_eq!(
                (row.cached, row.uncached, row.output, row.reasoning),
                (0, 0, 0, 0)
            );
            assert_eq!(
                row.cost, None,
                "an unpriced attempt is a NULL, never a guess"
            );
        }
    })
    .await;
}

/// A capped job makes no HTTP call, so it writes no ledger row (T4).
#[tokio::test]
async fn a_capped_job_writes_no_ledger_row() {
    TestDb::with(|db| async move {
        let (_, id) = session_with_spent(&db, "capbill@example.test", 1).await;
        let server = FakeModel::start(Vec::new()).await;

        diagnose_to(&db, &server, 1, id, Outcome::Capped, "capped").await;

        assert!(ledger(&db.admin).await.is_empty(), "no call, no bill");
    })
    .await;
}

/// The U11 acceptance literal: `cadus_app` cannot read, write, or `nextval` the
/// ledger or its sequence — with rows in it.
///
/// `42501` is `insufficient_privilege`. The table stays outside row-level
/// security on this basis (`docs/SCHEMA.md`, findings #5 and #12): the runtime
/// role reaches no row of it with any statement. The two aggregate readers of
/// migration 0009 are the one exception, and they hand back sums by purpose and
/// no row at all.
#[tokio::test]
async fn the_app_role_cannot_read_write_or_advance_the_ledger() {
    TestDb::with(|db| async move {
        one_job(&db, "locked-out@example.test").await;
        let server = FakeModel::start(vec![diagnosis_reply(SIGN_ERROR)]).await;
        diagnose(&db, &server, 0).await;
        assert_eq!(
            ledger(&db.admin).await.len(),
            1,
            "the ledger holds a row now"
        );

        for statement in [
            "SELECT count(*) FROM model_call_log",
            "INSERT INTO model_call_log (purpose, model_id, latency_ms) VALUES ('x', 'y', 1)",
            "DELETE FROM model_call_log",
            "SELECT nextval('model_call_log_id_seq')",
            "SELECT last_value FROM model_call_log_id_seq",
        ] {
            assert_eq!(
                refused_code(&db.app, statement).await.as_deref(),
                Some("42501"),
                "{statement}"
            );
        }
    })
    .await;
}

/// The two aggregate readers of migration 0009 give `cadus_app` sums and no row.
///
/// `/metrics` runs on a `cadus_app` connection with no tenant bound, so this is
/// the one path from the request tier to the worker's two tables. The numbers
/// are the sums of the row above; nothing in the result names a learner, a
/// session, a request id or a cost.
#[tokio::test]
async fn the_app_role_reads_the_ledger_totals_and_no_row() {
    TestDb::with(|db| async move {
        one_job(&db, "totals@example.test").await;
        let server =
            FakeModel::start(vec![full_reply("gen-full", None, Some(detailed_usage()))]).await;
        diagnose(&db, &server, 0).await;

        let totals = sqlx::query_as::<_, (String, i64, i64, i64, i64, i64)>(
            "SELECT purpose, input_cached, input_uncached, output_tokens, reasoning_tokens, calls
               FROM model_call_totals()",
        )
        .fetch_all(&db.app)
        .await
        .unwrap();
        assert_eq!(totals, vec![("diagnosis".to_owned(), 200, 300, 20, 40, 1)]);

        let jobs =
            sqlx::query_as::<_, (String, i64)>("SELECT status, jobs FROM diagnosis_job_totals()")
                .fetch_all(&db.app)
                .await
                .unwrap();
        assert_eq!(jobs, vec![("done".to_owned(), 1)]);
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The tick loop reaches the diagnosis pass (V7)
// --------------------------------------------------------------------------- //

/// A shutdown future that completes half a second after the row `id` is done.
///
/// The wait after the row is ten tick periods, so the loop takes a later tick
/// against the empty queue before it stops. A row that is not done in thirty
/// seconds fails the test.
async fn after_the_row_is_done(pool: &PgPool, id: Uuid) {
    let started = std::time::Instant::now();
    while row_of(pool, id).await.0 != "done" {
        assert!(
            started.elapsed() < std::time::Duration::from_secs(30),
            "the loop must finish the queued row in 30 s"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
}

/// A `None` refill job must not skip the diagnosis pass.
///
/// `run_with` read a `None` refill as a `continue`, so the tick returned to the
/// heartbeat and step 5 never ran. `run` itself passes `None` for both jobs, so
/// a `None` refill is a legal input that means "skip the refill pass" and
/// nothing more.
///
/// The loop below runs with a queued row and no refill job. The first tick
/// claims the row, calls the fake endpoint and bills one ledger row; a later
/// tick finds the queue empty and calls nobody. Every value below is a literal:
/// the status text, the call count, and the one ledger row.
///
/// The shutdown future ends half a second after the row is done, not at a
/// fixed wall-clock time: a loaded machine stretches the first pass, and a
/// fixed window then stops the loop before a later tick (finding #43).
#[tokio::test]
async fn a_none_refill_job_still_runs_the_diagnosis_pass() {
    TestDb::with(|db| async move {
        let (user, id) = one_job(&db, "looped@example.test").await;
        let server = FakeModel::start(vec![diagnosis_reply(SIGN_ERROR)]).await;
        let mut job = server.diagnosis_job(0);
        let cfg = WorkerConfig {
            tick: std::time::Duration::from_millis(50),
        };

        let ticks = run_with(
            &handle(&db),
            &cfg,
            None,
            Some(&mut job),
            after_the_row_is_done(&db.admin, id),
        )
        .await
        .unwrap();

        assert!(
            ticks >= 2,
            "the loop must take a later tick after the row is done, it reached {ticks}"
        );
        let (status, attempts, result) = row_of(&db.admin, id).await;
        assert_eq!(status, "done", "the tick loop must finish the queued row");
        assert_eq!(attempts, 1, "one claim finished the row");
        assert_eq!(result.unwrap()["error_tags"], json!(["sign-error"]));
        assert_eq!(
            server.calls().len(),
            1,
            "one queued row is one model call, and an empty queue calls nobody"
        );
        assert_eq!(
            ledger(&db.admin).await,
            vec![Ledger::diagnosis(user, "session-1", 500, 60, "gen-1")]
        );
    })
    .await;
}
