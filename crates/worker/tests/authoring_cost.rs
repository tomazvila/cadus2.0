//! M6 R3 acceptance: the T3 accounting of the authoring pipeline (T3, T6).
//!
//! The section 7 row of the spec names three checks, and each one is a test
//! here:
//!
//! 1. a 4-attempt knowledge point raises the alert and the row's
//!    `authoring_attempts` is `4`;
//! 2. a reply with no `usage` block writes a zeros row with a NULL cost;
//! 3. the cost on the row equals the sum of that knowledge point's call rows.
//!
//! The rest of the file holds the checks the accounting needs beside those
//! three: the per-term rounding that makes check 3 an equality, a total the
//! money column cannot hold, a pass whose calls report no price, the alert of a
//! declined knowledge point, and the operator list.
//!
//! Every expected value is a LITERAL: a literal money text, a literal row count,
//! a literal attempt count. Nothing is read back from the code under test.
//!
//! The endpoint is a fake OpenAI-compatible server in this file, as in
//! `authoring_job.rs` and `diagnosis.rs`. No test reaches a real provider.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use cadus_core::curriculum::{AnswerKind, Exemplar};
use cadus_model_client::{Client, ModelConfig};
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_worker::authoring::cost::{ATTEMPT_ALERT, alerting};
use cadus_worker::authoring::job::{AuthoringJob, Outcome, author_one, store_pending};
use cadus_worker::authoring::prompt::{AuthoringSpec, Kind};
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

// --------------------------------------------------------------------------- //
// The literals of this file
// --------------------------------------------------------------------------- //

/// The serving key of the knowledge point under test, `"<topic_id>/<kp_id>"`.
const KP_KEY: &str = "perfect-squares/squares";

/// The digest of the body every accepted reply of this file stores.
///
/// `authoring_job.rs` pins the same value beside the body text it covers.
const STORED_DIGEST: &str = "sha256:fbed1683b615cbc0";

/// The three prices of the three-attempt pass, in the order the calls run.
const PRICES: [f64; 3] = [0.001234, 0.0002, 0.5];

/// The sum of [`PRICES`], as `numeric(12,6)` writes it.
///
/// Worked by hand: 0.001234 + 0.000200 + 0.500000 = 0.501434.
const PRICE_SUM: &str = "0.501434";

// --------------------------------------------------------------------------- //
// The fake OpenAI-compatible server
// --------------------------------------------------------------------------- //

/// A local endpoint that answers a fixed list of replies, in order. A call past
/// the end of the list gets `500` with an empty body.
struct FakeModel {
    base_url: String,
    calls: Arc<AtomicUsize>,
}

impl FakeModel {
    async fn start(replies: Vec<(u16, String)>) -> FakeModel {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let calls = Arc::new(AtomicUsize::new(0));
        let count = Arc::clone(&calls);

        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let mut raw: Vec<u8> = Vec::new();
                let mut buffer = [0_u8; 4096];
                loop {
                    let read = socket.read(&mut buffer).await.unwrap_or(0);
                    if read == 0 {
                        break;
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
                            break;
                        }
                    }
                }
                let index = count.fetch_add(1, Ordering::SeqCst);
                let (status, payload) = replies
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| (500, String::new()));
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

    /// How many requests the endpoint answered.
    fn call_count(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    /// An authoring job pointed at this endpoint, with the shipped bound.
    fn job(&self) -> AuthoringJob {
        AuthoringJob::new(self.client())
    }

    fn client(&self) -> Client {
        let cfg = ModelConfig {
            base_url: self.base_url.clone(),
            api_key: "test-key".to_owned(),
            model: "qwen3.6".to_owned(),
            output_tokens: 2_048,
            reasoning_max_tokens: 600,
            provider_order: Vec::new(),
            timeout: Duration::from_secs(5),
        };
        Client::new(cfg).unwrap()
    }
}

