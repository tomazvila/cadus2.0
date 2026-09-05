//! The one OpenAI-compatible model client of Cadus 2.0 (T2, T5, T6).
//!
//! Requirements: L6 and T1 (no request path reaches this crate; only
//! `cadus-worker` depends on it), T2 (async diagnosis and offline authoring are
//! the two spenders), T5 (prompt caching on, a reasoning cap set, provider order
//! pinned), T6 (every HTTP attempt reports its tokens and its latency).
//!
//! Spec: `docs/reference/web-service-1.0-spec.md` sections 6.3, 6.4 and 6.5, and
//! row U10 of section 11.
//!
//! # One client, two endpoints
//!
//! [`ModelConfig::base_url`] names either OpenRouter or a local
//! OpenAI-compatible endpoint. The difference is one block of the request body:
//! `provider` and `reasoning` are OpenRouter extensions, and posting them to a
//! plain OpenAI endpoint earns a non-retryable 400. [`is_routing_host`] decides
//! the branch on the PARSED host, never on a substring: 1.0 tested
//! `"openrouter.ai" in url`, which also matched
//! `https://openrouter.ai.attacker.example/v1`.
//!
//! # The retry contract (spec section 6.5)
//!
//! [`MAX_ATTEMPTS`] HTTP attempts per call, [`BACKOFF_MS`] between them and
//! doubling. A 429, a 5xx, a transport error and a reply that fails validation
//! all retry. Every other status — a 400 above all — returns at once, because a
//! rejected request body is reproduced exactly by a second copy of itself.
//!
//! # Truncation is not malformation
//!
//! A reasoning model shares its completion budget with its hidden reasoning, so
//! the forced tool call is cut off before one visible byte: `completion_tokens`
//! at the ceiling, `finish_reason: "length"`, and no `tool_calls` at all. An
//! identical retry reproduces that, so the retry widens the ceiling
//! [`TRUNCATION_FACTOR`]×. The three shapes are all read AFTER validation fails
//! ([`parse_reply`]), so a COMPLETE reply is never discarded for its
//! `finish_reason`.

#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )
)]

mod attempt;
mod config;
mod reply;
mod transport;

use std::time::{Duration, Instant};

use serde_json::{Value, json};

pub use attempt::{Attempt, Usage};
pub use config::{ModelConfig, is_routing_host};
pub use transport::{HttpClient, TransportError};

use attempt::{failed_attempt, read_attempt};
use reply::{ReplyProblem, parse_reply};

/// The environment variable that names the endpoint.
pub const BASE_URL_VAR: &str = "OPENAI_BASE_URL";

/// The environment variable that holds the API key.
pub const API_KEY_VAR: &str = "OPENAI_API_KEY";

/// The environment variable that names the model id.
pub const MODEL_VAR: &str = "OPENAI_MODEL";

/// The environment variable that pins the provider order (T5).
pub const PROVIDER_ORDER_VAR: &str = "OPENROUTER_PROVIDER_ORDER";

/// The environment variable that holds the per-call output bound (T4, T5).
pub const OUTPUT_TOKENS_VAR: &str = "DIAGNOSIS_OUTPUT_TOKENS";

/// The environment variable that holds the reasoning bound (T5).
pub const REASONING_MAX_TOKENS_VAR: &str = "DIAGNOSIS_REASONING_MAX_TOKENS";

/// The environment variable that holds the AUTHORING output bound (T3, T5).
pub const AUTHORING_OUTPUT_TOKENS_VAR: &str = "AUTHORING_OUTPUT_TOKENS";

/// The environment variable that holds the AUTHORING reasoning bound (T5).
pub const AUTHORING_REASONING_MAX_TOKENS_VAR: &str = "AUTHORING_REASONING_MAX_TOKENS";

/// The endpoint the deployment uses when the environment names none (O2).
pub const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";

/// The model the deployment uses when the environment names none (O2).
pub const DEFAULT_MODEL: &str = "deepseek/deepseek-v4-pro";

