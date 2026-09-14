//! Fresh, bounded calls to the configured self-hosted judgment service.
//!
//! Model output remains evidence for a caller-owned validation step. This module
//! neither executes tools nor grants a model authority to change learner state.

use std::time::Duration;

use serde_json::{Value, json};

use crate::{HttpClient, ModelConfig, ModelError};

const MAX_INPUT_BYTES: usize = 24 * 1024;
const MAX_FINAL_JSON_BYTES: usize = 64 * 1024;
const MAX_OUTPUT_TOKENS: u32 = 32_768;

/// A stateless client for one configured Qwen endpoint.
#[derive(Debug)]
pub struct QwenClient {
    http: HttpClient,
    config: ModelConfig,
}

impl QwenClient {
    /// Configure the trusted endpoint, exact model identifier, and call limits.
    ///
    /// # Errors
    /// Returns a configuration error for invalid endpoint or budget settings.
    pub fn new(
        base_url: &str,
        model: &str,
        max_tokens: u32,
        timeout: Duration,
    ) -> Result<Self, ModelError> {
        if model.trim().is_empty() || !(1..=MAX_OUTPUT_TOKENS).contains(&max_tokens) {
            return Err(ModelError::Config(
                "Qwen requires a model and an output budget of 1..=32768 tokens".to_owned(),
            ));
        }
        validate_endpoint(base_url, timeout)?;
        let http = HttpClient::new(base_url).map_err(|error| ModelError::Config(error.0))?;
        let config = transport_config(base_url, model, max_tokens, timeout);
        Ok(Self { http, config })
    }

    /// Send exactly one fresh system/user pair and return one completed JSON object.
    ///
    /// Reasoning is discarded. There are no tools, history, or automatic retries.
    ///
    /// # Errors
    /// Returns an error for oversized input, transport failure, an upstream
    /// refusal, an incomplete stream, or a final answer outside strict JSON.
    pub async fn complete(&self, system: &str, user: &str) -> Result<Value, ModelError> {
        if system.trim().is_empty()
            || user.trim().is_empty()
            || system.len().saturating_add(user.len()) > MAX_INPUT_BYTES
        {
            return Err(ModelError::Config(
                "Qwen requires two nonempty messages totaling at most 24576 bytes".to_owned(),
            ));
        }
        let body = self.request(system, user);
        let bytes = send(&self.http, &self.config, &body).await?;
        parse_stream(&bytes)
    }

    fn request(&self, system: &str, user: &str) -> Value {
        json!({
            "model": self.config.model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user}
            ],
            "stream": true,
            "stream_options": {"include_usage": true},
            "max_tokens": self.config.output_tokens,
            "temperature": 0,
            "chat_template_kwargs": {
                "enable_thinking": true,
                "reasoning_effort": "medium"
            }
        })
    }
}

/// POST to an exact, operator-configured internal verifier URL.
///
/// The URL must come from trusted application configuration, never model output
/// or learner input. Uses the shared bounded transport with no authentication,
/// redirects, endpoint suffix, or automatic retries.
///
/// # Errors
/// Returns an error for invalid settings, oversized input, failed HTTP, or a
/// response that is not one complete JSON value.
pub async fn post_json(url: &str, payload: &Value, timeout: Duration) -> Result<Value, ModelError> {
    validate_endpoint(url, timeout)?;
    if payload.to_string().len() > MAX_INPUT_BYTES {
        return Err(ModelError::Config(
            "the verifier request exceeds 24576 bytes".to_owned(),
        ));
    }
    let http = HttpClient::new_exact(url).map_err(|error| ModelError::Config(error.0))?;
    let config = transport_config(url, "", 1, timeout);
    let bytes = send(&http, &config, payload).await?;
    serde_json::from_slice(&bytes)
        .map_err(|_| ModelError::Reply("the verifier returned invalid JSON".to_owned()))
}

fn validate_endpoint(endpoint: &str, timeout: Duration) -> Result<(), ModelError> {
    let parsed = url::Url::parse(endpoint)
        .map_err(|_| ModelError::Config("the configured endpoint is not a URL".to_owned()))?;
    if timeout.is_zero()
        || !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.fragment().is_some()
    {
        return Err(ModelError::Config(
            "the endpoint requires HTTP(S), a host, no embedded credentials or fragment, and a positive timeout"
                .to_owned(),
        ));
    }
    Ok(())
}

