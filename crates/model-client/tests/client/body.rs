//! M5 U10 acceptance, the request half: the T5 request body of the routing
//! host and of a local endpoint (spec section 6.4).
//!
//! Every expected value is a LITERAL. Nothing is read back from the code under
//! test.

use std::time::Duration;

use crate::common::{chat_request, local_config};
use cadus_model_client::{ModelConfig, request_body};
use serde_json::json;

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
