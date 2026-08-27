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

mod transport;

use std::time::{Duration, Instant};

use serde_json::{Value, json};
use url::Url;

pub use transport::{HttpClient, TransportError};

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

/// The endpoint the deployment uses when the environment names none (O2).
pub const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";

/// The model the deployment uses when the environment names none (O2).
pub const DEFAULT_MODEL: &str = "deepseek/deepseek-v4-pro";

/// The per-call output bound (T4 knob `DIAGNOSIS_OUTPUT_TOKENS`).
///
/// O2 sets no spend cap. This one stays at a real value because it is a LATENCY
/// bound (T5), not a spend cap.
pub const DEFAULT_OUTPUT_TOKENS: u32 = 600;

/// The reasoning bound of every request (T5, `reasoning.max_tokens`).
pub const DEFAULT_REASONING_MAX_TOKENS: u32 = 600;

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

/// The endpoint, the credentials and the T5 defaults of one deployment.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelConfig {
    /// The base URL, without a trailing `/chat/completions`.
    pub base_url: String,
    /// The bearer token of the endpoint.
    pub api_key: String,
    /// The model id the request names.
    pub model: String,
    /// The output ceiling of the first attempt (T4, T5).
    pub output_tokens: u32,
    /// The reasoning ceiling of every attempt (T5).
    pub reasoning_max_tokens: u32,
    /// The pinned provider order (T5). It is empty for a non-routing endpoint.
    pub provider_order: Vec<String>,
    /// The client-side bound of one attempt.
    pub timeout: Duration,
}

impl ModelConfig {
    /// Read the endpoint from the environment.
    ///
    /// # Errors
    ///
    /// Returns [`ModelError::Config`] when [`API_KEY_VAR`] is absent or empty,
    /// when the base URL does not parse, when a token bound is not a whole
    /// number, and when the endpoint is the routing host and
    /// [`PROVIDER_ORDER_VAR`] names no provider. T5 makes the order a shipped
    /// default, so an empty one is an operator mistake and not a silent
    /// fallback to 1.0's unset routing.
    pub fn from_env() -> Result<Self, ModelError> {
        let base_url = var_or(BASE_URL_VAR, DEFAULT_BASE_URL);
        let api_key = var_or(API_KEY_VAR, "");
        if api_key.is_empty() {
            return Err(ModelError::Config(format!("{API_KEY_VAR} is empty")));
        }
        let provider_order: Vec<String> = var_or(PROVIDER_ORDER_VAR, "")
            .split(',')
            .map(|name| name.trim().to_owned())
            .filter(|name| !name.is_empty())
            .collect();
        if is_routing_host(&base_url)? && provider_order.is_empty() {
            return Err(ModelError::Config(format!(
                "{PROVIDER_ORDER_VAR} is empty; T5 pins the provider order of {ROUTING_HOST}"
            )));
        }
        Ok(Self {
            base_url,
            api_key,
            model: var_or(MODEL_VAR, DEFAULT_MODEL),
            output_tokens: number(OUTPUT_TOKENS_VAR, DEFAULT_OUTPUT_TOKENS)?,
            reasoning_max_tokens: number(REASONING_MAX_TOKENS_VAR, DEFAULT_REASONING_MAX_TOKENS)?,
            provider_order,
            timeout: Duration::from_secs(TIMEOUT_SECS),
        })
    }
}

/// The environment value, or the default when it is absent or blank.
fn var_or(name: &str, fallback: &str) -> String {
    match std::env::var(name) {
        Ok(raw) if !raw.trim().is_empty() => raw.trim().to_owned(),
        _ => fallback.to_owned(),
    }
}

/// One whole-number knob. A present value that is not a number is an error, so
/// an operator typo never falls back to the default in silence.
fn number(name: &str, fallback: u32) -> Result<u32, ModelError> {
    let raw = var_or(name, "");
    if raw.is_empty() {
        return Ok(fallback);
    }
    raw.parse()
        .map_err(|_| ModelError::Config(format!("{name} must be a whole number, not {raw:?}")))
}

/// Does this endpoint take the OpenRouter `provider` and `reasoning` blocks?
///
/// The check reads the parsed host and compares the WHOLE name, so
/// `https://openrouter.ai.attacker.example/v1` is not the routing host and
/// `https://openrouter.ai/api/v1` is.
///
/// # Errors
///
/// Returns [`ModelError::Config`] when the URL does not parse or names no host.
pub fn is_routing_host(base_url: &str) -> Result<bool, ModelError> {
    let url = Url::parse(base_url)
        .map_err(|err| ModelError::Config(format!("{BASE_URL_VAR} does not parse: {err}")))?;
    let host = url
        .host_str()
        .ok_or_else(|| ModelError::Config(format!("{BASE_URL_VAR} names no host")))?;
    Ok(host.eq_ignore_ascii_case(ROUTING_HOST))
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

/// The token counts of one HTTP attempt (T6, spec section 7).
///
/// Every field of an OpenAI-compatible `usage` block is optional on some
/// provider, so each read defaults to 0 and a missing block gives a zeros
/// record. An unmeasured call must be visible as unmeasured, never dropped.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    /// `usage.prompt_tokens_details.cached_tokens`.
    pub input_cached: u32,
    /// `usage.prompt_tokens` minus the cached part.
    pub input_uncached: u32,
    /// `usage.completion_tokens`.
    pub output: u32,
    /// `usage.completion_tokens_details.reasoning_tokens`.
    pub reasoning: u32,
}