/// A reply that carries an `emit_template` call and this `usage` block.
fn reply_with_usage(arguments: &Value, usage: Option<Value>) -> (u16, String) {
    let mut payload = json!({
        "id": "gen-1",
        "choices": [{"finish_reason": "tool_calls", "message": {"tool_calls": [{"function": {
            "name": "emit_template", "arguments": arguments.to_string()
        }}]}}]
    });
    if let Some(usage) = usage {
        payload["usage"] = usage;
    }
    (200, payload.to_string())
}

/// A `usage` block that reports these tokens and this price.
fn usage(cost: f64) -> Value {
    json!({"prompt_tokens": 900, "completion_tokens": 300, "cost": cost})
}

// --------------------------------------------------------------------------- //
// The fixtures
// --------------------------------------------------------------------------- //

/// The tool arguments of a template the gate accepts.
fn good_arguments() -> Value {
    json!({
        "statement": "Compute ${a}^{{2}}$.",
        "params": {"a": {"kind": "int", "low": 1, "high": 12}},
        "constraints": [],
        "answer_expr": "a**2",
        "solution_sketch": "${a} \\times {a}$ gives the answer.",
        "hints": ["What does squaring a number mean?"],
        "distractors": [],
        "samples": [
            {"params": {"a": 1}, "expected": "1"},
            {"params": {"a": 12}, "expected": "144"}
        ]
    })
}

/// The same template with the low-end sample missing. The gate refuses it.
fn missing_low_edge() -> Value {
    let mut arguments = good_arguments();
    arguments["samples"] = json!([{"params": {"a": 12}, "expected": "144"}]);
    arguments
}

/// The knowledge point every test authors for.
fn spec() -> AuthoringSpec {
    AuthoringSpec {
        kp_id: "squares".to_owned(),
        kp_name: "Squares of one-digit and two-digit numbers".to_owned(),
        topic_id: "perfect-squares".to_owned(),
        topic_name: "Perfect squares".to_owned(),
        answer_kind: AnswerKind::Numeric,
        difficulty_target: None,
        constraints: None,
        exemplars: vec![Exemplar {
            problem: "Compute $7^2$.".to_owned(),
            answer: "49".to_owned(),
            solution_sketch: None,
        }],
    }
}

// --------------------------------------------------------------------------- //
// Database helpers
// --------------------------------------------------------------------------- //

