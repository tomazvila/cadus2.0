//! The gates of the four kinds (spec section 2.2, step 2).
//!
//! Every gate returns the same [`Rejection`], so the retry block of the loop
//! carries a literal message whatever the kind is. `crate::authoring::repair`
//! runs before every gate, on every kind.

use cadus_core::instruction::{InstructionSpec, ServedInstance, gate_hint_ladder, gate_teach};
use cadus_core::pool::kp_key;
use cadus_core::template::{
    GateSpec, Rejection, TEMPLATE_VERSION, gate_body, gate_diagnosis_body, keep_known_tags,
    to_body, to_diagnosis_body, with_space_size,
};
use serde_json::Value;

use crate::authoring::prompt::{AuthoringSpec, Kind};
use crate::authoring::repair;
use crate::diagnosis::MODEL_ERROR_TAGS;

/// The refusal a forced tool call with no arguments object earns.
pub const NO_ARGUMENTS: &str = "the tool call carried no JSON object of arguments — emit every required field of the tool in \
one object";

/// The `Rejection::code` of a body the tool arguments cannot give.
const CODE_ARGUMENTS: &str = "tool-arguments";

/// Turn the arguments of one forced tool call into a template body.
///
/// The tool asks for the authored subset only. `v`, `topic_id` and `answer_kind`
/// are server-side: the knowledge point owns them, so a model that states them
/// wrong rewrites the row's own identity. This function drops all three, and
/// `space_size`, and writes the server's values instead
/// (`problem_templates.py:309`).
///
/// # Errors
///
/// Returns the [`Rejection`] the retry block carries when the tool call answered
/// no JSON object.
pub fn assemble(spec: &AuthoringSpec, arguments: &Value) -> Result<String, Rejection> {
    Ok(assemble_value(spec, arguments)?.to_string())
}

/// [`assemble`], before the body is written out.
fn assemble_value(spec: &AuthoringSpec, arguments: &Value) -> Result<Value, Rejection> {
    let Some(fields) = arguments.as_object() else {
        return Err(no_arguments());
    };
    let mut body = fields.clone();
    for server_side in [
        "v",
        "topic_id",
        "answer_kind",
        "space_size",
        "answer_contract",
    ] {
        body.remove(server_side);
    }
    body.insert("v".to_owned(), Value::from(TEMPLATE_VERSION));
    body.insert("topic_id".to_owned(), Value::from(spec.topic_id.clone()));
    body.insert(
        "answer_kind".to_owned(),
        Value::from(spec.answer_kind.as_str()),
    );
    Ok(Value::Object(body))
}

/// The refusal of a tool call that answered no JSON object.
fn no_arguments() -> Rejection {
    Rejection {
        code: CODE_ARGUMENTS,
        message: NO_ARGUMENTS.to_owned(),
    }
}

/// The refusal a document that the gate accepted but serde cannot write earns.
fn unwritable(err: serde_json::Error) -> Rejection {
    Rejection {
        code: CODE_ARGUMENTS,
        message: format!("the verified document does not write as JSON: {err}"),
    }
}

/// Assemble one body and drop the tags outside the vocabulary (spec section
/// 5.3, row R7).
///
/// The drop runs BEFORE the gate. A distractor note is a rendered field, so a
/// drop after the gate stores a document the gate refuses
/// (`cadus_core::template::keep_known_tags` gives the reason in full).
///
/// The template path calls this, because the template gate holds no vocabulary.
/// The diagnosis path does not: `cadus_core::template::gate_diagnosis_body`
/// takes the vocabulary and runs the same drop in the same place.
fn assemble_kept(kind: Kind, spec: &AuthoringSpec, arguments: &Value) -> Result<String, Rejection> {
    let mut body = assemble_value(spec, arguments)?;
    if kind == Kind::Template
        && let Some(contract) = spec.template_contract()
    {
        body["answer_contract"] = serde_json::json!(contract);
    }
    let dropped = keep_known_tags(&mut body, &authoring_vocabulary());
    report_dropped(spec, kind, &dropped);
    Ok(body.to_string())
}

