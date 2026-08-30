//! M6 R7 acceptance: the distractor authoring pass (A4, A2, C6, T3, T6).
//!
//! Spec: `docs/reference/authoring-and-spa-1.0-spec.md` section 2.2 and row R7
//! of section 7; `docs/reference/web-service-1.0-spec.md` sections 5.3 and 6.2.
//!
//! The first acceptance check of row R7 lands here, at the place the pipeline
//! runs it: an `error_tag` outside the vocabulary is dropped at the gate, so the
//! stored row never carries it
//! ([`an_error_tag_outside_the_vocabulary_never_reaches_the_stored_row`]).
//!
//! Every expected value is a LITERAL: a literal body, a literal digest, a
//! literal rejection sentence, a literal row count. Nothing is read back from
//! the code under test.
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
use cadus_core::template::{GateSpec, gate_body};
use cadus_model_client::{Client, ModelConfig};
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_worker::authoring::job::{
    AuthoringJob, Outcome, author_one, authoring_vocabulary, verify,
};
use cadus_worker::authoring::prompt::{AuthoringSpec, Kind, tool_schema};
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

// --------------------------------------------------------------------------- //
// The literals of this file
// --------------------------------------------------------------------------- //

/// The serving key of the knowledge point under test, `"<topic_id>/<kp_id>"`.
const KP_KEY: &str = "addition/kp1";

/// The tag outside the vocabulary of spec section 5.3.
const UNKNOWN_TAG: &str = "carelessness";

/// The digest of the stored body: `sha256:` and the first 16 hex characters of
/// its SHA-256.
///
/// The body the pass writes leads with the three server-side fields and holds
/// the one distractor the vocabulary names:
///
/// ```json
/// {"v":1,"topic_id":"addition","answer_kind":"numeric","distractors":[{"answer":"13","error_tag":"arithmetic-slip","note":"You added the whole parts and dropped the half."}]}
/// ```
///
/// The key of `content_store` is the knowledge point, the kind AND the body, so
/// the material is `KP_KEY`, one NUL byte, `diagnosis`, one NUL byte, and the
/// body. The digest is computed outside this tree with
///
/// ```sh
/// printf 'addition/kp1\0diagnosis\0%s' '<the body above>' | sha256sum
/// # 69ceac16786901e7c87fa0bbdf503b0b25738289cfe446593fe880c1c93c48bc
/// ```
///
/// It pins the stored bytes, which the `content_store.body` column cannot: the
/// column holds jsonb, and jsonb keeps neither key order nor whitespace.
const STORED_DIGEST: &str = "sha256:69ceac16786901e7";

/// The first line of the retry block (1.0 `prompts.py:767-769`).
const RETRY_HEADER: &str = "YOUR PREVIOUS ATTEMPT WAS REFUSED. The server's exact reason was:";

/// The gate's sentence for a distractor that answers what the exemplar answers.
const RIGHT_ANSWER: &str = "distractor 0 answers '13.5', which is the right answer of exemplar 0 \
— a distractor names a mistake";

/// The closing line of the distractor retry block.
const RETRY_FIX: &str = "Fix that specifically. Do not restate the same list — change the \
answers, the tags, or the notes so the reason no longer applies.";

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
        let cfg = ModelConfig {
            base_url: self.base_url.clone(),
            api_key: "test-key".to_owned(),
            model: "qwen3.6".to_owned(),
            output_tokens: 2_048,
            reasoning_max_tokens: 600,
            provider_order: Vec::new(),
            timeout: Duration::from_secs(5),
        };
        AuthoringJob::new(Client::new(cfg).unwrap())
    }
}

/// A reply that carries a complete `emit_distractors` call with these arguments.
fn tool_reply(arguments: &Value) -> (u16, String) {
    let payload = json!({
        "id": "gen-1",
        "choices": [{"finish_reason": "tool_calls", "message": {"tool_calls": [{"function": {
            "name": "emit_distractors", "arguments": arguments.to_string()
        }}]}}],
        "usage": {"prompt_tokens": 700, "completion_tokens": 200}
    });
    (200, payload.to_string())
}

// --------------------------------------------------------------------------- //
// The fixtures
// --------------------------------------------------------------------------- //

