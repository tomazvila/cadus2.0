//! The prompt, the forced tool, and the result document of one diagnosis call
//! (spec sections 5.3 and 6.3).

use cadus_model_client::ToolSpec;
use cadus_store::diagnosis::JobPayload;
use serde_json::{Value, json};

use super::{MODEL_ERROR_TAGS, RESULT_VERSION, TOOL_NAME};

/// Keep only the tags of [`MODEL_ERROR_TAGS`], in the order the model gave them.
///
/// The 1.0 lesson (`prompts.py:529-536`): a tag the prompt invites and the filter
/// lacks is dropped silently, and the diagnosis is lost with no error anywhere.
/// [`system_prompt`] therefore renders its vocabulary from this same list, so the
/// prompt and the filter are one statement.
#[must_use]
pub fn filter_tags(tags: &Value) -> Vec<String> {
    let Some(list) = tags.as_array() else {
        return Vec::new();
    };
    list.iter()
        .filter_map(Value::as_str)
        .filter(|tag| MODEL_ERROR_TAGS.contains(tag))
        .map(str::to_owned)
        .collect()
}

/// The system message (spec section 6.3).
///
/// It is 1.0's `GRADE_SYSTEM` minus what 2.0 already knows: the `correct`
/// paragraph, the timing paragraph, and the `work_quality` paragraphs. All three
/// are the VERDICT, and the verdict is decided locally before this call exists
/// (A3, D-M5-2). The mandatory unaided re-solve paragraph stays, and the
/// vocabulary is rendered from [`MODEL_ERROR_TAGS`].
#[must_use]
pub fn system_prompt() -> String {
    format!(
        "You are the grader for Cadus. The server already decided that the answer is WRONG. \
Name the misconception and write the diagnosis with the {TOOL_NAME} tool. Be honest and \
structural.\n\n\
SHOWN WORK IS OPTIONAL, and its absence is NOT a defect. The interface labels the working \
field 'optional'. Judge method only from work that IS shown. When no work is shown, judge on \
the answer alone and do NOT tag 'incomplete' merely because the field is empty.\n\n\
'error_tags' come ONLY from this controlled vocabulary: {}. Use [] when you cannot name the \
error. Do not invent tags; a tag outside this list is dropped.\n\n\
'prose' is 1-3 sentences, brisk and encouraging. Praise the specific STRATEGY or process the \
learner used, never raw ability. Put any math in $...$ LaTeX.\n\n\
Mandatory unaided re-solve (pp. 427, 431): a miss is NOT the end of the task. In 'prose', have \
the learner study the worked solution and then re-solve the ORIGINAL problem THEMSELVES, \
unaided and from memory, before moving on.\n\n\
'confidence' is 'high' when you can name the misconception and 'low' when you are guessing. A \
low-confidence diagnosis is stored and its tags are not shown.",
        MODEL_ERROR_TAGS.join(", ")
    )
}

/// The user message (spec section 6.3).
#[must_use]
pub fn user_message(payload: &JobPayload) -> String {
    let work = payload.work.as_deref().unwrap_or("(none provided)");
    format!(
        "Problem: {}\n\
Correct final answer (reference): {}\n\
Answer kind: {}\n\
Learner's answer: '{}'\n\
Learner's shown work: {work}\n\
The answer is WRONG; the server decided that. Do not restate the verdict.\n\
Name the misconception and write the diagnosis via the {TOOL_NAME} tool.",
        payload.problem, payload.expected, payload.answer_kind, payload.given_answer
    )
}

/// The forced tool of spec section 6.3, `additionalProperties: false`.
#[must_use]
pub fn tool_spec() -> ToolSpec {
    ToolSpec {
        name: TOOL_NAME.to_owned(),
        description: "Report the misconception behind one wrong answer.".to_owned(),
        parameters: json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["error_tags", "prose"],
            "properties": {
                "error_tags": {
                    "type": "array",
                    "items": {"type": "string", "enum": MODEL_ERROR_TAGS},
                },
                "prose": {"type": "string"},
                "confidence": {"type": "string", "enum": ["high", "low"]},
            },
        }),
    }
}

