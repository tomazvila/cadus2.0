//! M5 U10 acceptance, the record half: the T6 attempt record, the request
//! headers, the bound of a silent endpoint, and the schema-driven required
//! fields (spec sections 6.3 and 7).
//!
//! Every expected value is a LITERAL. Nothing is read back from the code under
//! test.

use std::time::Duration;

use crate::common::{FakeModel, call_with, chat_request, good_reply, local_config};
use cadus_model_client::{ChatRequest, Client, ModelError, ToolSpec};
use serde_json::json;
use tokio::net::TcpListener;

// --------------------------------------------------------------------------- //
// The T6 record and the request headers
// --------------------------------------------------------------------------- //

/// The attempt record reads the four token counts and the cost (spec section 7).
#[tokio::test]
async fn the_attempt_record_reads_the_usage_block() {
    let (_server, call) = call_with(vec![(200, good_reply("tool_calls"))]).await;

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
    let (_server, call) = call_with(vec![(200, bare)]).await;

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
    let (server, _call) = call_with(vec![(200, good_reply("tool_calls"))]).await;

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
