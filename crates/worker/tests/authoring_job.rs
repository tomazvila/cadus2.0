//! M6 R2 acceptance: the per-knowledge-point batch loop (A2, C6, T3, T6).
//!
//! The section 7 row of the spec names three checks, and each one is a test
//! here:
//!
//! 1. a template refused for a missing low edge is re-prompted with that exact
//!    message and is rescued on attempt 2;
//! 2. a knowledge point refused 5 times writes no `content_store` row and one
//!    decline record;
//! 3. the loop makes zero calls for an already-approved knowledge point.
//!
//! The rest of the file holds the checks the loop needs beside those three: the
//! T6 ledger of an authoring call, the stored row's own columns, a `rejected`
//! row that does not occupy a slot, a duplicate body, an endpoint that refuses
//! every attempt, and a batch that keeps going past one decline.
//!
//! Every expected value is a LITERAL: a literal rejection sentence, a literal
//! digest, a literal row count, a literal status. Nothing is read back from the
//! code under test.
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

use cadus_core::curriculum::{AnswerKind, Exemplar};
use cadus_model_client::{Client, ModelConfig};
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_worker::authoring::job::{
    AuthoringJob, Outcome, author_one, bank_target, run_batch, slots_taken,
};
use cadus_worker::authoring::prompt::{AuthoringSpec, Kind};
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

// --------------------------------------------------------------------------- //
// The literals of this file
// --------------------------------------------------------------------------- //

/// The gate's edge-coverage sentence for the low end of `a`
/// (`crates/core/src/template/gate.rs`, row "edge coverage").
const LOW_EDGE: &str = "no worked sample uses the low end of a (1) — the edges are where an \
expression stops being right";

/// The first line of the retry block (1.0 `prompts.py:767-769`).
const RETRY_HEADER: &str = "YOUR PREVIOUS ATTEMPT WAS REFUSED. The server's exact reason was:";

/// The serving key of the knowledge point under test, `"<topic_id>/<kp_id>"`.
const KP_KEY: &str = "perfect-squares/squares";

/// The body the loop stores for [`good_arguments`], character for character.
///
/// The three server-side fields lead it, the gate's `space_size` closes it, and
/// the model's `constraints` and `distractors` are absent because both are empty
/// and the document skips an empty list.
const STORED_BODY: &str = r#"{"v":1,"topic_id":"perfect-squares","answer_kind":"numeric","statement":"Compute ${a}^{{2}}$.","params":{"a":{"kind":"int","low":1,"high":12}},"answer_expr":"a**2","solution_sketch":"${a} \\times {a}$ gives the answer.","hints":["What does squaring a number mean?"],"samples":[{"params":{"a":1},"expected":"1"},{"params":{"a":12},"expected":"144"}],"space_size":12}"#;

