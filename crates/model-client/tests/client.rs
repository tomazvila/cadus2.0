//! M5 U10 acceptance, the client half: the T5 request body, the three
//! truncation shapes, and the retry contract (spec sections 6.4 and 6.5).
//!
//! Every expected value is a LITERAL: a literal `max_tokens`, a literal status,
//! a literal request count, a literal token count. Nothing is read back from the
//! code under test.
//!
//! The endpoint is a fake OpenAI-compatible server in this file: one
//! `TcpListener` on `127.0.0.1`, a hand-written HTTP/1.1 reply per call, and a
//! record of every request it received. No test reaches a real provider.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use cadus_model_client::{ChatRequest, Client, ModelConfig, ModelError, ToolSpec, request_body};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

// --------------------------------------------------------------------------- //
// The fake OpenAI-compatible server
// --------------------------------------------------------------------------- //

/// One request the fake server received.
#[derive(Debug, Clone)]
struct Seen {
    /// The request line and the headers, lowercased.
    head: String,
    /// The JSON body.
    body: Value,
}

/// A local endpoint that answers a fixed list of replies, in order.
struct FakeModel {
    base_url: String,
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl FakeModel {
    /// Bind `127.0.0.1:0` and serve `replies` in order, one connection each.
    ///
    /// A call past the end of the list gets `500` with an empty body, so a test
    /// that expects three attempts and gets four still fails on the count.
    async fn start(replies: Vec<(u16, String)>) -> FakeModel {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let record = Arc::clone(&seen);

        tokio::spawn(async move {
            let mut index = 0_usize;
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let mut raw: Vec<u8> = Vec::new();
                let mut buffer = [0_u8; 4096];
                // Read the head, then the body that content-length names.
                let request = loop {
                    let read = socket.read(&mut buffer).await.unwrap_or(0);
                    if read == 0 {
                        break String::from_utf8_lossy(&raw).to_string();
                    }
                    raw.extend_from_slice(&buffer[..read]);
                    let text = String::from_utf8_lossy(&raw).to_string();
                    if let Some(split) = text.find("\r\n\r\n") {
                        let head = text[..split].to_lowercase();
                        let length: usize = head
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
                let head = request[..split].to_lowercase();
                let body: Value = serde_json::from_str(request.get(split + 4..).unwrap_or(""))
                    .unwrap_or(Value::Null);
                record.lock().unwrap().push(Seen { head, body });

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
            seen,
        }
    }

    /// The requests the server received, in order.
    fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }

    /// A client pointed at this endpoint, with the shipped T5 defaults.
    fn client(&self) -> Client {
        Client::new(local_config(&self.base_url)).unwrap()
    }
}

/// The configuration of a local OpenAI-compatible endpoint.
fn local_config(base_url: &str) -> ModelConfig {
    ModelConfig {
        base_url: base_url.to_owned(),
        api_key: "test-key".to_owned(),
        model: "qwen3.6".to_owned(),
        output_tokens: 600,
        reasoning_max_tokens: 600,
        provider_order: Vec::new(),
        timeout: Duration::from_secs(5),
    }
}

/// The one request every test sends.
fn chat_request() -> ChatRequest {
    ChatRequest {
        system: "You are the grader for Cadus.".to_owned(),
        user: "Problem: 8 - 5".to_owned(),
        tool: ToolSpec {
            name: "emit_diagnosis".to_owned(),
            description: "Name the misconception.".to_owned(),
            // The required list is the schema of spec section 6.3. The client
            // reads it to decide truncation shape 3, so the fixture states it.
            parameters: json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["error_tags", "prose"],
                "properties": {
                    "error_tags": {"type": "array", "items": {"type": "string"}},
                    "prose": {"type": "string"},
                },
            }),
        },
    }
}

/// A reply that carries a complete forced tool call.
fn good_reply(finish: &str) -> String {
    json!({
        "id": "gen-1",
        "provider": "DeepInfra",
        "choices": [{
            "finish_reason": finish,
            "message": {"tool_calls": [{"function": {
                "name": "emit_diagnosis",
                "arguments": "{\"error_tags\":[\"sign-error\"],\"prose\":\"Watch the sign.\"}"
            }}]}
        }],
        "usage": {
            "prompt_tokens": 1000,
            "prompt_tokens_details": {"cached_tokens": 900},
            "completion_tokens": 120,
            "completion_tokens_details": {"reasoning_tokens": 80},
            "cost": 0.000123
        }
    })
    .to_string()
}

// --------------------------------------------------------------------------- //
// The T5 request body (spec section 6.4)
// --------------------------------------------------------------------------- //

/// The OpenRouter body pins the provider order, caps reasoning, and caches the
/// system prefix. 1.0 shipped all three unset (T5).
#[test]
fn the_openrouter_body_carries_the_three_t5_defaults() {
    let cfg = ModelConfig {
        base_url: "https://openrouter.ai/api/v1".to_owned(),
        api_key: "k".to_owned(),
        model: "deepseek/deepseek-v4-pro".to_owned(),
        output_tokens: 600,
        reasoning_max_tokens: 600,
        provider_order: vec!["deepinfra".to_owned(), "novita".to_owned()],
        timeout: Duration::from_secs(60),
    };
    let body = request_body(&cfg, &chat_request(), 600);

    assert_eq!(body["model"], json!("deepseek/deepseek-v4-pro"));
    assert_eq!(body["max_tokens"], json!(600));
    assert_eq!(body["temperature"], json!(0.0));
    assert_eq!(body["provider"]["order"], json!(["deepinfra", "novita"]));
    assert_eq!(body["provider"]["allow_fallbacks"], json!(true));
    assert_eq!(body["reasoning"]["max_tokens"], json!(600));
    assert_eq!(body["prompt_cache_key"], json!("cadus-diagnosis-v1"));
    assert_eq!(
        body["messages"][0]["content"][0]["cache_control"],
        json!({"type": "ephemeral"})
    );
    assert_eq!(
        body["tool_choice"],
        json!({"type": "function", "function": {"name": "emit_diagnosis"}})
    );
}

/// A local endpoint gets the plain OpenAI shape. `provider` and `reasoning` are
/// OpenRouter extensions and earn a non-retryable 400 elsewhere.
#[test]
fn a_local_endpoint_gets_no_routing_block() {
    let body = request_body(
        &local_config("http://10.8.0.3:8080/v1"),
        &chat_request(),
        600,
    );

    assert_eq!(body.get("provider"), None);
    assert_eq!(body.get("reasoning"), None);
    assert_eq!(
        body["messages"][0]["content"],
        json!("You are the grader for Cadus.")
    );
}

/// A look-alike host is NOT the routing host, so it never gets the routing
/// block. 1.0's substring test sent the key and the extensions to it.
#[test]
fn a_look_alike_host_gets_no_routing_block() {
    let mut cfg = local_config("https://openrouter.ai.attacker.example/v1");
    cfg.provider_order = vec!["deepinfra".to_owned()];
    let body = request_body(&cfg, &chat_request(), 600);

    assert_eq!(body.get("provider"), None);
    assert_eq!(body.get("reasoning"), None);
}

// --------------------------------------------------------------------------- //
// The retry contract (spec section 6.5)
// --------------------------------------------------------------------------- //

/// The acceptance literal: `finish_reason: "length"` with no tool call retries
/// once with a 4× ceiling, and the second reply is accepted.
#[tokio::test]
async fn a_truncated_reply_retries_with_a_four_times_budget() {
    let truncated = json!({
        "choices": [{"finish_reason": "length", "message": {"content": null}}],
        "usage": {"prompt_tokens": 900, "completion_tokens": 600,
                  "completion_tokens_details": {"reasoning_tokens": 600}}
    })
    .to_string();
    let server = FakeModel::start(vec![(200, truncated), (200, good_reply("tool_calls"))]).await;

    let call = server.client().call(&chat_request()).await;

    let seen = server.seen();
    assert_eq!(seen.len(), 2, "the call must make exactly two attempts");
    assert_eq!(seen[0].body["max_tokens"], json!(600));
    assert_eq!(seen[1].body["max_tokens"], json!(2400));
    assert_eq!(
        call.result.unwrap(),
        json!({"error_tags": ["sign-error"], "prose": "Watch the sign."})
    );
    assert_eq!(call.attempts.len(), 2, "T6 bills one row per HTTP attempt");
    assert_eq!(call.attempts[0].max_tokens, 600);
    assert_eq!(call.attempts[1].max_tokens, 2400);
}

/// Truncation shape 2: the arguments are cut off mid-JSON. Same widened retry.
#[tokio::test]
async fn arguments_cut_off_mid_json_take_the_widened_retry() {
    let cut = json!({
        "choices": [{"finish_reason": "length", "message": {"tool_calls": [{"function": {
            "name": "emit_diagnosis",
            "arguments": "{\"error_tags\":[\"sign-error\"],\"prose\":\"Watch the"
        }}]}}]
    })
    .to_string();
    let server = FakeModel::start(vec![(200, cut), (200, good_reply("tool_calls"))]).await;

    let call = server.client().call(&chat_request()).await;

    let seen = server.seen();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[1].body["max_tokens"], json!(2400));
    assert!(call.result.is_ok(), "the second reply must be accepted");
}