/// What one HTTP attempt cost and how it ended (T6).
///
/// Unit U11 writes one `model_call_log` row per record, so a truncation retry is
/// two calls and two bills.
#[derive(Debug, Clone, PartialEq)]
pub struct Attempt {
    /// The zero-based index of the attempt.
    pub index: u32,
    /// The `max_tokens` the request carried.
    pub max_tokens: u32,
    /// The HTTP status, or 0 when the attempt never reached a status.
    pub status: u16,
    /// The wall clock around the HTTP call, in milliseconds.
    pub latency_ms: u32,
    /// The tokens the reply reported.
    pub usage: Usage,
    /// The model id the request sent.
    pub model_id: String,
    /// OpenRouter's `provider` field; `None` elsewhere.
    pub provider: Option<String>,
    /// The provider's `id` field, for a support ticket.
    pub request_id: Option<String>,
    /// OpenRouter's `usage.cost`, as the exact text of the body. `None` keeps
    /// the money column NULL rather than guessing at it.
    pub cost_usd: Option<String>,
}

/// One finished call: the arguments of the forced tool, plus its bill.
#[derive(Debug)]
pub struct Call {
    /// One record per HTTP attempt, in order (T6).
    pub attempts: Vec<Attempt>,
    /// The arguments object of the forced tool, or why there is none.
    pub result: Result<Value, ModelError>,
}