/// The tool arguments of a list that carries one known tag and one tag outside
/// the vocabulary.
fn mixed_tags() -> Value {
    json!({
        "distractors": [
            {"answer": "13", "error_tag": "arithmetic-slip",
             "note": "You added the whole parts and dropped the half."},
            {"answer": "2.5", "error_tag": UNKNOWN_TAG,
             "note": "You subtracted where the problem adds."}
        ]
    })
}

/// A list whose first distractor answers what the exemplar answers. The gate
/// refuses it with [`RIGHT_ANSWER`].
fn names_the_right_answer() -> Value {
    json!({
        "distractors": [
            {"answer": "13.5", "error_tag": "arithmetic-slip",
             "note": "You added the whole parts and dropped the half."}
        ]
    })
}

/// The knowledge point every test authors for. Its exemplar answers `13.5`.
fn spec() -> AuthoringSpec {
    AuthoringSpec {
        kp_id: "kp1".to_owned(),
        kp_name: "Add a whole number and a decimal".to_owned(),
        topic_id: "addition".to_owned(),
        topic_name: "Addition".to_owned(),
        answer_kind: AnswerKind::Numeric,
        difficulty_target: None,
        constraints: None,
        exemplars: vec![Exemplar {
            problem: "Compute $8 + 5.5$.".to_owned(),
            answer: "13.5".to_owned(),
            solution_sketch: None,
        }],
    }
}

/// Every `content_store` row of one knowledge point, as
/// `(digest, kind, status, authoring_attempts, body)`.
async fn rows_of(pool: &PgPool, kp_id: &str) -> Vec<(String, String, String, i32, Value)> {
    sqlx::query_as::<_, (String, String, String, i32, Value)>(
        "SELECT digest, kind, status, authoring_attempts, body FROM content_store
          WHERE kp_id = $1 ORDER BY digest",
    )
    .bind(kp_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

// --------------------------------------------------------------------------- //
// Acceptance: the dropped tag never reaches the row
// --------------------------------------------------------------------------- //

/// Row R7, first acceptance check, at the place the pipeline runs it.
///
/// The model answers two distractors and one of them carries `carelessness`,
/// which the vocabulary of spec section 5.3 does not hold. The gate drops it,
/// the pass stores the other one, and the stored body is pinned whole because
/// the C6 approval binds to its digest.
#[tokio::test]
async fn an_error_tag_outside_the_vocabulary_never_reaches_the_stored_row() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![tool_reply(&mixed_tags())]).await;
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);

        let report = author_one(&handle, &fake.job(), Kind::Diagnosis, &spec())
            .await
            .unwrap();

        assert_eq!(report.outcome, Outcome::Stored);
        assert_eq!(report.attempts, 1, "one call authored the list");
        assert_eq!(report.digest.as_deref(), Some(STORED_DIGEST));
        assert_eq!(fake.calls().len(), 1);

        let rows = rows_of(&db.admin, KP_KEY).await;
        assert_eq!(rows.len(), 1, "the pass stores one row");
        assert_eq!(rows[0].0, STORED_DIGEST);
        assert_eq!(rows[0].1, "diagnosis");
        assert_eq!(rows[0].2, "pending", "a human reviews it before it serves");
        assert_eq!(rows[0].3, 1);
        // The column holds jsonb, which keeps neither key order nor whitespace,
        // so the row is read as a value and [`STORED_DIGEST`] pins the bytes.
        assert_eq!(
            rows[0].4,
            json!({
                "v": 1,
                "topic_id": "addition",
                "answer_kind": "numeric",
                "distractors": [{
                    "answer": "13",
                    "error_tag": "arithmetic-slip",
                    "note": "You added the whole parts and dropped the half."
                }]
            }),
            "the dropped tag is not in the stored body"
        );
    })
    .await;
}

/// The rejection message is the yield lever (1.0 `problem_templates.py:1386-1393`),
/// and it works for this kind too: a refused list is re-prompted with the gate's
/// own sentence and is rescued on attempt 2.
#[tokio::test]
async fn a_refused_list_is_re_prompted_verbatim_and_rescued_on_attempt_two() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![
            tool_reply(&names_the_right_answer()),
            tool_reply(&mixed_tags()),
        ])
        .await;
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);

        let report = author_one(&handle, &fake.job(), Kind::Diagnosis, &spec())
            .await
            .unwrap();

        assert_eq!(report.outcome, Outcome::Stored);
        assert_eq!(report.attempts, 2);
        assert_eq!(fake.calls().len(), 2);

        let retry = format!("{RETRY_HEADER}\n    {RIGHT_ANSWER}\n{RETRY_FIX}");
        assert!(
            fake.user_message(1).contains(&retry),
            "the second call carries the retry block verbatim: {}",
            fake.user_message(1)
        );
        assert!(
            !fake.user_message(0).contains(RETRY_HEADER),
            "the first call carries no retry block"
        );

        let rows = rows_of(&db.admin, KP_KEY).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].3, 2, "the row records both calls (T3)");
    })
    .await;
}