/// Truncation shape 3: the arguments parse and miss a required field.
#[tokio::test]
async fn arguments_missing_a_required_field_take_the_widened_retry() {
    let partial = json!({
        "choices": [{"native_finish_reason": "length", "message": {"tool_calls": [{"function": {
            "name": "emit_diagnosis",
            "arguments": "{\"error_tags\":[\"sign-error\"]}"
        }}]}}]
    })
    .to_string();
    let server = FakeModel::start(vec![(200, partial), (200, good_reply("tool_calls"))]).await;

    let call = server.client().call(&chat_request()).await;

    let seen = server.seen();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[1].body["max_tokens"], json!(2400));
    assert!(call.result.is_ok());
}

/// A COMPLETE reply that carries `finish_reason: "length"` is accepted. The
/// three shapes are read only after validation fails, so a finished diagnosis is
/// never discarded for its finish reason.
#[tokio::test]
async fn a_complete_reply_with_finish_reason_length_is_accepted() {
    let server = FakeModel::start(vec![(200, good_reply("length"))]).await;

    let call = server.client().call(&chat_request()).await;

    assert_eq!(server.seen().len(), 1, "a complete reply must not retry");
    assert_eq!(
        call.result.unwrap(),
        json!({"error_tags": ["sign-error"], "prose": "Watch the sign."})
    );
}

