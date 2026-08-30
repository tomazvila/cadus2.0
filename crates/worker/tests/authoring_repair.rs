//! FIX-M6-A2 acceptance: the LaTeX escape repair and the prompt digest.
//!
//! The M6 review names two defects of the authoring boundary, and each one is a
//! test here:
//!
//! - **F3** — no repair of the model's under-escaped LaTeX runs anywhere in the
//!   pipeline. `verify_kind` hands the decoded tool arguments straight to the
//!   gates, no gate reads a control character, and `"$\times$"` with one
//!   backslash reaches `content_store` as `$<TAB>imes$` on `template`, `teach`
//!   and `hint_ladder` alike (spec section 5, trap T1).
//! - **F4** — spec section 2.2 asks for the prompt digest on the row, so a
//!   prompt edit marks the affected rows for re-authoring. `content_store` had
//!   no such column, and nothing recorded which prompt authored which approved
//!   row.
//!
//! Every expected value is a LITERAL: a literal repaired statement, a literal
//! rejection sentence, a literal status, a literal row count. Nothing is read
//! back from the code under test.
//!
//! Every `\t`, `\n` and `\r` inside a string literal of this file is the control
//! character a JSON decoder hands back for an under-escaped `\times`, `\neq` and
//! `\rightarrow`. That is the input the repair exists for.
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
    AuthoringJob, Outcome, author_one, render_stale, stale_rows, stale_slots,
};
use cadus_worker::authoring::prompt::{AuthoringSpec, Kind, prompt_digest};
use cadus_worker::authoring::repair::CONTROL_CHARACTER;
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

// --------------------------------------------------------------------------- //
// The literals of this file
// --------------------------------------------------------------------------- //

/// The serving key of the knowledge point under test.
const KP_KEY: &str = "perfect-squares/squares";

/// The statement the model sends, as the JSON decoder hands it back.
///
/// The model wrote `"Compute ${a} \times {a}$."` with ONE backslash. `\t` is a
/// valid JSON escape, so the decoded value carries a TAB and the command is
/// gone.
const MANGLED_STATEMENT: &str = "Compute ${a} \times {a}$.";

/// The statement the repair restores, character for character.
const REPAIRED_STATEMENT: &str = "Compute ${a} \\times {a}$.";

/// The concept line the model sends for a teach page, mangled the same way.
const MANGLED_CONCEPT: &str = "A square is $a \times a$.";

/// The concept line the repair restores.
const REPAIRED_CONCEPT: &str = "A square is $a \\times a$.";

/// A prompt digest that is NOT the digest of any current kind. It stands for
/// the prompt an operator has since edited (spec section 2.2, "Prompt digest").
const OLD_PROMPT: &str = "sha256:0000000000000000";

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

    /// An authoring job with this attempt bound.
    fn job_with_attempts(&self, attempts: u32) -> AuthoringJob {
        let cfg = ModelConfig {
            base_url: self.base_url.clone(),
            api_key: "test-key".to_owned(),
            model: "qwen3.6".to_owned(),
            output_tokens: 2_048,
            reasoning_max_tokens: 600,
            provider_order: Vec::new(),
            timeout: Duration::from_secs(5),
        };
        AuthoringJob::with_attempts(Client::new(cfg).unwrap(), attempts)
    }
}

/// A reply that carries a complete tool call with these arguments.
fn tool_reply(arguments: &Value) -> (u16, String) {
    let payload = json!({
        "id": "gen-1",
        "choices": [{"finish_reason": "tool_calls", "message": {"tool_calls": [{"function": {
            "name": "emit", "arguments": arguments.to_string()
        }}]}}],
        "usage": {"prompt_tokens": 900, "completion_tokens": 300}
    });
    (200, payload.to_string())
}

// --------------------------------------------------------------------------- //
// The fixtures
// --------------------------------------------------------------------------- //