/// The digest of [`STORED_BODY`]: `sha256:` and the first 16 hex characters of
/// its SHA-256. Computed outside this tree with
///
/// ```sh
/// printf '%s' '<STORED_BODY>' | sha256sum
/// # eeef7de45ea6a41dc0f3ac2be84a1e3dfecd61af994b796a2a51914c1f23d2d3
/// ```
///
/// The value changes when the stored body changes, which is the point: C6 binds
/// the approval to it.
const STORED_DIGEST: &str = "sha256:eeef7de45ea6a41d";

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

    /// The user message of the call at this position.
    fn user_message(&self, index: usize) -> String {
        let calls = self.calls();
        let call = calls
            .get(index)
            .unwrap_or_else(|| panic!("the endpoint saw no call {index}"));
        call["messages"][1]["content"]
            .as_str()
            .expect("the user message is a string")
            .to_owned()
    }

    /// An authoring job pointed at this endpoint, with the shipped bound.
    fn job(&self) -> AuthoringJob {
        AuthoringJob::new(self.client())
    }

    /// An authoring job with another attempt bound.
    fn job_with_attempts(&self, attempts: u32) -> AuthoringJob {
        AuthoringJob::with_attempts(self.client(), attempts)
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

/// A reply that carries a complete `emit_template` call with these arguments.
fn tool_reply(arguments: &Value) -> (u16, String) {
    let payload = json!({
        "id": "gen-1",
        "choices": [{"finish_reason": "tool_calls", "message": {"tool_calls": [{"function": {
            "name": "emit_template", "arguments": arguments.to_string()
        }}]}}],
        "usage": {"prompt_tokens": 900, "completion_tokens": 300}
    });
    (200, payload.to_string())
}

// --------------------------------------------------------------------------- //
// The fixtures
// --------------------------------------------------------------------------- //

/// The tool arguments of a template the gate accepts.
///
/// It is the 1.0 perfect-squares template in the 2.0 document shape
/// (`docs/reference/serving-1.0-spec.md` section 2.1), with the samples that
/// cover both ends of `a`.
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

/// The same template with the low-end sample missing. The gate refuses it with
/// [`LOW_EDGE`].
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

/// A second knowledge point, for the batch test.
fn other_spec() -> AuthoringSpec {
    AuthoringSpec {
        kp_id: "cubes".to_owned(),
        topic_id: "perfect-cubes".to_owned(),
        ..spec()
    }
}

// --------------------------------------------------------------------------- //
// Database helpers
// --------------------------------------------------------------------------- //

/// Seed one `content_store` row of `kind = 'template'` with this status.
async fn seed_row(pool: &PgPool, digest: &str, kp_id: &str, status: &str) {
    sqlx::query(
        "INSERT INTO content_store (digest, kp_id, kind, body, status)
         VALUES ($1, $2, 'template', '{}'::jsonb, $3)",
    )
    .bind(digest)
    .bind(kp_id)
    .bind(status)
    .execute(pool)
    .await
    .unwrap();
}

/// Every `content_store` row of one knowledge point, as
/// `(digest, status, authoring_attempts, body)`.
async fn rows_of(pool: &PgPool, kp_id: &str) -> Vec<(String, String, i32, Value)> {
    sqlx::query_as::<_, (String, String, i32, Value)>(
        "SELECT digest, status, authoring_attempts, body FROM content_store
          WHERE kp_id = $1 ORDER BY digest",
    )
    .bind(kp_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// The `model_call_log` rows of `purpose = 'authoring'`, as
/// `(count, non-NULL user ids)`.
async fn ledger(pool: &PgPool) -> (i64, i64) {
    sqlx::query_as::<_, (i64, i64)>(
        "SELECT count(*), count(user_id) FROM model_call_log WHERE purpose = 'authoring'",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

// --------------------------------------------------------------------------- //
// 1. The rescue on attempt 2
// --------------------------------------------------------------------------- //

/// The acceptance literal: a template refused for a missing low edge is
/// re-prompted with that EXACT message and is rescued on attempt 2.
///
/// The retry block is what 1.0 measures as the yield lever
/// (`problem_templates.py:1386-1393`), so the test reads the second request's
/// user message and asserts the whole two-line block, not a substring of it.
#[tokio::test]
async fn a_missing_low_edge_is_re_prompted_verbatim_and_rescued_on_attempt_two() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![
            tool_reply(&missing_low_edge()),
            tool_reply(&good_arguments()),
        ])
        .await;
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);

        let report = author_one(&handle, &fake.job(), Kind::Template, &spec())
            .await
            .unwrap();

        assert_eq!(report.outcome, Outcome::Stored);
        assert_eq!(report.attempts, 2);
        assert_eq!(report.kp_id, KP_KEY);
        assert_eq!(report.digest.as_deref(), Some(STORED_DIGEST));
        assert!(report.decline.is_none());

        // The first attempt carries no retry block at all.
        let first = fake.user_message(0);
        assert!(
            !first.contains(RETRY_HEADER),
            "attempt 1 must carry no retry block, it read:\n{first}"
        );

        // The second attempt carries the gate's own sentence, indented by four
        // spaces under the header.
        let second = fake.user_message(1);
        assert!(
            second.contains(&format!("{RETRY_HEADER}\n    {LOW_EDGE}\n")),
            "attempt 2 must quote the rejection verbatim, it read:\n{second}"
        );
        assert_eq!(fake.calls().len(), 2);

        // One row, `pending`, with the attempt count on it (T3).
        let rows = rows_of(&db.admin, KP_KEY).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, STORED_DIGEST);
        assert_eq!(rows[0].1, "pending");
        assert_eq!(rows[0].2, 2);
    })
    .await;
}

