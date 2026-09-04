//! The reader of one reply: the forced tool call, the two 1.0 fallbacks, and
//! the three truncation shapes (spec section 6.5).

use serde_json::Value;

use crate::ToolSpec;

/// Why one reply is unusable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReplyProblem {
    /// The completion budget ran out. The retry widens the ceiling.
    Truncated(String),
    /// The reply is shaped wrong. The retry repeats the same ceiling.
    Malformed(String),
}

impl ReplyProblem {
    /// The words of the problem, whichever kind it is.
    pub(crate) fn into_message(self) -> String {
        match self {
            Self::Truncated(why) | Self::Malformed(why) => why,
        }
    }
}

/// Read the arguments of the forced tool out of one reply.
///
/// The order is the whole point of spec section 6.5: the arguments are read and
/// validated FIRST, and `finish_reason` is consulted only after that read fails.
/// A complete reply that happens to carry `finish_reason: "length"` is therefore
/// accepted, and the three truncation shapes — no tool call, arguments cut off
/// mid-JSON, and arguments that parse but miss a required field — all reach the
/// widened retry.
pub(crate) fn parse_reply(body: &Value, tool: &ToolSpec) -> Result<Value, ReplyProblem> {
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
            Ok(parsed) => match missing_field(&parsed, &tool.parameters) {
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

/// The first required field of the TOOL'S OWN schema that is absent or that
/// carries the wrong type, if any.
///
/// The check reads `parameters.required` and `parameters.properties`, so one
/// client serves every tool. M5 hard-coded the two fields of the diagnosis
/// document here, and that spelling refused every reply of the M6 authoring
/// tools with `name no error_tags`: the client is the ONE crate that reaches a
/// model (L6), so its validation must come from the schema the request carried.
///
/// A schema that names no required field validates nothing here. The reply still
/// has to parse as JSON, and the caller still decides whether the document is
/// usable — for authoring, that decision is the gate (A2).
fn missing_field(arguments: &Value, schema: &Value) -> Option<String> {
    let required = schema.get("required").and_then(Value::as_array)?;
    for name in required.iter().filter_map(Value::as_str) {
        let Some(value) = arguments.get(name) else {
            return Some(name.to_owned());
        };
        let declared = schema
            .get("properties")
            .and_then(|properties| properties.get(name))
            .and_then(|property| property.get("type"))
            .and_then(Value::as_str);
        let holds = match declared {
            Some("array") => value.is_array(),
            Some("string") => value.is_string(),
            Some("object") => value.is_object(),
            Some("boolean") => value.is_boolean(),
            Some("integer") => value.is_i64() || value.is_u64(),
            Some("number") => value.is_number(),
            // A property with no stated type, or a union of types, is present or
            // it is not. A null is absent: JSON writes an unset field that way.
            _ => !value.is_null(),
        };
        if !holds {
            return Some(name.to_owned());
        }
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
    use serde_json::json;

    use super::{ReplyProblem, missing_field, strip_fence};

    /// A fenced object loses its fence and a bare one is unchanged.
    #[test]
    fn a_markdown_fence_comes_off() {
        assert_eq!(strip_fence("```json\n{\"a\":1}\n```"), "{\"a\":1}");
        assert_eq!(strip_fence("```\n{\"a\":1}\n```"), "{\"a\":1}");
        assert_eq!(strip_fence("  {\"a\":1}  "), "{\"a\":1}");
    }

    /// Every declared type of a required field is checked, a field with no
    /// declared type is present when it is not null, and a schema with no
    /// required list validates nothing.
    #[test]
    fn a_required_field_holds_its_declared_type() {
        let schema = json!({
            "required": ["a", "s", "o", "b", "i", "n", "u"],
            "properties": {
                "a": {"type": "array"}, "s": {"type": "string"}, "o": {"type": "object"},
                "b": {"type": "boolean"}, "i": {"type": "integer"}, "n": {"type": "number"},
                "u": {}
            }
        });
        let good = json!({"a": [], "s": "x", "o": {}, "b": true, "i": 1, "n": 1.5, "u": 0});
        assert_eq!(missing_field(&good, &schema), None);
        for (name, value) in [
            ("a", json!("x")),
            ("s", json!(1)),
            ("o", json!([])),
            ("b", json!(0)),
            ("i", json!(1.5)),
            ("n", json!("1")),
            ("u", json!(null)),
        ] {
            let mut bad = good.clone();
            bad[name] = value;
            assert_eq!(missing_field(&bad, &schema).as_deref(), Some(name));
        }
        let mut absent = good.clone();
        absent.as_object_mut().unwrap().remove("s");
        assert_eq!(missing_field(&absent, &schema).as_deref(), Some("s"));
        assert_eq!(missing_field(&json!({}), &json!({})), None);
    }

    /// The message of a problem is its words, for both kinds.
    #[test]
    fn a_problem_gives_its_words_back() {
        assert_eq!(
            ReplyProblem::Truncated("cut".to_owned()).into_message(),
            "cut"
        );
        assert_eq!(
            ReplyProblem::Malformed("odd".to_owned()).into_message(),
            "odd"
        );
    }
}