/// The tool arguments of a template the gate accepts, with a MANGLED statement.
fn mangled_template() -> Value {
    json!({
        "statement": MANGLED_STATEMENT,
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

/// The same template with a statement the repair cannot name.
///
/// `\u{1}` is not one of the six JSON escape letters, so no repair restores it
/// and the boundary refuses the body.
fn unrepairable_template() -> Value {
    let mut arguments = mangled_template();
    arguments["statement"] = json!("Compute ${a}\u{1}^{{2}}$.");
    arguments
}

/// The tool arguments of a teach page the gate accepts, MANGLED.
fn mangled_teach() -> Value {
    json!({
        "concept": MANGLED_CONCEPT,
        "worked_example": {
            "problem": "Compute $6^2$.",
            "steps": ["$6 \times 6 = 36$."]
        }
    })
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

/// Every `content_store` row of one knowledge point, as
/// `(digest, kind, status, body, prompt_digest)`.
async fn rows_of(
    pool: &PgPool,
    kp_id: &str,
) -> Vec<(String, String, String, Value, Option<String>)> {
    sqlx::query_as::<_, (String, String, String, Value, Option<String>)>(
        "SELECT digest, kind, status, body, prompt_digest FROM content_store
          WHERE kp_id = $1 ORDER BY status, digest",
    )
    .bind(kp_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// Seed one APPROVED `content_store` row with this prompt digest.
async fn seed_approved(pool: &PgPool, digest: &str, kp_id: &str, kind: &str, prompt: Option<&str>) {
    sqlx::query(
        "INSERT INTO content_store
             (digest, kp_id, kind, body, status, prompt_digest)
         VALUES ($1, $2, $3, '{}'::jsonb, 'approved', $4)",
    )
    .bind(digest)
    .bind(kp_id)
    .bind(kind)
    .bind(prompt)
    .execute(pool)
    .await
    .unwrap();
}

// --------------------------------------------------------------------------- //
// F3: the LaTeX escape repair of the boundary
// --------------------------------------------------------------------------- //

/// F3: an under-escaped `\times` is repaired before the gate, on a TEMPLATE.
///
/// A mangled template is worse than one mangled problem: the structure mangles
/// every instance it ever renders, and the C6 approval then binds to those
/// bytes. The test therefore reads the STORED body and asserts the repaired
/// statement, character for character, and that the row carries no TAB at all.
#[tokio::test]
async fn an_under_escaped_command_is_repaired_before_the_template_gate() {
    TestDb::with(|db| async move {
        // The decoded arguments carry the TAB. That is the bug under test.
        assert!(
            MANGLED_STATEMENT.contains('\t'),
            "the TAB is the bug being repaired"
        );
        assert!(!MANGLED_STATEMENT.contains("\\times"));

        let fake = FakeModel::start(vec![tool_reply(&mangled_template())]).await;
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);

        let report = author_one(&handle, &fake.job_with_attempts(1), Kind::Template, &spec())
            .await
            .unwrap();

        assert_eq!(report.outcome, Outcome::Stored);
        let rows = rows_of(&db.admin, KP_KEY).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].3["statement"], json!(REPAIRED_STATEMENT));
        assert!(
            !rows[0].3.to_string().contains('\t'),
            "no stored field keeps the TAB"
        );
    })
    .await;
}

/// F3: the repair runs on a TEACH page too, and it reaches the worked steps.
///
/// The finding names `template`, `teach` and `hint_ladder` together, because
/// `verify_kind` is the one door all three go through.
#[tokio::test]
async fn an_under_escaped_command_is_repaired_before_the_teach_gate() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![tool_reply(&mangled_teach())]).await;
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);

        let report = author_one(&handle, &fake.job_with_attempts(1), Kind::Teach, &spec())
            .await
            .unwrap();

        assert_eq!(report.outcome, Outcome::Stored);
        let rows = rows_of(&db.admin, KP_KEY).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, "teach");
        assert_eq!(rows[0].3["concept"], json!(REPAIRED_CONCEPT));
        assert_eq!(
            rows[0].3["worked_example"]["steps"][0],
            json!("$6 \\times 6 = 36$.")
        );
        assert!(
            !rows[0].3.to_string().contains('\t'),
            "no stored field keeps the TAB"
        );
    })
    .await;
}

