//! The fake OpenAI-compatible endpoint, and the authoring fixtures.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use cadus_core::curriculum::{AnswerKind, Exemplar};
use cadus_model_client::{Client, ModelConfig};
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_testkit::http::{read_request, status_reply};
use cadus_worker::authoring::job::{
    AUTHORING_ATTEMPTS, AuthoringJob, BatchReport, Decline, Outcome, Report, author_one, run_batch,
    slots_taken,
};
use cadus_worker::authoring::prompt::{AuthoringSpec, Kind};
use cadus_worker::diagnosis::DiagnosisJob;
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;

use super::trace;

/// A local endpoint that answers a fixed list of replies, in order. A call past
/// the end of the list gets `500` with an empty body.
pub struct FakeModel {
    pub base_url: String,
    calls: Arc<Mutex<Vec<Value>>>,
}

impl FakeModel {
    /// Start the endpoint with these replies.
    pub async fn start(replies: Vec<(u16, String)>) -> FakeModel {
        Self::start_with_delay(replies, Duration::ZERO).await
    }

    /// Start the endpoint with these replies, and wait `delay` before each one.
    pub async fn start_with_delay(replies: Vec<(u16, String)>, delay: Duration) -> FakeModel {
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
                let request = read_request(&mut socket).await;
                let split = request.find("\r\n\r\n").unwrap_or(request.len());
                let body: Value = serde_json::from_str(request.get(split + 4..).unwrap_or(""))
                    .unwrap_or(Value::Null);
                record.lock().unwrap().push(body);

                let (status, payload) = replies
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| (500, String::new()));
                index += 1;
                tokio::time::sleep(delay).await;
                let reply = status_reply(status, &payload);
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
    pub fn calls(&self) -> Vec<Value> {
        self.calls.lock().unwrap().clone()
    }

    /// The count of requests the endpoint received.
    pub fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }

    /// The user message of the call at this position.
    pub fn user_message(&self, index: usize) -> String {
        let calls = self.calls();
        let call = calls
            .get(index)
            .unwrap_or_else(|| panic!("the endpoint saw no call {index}"));
        call["messages"][1]["content"]
            .as_str()
            .expect("the user message is a string")
            .to_owned()
    }

    /// A client pointed at this endpoint, with these two token ceilings.
    pub fn client(&self, output_tokens: u32, reasoning_max_tokens: u32) -> Client {
        let cfg = ModelConfig {
            base_url: self.base_url.clone(),
            api_key: "test-key".to_owned(),
            model: "qwen3.6".to_owned(),
            output_tokens,
            reasoning_max_tokens,
            provider_order: Vec::new(),
            timeout: Duration::from_secs(5),
        };
        Client::new(cfg).unwrap()
    }

    /// An authoring job pointed at this endpoint, with the shipped bound.
    pub fn job(&self) -> AuthoringJob {
        AuthoringJob::new(self.client(2_048, 600))
    }

    /// An authoring job with another attempt bound.
    pub fn job_with_attempts(&self, attempts: u32) -> AuthoringJob {
        AuthoringJob::with_attempts(self.client(2_048, 600), attempts)
    }

    /// A diagnosis job pointed at this endpoint, with this T4 cap.
    pub fn diagnosis_job(&self, calls_per_session: u32) -> DiagnosisJob {
        DiagnosisJob::new(self.client(600, 600), calls_per_session)
    }
}

/// A `200` reply that carries a complete call of the tool `name` with these
/// arguments, and this `usage` block when there is one.
pub fn reply(name: &str, arguments: &str, usage: Option<Value>) -> (u16, String) {
    let mut payload = json!({
        "id": "gen-1",
        "choices": [{"finish_reason": "tool_calls", "message": {"tool_calls": [{"function": {
            "name": name, "arguments": arguments
        }}]}}]
    });
    if let Some(usage) = usage {
        payload["usage"] = usage;
    }
    (200, payload.to_string())
}

/// A reply that carries a complete call of the tool `name` with these
/// arguments and the authoring token counts.
pub fn named_reply(name: &str, arguments: &Value) -> (u16, String) {
    reply(
        name,
        &arguments.to_string(),
        Some(json!({"prompt_tokens": 900, "completion_tokens": 300})),
    )
}

/// A reply that carries a complete `emit_template` call with these arguments.
pub fn tool_reply(arguments: &Value) -> (u16, String) {
    named_reply("emit_template", arguments)
}