/// Why one reply is unusable.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ReplyProblem {
    /// The completion budget ran out. The retry widens the ceiling.
    Truncated(String),
    /// The reply is shaped wrong. The retry repeats the same ceiling.
    Malformed(String),
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
            let body = request_body(&self.config, request, max_tokens);
            let started = Instant::now();
            let sent = self.http.post_json(&self.config, &body).await;
            let latency_ms = u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);

            let (status, reply) = match sent {
                Ok(answer) => answer,
                Err(err) => {
                    attempts.push(failed_attempt(index, max_tokens, latency_ms, &self.config));
                    last = ModelError::Transport(err.0);
                    // A transport error is retryable: the endpoint said nothing,
                    // so nothing about the request is known to be wrong.
                    if self.wait(index).await {
                        continue;
                    }
                    break;
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
                let text = String::from_utf8_lossy(&reply).chars().take(400).collect();
                last = ModelError::Status { status, body: text };
                // Spec section 6.5: 429 and 5xx retry; every other status —
                // a 400 above all — returns at once.
                if status == 429 || status >= 500 {
                    if self.wait(index).await {
                        continue;
                    }
                    break;
                }
                return Call {
                    attempts,
                    result: Err(last),
                };
            }

            match parse_reply(&parsed, &request.tool) {
                Ok(arguments) => {
                    return Call {
                        attempts,
                        result: Ok(arguments),
                    };
                }
                Err(problem) => {
                    let widen = matches!(problem, ReplyProblem::Truncated(_));
                    last = ModelError::Reply(match &problem {
                        ReplyProblem::Truncated(why) | ReplyProblem::Malformed(why) => why.clone(),
                    });
                    if !self.wait(index).await {
                        break;
                    }
                    if widen {
                        max_tokens = max_tokens.saturating_mul(TRUNCATION_FACTOR);
                    }
                }
            }
        }

        Call {
            attempts,
            result: Err(last),
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

/// The record of an attempt that never reached a status.
fn failed_attempt(index: u32, max_tokens: u32, latency_ms: u32, cfg: &ModelConfig) -> Attempt {
    Attempt {
        index,
        max_tokens,
        status: 0,
        latency_ms,
        usage: Usage::default(),
        model_id: cfg.model.clone(),
        provider: None,
        request_id: None,
        cost_usd: None,
    }
}

/// Read the T6 fields of one reply body (spec section 7).
fn read_attempt(
    index: u32,
    max_tokens: u32,
    status: u16,
    latency_ms: u32,
    body: &Value,
    cfg: &ModelConfig,
) -> Attempt {
    let usage = body.get("usage");
    let count = |path: &[&str]| -> u32 {
        let mut node = match usage {
            Some(node) => node,
            None => return 0,
        };
        for key in path {
            node = match node.get(key) {
                Some(next) => next,
                None => return 0,
            };
        }
        node.as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(0)
    };
    let prompt = count(&["prompt_tokens"]);
    let input_cached = count(&["prompt_tokens_details", "cached_tokens"]);
    Attempt {
        index,
        max_tokens,
        status,
        latency_ms,
        usage: Usage {
            input_cached,
            input_uncached: prompt.saturating_sub(input_cached),
            output: count(&["completion_tokens"]),
            reasoning: count(&["completion_tokens_details", "reasoning_tokens"]),
        },
        model_id: cfg.model.clone(),
        provider: text(body.get("provider")),
        request_id: text(body.get("id")),
        cost_usd: usage
            .and_then(|node| node.get("cost"))
            .map(std::string::ToString::to_string),
    }
}

/// One JSON string, or `None` for anything else.
fn text(node: Option<&Value>) -> Option<String> {
    match node {
        Some(Value::String(value)) if !value.is_empty() => Some(value.clone()),
        _ => None,
    }
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

/// Read the arguments of the forced tool out of one reply.
///
/// The order is the whole point of spec section 6.5: the arguments are read and
/// validated FIRST, and `finish_reason` is consulted only after that read fails.
/// A complete reply that happens to carry `finish_reason: "length"` is therefore
/// accepted, and the three truncation shapes — no tool call, arguments cut off
/// mid-JSON, and arguments that parse but miss a required field — all reach the
/// widened retry.
fn parse_reply(body: &Value, tool: &ToolSpec) -> Result<Value, ReplyProblem> {
    let choice = body
        .get("choices")
        .and_then(|choices| choices.get(0))
        .unwrap_or(&Value::Null);
    let message = choice.get("message").unwrap_or(&Value::Null);

    // Shape 1: the forced tool call. 1.0 also tolerates a model that ignores the
    // forced tool and writes the object as message content, with or without a
    // markdown fence, so both readers run here.
    let arguments = message
        .get("tool_calls")
        .and_then(|calls| calls.get(0))
        .and_then(|call| call.get("function"))
        .and_then(|function| function.get("arguments"))
        .and_then(Value::as_str)
        .or_else(|| message.get("content").and_then(Value::as_str));

    let problem = match arguments {
        None => "the reply carries no tool call and no content".to_owned(),
        // Shape 2: the arguments are cut off mid-JSON.
        Some(raw) => match serde_json::from_str::<Value>(strip_fence(raw)) {
            Err(err) => format!("the arguments of {} do not parse: {err}", tool.name),
            // Shape 3: they parse and miss a required field.
            Ok(parsed) => match missing_field(&parsed) {
                Some(field) => format!("the arguments of {} name no {field}", tool.name),
                None => return Ok(parsed),
            },
        },
    };

    if truncated(choice) {
        return Err(ReplyProblem::Truncated(problem));
    }
    Err(ReplyProblem::Malformed(problem))
}

/// The first required field of the section 6.3 document that is absent or has
/// the wrong type, if any.
fn missing_field(arguments: &Value) -> Option<&'static str> {
    if !arguments.get("error_tags").is_some_and(Value::is_array) {
        return Some("error_tags");
    }
    if !arguments.get("prose").is_some_and(Value::is_string) {
        return Some("prose");
    }
    None
}

/// Did this choice run out of completion budget?
///
/// Both spellings count: OpenRouter reports the normalized `finish_reason` and
/// the provider's own `native_finish_reason`, and a provider that fills only the
/// second one truncated the reply just the same.
fn truncated(choice: &Value) -> bool {
    ["finish_reason", "native_finish_reason"]
        .iter()
        .any(|key| choice.get(*key).and_then(Value::as_str) == Some("length"))
}

/// Drop a markdown fence around a JSON object (1.0 `openai_engine.py:241-251`).
fn strip_fence(raw: &str) -> &str {
    let trimmed = raw.trim();
    let Some(rest) = trimmed.strip_prefix("```") else {
        return trimmed;
    };
    let rest = rest.strip_prefix("json").unwrap_or(rest);
    rest.trim_end().strip_suffix("```").unwrap_or(rest).trim()
}

#[cfg(test)]
mod tests {
    use super::{ROUTING_HOST, is_routing_host, strip_fence};

    /// The routing check reads the parsed host, not a substring of the URL.
    ///
    /// 1.0's `"openrouter.ai" in url` accepted the attacker host of the third
    /// case, which sends the API key to it (spec section 6.4).
    #[test]
    fn the_routing_host_is_the_parsed_host() {
        assert_eq!(ROUTING_HOST, "openrouter.ai");
        assert_eq!(
            is_routing_host("https://openrouter.ai/api/v1").ok(),
            Some(true)
        );
        assert_eq!(
            is_routing_host("https://OpenRouter.ai/api/v1").ok(),
            Some(true)
        );
        assert_eq!(
            is_routing_host("https://openrouter.ai.attacker.example/v1").ok(),
            Some(false)
        );
        assert_eq!(is_routing_host("http://10.8.0.3:8080/v1").ok(), Some(false));
        assert!(is_routing_host("not a url").is_err());
    }

    /// A fenced object loses its fence and a bare one is unchanged.
    #[test]
    fn a_markdown_fence_comes_off() {
        assert_eq!(strip_fence("```json\n{\"a\":1}\n```"), "{\"a\":1}");
        assert_eq!(strip_fence("```\n{\"a\":1}\n```"), "{\"a\":1}");
        assert_eq!(strip_fence("  {\"a\":1}  "), "{\"a\":1}");
    }
}