/// F3: a control character the repair cannot name refuses the body, with the
/// literal message the next attempt reads.
///
/// 1.0 DROPS such a character (`prompts.py:1046`). 2.0 refuses instead: the
/// pipeline is offline and it retries, so the refusal costs one attempt and
/// buys a document nobody has to read twice.
#[tokio::test]
async fn a_control_character_the_repair_cannot_name_refuses_the_body() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![
            tool_reply(&unrepairable_template()),
            tool_reply(&mangled_template()),
        ])
        .await;
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);

        let report = author_one(&handle, &fake.job_with_attempts(2), Kind::Template, &spec())
            .await
            .unwrap();

        // Attempt 1 is refused, and the LITERAL message is the feedback of
        // attempt 2.
        assert_eq!(report.attempts, 2);
        assert_eq!(report.outcome, Outcome::Stored);
        assert!(
            fake.user_message(1).contains(CONTROL_CHARACTER),
            "the retry block carries the literal control-character message"
        );

        // Attempt 2 stored the repaired document, and nothing else is in the
        // table.
        let rows = rows_of(&db.admin, KP_KEY).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].3["statement"], json!(REPAIRED_STATEMENT));
    })
    .await;
}

// --------------------------------------------------------------------------- //
// F4: the prompt digest on the row
// --------------------------------------------------------------------------- //

/// F4: the stored row names the prompt that authored it.
///
/// Spec section 2.2, "Prompt digest": the digest is a column on
/// `content_store`, never part of the content digest, because the C6 approval
/// binds to the content.
#[tokio::test]
async fn the_stored_row_carries_the_prompt_digest_of_its_kind() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![tool_reply(&mangled_template())]).await;
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);

        author_one(&handle, &fake.job_with_attempts(1), Kind::Template, &spec())
            .await
            .unwrap();

        let rows = rows_of(&db.admin, KP_KEY).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].4, Some(prompt_digest(Kind::Template)));
        // The prompt digest is 16 hex characters, as every 2.0 digest is.
        assert_eq!(rows[0].4.as_deref().unwrap_or_default().len(), 16);
        // It is NOT the content digest: the two answer different questions.
        assert_ne!(rows[0].4.as_deref().unwrap_or_default(), rows[0].0);
    })
    .await;
}

/// F4, the acceptance check: a prompt edit re-authors the row and never
/// unapproves it.
///
/// The seeded row stands for a document an EARLIER prompt authored: its
/// `prompt_digest` is not the digest of any current kind. The pass then
///
/// - re-authors the knowledge point, although the bank of `teach` is 1 and the
///   row occupies it, and
/// - leaves the old row `approved`, because C6 binds the approval to the
///   content and a prompt edit changes no content (spec section 2.2).
#[tokio::test]
async fn a_prompt_edit_re_authors_the_row_and_keeps_the_old_approval() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![tool_reply(&mangled_teach())]).await;
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
        seed_approved(
            &db.admin,
            "sha256:old-teach-row",
            KP_KEY,
            "teach",
            Some(OLD_PROMPT),
        )
        .await;

        assert_eq!(
            stale_slots(&handle, KP_KEY, Kind::Teach).await.unwrap(),
            1,
            "the seeded row names a prompt that is not the current one"
        );

        let report = author_one(&handle, &fake.job_with_attempts(1), Kind::Teach, &spec())
            .await
            .unwrap();

        assert_eq!(report.outcome, Outcome::Stored);
        assert_eq!(fake.calls().len(), 1);

        let rows = rows_of(&db.admin, KP_KEY).await;
        assert_eq!(rows.len(), 2);
        // The old row is untouched and still approved.
        assert_eq!(rows[0].0, "sha256:old-teach-row");
        assert_eq!(rows[0].2, "approved");
        assert_eq!(rows[0].4.as_deref(), Some(OLD_PROMPT));
        // The re-authored row is pending, and it names the current prompt.
        assert_eq!(rows[1].2, "pending");
        assert_eq!(rows[1].4, Some(prompt_digest(Kind::Teach)));
        assert_eq!(rows[1].3["concept"], json!(REPAIRED_CONCEPT));
    })
    .await;
}