/// The tool arguments of a template the gate accepts.
///
/// It is the 1.0 perfect-squares template in the 2.0 document shape
/// (`docs/reference/serving-1.0-spec.md` section 2.1), with the samples that
/// cover both ends of `a`.
pub fn good_arguments() -> Value {
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
pub fn missing_low_edge() -> Value {
    let mut arguments = good_arguments();
    arguments["samples"] = json!([{"params": {"a": 12}, "expected": "144"}]);
    arguments
}

/// The perfect-squares knowledge point. Its exemplar answers `49`.
pub fn squares_spec() -> AuthoringSpec {
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

/// The perfect-squares knowledge point with an answer kind no checker decides.
pub fn proof_spec() -> AuthoringSpec {
    AuthoringSpec {
        answer_kind: AnswerKind::Proof,
        ..squares_spec()
    }
}

/// Wrap the admin pool in the `Db` the jobs take, with the log on.
pub fn handle(db: &TestDb) -> Db {
    trace();
    Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS)
}

/// Author one knowledge point and kind against the fake endpoint.
pub async fn author(db: &TestDb, fake: &FakeModel, kind: Kind, spec: &AuthoringSpec) -> Report {
    author_one(&handle(db), &fake.job(), kind, spec)
        .await
        .unwrap()
}

/// [`author`] with another attempt bound.
pub async fn author_within(
    db: &TestDb,
    fake: &FakeModel,
    attempts: u32,
    kind: Kind,
    spec: &AuthoringSpec,
) -> Report {
    author_one(&handle(db), &fake.job_with_attempts(attempts), kind, spec)
        .await
        .unwrap()
}

/// [`author_within`], and demand this outcome and this attempt count.
pub async fn author_within_expect(
    db: &TestDb,
    fake: &FakeModel,
    attempts: u32,
    kind: Kind,
    spec: &AuthoringSpec,
    outcome: Outcome,
    spent: u32,
) -> Report {
    let report = author_within(db, fake, attempts, kind, spec).await;
    assert_eq!(report.outcome, outcome);
    assert_eq!(report.attempts, spent, "{report:?}");
    report
}

/// [`author`], and demand this outcome and this attempt count.
pub async fn author_expect(
    db: &TestDb,
    fake: &FakeModel,
    kind: Kind,
    spec: &AuthoringSpec,
    outcome: Outcome,
    spent: u32,
) -> Report {
    author_within_expect(db, fake, AUTHORING_ATTEMPTS, kind, spec, outcome, spent).await
}

/// The five counts of one batch, in the order the summary prints them:
/// stored, duplicate, skipped, declined, calls.
pub fn assert_batch(batch: &BatchReport, counts: [u32; 5]) {
    let [stored, duplicate, skipped, declined, calls] = counts;
    assert_eq!(batch.stored, stored, "stored");
    assert_eq!(batch.duplicate, duplicate, "duplicate");
    assert_eq!(batch.skipped, skipped, "skipped");
    assert_eq!(batch.declined, declined, "declined");
    assert_eq!(batch.calls, calls, "calls");
}

/// The slots of this knowledge point and kind, as the loop counts them.
pub async fn slots(db: &TestDb, kp_id: &str, kind: Kind) -> i64 {
    slots_taken(&handle(db), kp_id, kind).await.unwrap()
}

/// The one decline record of a batch, of this knowledge point, kind and
/// attempt count.
pub fn the_one_decline<'a>(
    batch: &'a BatchReport,
    kp_id: &str,
    kind: Kind,
    attempts: u32,
) -> &'a Decline {
    assert_eq!(batch.declines.len(), 1, "{:?}", batch.declines);
    let decline = &batch.declines[0];
    assert_eq!(decline.kp_id, kp_id);
    assert_eq!(decline.kind, kind);
    assert_eq!(decline.attempts, attempts);
    decline
}

/// Author one kind over these knowledge points against the fake endpoint.
pub async fn batch(
    db: &TestDb,
    fake: &FakeModel,
    kind: Kind,
    specs: &[AuthoringSpec],
) -> BatchReport {
    run_batch(&handle(db), &fake.job(), kind, specs)
        .await
        .unwrap()
}

/// The knowledge point both golden files render.
///
/// It is the shape 1.0 failed to template: "subtraction with borrowing" needs
/// `a > b` between two parameters, and 1.0 had no constraint language for it
/// (`problem_templates.py:59-66`).
pub fn golden_spec() -> AuthoringSpec {
    AuthoringSpec {
        kp_id: "sub-borrow-two-digit".to_owned(),
        kp_name: "Two-digit subtraction with borrowing".to_owned(),
        topic_id: "subtraction-borrowing".to_owned(),
        topic_name: "Subtraction with borrowing".to_owned(),
        answer_kind: AnswerKind::Numeric,
        difficulty_target: None,
        constraints: Some(
            "the ones digit of the first number is smaller than the ones digit of the second"
                .to_owned(),
        ),
        exemplars: vec![
            Exemplar {
                problem: "Compute $52 - 27$.".to_owned(),
                answer: "25".to_owned(),
                solution_sketch: Some("Borrow one ten, then subtract the ones column.".to_owned()),
            },
            Exemplar {
                problem: "Compute $81 - 46$.".to_owned(),
                answer: "35".to_owned(),
                solution_sketch: None,
            },
        ],
    }
}