/// The stored body carries the server's fields and the gate's satisfying count.
///
/// `v`, `topic_id`, `answer_kind` and `space_size` are the server's, never the
/// model's (`problem_templates.py:309`). The model sent none of them.
#[tokio::test]
async fn the_stored_body_carries_the_server_fields_and_the_gate_space_size() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![tool_reply(&good_arguments())]).await;
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);

        author_one(&handle, &fake.job(), Kind::Template, &spec())
            .await
            .unwrap();

        let rows = rows_of(&db.admin, KP_KEY).await;
        assert_eq!(rows.len(), 1);
        let body = &rows[0].3;
        assert_eq!(
            body,
            &serde_json::from_str::<Value>(STORED_BODY).expect("the literal body reads")
        );
        assert_eq!(body["v"], json!(1));
        assert_eq!(body["topic_id"], json!("perfect-squares"));
        assert_eq!(body["answer_kind"], json!("numeric"));
        assert_eq!(body["space_size"], json!(12));
        assert_eq!(body["statement"], json!("Compute ${a}^{{2}}$."));
        assert_eq!(body["answer_expr"], json!("a**2"));
        assert_eq!(rows[0].2, 1);
    })
    .await;
}

/// T6: one ledger row per HTTP attempt, `purpose = 'authoring'`, no user id.
///
/// Two authoring attempts against a fake endpoint that answers on the first HTTP
/// attempt of each call give two rows. An offline authoring call belongs to no
/// tenant, so `user_id` is NULL on both.
#[tokio::test]
async fn every_authoring_call_writes_its_own_ledger_row() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![
            tool_reply(&missing_low_edge()),
            tool_reply(&good_arguments()),
        ])
        .await;
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);

        let report = author_one(&handle, &fake.job(), Kind::Template, &spec())
            .await
            .unwrap();

        assert_eq!(report.http_attempts.len(), 2);
        assert_eq!(ledger(&db.admin).await, (2, 0));
    })
    .await;
}

// --------------------------------------------------------------------------- //
// 2. The decline path
// --------------------------------------------------------------------------- //

/// The acceptance literal: a knowledge point refused 5 times writes no
/// `content_store` row and one decline record.
#[tokio::test]
async fn five_refusals_write_no_row_and_one_decline_record() {
    TestDb::with(|db| async move {
        let refusals: Vec<(u16, String)> =
            (0..5).map(|_| tool_reply(&missing_low_edge())).collect();
        let fake = FakeModel::start(refusals).await;
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);

        let batch = run_batch(&handle, &fake.job(), Kind::Template, &[spec()])
            .await
            .unwrap();

        assert_eq!(batch.declined, 1);
        assert_eq!(batch.stored, 0);
        assert_eq!(batch.calls, 5);
        assert_eq!(batch.declines.len(), 1);

        let decline = &batch.declines[0];
        assert_eq!(decline.kp_id, KP_KEY);
        assert_eq!(decline.kind, Kind::Template);
        assert_eq!(decline.attempts, 5);
        assert_eq!(decline.reasons.len(), 5);
        for reason in &decline.reasons {
            assert_eq!(reason, &format!("edge-coverage: {LOW_EDGE}"));
        }

        assert_eq!(fake.calls().len(), 5);
        assert!(rows_of(&db.admin, KP_KEY).await.is_empty());
    })
    .await;
}

/// A knowledge point whose answer kind the gate can never accept declines with
/// ZERO model calls (T3).
///
/// `TEMPLATABLE_KINDS` is `numeric` and `expression`. Five calls for a `proof`
/// knowledge point buy five copies of one refusal.
#[tokio::test]
async fn an_undecidable_answer_kind_declines_with_no_call() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![tool_reply(&good_arguments())]).await;
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
        let spec = AuthoringSpec {
            answer_kind: AnswerKind::Proof,
            ..spec()
        };

        let report = author_one(&handle, &fake.job(), Kind::Template, &spec)
            .await
            .unwrap();

        assert_eq!(report.outcome, Outcome::Declined);
        assert_eq!(report.attempts, 0);
        assert_eq!(
            report.decline.expect("a decline record").reasons,
            vec!["answer kind proof is not symbolically decidable".to_owned()]
        );
        assert_eq!(fake.calls().len(), 0);
        assert!(rows_of(&db.admin, KP_KEY).await.is_empty());
    })
    .await;
}