fn transport_config(
    base_url: &str,
    model: &str,
    max_tokens: u32,
    timeout: Duration,
) -> ModelConfig {
    ModelConfig {
        base_url: base_url.to_owned(),
        api_key: String::new(),
        model: model.to_owned(),
        output_tokens: max_tokens,
        reasoning_max_tokens: 0,
        provider_order: Vec::new(),
        timeout,
    }
}

async fn send(
    http: &HttpClient,
    config: &ModelConfig,
    body: &Value,
) -> Result<Vec<u8>, ModelError> {
    let (status, bytes) = http
        .post_json(config, body)
        .await
        .map_err(|error| ModelError::Transport(error.0))?;
    if !(200..300).contains(&status) {
        return Err(ModelError::Status {
            status,
            body: "the configured internal endpoint refused the request".to_owned(),
        });
    }
    Ok(bytes)
}

#[derive(Default)]
struct Completion {
    content: String,
    finished: bool,
    done: bool,
}

fn refused(reason: &str) -> ModelError {
    ModelError::Reply(reason.to_owned())
}

impl Completion {
    fn frame(&mut self, data: &str) -> Result<(), ModelError> {
        if data.is_empty() {
            return Ok(());
        }
        if self.done {
            return Err(refused(
                "the Qwen stream continued after its terminal marker",
            ));
        }
        if data.trim() == "[DONE]" {
            self.done = true;
            return Ok(());
        }
        let event: Value = serde_json::from_str(data)
            .map_err(|_| refused("the Qwen stream contained malformed JSON"))?;
        let choices = event
            .get("choices")
            .and_then(Value::as_array)
            .ok_or_else(|| refused("the Qwen stream omitted its choices"))?;
        if choices.len() > 1 {
            return Err(refused("the Qwen stream returned multiple choices"));
        }
        for choice in choices {
            self.choice(choice)?;
        }
        Ok(())
    }

    fn choice(&mut self, choice: &Value) -> Result<(), ModelError> {
        if !choice.is_object()
            || choice
                .get("index")
                .is_some_and(|index| index.as_u64() != Some(0))
        {
            return Err(refused("the Qwen stream returned an invalid choice"));
        }
        if let Some(delta) = choice.get("delta").filter(|delta| !delta.is_null()) {
            self.delta(delta)?;
        }
        if let Some(reason) = choice
            .get("finish_reason")
            .filter(|reason| !reason.is_null())
        {
            if reason.as_str() != Some("stop") || self.finished {
                return Err(refused(
                    "the Qwen completion was truncated or did not stop normally",
                ));
            }
            self.finished = true;
        }
        Ok(())
    }

    fn delta(&mut self, delta: &Value) -> Result<(), ModelError> {
        if !delta.is_object() {
            return Err(refused("the Qwen stream returned an invalid delta"));
        }
        if let Some(calls) = delta.get("tool_calls").filter(|calls| !calls.is_null())
            && !calls.as_array().is_some_and(Vec::is_empty)
        {
            return Err(refused("the Qwen completion attempted a tool call"));
        }
        // The reasoning field is intentionally never copied or returned.
        if let Some(value) = delta.get("content").filter(|value| !value.is_null()) {
            let content = value
                .as_str()
                .ok_or_else(|| refused("the Qwen content was not text"))?;
            if (self.finished && !content.is_empty())
                || self.content.len().saturating_add(content.len()) > MAX_FINAL_JSON_BYTES
            {
                return Err(refused("the Qwen final content exceeded its bounds"));
            }
            self.content.push_str(content);
        }
        Ok(())
    }

    fn judgment(self) -> Result<Value, ModelError> {
        if !self.finished || !self.done {
            return Err(refused("the Qwen stream has no complete terminal sequence"));
        }
        let value: Value = serde_json::from_str(&self.content)
            .map_err(|_| refused("the Qwen final answer was not strict JSON"))?;
        if !value.is_object() {
            return Err(refused("the Qwen final answer was not a JSON object"));
        }
        Ok(value)
    }
}