/// The `(authoring_attempts, authoring_cost_usd)` of the one row of a knowledge
/// point.
async fn accounting(pool: &PgPool, kp_id: &str) -> (i32, Option<String>) {
    sqlx::query_as::<_, (i32, Option<String>)>(
        "SELECT authoring_attempts, authoring_cost_usd::text FROM content_store
          WHERE kp_id = $1",
    )
    .bind(kp_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// The count of `content_store` rows of a knowledge point.
async fn row_count(pool: &PgPool, kp_id: &str) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT count(*) FROM content_store WHERE kp_id = $1")
        .bind(kp_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// The sum of the `cost_usd` of every authoring call row, as its exact text.
async fn ledger_sum(pool: &PgPool) -> Option<String> {
    sqlx::query_scalar::<_, Option<String>>(
        "SELECT sum(cost_usd)::text FROM model_call_log WHERE purpose = 'authoring'",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

/// The count of authoring call rows, and how many of them name a tenant.
async fn ledger_shape(pool: &PgPool) -> (i64, i64) {
    sqlx::query_as::<_, (i64, i64)>(
        "SELECT count(*), count(user_id) FROM model_call_log WHERE purpose = 'authoring'",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Seed one `content_store` row with this attempt count and price.
async fn seed_row(pool: &PgPool, digest: &str, kp_id: &str, attempts: i32, cost: Option<&str>) {
    sqlx::query(
        "INSERT INTO content_store
             (digest, kp_id, kind, body, status, authoring_attempts, authoring_cost_usd)
         VALUES ($1, $2, 'template', '{}'::jsonb, 'pending', $3, $4::text::numeric)",
    )
    .bind(digest)
    .bind(kp_id)
    .bind(attempts)
    .bind(cost)
    .execute(pool)
    .await
    .unwrap();
}

// --------------------------------------------------------------------------- //
// 1. The four-attempt alert
// --------------------------------------------------------------------------- //

/// The acceptance literal: a 4-attempt knowledge point raises the alert and the
/// row's `authoring_attempts` is 4.
///
/// Three refusals and one accepted document make four model calls. T3 alerts
/// above three (`REQUIREMENTS.md`, T3), so this pass alerts, the row says `4`,
/// and the operator list names it.
#[tokio::test]
async fn a_four_attempt_knowledge_point_alerts_and_the_row_says_four() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![
            reply_with_usage(&missing_low_edge(), Some(usage(0.001))),
            reply_with_usage(&missing_low_edge(), Some(usage(0.001))),
            reply_with_usage(&missing_low_edge(), Some(usage(0.001))),
            reply_with_usage(&good_arguments(), Some(usage(0.001))),
        ])
        .await;
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);

        let report = author_one(&handle, &fake.job(), Kind::Template, &spec())
            .await
            .unwrap();

        assert_eq!(report.outcome, Outcome::Stored);
        assert_eq!(report.attempts, 4);
        assert!(report.alert, "four attempts must raise the T3 alert");
        assert_eq!(fake.call_count(), 4);

        // The stored row carries the count, and the bill of four calls.
        let (attempts, cost) = accounting(&db.admin, KP_KEY).await;
        assert_eq!(attempts, 4);
        assert_eq!(cost.as_deref(), Some("0.004000"));

        // The operator list names the knowledge point, once.
        let alerts = alerting(&handle).await.unwrap();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].kp_id, KP_KEY);
        assert_eq!(alerts[0].kind, "template");
        assert_eq!(alerts[0].digest, STORED_DIGEST);
        assert_eq!(alerts[0].attempts, 4);
        assert_eq!(alerts[0].cost_usd.as_deref(), Some("0.004000"));

        // T6: one ledger row per HTTP attempt, and no tenant on an offline call.
        assert_eq!(ledger_shape(&db.admin).await, (4, 0));
    })
    .await;
}

/// Three attempts are inside the bound: the row says `3` and nothing alerts.
///
/// The bound is "above 3", so this is the pass that proves the alert reads `>`
/// and not `>=`.
#[tokio::test]
async fn a_three_attempt_knowledge_point_stays_quiet() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![
            reply_with_usage(&missing_low_edge(), Some(usage(0.001))),
            reply_with_usage(&missing_low_edge(), Some(usage(0.001))),
            reply_with_usage(&good_arguments(), Some(usage(0.001))),
        ])
        .await;
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);

        let report = author_one(&handle, &fake.job(), Kind::Template, &spec())
            .await
            .unwrap();

        assert_eq!(report.attempts, 3);
        assert_eq!(ATTEMPT_ALERT, 3);
        assert!(!report.alert, "three attempts are inside the T3 bound");
        assert_eq!(accounting(&db.admin, KP_KEY).await.0, 3);
        assert!(alerting(&handle).await.unwrap().is_empty());
    })
    .await;
}

/// A declined knowledge point alerts too, and stores no row.
///
/// Five refusals spend five calls. The row that would carry the bill never
/// exists, so the alert and the ledger are the only places that spend is named.
#[tokio::test]
async fn a_declined_knowledge_point_alerts_and_stores_no_row() {
    TestDb::with(|db| async move {
        let refusal = || reply_with_usage(&missing_low_edge(), Some(usage(0.001)));
        let fake =
            FakeModel::start(vec![refusal(), refusal(), refusal(), refusal(), refusal()]).await;
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);

        let report = author_one(&handle, &fake.job(), Kind::Template, &spec())
            .await
            .unwrap();

        assert_eq!(report.outcome, Outcome::Declined);
        assert_eq!(report.attempts, 5);
        assert!(report.alert);
        assert_eq!(report.cost_usd, None);
        assert_eq!(row_count(&db.admin, KP_KEY).await, 0);
        assert_eq!(ledger_shape(&db.admin).await, (5, 0));
        assert_eq!(ledger_sum(&db.admin).await.as_deref(), Some("0.005000"));
        assert!(alerting(&handle).await.unwrap().is_empty());
    })
    .await;
}