/// An endpoint that refuses every request declines with the endpoint's reason,
/// and the reason is not fed back as authoring feedback.
///
/// A 400 never retries inside the client (spec section 6.5), so five authoring
/// attempts are five HTTP attempts.
#[tokio::test]
async fn an_endpoint_that_refuses_every_attempt_declines() {
    TestDb::with(|db| async move {
        let refusals: Vec<(u16, String)> = (0..5)
            .map(|_| (400, json!({"error": "no such model"}).to_string()))
            .collect();
        let fake = FakeModel::start(refusals).await;
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);

        let report = author_one(&handle, &fake.job(), Kind::Template, &spec())
            .await
            .unwrap();

        assert_eq!(report.outcome, Outcome::Declined);
        assert_eq!(report.attempts, 5);
        let decline = report.decline.expect("a decline record");
        assert_eq!(decline.reasons.len(), 5);
        assert_eq!(
            decline.reasons[0],
            "model endpoint answered 400: {\"error\":\"no such model\"}"
        );
        // No gate ever ran, so no attempt carried a retry block.
        for index in 0..5 {
            let message = fake.user_message(index);
            assert!(
                !message.contains(RETRY_HEADER),
                "a transport failure is not authoring feedback, call {index} read:\n{message}"
            );
        }
        assert!(rows_of(&db.admin, KP_KEY).await.is_empty());
        assert_eq!(ledger(&db.admin).await, (5, 0));
    })
    .await;
}

/// One decline never stops the batch: the second knowledge point still stores.
#[tokio::test]
async fn a_decline_does_not_stop_the_batch() {
    TestDb::with(|db| async move {
        let mut replies: Vec<(u16, String)> =
            (0..5).map(|_| tool_reply(&missing_low_edge())).collect();
        replies.push(tool_reply(&good_arguments()));
        let fake = FakeModel::start(replies).await;
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);

        let batch = run_batch(
            &handle,
            &fake.job(),
            Kind::Template,
            &[spec(), other_spec()],
        )
        .await
        .unwrap();

        assert_eq!(batch.declined, 1);
        assert_eq!(batch.stored, 1);
        assert_eq!(batch.calls, 6);
        assert_eq!(batch.declines.len(), 1);
        assert_eq!(batch.declines[0].kp_id, KP_KEY);
        assert!(rows_of(&db.admin, KP_KEY).await.is_empty());
        assert_eq!(rows_of(&db.admin, "perfect-cubes/cubes").await.len(), 1);
    })
    .await;
}

// --------------------------------------------------------------------------- //
// 3. Zero calls for a knowledge point that is already served
// --------------------------------------------------------------------------- //

/// The acceptance literal: the loop makes zero calls for an already-approved
/// knowledge point.
///
/// The bank target of `template` is 3 (1.0 `BANK_TARGET`), so the test seeds the
/// full bank and asserts that the endpoint saw nothing at all.
#[tokio::test]
async fn an_approved_knowledge_point_costs_zero_calls() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![tool_reply(&good_arguments())]).await;
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
        for index in 0..3 {
            seed_row(
                &db.admin,
                &format!("sha256:approved-{index}"),
                KP_KEY,
                "approved",
            )
            .await;
        }

        let report = author_one(&handle, &fake.job(), Kind::Template, &spec())
            .await
            .unwrap();

        assert_eq!(report.outcome, Outcome::Skipped);
        assert_eq!(report.attempts, 0);
        assert!(report.digest.is_none());
        assert_eq!(fake.calls().len(), 0);
        assert_eq!(ledger(&db.admin).await, (0, 0));
        // The seeded rows are untouched: three rows, all `approved`.
        let rows = rows_of(&db.admin, KP_KEY).await;
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|row| row.1 == "approved"));
    })
    .await;
}