/// Turn the model's arguments into the document `diagnosis_jobs.result` holds.
///
/// A low-confidence diagnosis is STORED and its tags are NOT shown (spec section
/// 6.3): `error_tags` goes out empty and the model's own list stays under
/// `withheld_error_tags`, where an operator reads it and no learner does.
#[must_use]
pub fn result_document(arguments: &Value, model_id: &str) -> Value {
    let tags = filter_tags(arguments.get("error_tags").unwrap_or(&Value::Null));
    let low = arguments.get("confidence").and_then(Value::as_str) == Some("low");
    let prose = arguments
        .get("prose")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut document = json!({
        "v": RESULT_VERSION,
        "error_tags": if low { Vec::new() } else { tags.clone() },
        "prose": prose,
        "model_id": model_id,
        "confidence": if low { "low" } else { "high" },
    });
    if low && let Some(map) = document.as_object_mut() {
        map.insert("withheld_error_tags".to_owned(), json!(tags));
    }
    document
}

#[cfg(test)]
mod tests {
    use super::{MODEL_ERROR_TAGS, filter_tags, result_document, system_prompt};
    use serde_json::json;

    /// The vocabulary is the 11 tags of spec section 5.3, in 1.0 order.
    ///
    /// `blank-answer` is not one of them: the server stamps it, so the prompt
    /// never invites the model to claim that an answer was blank.
    #[test]
    fn the_model_vocabulary_is_the_eleven_tags() {
        assert_eq!(
            MODEL_ERROR_TAGS,
            [
                "sign-error",
                "arithmetic-slip",
                "algebra-slip",
                "wrong-method",
                "formula-recall",
                "misread-problem",
                "incomplete",
                "notation",
                "units",
                "timing-unreliable",
                "blowoff",
            ]
        );
    }

    /// A tag outside the vocabulary is dropped and the rest keep their order.
    #[test]
    fn a_tag_outside_the_vocabulary_is_dropped() {
        let tags = json!(["sign-error", "carelessness", "blank-answer", "units", 7]);
        assert_eq!(filter_tags(&tags), vec!["sign-error", "units"]);
        assert!(filter_tags(&json!("sign-error")).is_empty());
        assert!(filter_tags(&json!(null)).is_empty());
    }

    /// The prompt renders its vocabulary from the list the filter uses, so a tag
    /// the prompt invites can never be one the filter lacks.
    #[test]
    fn the_prompt_names_every_tag_the_filter_keeps() {
        let prompt = system_prompt();
        for tag in MODEL_ERROR_TAGS {
            assert!(prompt.contains(tag), "the prompt does not name {tag}");
        }
        assert!(!prompt.contains("blank-answer"));
    }

    /// A low-confidence diagnosis is stored and its tags are not shown.
    #[test]
    fn a_low_confidence_diagnosis_shows_no_tags() {
        let document = result_document(
            &json!({"error_tags": ["sign-error"], "prose": "Maybe the sign.",
                    "confidence": "low"}),
            "deepseek/deepseek-v4-pro",
        );
        assert_eq!(document["error_tags"], json!([]));
        assert_eq!(document["withheld_error_tags"], json!(["sign-error"]));
        assert_eq!(document["prose"], json!("Maybe the sign."));
        assert_eq!(document["model_id"], json!("deepseek/deepseek-v4-pro"));
    }

    /// A high-confidence diagnosis shows the filtered tags.
    #[test]
    fn a_high_confidence_diagnosis_shows_the_filtered_tags() {
        let document = result_document(
            &json!({"error_tags": ["sign-error", "made-up"], "prose": "Watch the sign."}),
            "qwen3.6",
        );
        assert_eq!(document["v"], json!(1));
        assert_eq!(document["error_tags"], json!(["sign-error"]));
        assert_eq!(document["confidence"], json!("high"));
        assert_eq!(document.get("withheld_error_tags"), None);
    }
}