/// The per-call output bound of a DIAGNOSIS call (T4 knob
/// `DIAGNOSIS_OUTPUT_TOKENS`).
///
/// O2 sets no spend cap. This one stays at a real value because it is a LATENCY
/// bound (T5), not a spend cap.
pub const DEFAULT_OUTPUT_TOKENS: u32 = 600;

/// The reasoning bound of every request (T5, `reasoning.max_tokens`).
pub const DEFAULT_REASONING_MAX_TOKENS: u32 = 600;

/// The per-call output bound of an AUTHORING call (knob
/// `AUTHORING_OUTPUT_TOKENS`).
///
/// A diagnosis reply is one short document about one answer. An authored
/// document is a whole template, a teach page, a hint ladder or a distractor
/// set, so it needs a wider budget. An authoring pass is also offline: it pays
/// per BATCH and no learner waits for it, so the latency bound of the diagnosis
/// path does not apply here.
pub const DEFAULT_AUTHORING_OUTPUT_TOKENS: u32 = 4000;

/// The reasoning bound of an AUTHORING call (T5,
/// `AUTHORING_REASONING_MAX_TOKENS`).
///
/// The reasoning budget is part of the output budget, so a reasoning ceiling
/// equal to the output ceiling leaves zero visible tokens. This value keeps half
/// of [`DEFAULT_AUTHORING_OUTPUT_TOKENS`] for the document itself.
pub const DEFAULT_AUTHORING_REASONING_MAX_TOKENS: u32 = 2000;

/// The host whose body carries the `provider` and `reasoning` blocks.
pub const ROUTING_HOST: &str = "openrouter.ai";

/// The HTTP attempts of one call (1.0 `engine.py:57`).
pub const MAX_ATTEMPTS: u32 = 2;

/// The wait before the second attempt, in milliseconds. It doubles per attempt
/// (1.0 `engine.py:58`).
pub const BACKOFF_MS: u64 = 500;

/// How much wider the ceiling of a truncation retry is (1.0
/// `openai_engine.py:549-552`).
pub const TRUNCATION_FACTOR: u32 = 4;

/// The client-side bound of one HTTP attempt (1.0 `openai_engine.py:125`).
pub const TIMEOUT_SECS: u64 = 60;

/// The temperature of a grading-shaped call (1.0 `openai_engine.py:465`).
pub const TEMPERATURE: f64 = 0.0;

/// The value of the `X-Title` header (1.0 `openai_engine.py:634-641`).
pub const TITLE: &str = "Cadus";

/// The stable prompt-cache key of the diagnosis prefix (T5, OpenAI style).
pub const PROMPT_CACHE_KEY: &str = "cadus-diagnosis-v1";

/// What went wrong in one call.
#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    /// The environment does not describe a usable endpoint.
    #[error("model configuration error: {0}")]
    Config(String),

    /// Every attempt failed to reach the endpoint.
    #[error("model transport error: {0}")]
    Transport(String),

    /// The endpoint refused the request and a retry reproduces the refusal.
    #[error("model endpoint answered {status}: {body}")]
    Status {
        /// The HTTP status code.
        status: u16,
        /// The first bytes of the body, for the operator log.
        body: String,
    },

    /// Every attempt answered, and no answer held a usable diagnosis.
    #[error("model reply error: {0}")]
    Reply(String),
}

/// The forced tool of one call (spec section 6.3).
#[derive(Debug, Clone)]
pub struct ToolSpec {
    /// The function name the reply must call.
    pub name: String,
    /// What the function is for, in the model's words.
    pub description: String,
    /// The JSON schema of the arguments, `additionalProperties: false`.
    pub parameters: Value,
}

/// One chat completion: two messages and one forced tool.
#[derive(Debug, Clone)]
pub struct ChatRequest {
    /// The system message. It is the cached prefix (T5).
    pub system: String,
    /// The user message.
    pub user: String,
    /// The forced tool.
    pub tool: ToolSpec,
}