/// Assemble, gate, and fill in the satisfying count.
///
/// The returned text is what the row stores and what the digest covers.
///
/// # Errors
///
/// Returns the [`Rejection`] of [`assemble`] and every rejection of
/// `cadus_core::template::gate`. The message is the literal text of the check
/// that refused the document, and it is what the next attempt reads.
pub fn verify(spec: &AuthoringSpec, arguments: &Value) -> Result<String, Rejection> {
    // Spec section 5.3, and row R7: an error_tag outside the vocabulary is
    // dropped, on this document and on the diagnosis document alike, and the
    // drop runs before the gate reads the body.
    let body = assemble_kept(Kind::Template, spec, arguments)?;
    let gate_spec = GateSpec {
        answer_kind: spec.answer_kind,
        exemplars: &spec.exemplars,
    };
    let (doc, verified) = gate_body(&body, &gate_spec)?;
    let filled = with_space_size(&doc, &verified);
    to_body(&filled).map_err(unwritable)
}

/// The tool arguments of an instruction document, as the text its gate reads.
///
/// An instruction document carries no server-side field: the serve reader of L4
/// and L5 refuses an unknown field, so `v` or `topic_id` on a teach body would
/// serve a `500` instead of a page. The tool arguments are therefore the body,
/// and the gate is the only thing between them and the table.
fn instruction_body(arguments: &Value) -> Result<String, Rejection> {
    if arguments.is_object() {
        Ok(arguments.to_string())
    } else {
        Err(no_arguments())
    }
}

