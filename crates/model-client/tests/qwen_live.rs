//! Explicit opt-in smoke against the self-hosted Qwen service.
use cadus_model_client::QwenClient;
use serde_json::json;
use std::time::Duration;

#[tokio::test]
#[ignore = "requires explicit access to the self-hosted Qwen endpoint"]
async fn live_qwen_returns_a_complete_json_answer() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(std::env::var("CADUS_QWEN_LIVE_TEST").as_deref(), Ok("1"));
    let base = std::env::var("QWEN_BASE_URL")?;
    let model = std::env::var("QWEN_MODEL")?;
    let client = QwenClient::new(&base, &model, 8192, Duration::from_secs(600))?;
    let reply = tokio::time::timeout(
        Duration::from_secs(660),
        client.complete(
            "Solve the arithmetic. Return only one JSON object with an answer field containing a string. Do not use tools.",
            "What is 2 + 2?",
        ),
    ).await??;
    assert_eq!(reply, json!({"answer":"4"}));
    Ok(())
}