/// One finished call: the arguments of the forced tool, plus its bill.
#[derive(Debug)]
pub struct Call {
    /// One record per HTTP attempt, in order (T6).
    pub attempts: Vec<Attempt>,
    /// The arguments object of the forced tool, or why there is none.
    pub result: Result<Value, ModelError>,
}

/// What one attempt decided.
#[derive(Debug)]
enum Verdict {
    /// The reply held the arguments of the forced tool.
    Done(Value),
    /// The endpoint refused the request and a retry reproduces the refusal.
    Stop(ModelError),
    /// The attempt failed in a way a second attempt can mend. `widen` asks
    /// the retry for a wider completion ceiling.
    Retry { widen: bool, error: ModelError },
}

/// The client. It holds the configuration and the TLS setup, so one worker
/// builds it once and calls it per job.
#[derive(Debug)]
pub struct Client {
    config: ModelConfig,
    http: HttpClient,
}

impl Client {
    /// Build a client for this endpoint.
    ///
    /// # Errors
    ///
    /// Returns [`ModelError::Config`] when the base URL does not parse and when
    /// the TLS trust store does not build.
    pub fn new(config: ModelConfig) -> Result<Self, ModelError> {
        let http = HttpClient::new(&config.base_url).map_err(|err| ModelError::Config(err.0))?;
        Ok(Self { config, http })
    }

    /// The configuration this client was built with.
    #[must_use]
    pub fn config(&self) -> &ModelConfig {
        &self.config
    }

    /// Run one call under the retry contract of spec section 6.5.
    ///
    /// The answer always carries one [`Attempt`] record per HTTP attempt, in
    /// order, whether the call ended in a diagnosis or in an error: T6 bills a
    /// failed attempt too.
    pub async fn call(&self, request: &ChatRequest) -> Call {
        let mut attempts: Vec<Attempt> = Vec::new();
        let mut max_tokens = self.config.output_tokens;
        let mut last = ModelError::Transport("no attempt ran".to_owned());

        for index in 0..MAX_ATTEMPTS {
            match self
                .attempt(index, max_tokens, request, &mut attempts)
                .await
            {
                Verdict::Done(arguments) => {
                    return Call {
                        attempts,
                        result: Ok(arguments),
                    };
                }
                Verdict::Stop(error) => {
                    return Call {
                        attempts,
                        result: Err(error),
                    };
                }
                Verdict::Retry { widen, error } => {
                    last = error;
                    if widen {
                        max_tokens = max_tokens.saturating_mul(TRUNCATION_FACTOR);
                    }
                }
            }
            if !self.wait(index).await {
                break;
            }
        }

        Call {
            attempts,
            result: Err(last),
        }
    }

    /// Run one HTTP attempt with the ceiling `max_tokens`, record it in
    /// `attempts`, and decide what the caller does next.
    async fn attempt(
        &self,
        index: u32,
        max_tokens: u32,
        request: &ChatRequest,
        attempts: &mut Vec<Attempt>,
    ) -> Verdict {
        let body = request_body(&self.config, request, max_tokens);
        let started = Instant::now();
        let sent = self.http.post_json(&self.config, &body).await;
        let latency_ms = u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);

        let (status, reply) = match sent {
            Ok(answer) => answer,
            Err(err) => {
                attempts.push(failed_attempt(index, max_tokens, latency_ms, &self.config));
                // A transport error is retryable: the endpoint said nothing,
                // so nothing about the request is known to be wrong.
                return Verdict::Retry {
                    widen: false,
                    error: ModelError::Transport(err.0),
                };
            }
        };
        let parsed: Value = serde_json::from_slice(&reply).unwrap_or(Value::Null);
        attempts.push(read_attempt(
            index,
            max_tokens,
            status,
            latency_ms,
            &parsed,
            &self.config,
        ));