/// The two tools ask for a different `answer`, because the two documents hold a
/// different one.
///
/// A template distractor is an expression over the declared parameters. A
/// `diagnosis` document declares none, so its answer is the wrong answer itself.
/// One description for both kinds sends the distractor author to write a formula
/// over parameters that do not exist, and the gate spends an attempt on it.
#[test]
fn the_distractor_tool_asks_for_a_literal_answer() {
    let of = |kind| {
        tool_schema(kind)["properties"]["distractors"]["items"]["properties"]["answer"]
        ["description"]
        .clone()
    };

    assert_eq!(
        of(Kind::Diagnosis),
        json!(
            "The wrong answer itself, written the way a learner writes it. It is a literal \
answer, not a formula: this knowledge point declares no parameters."
        )
    );
    assert_eq!(
        of(Kind::Template),
        json!(
            "The wrong answer, as an expression over the declared parameters, so the server \
computes it per instance."
        )
    );
}

/// The gate keeps exactly the 11 tags of spec section 5.3, and the grade path
/// keeps every one of them.
///
/// The two filters are the wiring of row R7: the gate drops a tag at authoring
/// time, and `cadus_core::template::match_answer` drops one at read time. A tag
/// the gate keeps and the reader drops would store a diagnosis no learner ever
/// reads, so this test pins the two lists against each other.
#[test]
fn the_gate_keeps_only_tags_the_grade_path_also_keeps() {
    assert_eq!(
        authoring_vocabulary(),
        vec![
            "sign-error".to_owned(),
            "arithmetic-slip".to_owned(),
            "algebra-slip".to_owned(),
            "wrong-method".to_owned(),
            "formula-recall".to_owned(),
            "misread-problem".to_owned(),
            "incomplete".to_owned(),
            "notation".to_owned(),
            "units".to_owned(),
            "timing-unreliable".to_owned(),
            "blowoff".to_owned(),
        ]
    );
    let grade_path = cadus_core::config::default_error_tags();
    for tag in authoring_vocabulary() {
        assert!(
            grade_path.contains(&tag),
            "the grade path drops {tag}, which the gate keeps"
        );
    }
    // 2.0 spells the tag `blank-answer` and 1.0 spells it `blank_answer`
    // (`cadus_web::grade::TAG_BLANK_ANSWER`, spec section 5.3, the trap). The
    // gate keeps neither: the grade path stamps that tag on a blank submission,
    // and a distractor names an answer the learner wrote.
    for spelling in ["blank-answer", "blank_answer"] {
        assert!(
            !authoring_vocabulary().contains(&spelling.to_owned()),
            "{spelling} is server-assigned, so no distractor carries it"
        );
    }
}

