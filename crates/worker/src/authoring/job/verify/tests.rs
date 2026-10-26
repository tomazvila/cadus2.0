use super::{
    NO_ARGUMENTS, assemble, unwritable, verify, verify_diagnosis, verify_hint_ladder, verify_kind,
    verify_teach,
};
use crate::authoring::prompt::{AuthoringSpec, Kind};
use cadus_core::curriculum::AnswerKind;
use serde_json::json;

/// A spec for the perfect-squares knowledge point.
fn spec() -> AuthoringSpec {
    AuthoringSpec {
        visuals: vec![],
        kp_id: "squares".to_owned(),
        kp_name: "Perfect squares".to_owned(),
        topic_id: "perfect-squares".to_owned(),
        topic_name: "Perfect squares".to_owned(),
        answer_kind: AnswerKind::Numeric,
        difficulty_target: None,
        constraints: None,
        exemplars: Vec::new(),
        finite: None,
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

/// The stored text of an instruction document is the REFUSED document and
/// carries no server-side field: the serve reader refuses one (unit R6).
#[test]
fn a_checked_teach_page_stores_the_document_and_nothing_else() {
    let body = verify_kind(
        Kind::Teach,
        &spec(),
        &json!({
            "concept": "A square multiplies a number by itself.",
            "worked_example": {"problem": "Compute $6^2$.", "steps": ["$6 \\times 6 = 36$."]}
        }),
        &[],
    )
    .expect("the check accepts the page");

    assert_eq!(
        body,
        r#"{"concept":"A square multiplies a number by itself.","worked_example":{"problem":"Compute $6^2$.","steps":["$6 \\times 6 = 36$."]}}"#
    );
}

/// Every kind reaches a check through [`verify_kind`], so no unverified body
/// reaches the table (units R6 and R7).
#[test]
fn every_kind_reaches_a_check() {
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