// --------------------------------------------------------------------------- //
// 2. The reply with no usage block
// --------------------------------------------------------------------------- //

/// The acceptance literal: a reply with no `usage` block writes a zeros row with
/// a NULL cost.
///
/// The call happened and the operator paid for it, so the row exists. Every
/// token count the reply did not report is 0, and the price it did not report is
/// NULL: an unknown price is never a guess (`crate::model_log`). The stored
/// document then carries a NULL cost and an exact attempt count.
#[tokio::test]
async fn a_reply_with_no_usage_block_writes_a_zeros_row_with_a_null_cost() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![reply_with_usage(&good_arguments(), None)]).await;
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);

        let report = author_one(&handle, &fake.job(), Kind::Template, &spec())
            .await
            .unwrap();
        assert_eq!(report.outcome, Outcome::Stored);
        assert_eq!(report.cost_usd, None);

        let row =
            sqlx::query_as::<_, (String, i32, i32, i32, i32, Option<String>, Option<String>)>(
                "SELECT purpose, input_tokens_cached, input_tokens_uncached, output_tokens,
                    reasoning_tokens, cost_usd::text, model_id
               FROM model_call_log",
            )
            .fetch_one(&db.admin)
            .await
            .unwrap();
        assert_eq!(row.0, "authoring");
        assert_eq!(row.1, 0);
        assert_eq!(row.2, 0);
        assert_eq!(row.3, 0);
        assert_eq!(row.4, 0);
        assert_eq!(row.5, None);
        assert_eq!(row.6.as_deref(), Some("qwen3.6"));
        assert_eq!(ledger_shape(&db.admin).await, (1, 0));

        // The document is stored, with no money on it and an exact count.
        assert_eq!(accounting(&db.admin, KP_KEY).await, (1, None));
    })
    .await;
}

// --------------------------------------------------------------------------- //
// 3. The cost on the row is the sum of the call rows
// --------------------------------------------------------------------------- //

/// The acceptance literal: the cost on the row equals the sum of that knowledge
/// point's call rows.
///
/// Three calls at 0.001234, 0.0002 and 0.5 cost 0.501434 together. The test
/// asserts that literal on the stored row AND against the sum the ledger holds,
/// so a change to either side breaks it.
#[tokio::test]
async fn the_cost_on_the_row_is_the_sum_of_that_knowledge_points_call_rows() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![
            reply_with_usage(&missing_low_edge(), Some(usage(PRICES[0]))),
            reply_with_usage(&missing_low_edge(), Some(usage(PRICES[1]))),
            reply_with_usage(&good_arguments(), Some(usage(PRICES[2]))),
        ])
        .await;
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);

        let report = author_one(&handle, &fake.job(), Kind::Template, &spec())
            .await
            .unwrap();

        assert_eq!(report.outcome, Outcome::Stored);
        assert_eq!(report.attempts, 3);
        assert_eq!(report.cost_usd.as_deref(), Some(PRICE_SUM));

        let (attempts, cost) = accounting(&db.admin, KP_KEY).await;
        assert_eq!(attempts, 3);
        assert_eq!(cost.as_deref(), Some("0.501434"));

        // The same number, read from the ledger rows of those three calls.
        assert_eq!(ledger_shape(&db.admin).await, (3, 0));
        assert_eq!(ledger_sum(&db.admin).await.as_deref(), Some("0.501434"));
        assert_eq!(cost, ledger_sum(&db.admin).await);
    })
    .await;
}

