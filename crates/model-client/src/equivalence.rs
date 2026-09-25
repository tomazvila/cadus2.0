//! The one equivalence question of Amendment K (steer note 114): is the
//! learner's answer mathematically equivalent to the key?
//!
//! One fresh, bounded call to the owner's local endpoint with the strict
//! prompt, temperature 0, one short reply. No tools, no history, no retries:
//! a call that misses its budget is the caller's timeout path, not a retry.
//!
//! Model output stays evidence: the caller owns the verdict policy, and a
//! reply this parser cannot read is an error, never a verdict.

use std::time::Duration;

use serde_json::{Value, json};

use crate::{HttpClient, ModelError};

/// The system prompt of every equivalence call.
///
/// Strict on purpose: the model judges MATH content, never surface form, and
/// it answers in exactly the two-word shape the parser reads.
pub const SYSTEM_PROMPT: &str = "You decide whether a learner's answer to a math practice \
problem is mathematically equivalent to the stored key. Judge meaning, not wording: the same \
value in another spelling, the same choice of option in other words, the same list in another \
order (unless the order is the point), or extra words around the value are still EQUIVALENT. \
A different value, a different option, or a missing part is NOT. Ignore politeness, spelling \
and punctuation. Reply with exactly one word on the first line, EQUIVALENT or NOT, and one \
line why on the second line.";

/// The output budget of one equivalence call, in tokens.
///
/// The reply is one word and one line; the budget exists so a reasoning loop
/// cannot run away.
pub const MAX_OUTPUT_TOKENS: u32 = 220;

/// One equivalence question, in the fields the note-114 design names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question<'a> {
    /// The problem text the learner read.
    pub problem: &'a str,
    /// The stored key.
    pub key: &'a str,
    /// The answer contract, in the checker's spelling, or `None`.
    pub contract: Option<&'a str>,
    /// The learner's raw text.
    pub learner: &'a str,
}

impl Question<'_> {
    /// The user message: the four fields, then the question.
    #[must_use]
    pub fn message(&self) -> String {
        let contract = self.contract.unwrap_or("none recorded");
        format!(
            "Problem: {problem}\nStored key: {key}\nAnswer contract: {contract}\nLearner \
             answer: {learner}\n\nIs the learner answer mathematically equivalent to the \
             stored key? Reply with exactly one word, EQUIVALENT or NOT, then one line why.",
            problem = self.problem,
            key = self.key,
            contract = contract,
            learner = self.learner,
        )
    }
}

/// What the model answered, with its bill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    /// Whether the answer is equivalent.
    pub equivalent: bool,
    /// The model's one-line why, trimmed to one line.
    pub reason: String,
}

/// One finished call: the reply plus the record the ledger needs (T6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    /// The parsed verdict.
    pub reply: Reply,
    /// The usage block of the reply body.
    pub usage: crate::Usage,
    /// The wall clock around the HTTP call, in milliseconds.
    pub latency_ms: u32,
}

/// The client of one local endpoint. Build it once, call it per question.
#[derive(Debug)]
pub struct EquivalenceClient {
    http: HttpClient,
    url: String,
    model: String,
}

impl EquivalenceClient {
    /// Build a client for `base_url` (an origin, e.g. `http://10.8.0.1:8081/v1`)
    /// and the model id `model`.
    ///
    /// # Errors
    ///
    /// Returns [`ModelError::Config`] when the base URL does not parse or the
    /// TLS trust store does not build.
    pub fn new(base_url: &str, model: &str) -> Result<Self, ModelError> {
        if model.trim().is_empty() {
            return Err(ModelError::Config(
                "the equivalence client requires a model id".to_owned(),
            ));
        }
        let http = HttpClient::new(base_url).map_err(|err| ModelError::Config(err.0))?;
        let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
        Ok(Self { http, url, model: model.to_owned() })
    }

    /// The model id this client asks.
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Ask the one question, once, with the strict prompt and temperature 0.
    ///
    /// `budget` bounds the whole call: a reply that comes later is an error,
    /// and the caller takes its timeout path.
    ///
    /// # Errors
    ///
    /// Returns [`ModelError::Transport`] when the endpoint is unreachable or
    /// past the budget, [`ModelError::Status`] for a refusal, and
    /// [`ModelError::Reply`] for a body this parser cannot read.
    pub async fn ask(&self, question: &Question<'_>, budget: Duration) -> Result<Call, ModelError> {
        let body = json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": SYSTEM_PROMPT},
                {"role": "user", "content": question.message()},
            ],
            "temperature": 0,
            "max_tokens": MAX_OUTPUT_TOKENS,
            "stream": false,
            "chat_template_kwargs": {"enable_thinking": false},
        });
        let config = crate::ModelConfig {
            base_url: self.url.clone(),
            api_key: String::new(),
            model: self.model.clone(),
            output_tokens: MAX_OUTPUT_TOKENS,
            reasoning_max_tokens: 0,
            provider_order: Vec::new(),
            timeout: budget,
        };
        let started = std::time::Instant::now();
        let wait = |error: crate::TransportError| ModelError::Transport(error.0);
        // `post_json` runs the whole call inside `config.timeout`, so the
        // budget bounds the attempt in the transport itself.
        let (status, bytes) = self.http.post_json(&config, &body).await.map_err(wait)?;
        let latency_ms = u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);
        if !(200..300).contains(&status) {
            return Err(ModelError::Status {
                status,
                body: "the equivalence endpoint refused the request".to_owned(),
            });
        }
        let parsed: Value = serde_json::from_slice(&bytes)
            .map_err(|_| ModelError::Reply("the equivalence reply is not JSON".to_owned()))?;
        let content = parsed["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| ModelError::Reply("the equivalence reply holds no content".to_owned()))?;
        let reply = parse_reply(content)?;
        Ok(Call {
            usage: crate::Usage {
                input_cached: parsed["usage"]["prompt_tokens_details"]["cached_tokens"]
                    .as_u64()
                    .unwrap_or(0) as u32,
                input_uncached: parsed["usage"]["prompt_tokens"].as_u64().unwrap_or(0)
                    .saturating_sub(
                        parsed["usage"]["prompt_tokens_details"]["cached_tokens"]
                            .as_u64()
                            .unwrap_or(0),
                    ) as u32,
                output: parsed["usage"]["completion_tokens"].as_u64().unwrap_or(0) as u32,
                reasoning: parsed["usage"]["completion_tokens_details"]["reasoning_tokens"]
                    .as_u64()
                    .unwrap_or(0) as u32,
            },
            latency_ms,
            reply,
        })
    }
}