/// The acceptance literal: a 400 does not retry.
#[tokio::test]
async fn a_four_hundred_does_not_retry() {
    let refusal = json!({"error": {"message": "unknown field provider"}}).to_string();
    let server = FakeModel::start(vec![(400, refusal), (200, good_reply("tool_calls"))]).await;

    let call = server.client().call(&chat_request()).await;

    assert_eq!(
        server.seen().len(),
        1,
        "a 400 must make exactly one attempt"
    );
    match call.result {
        Err(ModelError::Status { status, .. }) => assert_eq!(status, 400),
        other => panic!("a 400 must give ModelError::Status, it gave {other:?}"),
    }
    assert_eq!(call.attempts.len(), 1);
    assert_eq!(call.attempts[0].status, 400);
}

/// A 429 and a 5xx retry, and the second reply stands.
#[tokio::test]
async fn a_rate_limit_retries_and_the_second_reply_stands() {
    let server = FakeModel::start(vec![
        (429, json!({"error": "slow down"}).to_string()),
        (200, good_reply("tool_calls")),
    ])
    .await;

    let call = server.client().call(&chat_request()).await;

    assert_eq!(server.seen().len(), 2);
    assert!(call.result.is_ok());
    assert_eq!(call.attempts[0].status, 429);
    assert_eq!(call.attempts[1].status, 200);
}

/// Two failures end the call. The contract is two attempts, not two retries.
#[tokio::test]
async fn two_server_errors_end_the_call() {
    let server = FakeModel::start(vec![
        (503, String::new()),
        (503, String::new()),
        (200, good_reply("tool_calls")),
    ])
    .await;

    let call = server.client().call(&chat_request()).await;

    assert_eq!(server.seen().len(), 2, "the contract is two HTTP attempts");
    assert!(call.result.is_err());
    assert_eq!(call.attempts.len(), 2);
}

/// A model that ignores the forced tool and writes a fenced object is read.
#[tokio::test]
async fn a_fenced_object_in_the_content_is_read() {
    let fenced = json!({
        "choices": [{"finish_reason": "stop", "message": {
            "content": "```json\n{\"error_tags\":[\"units\"],\"prose\":\"Name the unit.\"}\n```"
        }}]
    })
    .to_string();
    let server = FakeModel::start(vec![(200, fenced)]).await;

    let call = server.client().call(&chat_request()).await;

    assert_eq!(server.seen().len(), 1);
    assert_eq!(
        call.result.unwrap(),
        json!({"error_tags": ["units"], "prose": "Name the unit."})
    );
}

// --------------------------------------------------------------------------- //
// The T6 record and the request headers
// --------------------------------------------------------------------------- //

/// The attempt record reads the four token counts and the cost (spec section 7).
#[tokio::test]
async fn the_attempt_record_reads_the_usage_block() {
    let server = FakeModel::start(vec![(200, good_reply("tool_calls"))]).await;

    let call = server.client().call(&chat_request()).await;

    let attempt = &call.attempts[0];
    assert_eq!(attempt.usage.input_cached, 900);
    assert_eq!(attempt.usage.input_uncached, 100);
    assert_eq!(attempt.usage.output, 120);
    assert_eq!(attempt.usage.reasoning, 80);
    assert_eq!(attempt.cost_usd.as_deref(), Some("0.000123"));
    assert_eq!(attempt.provider.as_deref(), Some("DeepInfra"));
    assert_eq!(attempt.request_id.as_deref(), Some("gen-1"));
    assert_eq!(attempt.model_id, "qwen3.6");
}