        if status != 200 {
            return status_verdict(status, &reply);
        }
        match parse_reply(&parsed, &request.tool) {
            Ok(arguments) => Verdict::Done(arguments),
            Err(problem) => Verdict::Retry {
                widen: matches!(problem, ReplyProblem::Truncated(_)),
                error: ModelError::Reply(problem.into_message()),
            },
        }
    }

    /// Wait the backoff of `index`. Return `false` when `index` was the last
    /// attempt, so the caller stops instead of waiting for nothing.
    async fn wait(&self, index: u32) -> bool {
        if index + 1 >= MAX_ATTEMPTS {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(BACKOFF_MS << index)).await;
        true
    }
}

/// The verdict of a status other than 200.
///
/// Spec section 6.5: 429 and 5xx retry; every other status — a 400 above all —
/// returns at once. The error carries the first 400 characters of the body,
/// for the operator log.
fn status_verdict(status: u16, reply: &[u8]) -> Verdict {
    let body = String::from_utf8_lossy(reply).chars().take(400).collect();
    let error = ModelError::Status { status, body };
    if status == 429 || status >= 500 {
        return Verdict::Retry {
            widen: false,
            error,
        };
    }
    Verdict::Stop(error)
}

/// Build the request body of one attempt (spec section 6.4).
///
/// The T5 defaults are in every body:
///
/// - prompt caching: the system message carries `cache_control` on a content
///   block (Anthropic style, which OpenRouter forwards) and the body carries
///   `prompt_cache_key` (OpenAI style). 1.0 set neither.
/// - `reasoning.max_tokens` and `provider.order`: on the routing host only. Both
///   are OpenRouter extensions, and a plain OpenAI endpoint answers 400 for
///   them, which the retry contract must never provoke.
/// - `provider.allow_fallbacks` stays `true`: a hard pin turns one provider's
///   outage into a dead worker.
#[must_use]
pub fn request_body(cfg: &ModelConfig, request: &ChatRequest, max_tokens: u32) -> Value {
    let routing = is_routing_host(&cfg.base_url).unwrap_or(false);
    let system = if routing {
        json!([{ "type": "text", "text": request.system,
                 "cache_control": { "type": "ephemeral" } }])
    } else {
        json!(request.system)
    };
    let mut body = json!({
        "model": cfg.model,
        "max_tokens": max_tokens,
        "temperature": TEMPERATURE,
        "prompt_cache_key": PROMPT_CACHE_KEY,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": request.user },
        ],
        "tools": [{
            "type": "function",
            "function": {
                "name": request.tool.name,
                "description": request.tool.description,
                "parameters": request.tool.parameters,
            },
        }],
        "tool_choice": {
            "type": "function",
            "function": { "name": request.tool.name },
        },
    });
    if routing && let Some(map) = body.as_object_mut() {
        map.insert(
            "provider".to_owned(),
            json!({ "order": cfg.provider_order, "allow_fallbacks": true }),
        );
        map.insert(
            "reasoning".to_owned(),
            json!({ "max_tokens": cfg.reasoning_max_tokens }),
        );
    }
    body
}

#[cfg(test)]
mod tests {
    use super::{Verdict, status_verdict};

    /// A 429 and every 5xx retry; every other status stops the call. The
    /// error carries at most 400 characters of the body.
    #[test]
    fn a_rate_limit_and_a_server_error_retry_and_every_other_status_stops() {
        for (status, retries) in [
            (429, true),
            (500, true),
            (503, true),
            (400, false),
            (404, false),
        ] {
            let verdict = status_verdict(status, b"body");
            let retry = matches!(verdict, Verdict::Retry { widen: false, .. });
            assert_eq!(retry, retries, "status {status}");
        }
        let long = "x".repeat(500);
        assert_eq!(
            format!("{:?}", status_verdict(400, long.as_bytes())),
            format!(
                "Stop(Status {{ status: 400, body: {:?} }})",
                "x".repeat(400)
            )
        );
    }
}
