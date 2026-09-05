//! The T6 record of one HTTP attempt: its tokens, its latency, and how it
//! ended.

use serde_json::Value;

use crate::ModelConfig;

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

/// The record of an attempt that never reached a status.
pub(crate) fn failed_attempt(
    index: u32,
    max_tokens: u32,
    latency_ms: u32,
    cfg: &ModelConfig,
) -> Attempt {
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
pub(crate) fn read_attempt(
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

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::text;

    /// A non-empty JSON string is read; every other node is `None`.
    #[test]
    fn a_text_field_is_a_non_empty_string() {
        assert_eq!(text(Some(&json!("abc"))), Some("abc".to_owned()));
        assert_eq!(text(Some(&json!(""))), None);
        assert_eq!(text(Some(&json!(7))), None);
        assert_eq!(text(Some(&Value::Null)), None);
        assert_eq!(text(None), None);
    }
}