/// A reply with no `usage` block gives zeros and no cost. An unmeasured call
/// must be visible as unmeasured, never dropped (spec section 7).
#[tokio::test]
async fn a_reply_with_no_usage_block_gives_zeros_and_no_cost() {
    let bare = json!({
        "choices": [{"finish_reason": "stop", "message": {"tool_calls": [{"function": {
            "name": "emit_diagnosis",
            "arguments": "{\"error_tags\":[],\"prose\":\"Try again.\"}"
        }}]}}]
    })
    .to_string();
    let server = FakeModel::start(vec![(200, bare)]).await;

    let call = server.client().call(&chat_request()).await;

    let attempt = &call.attempts[0];
    assert_eq!(attempt.usage.input_cached, 0);
    assert_eq!(attempt.usage.input_uncached, 0);
    assert_eq!(attempt.usage.output, 0);
    assert_eq!(attempt.usage.reasoning, 0);
    assert_eq!(attempt.cost_usd, None);
    assert_eq!(attempt.provider, None);
}

/// Every request carries the bearer, the content type and `X-Title` (1.0
/// `openai_engine.py:634-641`).
#[tokio::test]
async fn every_request_carries_the_three_headers() {
    let server = FakeModel::start(vec![(200, good_reply("tool_calls"))]).await;

    let _ = server.client().call(&chat_request()).await;

    let head = server.seen()[0].head.clone();
    assert!(
        head.contains("post /v1/chat/completions http/1.1"),
        "{head}"
    );
    assert!(head.contains("authorization: bearer test-key"), "{head}");
    assert!(head.contains("content-type: application/json"), "{head}");
    assert!(head.contains("x-title: cadus"), "{head}");
}

/// An endpoint that accepts the socket and answers nothing costs one bounded
/// attempt per try, never a wait without end.
#[tokio::test]
async fn a_silent_endpoint_ends_at_the_bound() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((socket, _)) = listener.accept().await {
            held.push(socket);
        }
    });

    let mut cfg = local_config(&format!("http://127.0.0.1:{port}/v1"));
    cfg.timeout = Duration::from_millis(200);
    let client = Client::new(cfg).unwrap();

    let call = tokio::time::timeout(Duration::from_secs(5), client.call(&chat_request()))
        .await
        .expect("the call must end inside its own bound");

    assert_eq!(call.attempts.len(), 2);
    assert_eq!(call.attempts[0].status, 0);
    match call.result {
        Err(ModelError::Transport(_)) => {}
        other => panic!("a silent endpoint must give a transport error, it gave {other:?}"),
    }
}

/// The required-field check comes from the TOOL'S schema, not from one document.
///
/// M6 R2 sends a second tool through this client. The M5 spelling hard-coded the
/// two fields of the diagnosis document, so it answered `name no error_tags` for
/// every authoring reply and no authoring call could land.
#[tokio::test]
async fn the_required_fields_come_from_the_tool_schema() {
    let request = ChatRequest {
        system: "You are the author for Cadus.".to_owned(),
        user: "Author a template.".to_owned(),
        tool: ToolSpec {
            name: "emit_template".to_owned(),
            description: "Emit one template.".to_owned(),
            parameters: json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["statement", "samples"],
                "properties": {
                    "statement": {"type": "string"},
                    "samples": {"type": "array", "items": {"type": "object"}},
                },
            }),
        },
    };
    let complete = json!({
        "choices": [{"finish_reason": "tool_calls", "message": {"tool_calls": [{"function": {
            "name": "emit_template",
            "arguments": "{\"statement\":\"Compute $2+2$.\",\"samples\":[]}"
        }}]}}]
    })
    .to_string();
    let partial = json!({
        "choices": [{"finish_reason": "length", "message": {"tool_calls": [{"function": {
            "name": "emit_template",
            "arguments": "{\"statement\":\"Compute $2+2$.\"}"
        }}]}}]
    })
    .to_string();

    // A reply that satisfies the tool's own schema is accepted at once, and no
    // field of the diagnosis document is named anywhere.
    let server = FakeModel::start(vec![(200, complete)]).await;
    let call = server.client().call(&request).await;
    assert_eq!(server.seen().len(), 1);
    assert_eq!(
        call.result.unwrap(),
        json!({"statement": "Compute $2+2$.", "samples": []})
    );

    // A reply that misses one of the tool's required fields names THAT field.
    let server = FakeModel::start(vec![(200, partial.clone()), (200, partial)]).await;
    let call = server.client().call(&request).await;
    assert_eq!(server.seen().len(), 2);
    match call.result {
        Err(ModelError::Reply(why)) => {
            assert_eq!(why, "the arguments of emit_template name no samples");
        }
        other => panic!("a missing required field must give a reply error, it gave {other:?}"),
    }
}