fn parse_stream(bytes: &[u8]) -> Result<Value, ModelError> {
    let text = std::str::from_utf8(bytes).map_err(|_| refused("the Qwen stream was not UTF-8"))?;
    let mut completion = Completion::default();
    let mut data = String::new();
    for line in text.lines() {
        if line.is_empty() {
            completion.frame(&data)?;
            data.clear();
        } else if let Some(value) = line.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(value.strip_prefix(' ').unwrap_or(value));
        } else if !line.starts_with(':')
            && !line.starts_with("event:")
            && !line.starts_with("id:")
            && !line.starts_with("retry:")
        {
            return Err(refused("the Qwen response was not an event stream"));
        }
    }
    completion.frame(&data)?;
    completion.judgment()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(content: &str, finish: &str, done: bool) -> Vec<u8> {
        let first = json!({"choices":[{"index":0,"delta":{"reasoning":"private reasoning","content":content},"finish_reason":null}]});
        let last = json!({"choices":[{"index":0,"delta":{},"finish_reason":finish}]});
        let usage = json!({"choices":[],"usage":{"completion_tokens":12}});
        format!(
            ": keepalive\n\ndata: {first}\n\ndata: {last}\n\ndata: {usage}\n\n{}",
            if done { "data: [DONE]\n\n" } else { "" }
        )
        .into_bytes()
    }

    #[test]
    fn completed_json_preserves_content_and_excludes_reasoning() {
        assert_eq!(
            parse_stream(&stream(
                r#"{"message":"  exact text  ","correct":true}"#,
                "stop",
                true
            ))
            .unwrap(),
            json!({"message":"  exact text  ","correct":true})
        );
    }

    #[test]
    fn refuses_truncation_missing_terminal_and_non_object_answers() {
        for bytes in [
            stream("{}", "length", true),
            stream("{}", "stop", false),
            stream("[]", "stop", true),
            stream("true", "stop", true),
            stream("```json\n{}\n```", "stop", true),
            stream("{} trailing words", "stop", true),
            b"data: [DONE]\n\n".to_vec(),
            b"{\"choices\":[]}".to_vec(),
        ] {
            assert!(parse_stream(&bytes).is_err());
        }
    }

    #[test]
    fn refuses_tool_calls_and_malformed_frames() {
        for frame in [
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{}]}}]}\n\n",
            "data: not-json\n\n",
            "data: {\"choices\":[{\"index\":1,\"delta\":{}}]}\n\n",
        ] {
            assert!(parse_stream(frame.as_bytes()).is_err());
        }
    }

    #[test]
    fn refuses_oversized_final_content() {
        let answer = json!({"text":"x".repeat(MAX_FINAL_JSON_BYTES)}).to_string();
        assert!(parse_stream(&stream(&answer, "stop", true)).is_err());
    }

    #[test]
    fn fresh_request_has_exactly_two_messages_and_no_tools() {
        let client = QwenClient::new(
            "http://127.0.0.1:8080/v1",
            "configured-model",
            8192,
            Duration::from_secs(600),
        )
        .unwrap();
        let request = client.request("fresh system", "fresh user");
        assert_eq!(
            request["messages"],
            json!([
                {"role":"system","content":"fresh system"},
                {"role":"user","content":"fresh user"}
            ])
        );
        assert!(request.get("tools").is_none());
        assert!(request.get("tool_choice").is_none());
        assert_eq!(
            request["chat_template_kwargs"],
            json!({"enable_thinking":true,"reasoning_effort":"medium"})
        );
        assert_eq!(request["max_tokens"], 8192);
        assert!(client.config.api_key.is_empty());
    }

    #[test]
    fn refuses_invalid_configuration() {
        for (url, tokens, timeout) in [
            ("http://localhost/v1", 0, Duration::from_secs(1)),
            ("http://localhost/v1", 32_769, Duration::from_secs(1)),
            ("http://localhost/v1", 1, Duration::ZERO),
            (
                "http://name:password@localhost/v1",
                1,
                Duration::from_secs(1),
            ),
            ("file:///tmp/model", 1, Duration::from_secs(1)),
        ] {
            assert!(QwenClient::new(url, "model", tokens, timeout).is_err());
        }
    }
}