/// F4: a SECOND pass over the same knowledge point makes no call.
///
/// The re-authored row holds the slot from that point on, so a nightly pass
/// after a prompt edit re-authors once and never grows the review queue.
#[tokio::test]
async fn the_pass_after_a_re_author_makes_no_model_call() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![tool_reply(&mangled_teach())]).await;
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
        seed_approved(
            &db.admin,
            "sha256:old-teach-row",
            KP_KEY,
            "teach",
            Some(OLD_PROMPT),
        )
        .await;

        author_one(&handle, &fake.job_with_attempts(1), Kind::Teach, &spec())
            .await
            .unwrap();
        let second = author_one(&handle, &fake.job_with_attempts(1), Kind::Teach, &spec())
            .await
            .unwrap();

        assert_eq!(second.outcome, Outcome::Skipped);
        assert_eq!(second.attempts, 0);
        assert_eq!(fake.calls().len(), 1, "the second pass called nothing");
        assert_eq!(rows_of(&db.admin, KP_KEY).await.len(), 2);
    })
    .await;
}

/// F4: a row with NO prompt digest is not stale.
///
/// NULL means "the prompt is not recorded", which every row written before
/// migration `0012` carries. A pass that read NULL as stale would re-author the
/// whole bank on the first run after the deployment.
#[tokio::test]
async fn a_row_with_no_prompt_digest_is_not_stale() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![tool_reply(&mangled_teach())]).await;
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
        seed_approved(&db.admin, "sha256:legacy-row", KP_KEY, "teach", None).await;

        assert_eq!(stale_slots(&handle, KP_KEY, Kind::Teach).await.unwrap(), 0);

        let report = author_one(&handle, &fake.job_with_attempts(1), Kind::Teach, &spec())
            .await
            .unwrap();

        assert_eq!(report.outcome, Outcome::Skipped);
        assert_eq!(fake.calls().len(), 0);
        assert_eq!(rows_of(&db.admin, KP_KEY).await.len(), 1);
    })
    .await;
}

/// F4: `--stale` lists the approved rows an older prompt wrote, and nothing
/// else.
///
/// The listed text is the operator's output, so the test asserts it byte for
/// byte.
#[tokio::test]
async fn the_stale_listing_names_the_approved_rows_of_an_older_prompt() {
    TestDb::with(|db| async move {
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
        // One approved row of an older prompt: it is listed.
        seed_approved(
            &db.admin,
            "sha256:old-teach",
            KP_KEY,
            "teach",
            Some(OLD_PROMPT),
        )
        .await;
        // One approved row of the CURRENT prompt: it is not.
        seed_approved(
            &db.admin,
            "sha256:current-teach",
            "perfect-cubes/cubes",
            "teach",
            Some(&prompt_digest(Kind::Teach)),
        )
        .await;
        // One row with no prompt digest: it is not.
        seed_approved(&db.admin, "sha256:legacy-teach", "bare/kp1", "teach", None).await;

        let rows = stale_rows(&handle, &[Kind::Teach]).await.unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kp_id, KP_KEY);
        assert_eq!(rows[0].digest, "sha256:old-teach");
        assert_eq!(rows[0].prompt_digest, OLD_PROMPT);
        assert_eq!(
            render_stale(&rows),
            "stale documents\n\
             kp_id kind digest prompt_digest\n\
             perfect-squares/squares teach sha256:old-teach sha256:0000000000000000\n\
             stale: rows 1\n"
        );
    })
    .await;
}

/// F4: a PENDING row of an older prompt is not listed.
///
/// It is already in front of a reviewer, and a reviewer reads the body and not
/// the prompt.
#[tokio::test]
async fn the_stale_listing_skips_a_pending_row() {
    TestDb::with(|db| async move {
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
        sqlx::query(
            "INSERT INTO content_store (digest, kp_id, kind, body, status, prompt_digest)
             VALUES ('sha256:pending-teach', $1, 'teach', '{}'::jsonb, 'pending', $2)",
        )
        .bind(KP_KEY)
        .bind(OLD_PROMPT)
        .execute(&db.admin)
        .await
        .unwrap();

        let rows = stale_rows(&handle, &[Kind::Teach]).await.unwrap();

        assert!(rows.is_empty());
        assert_eq!(
            render_stale(&rows),
            "stale documents\nkp_id kind digest prompt_digest\nstale: rows 0\n"
        );
    })
    .await;
}