/// The sum rounds every term to six places first, exactly as each ledger row
/// does, so the two sides agree on a price under the scale of the column.
///
/// Two calls at 0.0000005 each store 0.000001 apiece, and the stored total is
/// 0.000002. A sum of the RAW texts would be 0.000001, which no ledger row
/// holds.
#[tokio::test]
async fn the_sum_rounds_every_term_the_way_a_ledger_row_does() {
    TestDb::with(|db| async move {
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
        let spend = vec!["0.0000005".to_owned(), "0.0000005".to_owned()];

        let stored = store_pending(&handle, KP_KEY, Kind::Template, "{}", 2, &spend)
            .await
            .unwrap();

        assert!(stored.inserted);
        assert_eq!(stored.cost_usd.as_deref(), Some("0.000002"));
        assert_eq!(
            accounting(&db.admin, KP_KEY).await,
            (2, Some("0.000002".to_owned()))
        );
    })
    .await;
}

/// A pass whose calls report no price stores a NULL cost and an exact count.
///
/// Zero is a measurement and NULL is the absence of one, so an unpriced pass is
/// never stored as free.
#[tokio::test]
async fn a_pass_with_no_price_stores_a_null_cost_and_an_exact_count() {
    TestDb::with(|db| async move {
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);

        let stored = store_pending(&handle, KP_KEY, Kind::Template, "{}", 2, &[])
            .await
            .unwrap();

        assert!(stored.inserted);
        assert_eq!(stored.cost_usd, None);
        assert_eq!(accounting(&db.admin, KP_KEY).await, (2, None));
    })
    .await;
}

/// A total the money column cannot hold is NULL, and the document still stores.
///
/// `numeric(12,6)` keeps six digits before the point. An overflow raises inside
/// the statement and would lose the document, so the sum guards itself and the
/// row keeps the attempt count.
#[tokio::test]
async fn a_total_the_money_column_cannot_hold_is_null() {
    TestDb::with(|db| async move {
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
        let spend = vec!["999999".to_owned(), "999999".to_owned()];

        let stored = store_pending(&handle, KP_KEY, Kind::Template, "{}", 1, &spend)
            .await
            .unwrap();

        assert!(stored.inserted);
        assert_eq!(stored.cost_usd, None);
        assert_eq!(accounting(&db.admin, KP_KEY).await, (1, None));
    })
    .await;
}

/// The operator list holds the documents above the bound only, dearest first.
#[tokio::test]
async fn alerting_lists_the_documents_above_the_bound_dearest_first() {
    TestDb::with(|db| async move {
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
        seed_row(&db.admin, "sha256:aaaa", "t/one", 1, Some("0.1")).await;
        seed_row(&db.admin, "sha256:bbbb", "t/two", 3, Some("0.2")).await;
        seed_row(&db.admin, "sha256:cccc", "t/three", 4, Some("0.3")).await;
        seed_row(&db.admin, "sha256:dddd", "t/four", 5, None).await;

        let alerts = alerting(&handle).await.unwrap();

        assert_eq!(alerts.len(), 2);
        assert_eq!(alerts[0].kp_id, "t/four");
        assert_eq!(alerts[0].attempts, 5);
        assert_eq!(alerts[0].cost_usd, None);
        assert_eq!(alerts[1].kp_id, "t/three");
        assert_eq!(alerts[1].attempts, 4);
        assert_eq!(alerts[1].cost_usd.as_deref(), Some("0.300000"));
    })
    .await;
}

/// A second pass that authors the same body leaves the first bill alone.
///
/// The digest is the primary key and the approval binds to it (C6), so the
/// second pass inserts nothing. Its own calls are in the ledger; the row keeps
/// the accounting of the pass that wrote it.
#[tokio::test]
async fn a_duplicate_body_leaves_the_first_bill_alone() {
    TestDb::with(|db| async move {
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
        let first = store_pending(
            &handle,
            KP_KEY,
            Kind::Template,
            "{}",
            1,
            &["0.25".to_owned()],
        )
        .await
        .unwrap();
        assert!(first.inserted);

        let second = store_pending(
            &handle,
            KP_KEY,
            Kind::Template,
            "{}",
            4,
            &["0.75".to_owned()],
        )
        .await
        .unwrap();

        assert!(!second.inserted);
        assert_eq!(second.cost_usd, None);
        assert_eq!(
            accounting(&db.admin, KP_KEY).await,
            (1, Some("0.250000".to_owned()))
        );
    })
    .await;
}