/// A knowledge point whose diagnosis list is already approved pays nothing: the
/// bank of this kind is one document (spec section 2.2, "Bank target").
#[tokio::test]
async fn an_approved_list_makes_no_call() {
    TestDb::with(|db| async move {
        sqlx::query(
            "INSERT INTO content_store (digest, kp_id, kind, body, status)
             VALUES ('sha256:already0000000a', $1, 'diagnosis', '{}'::jsonb, 'approved')",
        )
        .bind(KP_KEY)
        .execute(&db.admin)
        .await
        .unwrap();
        let fake = FakeModel::start(vec![tool_reply(&mixed_tags())]).await;
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);

        let report = author_one(&handle, &fake.job(), Kind::Diagnosis, &spec())
            .await
            .unwrap();

        assert_eq!(report.outcome, Outcome::Skipped);
        assert_eq!(report.attempts, 0);
        assert_eq!(fake.calls().len(), 0, "a full bank makes zero model calls");
        assert_eq!(rows_of(&db.admin, KP_KEY).await.len(), 1);
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The template document: the same drop, in the same place
// --------------------------------------------------------------------------- //

/// The knowledge point the two template tests author for. Its exemplar answers
/// `49`.
fn template_spec() -> AuthoringSpec {
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

/// A template whose two distractors carry one known tag and one tag outside the
/// vocabulary. Neither note renders a parameter, so the drop takes the
/// distractor and nothing else.
fn mixed_template() -> Value {
    json!({
        "statement": "Compute ${a}^{{2}}$.",
        "params": {"a": {"kind": "int", "low": 1, "high": 12}},
        "constraints": [],
        "answer_expr": "a**2",
        "solution_sketch": "${a} \\times {a}$ gives the answer.",
        "hints": ["What does squaring a number mean?"],
        "distractors": [
            {"answer": "2*a", "error_tag": "arithmetic-slip",
             "note": "You doubled the number instead of squaring it."},
            {"answer": "a+2", "error_tag": UNKNOWN_TAG,
             "note": "You added two instead of squaring."}
        ],
        "samples": [
            {"params": {"a": 1}, "expected": "1"},
            {"params": {"a": 12}, "expected": "144"}
        ]
    })
}

/// The same template, with the parameter `b` rendered in ONE place: the note of
/// the single distractor. The tag decides whether that note survives.
fn note_holds_the_only_use(tag: &str) -> Value {
    json!({
        "statement": "Compute ${a}^{{2}}$.",
        "params": {
            "a": {"kind": "int", "low": 1, "high": 12},
            "b": {"kind": "int", "low": 1, "high": 2}
        },
        "constraints": [],
        "answer_expr": "a**2",
        "solution_sketch": "${a} \\times {a}$ gives the answer.",
        "hints": ["What does squaring a number mean?"],
        "distractors": [
            {"answer": "2*a", "error_tag": tag,
             "note": "You multiplied by {b} instead of squaring."}
        ],
        "samples": [
            {"params": {"a": 1, "b": 1}, "expected": "1"},
            {"params": {"a": 12, "b": 2}, "expected": "144"},
            {"params": {"a": 1, "b": 2}, "expected": "1"},
            {"params": {"a": 12, "b": 1}, "expected": "144"}
        ]
    })
}

/// Row R7, first acceptance check, on the OTHER document that carries
/// distractors: the template. The stored body holds the distractor the
/// vocabulary names, and the gate accepts that body a second time.
///
/// The re-gate is the point. `content_store` keeps the body the digest covers,
/// and the reviewer of unit R5 reads the gate block of that stored body. A
/// stored body the gate refuses shows the reviewer a refusal the authored
/// document never earned.
#[test]
fn a_template_tag_outside_the_vocabulary_is_dropped_and_the_stored_body_re_gates() {
    let spec = template_spec();

    let body = verify(&spec, &mixed_template()).expect("the gate accepts the template");

    let stored: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        stored["distractors"],
        json!([{
            "answer": "2*a",
            "error_tag": "arithmetic-slip",
            "note": "You doubled the number instead of squaring it."
        }]),
        "the dropped tag is not in the stored body: {body}"
    );
    let gate_spec = GateSpec {
        answer_kind: AnswerKind::Numeric,
        exemplars: &spec.exemplars,
    };
    gate_body(&body, &gate_spec).expect("the stored body passes the gate a second time");
}

/// The drop runs BEFORE the gate, and that order is the whole rule.
///
/// One document, one tag apart. A distractor note is a rendered field, so the
/// note is where the parameter `b` does its work. The known tag keeps the note,
/// and the gate accepts. The unknown tag takes the note with the distractor, and
/// the gate then reads the FILTERED document: it names the dead parameter, and
/// the next attempt reads that sentence.
///
/// A drop after the gate stores the second document instead, and the row then
/// holds a body the gate refuses.
#[test]
fn the_template_drop_runs_before_the_gate() {
    let spec = template_spec();

    verify(&spec, &note_holds_the_only_use("arithmetic-slip"))
        .expect("the surviving note renders the parameter");

    let rejection = verify(&spec, &note_holds_the_only_use(UNKNOWN_TAG))
        .expect_err("the drop leaves the parameter dead");

    assert_eq!(rejection.code, "dead-parameter");
    assert_eq!(
        rejection.message,
        "parameters ['b'] are declared but never used"
    );
}
