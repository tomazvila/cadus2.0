//! The prompt, the forced tool, and the result document of one diagnosis call
//! (spec sections 5.3 and 6.3).

use cadus_model_client::ToolSpec;
use cadus_store::diagnosis::JobPayload;
use serde_json::{Value, json};

use super::candidates::{Candidate, candidates};
use super::circular::is_circular;
use super::latex::repair_latex;
use super::{MODEL_ERROR_TAGS, RESULT_VERSION, TOOL_NAME};

/// The sentence a diagnosis falls back to when no misconception is named, or the
/// explanation argues in a circle.
pub const NEUTRAL_PROSE: &str = "The mistake could not be identified. Study the worked solution, \
then solve the problem again yourself.";

/// The `misconception` value that means no candidate matched.
pub const NO_MISCONCEPTION: &str = "none";

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
        "You are the grader for Cadus. The server's checker marked the answer WRONG. \
Name the misconception and write the diagnosis with the {TOOL_NAME} tool. Be honest and \
structural.\n\n\
THE CHECKER CAN BE WRONG. It compares the answer with a stored key and can reject a correct \
answer written in another valid form, order, or wording. When the learner's answer is \
mathematically correct, say so plainly in 'prose', give no error_tags, and do NOT invent a \
rule (such as a required order or format) to justify the mark.\n\n\
SHOWN WORK IS OPTIONAL, and its absence is NOT a defect. The interface labels the working \
field 'optional'. Judge method only from work that IS shown. When no work is shown, judge on \
the answer alone and do NOT tag 'incomplete' merely because the field is empty.\n\n\
The user message lists CANDIDATE MISCONCEPTIONS that the server computed for this exact \
answer. If one candidate explains the learner's answer, put its name in 'misconception' and \
state that mistake in 'prose' in your own words. If none matches, put '{NO_MISCONCEPTION}' in \
'misconception' and begin 'prose' with 'The mistake could not be identified.' Never invent a \
misconception that is not in the list.\n\n\
Never call a method wrong and then name the same operation on the same numbers as the correct \
method. A 'correct method' must differ from the method you name as wrong, or you must leave it \
out.\n\n\
'error_tags' come ONLY from this controlled vocabulary: {}. Give tags only when you named a \
misconception. Use [] when 'misconception' is '{NO_MISCONCEPTION}'. Do not invent tags; a tag \
outside this list is dropped.\n\n\
Write every LaTeX backslash twice inside the JSON strings (\\\\frac, \\\\times, \\\\theta), so \
the text keeps its commands after the JSON is read.\n\n\
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
    // A property item asks for ANY object with a property: the stored answer
    // is one example, and the checker tested the property itself, exactly.
    let reference = payload.answer_property.as_deref().map_or_else(
        || format!("Correct final answer (reference): {}", payload.expected),
        |property| {
            format!(
                "One valid example (NOT the only answer): {}\n\
Required property: {property}. Any answer with this property is correct; the server tested \
the learner's answer against the property with exact arithmetic and found it does not have it. \
Do not compare the learner's answer with the example.",
                payload.expected
            )
        },
    );
    format!(
        "Problem: {}\n\
{reference}\n\
Answer kind: {}\n\
Learner's answer: '{}'\n\
Learner's shown work: {work}\n\
{}\n\
The checker marked the answer wrong. If it is in fact correct, say so.\n\
Name the misconception and write the diagnosis via the {TOOL_NAME} tool.",
        payload.problem,
        payload.answer_kind,
        payload.given_answer,
        candidate_lines(&candidates(payload))
    )
}

/// The candidate list of the user message.
fn candidate_lines(list: &[Candidate]) -> String {
    if list.is_empty() {
        return format!(
            "Candidate misconceptions: none. Put '{NO_MISCONCEPTION}' in 'misconception'."
        );
    }
    let lines: Vec<String> = list
        .iter()
        .map(|c| format!("- {}: {}", c.name, c.sentence))
        .collect();
    format!("Candidate misconceptions:\n{}", lines.join("\n"))
}

/// The names the model may put in `misconception` for this payload.
#[must_use]
pub fn misconception_names(payload: &JobPayload) -> Vec<&'static str> {
    candidates(payload).iter().map(|c| c.name).collect()
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
                "misconception": {"type": "string"},
                "confidence": {"type": "string", "enum": ["high", "low"]},
            },
        }),
    }
}

/// Turn the model's arguments into the document `diagnosis_jobs.result` holds.
///
/// `names` are the candidate misconceptions the server computed. Three rules apply:
///
/// - Error tags stay only when `misconception` names one candidate. With no named
///   misconception the tags go to `withheld_error_tags`, the prose becomes
///   [`NEUTRAL_PROSE`], and no learner reads a made-up cause.
/// - An explanation that calls a method wrong and names the same method as correct is
///   circular, and it gets the same fallback.
/// - A low-confidence diagnosis is STORED and its tags are NOT shown (spec section
///   6.3): `error_tags` goes out empty and the model's own list stays under
///   `withheld_error_tags`, where an operator reads it and no learner does.
///
/// LaTeX commands that a JSON decode damaged are restored in the prose first.
#[must_use]
pub fn result_document(arguments: &Value, model_id: &str, names: &[&str]) -> Value {
    let tags = filter_tags(arguments.get("error_tags").unwrap_or(&Value::Null));
    let mut low = arguments.get("confidence").and_then(Value::as_str) == Some("low");
    let prose = repair_latex(
        arguments
            .get("prose")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    )
    .text;
    let claimed = arguments
        .get("misconception")
        .and_then(Value::as_str)
        .map(str::trim)
        .and_then(|claimed| names.iter().find(|name| name.eq_ignore_ascii_case(claimed)));
    let named = claimed.is_some() && !is_circular(&prose);
    let prose = if named {
        prose
    } else {
        NEUTRAL_PROSE.to_owned()
    };
    low |= !named;
    let mut document = json!({
        "v": RESULT_VERSION,
        "error_tags": if low { Vec::new() } else { tags.clone() },
        "prose": prose,
        "model_id": model_id,
        "confidence": if low { "low" } else { "high" },
        "misconception": if named { claimed.copied().unwrap_or(NO_MISCONCEPTION) } else { NO_MISCONCEPTION },
    });
    if low && let Some(map) = document.as_object_mut() {
        map.insert("withheld_error_tags".to_owned(), json!(tags));
    }
    document
}