/// A slot a human has not read yet is occupied too (1.0 `:1352-1374`).
///
/// One approved row and two pending rows fill the bank, so a nightly pass does
/// not put a fourth document in front of a reviewer.
#[tokio::test]
async fn a_pending_slot_is_occupied_and_costs_zero_calls() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![tool_reply(&good_arguments())]).await;
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
        seed_row(&db.admin, "sha256:approved-0", KP_KEY, "approved").await;
        seed_row(&db.admin, "sha256:pending-0", KP_KEY, "pending").await;
        seed_row(&db.admin, "sha256:pending-1", KP_KEY, "pending").await;

        assert_eq!(
            slots_taken(&handle, KP_KEY, Kind::Template).await.unwrap(),
            3
        );
        assert_eq!(bank_target(Kind::Template), 3);

        let report = author_one(&handle, &fake.job(), Kind::Template, &spec())
            .await
            .unwrap();

        assert_eq!(report.outcome, Outcome::Skipped);
        assert_eq!(fake.calls().len(), 0);
    })
    .await;
}

/// A `rejected` row occupies nothing: a human refused that body, and the
/// knowledge point still needs a document.
#[tokio::test]
async fn a_rejected_row_does_not_occupy_a_slot() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![tool_reply(&good_arguments())]).await;
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
        for index in 0..3 {
            seed_row(
                &db.admin,
                &format!("sha256:rejected-{index}"),
                KP_KEY,
                "rejected",
            )
            .await;
        }

        assert_eq!(
            slots_taken(&handle, KP_KEY, Kind::Template).await.unwrap(),
            0
        );

        let report = author_one(&handle, &fake.job(), Kind::Template, &spec())
            .await
            .unwrap();

        assert_eq!(report.outcome, Outcome::Stored);
        assert_eq!(fake.calls().len(), 1);
        assert_eq!(rows_of(&db.admin, KP_KEY).await.len(), 4);
    })
    .await;
}

/// A body a human already rejected never comes back as `pending` (C6).
///
/// The digest is the identity of the body, so the second insert conflicts and
/// does nothing. The reviewer's verdict stands.
#[tokio::test]
async fn a_rejected_body_is_not_resurrected_as_pending() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![tool_reply(&good_arguments())]).await;
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
        seed_row(&db.admin, STORED_DIGEST, KP_KEY, "rejected").await;

        let report = author_one(&handle, &fake.job(), Kind::Template, &spec())
            .await
            .unwrap();

        assert_eq!(report.outcome, Outcome::Duplicate);
        assert_eq!(report.digest.as_deref(), Some(STORED_DIGEST));
        let rows = rows_of(&db.admin, KP_KEY).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, "rejected");
        assert_eq!(rows[0].3, json!({}));
    })
    .await;
}

/// The two kinds with no gate make no call and store nothing (unit R6).
///
/// `diagnosis` left this list in unit R7, which added its gate
/// (`cadus_core::template::distractor`); `crates/worker/tests/authoring_distractor.rs`
/// holds its pass.
#[tokio::test]
async fn a_kind_with_no_gate_makes_no_call() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![tool_reply(&good_arguments())]).await;
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);

        for kind in [Kind::Teach, Kind::HintLadder] {
            let report = author_one(&handle, &fake.job(), kind, &spec())
                .await
                .unwrap();
            assert_eq!(report.outcome, Outcome::NoGate);
            assert_eq!(report.attempts, 0);
        }

        assert_eq!(fake.calls().len(), 0);
        assert!(rows_of(&db.admin, KP_KEY).await.is_empty());
    })
    .await;
}

/// An attempt bound of 0 makes no call and declines, which is the dry run of R8.
#[tokio::test]
async fn an_attempt_bound_of_zero_makes_no_call() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![tool_reply(&good_arguments())]).await;
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);

        let report = author_one(&handle, &fake.job_with_attempts(0), Kind::Template, &spec())
            .await
            .unwrap();

        assert_eq!(report.outcome, Outcome::Declined);
        assert_eq!(report.attempts, 0);
        assert_eq!(fake.calls().len(), 0);
    })
    .await;
}