/// Read `EQUIVALENT` or `NOT` and the one-line why out of the reply text.
///
/// The strict shape is two lines. A model that answers in one line, in
/// lowercase, with leading punctuation, or in a short JSON object is still
/// read; a reply that names neither word is an error.
///
/// # Errors
///
/// Returns [`ModelError::Reply`] when no verdict word is present.
pub fn parse_reply(content: &str) -> Result<Reply, ModelError> {
    let text = content.trim();
    // A short JSON object is a valid reply shape.
    if text.starts_with('{')
        && let Ok(value) = serde_json::from_str::<Value>(text)
    {
        {
            let word = value.get("equivalent").map_or("", |verdict| {
                if verdict.as_bool().unwrap_or(false) {
                    "EQUIVALENT"
                } else {
                    "NOT"
                }
            });
            let reason = value
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !word.is_empty() {
                return Ok(Reply {
                    equivalent: word == "EQUIVALENT",
                    reason: one_line(reason),
                });
            }
        }
    }
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    let first = lines.next().unwrap_or_default();
    let (word, rest) = split_word(first);
    match word.to_ascii_uppercase().as_str() {
        "EQUIVALENT" => Ok(Reply {
            equivalent: true,
            reason: one_line(rest.or(lines.next()).unwrap_or_default()),
        }),
        "NOT" => Ok(Reply {
            equivalent: false,
            reason: one_line(rest.or(lines.next()).unwrap_or_default()),
        }),
        _ => Err(ModelError::Reply(format!(
            "the equivalence reply names no verdict: {}",
            one_line(first)
        ))),
    }
}

/// Split `EQUIVALENT: the parts match` into the word and the rest.
fn split_word(line: &str) -> (&str, Option<&str>) {
    let trimmed = line.trim_start_matches(['*', '-', ' ', '"', '\'', ':', '_']);
    match trimmed.split_once([':', ' ', '\t']) {
        Some((word, rest)) => (word.trim_end_matches(['*', ':']), Some(rest.trim())),
        None => (trimmed, None),
    }
}

/// Collapse the reason to one line, trimmed to a readable length.
fn one_line(text: &str) -> String {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty());
    let mut reason = line.unwrap_or_default().trim_matches('.').to_owned();
    if reason.chars().count() > 300 {
        reason = reason.chars().take(300).collect();
    }
    reason
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::{Question, parse_reply};

    /// The user message names the four fields and the question.
    #[test]
    fn the_message_carries_the_four_fields() {
        let question = Question {
            problem: "Compute 8 + 5.5.",
            key: "13.5",
            contract: Some("exact"),
            learner: "the answer is 13.5.",
        };
        let message = question.message();
        assert!(message.contains("Problem: Compute 8 + 5.5."));
        assert!(message.contains("Stored key: 13.5"));
        assert!(message.contains("Answer contract: exact"));
        assert!(message.contains("Learner answer: the answer is 13.5."));
        assert!(message.contains("EQUIVALENT or NOT"));
    }

    /// The strict two-line shape parses.
    #[test]
    fn the_strict_shape_parses() {
        let reply =
            parse_reply("EQUIVALENT\nThe learner stated the same value in other words.")
                .unwrap();
        assert!(reply.equivalent);
        assert_eq!(reply.reason, "The learner stated the same value in other words");

        let reply = parse_reply("NOT\nThe learner's D is 6, not 5.").unwrap();
        assert!(!reply.equivalent);
        assert_eq!(reply.reason, "The learner's D is 6, not 5");
    }

    /// One line, lowercase, and punctuation before the word still parse.
    #[test]
    fn a_loose_shape_still_parses() {
        let reply = parse_reply("equivalent — the same value").unwrap();
        assert!(reply.equivalent);
        assert_eq!(reply.reason, "— the same value");

        let reply = parse_reply("**NOT**: the verdict part disagrees").unwrap();
        assert!(!reply.equivalent);
    }

    /// A JSON reply parses, and a reply with no verdict word is an error.
    #[test]
    fn a_json_reply_parses_and_a_wordless_one_is_an_error() {
        let reply = parse_reply(r#"{"equivalent": true, "reason": "same value"}"#).unwrap();
        assert!(reply.equivalent);

        assert!(parse_reply("I am not sure what you mean.").is_err());
        assert!(parse_reply("").is_err());
    }
}