#[cfg(test)]
mod tests {
    use super::{
        MODEL_ERROR_TAGS, NEUTRAL_PROSE, filter_tags, result_document, system_prompt, user_message,
    };
    use cadus_store::diagnosis::JobPayload;
    use serde_json::json;

    fn payload(answer_property: Option<&str>) -> JobPayload {
        JobPayload {
            v: 1,
            session: None,
            task_id: "t".to_owned(),
            topic: "factors-and-multiples".to_owned(),
            kp: None,
            problem: "Give a number with exactly three factors.".to_owned(),
            expected: "9".to_owned(),
            answer_kind: "expression".to_owned(),
            given_answer: "6".to_owned(),
            work: None,
            answer_property: answer_property.map(str::to_owned),
        }
    }

    /// A property item's stored answer is one example; the prompt says so and
    /// states the property, so the model never compares 6 with 9.
    #[test]
    fn a_property_item_names_the_example_and_the_property() {
        let property = user_message(&payload(Some(
            "a positive integer with exactly 3 positive divisors",
        )));
        assert!(property.contains("One valid example (NOT the only answer): 9"));
        assert!(property.contains("Required property: a positive integer with exactly 3"));
        assert!(!property.contains("Correct final answer"));
        let plain = user_message(&payload(None));
        assert!(plain.contains("Correct final answer (reference): 9"));
        assert!(!plain.contains("Required property"));
    }

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
                    "misconception": "sign-slip", "confidence": "low"}),
            "deepseek/deepseek-v4-pro",
            &["sign-slip"],
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
            &json!({"error_tags": ["sign-error", "made-up"], "prose": "Watch the sign.",
                    "misconception": "sign-slip"}),
            "qwen3.6",
            &["sign-slip"],
        );
        assert_eq!(document["v"], json!(1));
        assert_eq!(document["error_tags"], json!(["sign-error"]));
        assert_eq!(document["confidence"], json!("high"));
        assert_eq!(document.get("withheld_error_tags"), None);
    }

    /// Tags need a named misconception. With none named the tags are withheld and the
    /// prose is the neutral sentence, so no learner reads a made-up cause.
    #[test]
    fn tags_need_a_named_misconception() {
        let unnamed = result_document(
            &json!({"error_tags": ["wrong-method"], "prose": "You used the wrong method.",
                    "misconception": "none"}),
            "m",
            &["rounded-up"],
        );
        assert_eq!(unnamed["error_tags"], json!([]));
        assert_eq!(unnamed["withheld_error_tags"], json!(["wrong-method"]));
        assert_eq!(unnamed["prose"], json!(NEUTRAL_PROSE));
        assert!(NEUTRAL_PROSE.starts_with("The mistake could not be identified."));
        let missing = result_document(
            &json!({"error_tags": ["wrong-method"], "prose": "x"}),
            "m",
            &["rounded-up"],
        );
        assert_eq!(missing["error_tags"], json!([]));
        let named = result_document(
            &json!({"error_tags": ["arithmetic-slip"], "prose": "You rounded 1.2 up to 2.",
                    "misconception": "Rounded-Up"}),
            "m",
            &["rounded-up"],
        );
        assert_eq!(named["error_tags"], json!(["arithmetic-slip"]));
        assert_eq!(named["misconception"], json!("rounded-up"));
    }

    /// The fault of owner report 11: a circular explanation never reaches the learner.
    #[test]
    fn a_circular_explanation_falls_back_to_the_neutral_sentence() {
        let document = result_document(
            &json!({"error_tags": ["wrong-method"],
                    "prose": "You divided 3/4 by 5/8 incorrectly. The correct method is to divide 3/4 by 5/8.",
                    "misconception": "divided-wrong-way"}),
            "m",
            &["divided-wrong-way"],
        );
        assert_eq!(document["prose"], json!(NEUTRAL_PROSE));
        assert_eq!(document["error_tags"], json!([]));
    }

    /// The user message lists the candidates the server computed, or says there are none.
    #[test]
    fn the_user_message_lists_the_candidates() {
        let mut item = payload(None);
        item.problem = "Divide $\\frac{3}{4}$ by $\\frac{5}{8}$.".to_owned();
        item.expected = "6/5".to_owned();
        item.given_answer = "2".to_owned();
        let message = user_message(&item);
        assert!(message.contains("Candidate misconceptions:\n- rounded-up:"));
        item.given_answer = "17".to_owned();
        assert!(user_message(&item).contains("Candidate misconceptions: none."));
        assert!(system_prompt().contains("\\\\frac"));
        assert!(system_prompt().contains("The mistake could not be identified."));
    }

    /// Damaged LaTeX in the prose is restored before the document is stored.
    #[test]
    fn damaged_latex_in_the_prose_is_restored() {
        let document = result_document(
            &json!({"error_tags": [], "prose": "$\u{c}rac{6}{5}$ is not $2 \times 1$.",
                    "misconception": "rounded-up"}),
            "m",
            &["rounded-up"],
        );
        assert_eq!(
            document["prose"],
            json!("$\\frac{6}{5}$ is not $2 \\times 1$.")
        );
    }
}