/// Gate one instruction document with `gate`, and write the gated document
/// with `write`: the body the row stores.
///
/// The written text comes from the GATED document and not from the arguments, so
/// a field the gate ignores never reaches the row and never reaches the digest.
///
/// `instance_answers` is the served material of the knowledge point
/// ([`served_instances`](super::served_instances)), and both gates READ it: the
/// last step of a worked example and every rung of a ladder name no answer of a
/// served problem (M6 review 2, findings V2 and V11).
fn verify_instruction<T>(
    gate: fn(&str, &InstructionSpec<'_>) -> Result<T, Rejection>,
    write: fn(&T) -> Result<String, serde_json::Error>,
    spec: &AuthoringSpec,
    arguments: &Value,
    instance_answers: &[ServedInstance],
) -> Result<String, Rejection> {
    let body = instruction_body(arguments)?;
    let gate_spec = InstructionSpec {
        exemplars: &spec.exemplars,
        instance_answers: instance_answers.to_vec(),
    };
    write(&gate(&body, &gate_spec)?).map_err(unwritable)
}

/// Gate one teach page, and write the body the row stores (L4, unit R6).
///
/// # Errors
///
/// Returns the [`Rejection`] of `cadus_core::instruction::gate_teach`, and the
/// rejection a tool call with no JSON object earns.
pub fn verify_teach(
    spec: &AuthoringSpec,
    arguments: &Value,
    instance_answers: &[ServedInstance],
) -> Result<String, Rejection> {
    verify_instruction(
        gate_teach,
        serde_json::to_string,
        spec,
        arguments,
        instance_answers,
    )
}

/// Gate one hint ladder, and write the body the row stores (L5, unit R6).
///
/// # Errors
///
/// Returns the [`Rejection`] of `cadus_core::instruction::gate_hint_ladder`, and
/// the rejection a tool call with no JSON object earns.
pub fn verify_hint_ladder(
    spec: &AuthoringSpec,
    arguments: &Value,
    instance_answers: &[ServedInstance],
) -> Result<String, Rejection> {
    verify_instruction(
        gate_hint_ladder,
        serde_json::to_string,
        spec,
        arguments,
        instance_answers,
    )
}

/// The error tags an authored document keeps (spec section 5.3).
///
/// It is the vocabulary the prompt states to the model
/// ([`MODEL_ERROR_TAGS`]), so the gate keeps exactly what the instruction
/// invites. `blank-answer` stands outside it: the grade path stamps that tag on
/// a blank submission, and no distractor claims a blank answer.
/// `cadus_core::config::default_error_tags` holds every tag of this list, so the
/// grade path never drops a tag the gate kept.
#[must_use]
pub fn authoring_vocabulary() -> Vec<String> {
    MODEL_ERROR_TAGS
        .iter()
        .map(|tag| (*tag).to_owned())
        .collect()
}

/// Log the tags the vocabulary filter dropped, for the operator (T3).
///
/// A drop is silent to the model, because the schema already constrains
/// `error_tag` to the vocabulary and the prompt states the rule. It is not
/// silent to an operator: a model that keeps inventing tags shows up here.
fn report_dropped(spec: &AuthoringSpec, kind: Kind, dropped: &[String]) {
    if dropped.is_empty() {
        return;
    }
    let kp = kp_key(&spec.topic_id, &spec.kp_id);
    let kind = kind.as_str();
    tracing::warn!(
        kp = %kp,
        kind,
        dropped = ?dropped,
        "authoring: an error_tag outside the vocabulary is dropped at the gate"
    );
}

/// Assemble one distractor list and verify it (A4; spec section 6.2).
///
/// The returned text is what the row stores and what the digest covers. The
/// document is the FILTERED one: `cadus_core::template::gate_diagnosis_body`
/// drops every distractor whose tag is outside
/// [`authoring_vocabulary`], and it refuses the list when nothing is left.
///
/// # Errors
///
/// Returns the [`Rejection`] of [`assemble`] and every rejection of
/// `cadus_core::template::gate_diagnosis`. The message is the literal text the
/// next attempt reads.
pub fn verify_diagnosis(spec: &AuthoringSpec, arguments: &Value) -> Result<String, Rejection> {
    let body = assemble(spec, arguments)?;
    let gate_spec = GateSpec {
        answer_kind: spec.answer_kind,
        exemplars: &spec.exemplars,
    };
    let (doc, dropped) = gate_diagnosis_body(&body, &gate_spec, &authoring_vocabulary())?;
    report_dropped(spec, Kind::Diagnosis, &dropped);
    to_diagnosis_body(&doc).map_err(unwritable)
}

/// The gate of one kind (spec section 2.2, step 2).
///
/// `instance_answers` holds the instances the templates of the knowledge point
/// serve ([`served_instances`](super::served_instances)). BOTH instruction
/// gates read it; the template gate and the diagnosis gate read the document's
/// own instances and ignore it.
///
/// # Errors
///
/// Returns the [`Rejection`] the kind's gate wrote. The message is the literal
/// text the next attempt reads.
pub fn verify_kind(
    kind: Kind,
    spec: &AuthoringSpec,
    arguments: &Value,
    instance_answers: &[ServedInstance],
) -> Result<String, Rejection> {
    // Trap T1, on EVERY kind and before EVERY gate. A model that writes one
    // backslash emits valid JSON whose decoded value is `$<TAB>imes$`; no gate
    // reads a control character, so the mangled text reached `content_store` on
    // `template`, `teach` and `hint_ladder` alike (M6 review finding F3).
    let arguments = &repair::repair_arguments(arguments)?;
    match kind {
        Kind::Template => verify(spec, arguments),
        Kind::Teach => verify_teach(spec, arguments, instance_answers),
        Kind::HintLadder => verify_hint_ladder(spec, arguments, instance_answers),
        Kind::Diagnosis => verify_diagnosis(spec, arguments),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        NO_ARGUMENTS, assemble, unwritable, verify, verify_diagnosis, verify_hint_ladder,
        verify_kind, verify_teach,
    };
    use crate::authoring::prompt::{AuthoringSpec, Kind};
    use cadus_core::curriculum::AnswerKind;
    use serde_json::json;

    /// A spec for the perfect-squares knowledge point.
    fn spec() -> AuthoringSpec {
        AuthoringSpec {
            kp_id: "squares".to_owned(),
            kp_name: "Perfect squares".to_owned(),
            topic_id: "perfect-squares".to_owned(),
            topic_name: "Perfect squares".to_owned(),
            answer_kind: AnswerKind::Numeric,
            difficulty_target: None,
            constraints: None,
            exemplars: Vec::new(),
        }
    }

    /// The three server-side fields are the server's, whatever the model sends.
    #[test]
    fn the_server_side_fields_overwrite_what_the_model_sends() {
        let body = assemble(
            &spec(),
            &json!({
                "v": 99,
                "topic_id": "another-topic",
                "answer_kind": "proof",
                "space_size": 1_000_000,
                "statement": "Compute ${a}^{{2}}$.",
                "answer_expr": "a**2"
            }),
        )
        .expect("the arguments are an object");
        let read: serde_json::Value = serde_json::from_str(&body).expect("the body reads");
        assert_eq!(read["v"], json!(1));
        assert_eq!(read["topic_id"], json!("perfect-squares"));
        assert_eq!(read["answer_kind"], json!("numeric"));
        assert_eq!(read.get("space_size"), None);
        assert_eq!(read["statement"], json!("Compute ${a}^{{2}}$."));
    }

    /// A tool call with no arguments object is a rejection, not a panic, on
    /// every path that reads the arguments.
    #[test]
    fn arguments_that_are_not_an_object_are_a_rejection() {
        let refusals = [
            assemble(&spec(), &json!("emit_template")),
            verify(&spec(), &json!("emit_template")),
            verify_diagnosis(&spec(), &json!("emit_distractors")),
            verify_teach(&spec(), &json!("emit_teach"), &[]),
            verify_hint_ladder(&spec(), &json!("emit_hint_ladder"), &[]),
        ];
        for refusal in refusals {
            let rejection = refusal.expect_err("a string is not an arguments object");
            assert_eq!(rejection.code, "tool-arguments");
            assert_eq!(rejection.message, NO_ARGUMENTS);
        }
    }

    /// A document serde cannot write is refused with the serde message.
    #[test]
    fn an_unwritable_document_names_the_serde_error() {
        let err = serde_json::from_str::<serde_json::Value>("{").expect_err("the text is cut");
        let rejection = unwritable(err);
        assert_eq!(rejection.code, "tool-arguments");
        assert!(
            rejection
                .message
                .starts_with("the verified document does not write as JSON: "),
            "{}",
            rejection.message
        );
    }

    /// The stored text of an instruction document is the GATED document and
    /// carries no server-side field: the serve reader refuses one (unit R6).
    #[test]
    fn a_gated_teach_page_stores_the_document_and_nothing_else() {
        let body = verify_kind(
            Kind::Teach,
            &spec(),
            &json!({
                "concept": "A square multiplies a number by itself.",
                "worked_example": {"problem": "Compute $6^2$.", "steps": ["$6 \\times 6 = 36$."]}
            }),
            &[],
        )
        .expect("the gate accepts the page");

        assert_eq!(
            body,
            r#"{"concept":"A square multiplies a number by itself.","worked_example":{"problem":"Compute $6^2$.","steps":["$6 \\times 6 = 36$."]}}"#
        );
    }

    /// Every kind reaches a gate through [`verify_kind`], so no unverified body
    /// reaches the table (units R6 and R7).
    #[test]
    fn every_kind_reaches_a_gate() {
        let rejection = verify_kind(Kind::Diagnosis, &spec(), &json!({"distractors": []}), &[])
            .expect_err("an empty distractor list is refused");
        assert_eq!(rejection.code, "distractor-missing");

        let rejection = verify_kind(Kind::Teach, &spec(), &json!({}), &[])
            .expect_err("a teach page with no concept is refused");
        assert_eq!(rejection.code, "teach-concept");

        let rejection = verify_kind(Kind::HintLadder, &spec(), &json!({"hints": []}), &[])
            .expect_err("an empty hint ladder is refused");
        assert_eq!(rejection.code, "hint-missing");
    }
}
