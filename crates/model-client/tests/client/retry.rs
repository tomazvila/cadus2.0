//! M5 U10 acceptance, the retry half: the three truncation shapes and the
//! retry contract (spec section 6.5).
//!
//! Every expected value is a LITERAL: a literal `max_tokens`, a literal status,
//! a literal request count. Nothing is read back from the code under test.

use crate::common::{FakeModel, call_with, good_reply, widened_retry};
use cadus_model_client::ModelError;
use serde_json::json;

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

    let call = widened_retry(truncated).await;

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

    let call = widened_retry(cut).await;

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

    let call = widened_retry(partial).await;

    assert!(call.result.is_ok());
}

/// A COMPLETE reply that carries `finish_reason: "length"` is accepted. The
/// three shapes are read only after validation fails, so a finished diagnosis is
/// never discarded for its finish reason.
#[tokio::test]
async fn a_complete_reply_with_finish_reason_length_is_accepted() {
    let (server, call) = call_with(vec![(200, good_reply("length"))]).await;

    assert_eq!(server.seen().len(), 1, "a complete reply must not retry");
    assert_eq!(
        call.result.unwrap(),
        json!({"error_tags": ["sign-error"], "prose": "Watch the sign."})
    );
}

/// A reply that is shaped wrong and NOT truncated retries at the same ceiling,
/// and the second wrong reply ends the call with the reply error.
#[tokio::test]
async fn a_malformed_reply_retries_at_the_same_ceiling() {
    let odd = json!({"choices": [{"finish_reason": "stop", "message": {"content": "not json"}}]})
        .to_string();
    let (server, call) = call_with(vec![(200, odd.clone()), (200, odd)]).await;

    let seen = server.seen();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[1].body["max_tokens"], json!(600));
    match call.result {
        Err(ModelError::Reply(why)) => assert!(
            why.starts_with("the arguments of emit_diagnosis do not parse: "),
            "{why}"
        ),
        other => panic!("a malformed reply must give a reply error, it gave {other:?}"),
    }
}

/// The acceptance literal: a 400 does not retry.
#[tokio::test]
async fn a_four_hundred_does_not_retry() {
    let refusal = json!({"error": {"message": "unknown field provider"}}).to_string();
    let (server, call) = call_with(vec![(400, refusal), (200, good_reply("tool_calls"))]).await;

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
    let (server, call) = call_with(vec![
        (429, json!({"error": "slow down"}).to_string()),
        (200, good_reply("tool_calls")),
    ])
    .await;

    assert_eq!(server.seen().len(), 2);
    assert!(call.result.is_ok());
    assert_eq!(call.attempts[0].status, 429);
    assert_eq!(call.attempts[1].status, 200);
}

/// Two failures end the call. The contract is two attempts, not two retries.
#[tokio::test]
async fn two_server_errors_end_the_call() {
    let (server, call) = call_with(vec![
        (503, String::new()),
        (503, String::new()),
        (200, good_reply("tool_calls")),
    ])
    .await;

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

    let call = server.call().await;

    assert_eq!(server.seen().len(), 1);
    assert_eq!(
        call.result.unwrap(),
        json!({"error_tags": ["units"], "prose": "Name the unit."})
    );
}
