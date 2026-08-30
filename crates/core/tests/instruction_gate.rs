//! M6 R6 acceptance, the gate half: the teach page (L4) and the hint ladder
//! (L5).
//!
//! Spec: `docs/reference/authoring-and-spa-1.0-spec.md` section 7 row R6. Two of
//! the three acceptance checks of that row are gate checks, and each one is a
//! test here:
//!
//! 1. a hint ladder whose last rung contains the expected answer is rejected
//!    with a literal message —
//!    [`a_ladder_whose_last_rung_names_the_answer_is_rejected`];
//! 2. a teach body missing `worked_example.steps` is rejected —
//!    [`a_teach_body_with_no_worked_example_steps_is_rejected`].
//!
//! The third check (an approved teach body serves through the M5 teach route
//! with no model call) is `crates/web/tests/serve_routes.rs`.
//!
//! Every expected value is a LITERAL: the whole rejection sentence, character
//! for character, and the code beside it. No expected value is re-derived from
//! the code under test.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use cadus_core::curriculum::Exemplar;
use cadus_core::instruction::{
    HintLadder, InstructionSpec, TeachPage, WorkedExample, gate_hint_ladder, gate_teach,
};

// --------------------------------------------------------------------------- //
// The fixtures
// --------------------------------------------------------------------------- //

/// The one exemplar every test judges against: a problem that does NOT carry its
/// own answer, so the give-away rule reads every rung.
fn exemplars() -> Vec<Exemplar> {
    vec![Exemplar {
        problem: "Compute $7^2$.".to_owned(),
        answer: "49".to_owned(),
        solution_sketch: None,
    }]
}

/// The spec of the knowledge point under test.
fn spec(exemplars: &[Exemplar]) -> InstructionSpec<'_> {
    InstructionSpec { exemplars }
}

/// A teach body the gate accepts.
const GOOD_TEACH: &str = r#"{
    "concept": "Squaring a number multiplies it by itself.",
    "worked_example": {
        "problem": "Compute $6^2$.",
        "steps": ["Write $6^2$ as $6 \\times 6$.", "Multiply: $6 \\times 6 = 36$."]
    }
}"#;

/// A hint ladder the gate accepts. No rung names 49.
const GOOD_LADDER: &str = r#"{
    "hints": [
        "What does the small 2 above the number ask you to do?",
        "A square is the number multiplied by itself.",
        "Write the base twice with a multiplication sign between them, then multiply."
    ]
}"#;

// --------------------------------------------------------------------------- //
// Acceptance 1: a hint ladder whose last rung names the answer
// --------------------------------------------------------------------------- //

/// Hard Rule 3. The last rung escalates to teaching, and it still stops short of
/// the final answer, so a rung that states 49 is refused with the sentence the
/// next authoring attempt reads.
#[test]
fn a_ladder_whose_last_rung_names_the_answer_is_rejected() {
    let exemplars = exemplars();
    let body = r#"{"hints": [
        "What does the small 2 above the number ask you to do?",
        "A square is the number multiplied by itself, so $7^2$ is 49."
    ]}"#;

    let rejection = gate_hint_ladder(body, &spec(&exemplars)).expect_err("the rung names 49");

    assert_eq!(rejection.code, "hint-answer");
    assert_eq!(
        rejection.message,
        "rung 1 reads 'A square is the number multiplied by itself, so $7^2$ is 49.', which names \
the answer '49' of the exemplar 'Compute $7^2$.' — a hint is a question, never the final step \
(Hard Rule 3)"
    );
}

/// The same ladder without that rung is accepted, and the accepted document
/// holds the rungs in the authored order.
#[test]
fn a_ladder_that_stops_short_of_the_answer_is_accepted() {
    let exemplars = exemplars();
    let ladder = gate_hint_ladder(GOOD_LADDER, &spec(&exemplars)).expect("the ladder is clean");

    assert_eq!(
        ladder,
        HintLadder {
            hints: vec![
                "What does the small 2 above the number ask you to do?".to_owned(),
                "A square is the number multiplied by itself.".to_owned(),
                "Write the base twice with a multiplication sign between them, then multiply."
                    .to_owned(),
            ],
        }
    );
}

/// A token the exemplar's own problem shows is not a give-away: the learner is
/// reading it in the problem. This is the exemption the template gate takes over
/// an instance whose statement carries its answer.
#[test]
fn a_rung_may_name_a_number_the_exemplar_problem_already_shows() {
    let exemplars = vec![Exemplar {
        problem: "What is 49 divided by 7?".to_owned(),
        answer: "7".to_owned(),
        solution_sketch: None,
    }];
    let body = r#"{"hints": ["How many 7s fit inside 49?"]}"#;

    let ladder = gate_hint_ladder(body, &spec(&exemplars)).expect("the exemption applies");

    assert_eq!(ladder.hints.len(), 1);
}

/// A rung that repeats an earlier one leaves the learner exactly as stuck.
#[test]
fn a_repeated_rung_is_rejected() {
    let exemplars = exemplars();
    let body =
        r#"{"hints": ["Think about what squaring means.", "Think about what squaring means."]}"#;

    let rejection = gate_hint_ladder(body, &spec(&exemplars)).expect_err("rung 1 repeats rung 0");

    assert_eq!(rejection.code, "hint-repeat");
    assert_eq!(
        rejection.message,
        "rung 1 repeats rung 0 — every rung goes one small step past the one before it, and a \
repeated rung leaves the learner exactly as stuck"
    );
}

/// A ladder with no rung serves nothing: the hint route reads the rungs and
/// nothing else.
#[test]
fn a_ladder_with_no_rung_is_rejected() {
    let exemplars = exemplars();

    let rejection =
        gate_hint_ladder(r#"{"hints": []}"#, &spec(&exemplars)).expect_err("the list is empty");

    assert_eq!(rejection.code, "hint-missing");
    assert_eq!(
        rejection.message,
        "a hint ladder needs at least one rung, and a hint may never give the answer away"
    );

    let absent = gate_hint_ladder("{}", &spec(&exemplars)).expect_err("there is no list at all");
    assert_eq!(absent.code, "hint-missing");
    assert_eq!(
        absent.message,
        "a hint ladder needs a 'hints' list, widest rung first"
    );
}

/// A rung that is not a non-empty string is refused by position.
#[test]
fn a_rung_that_is_not_text_is_rejected() {
    let exemplars = exemplars();

    let rejection = gate_hint_ladder(r#"{"hints": ["Start here.", "   "]}"#, &spec(&exemplars))
        .expect_err("rung 1 is blank");

    assert_eq!(rejection.code, "hint-rung");
    assert_eq!(
        rejection.message,
        "rung 1 is not a non-empty string — every rung is one nudge a learner reads"
    );
}

/// The serve reader refuses an unknown field, so the gate refuses it first.
#[test]
fn a_ladder_with_a_spare_field_is_rejected() {
    let exemplars = exemplars();
    let body = r#"{"hints": ["Start here."], "kp_id": "squares"}"#;

    let rejection = gate_hint_ladder(body, &spec(&exemplars)).expect_err("kp_id is not a field");

    assert_eq!(rejection.code, "hint-unknown-field");
    assert_eq!(
        rejection.message,
        "the hint ladder carries the unknown field 'kp_id' — it holds 'hints' and nothing else"
    );
}

// --------------------------------------------------------------------------- //
// Acceptance 2: a teach body with no worked example steps
// --------------------------------------------------------------------------- //

/// The worked solution IS the page. A concept and a bare problem teach nothing,
/// so the gate refuses the body with the sentence the next attempt reads.
#[test]
fn a_teach_body_with_no_worked_example_steps_is_rejected() {
    let exemplars = exemplars();
    let body = r#"{
        "concept": "Squaring a number multiplies it by itself.",
        "worked_example": {"problem": "Compute $6^2$."}
    }"#;

    let rejection = gate_teach(body, &spec(&exemplars)).expect_err("there are no steps");

    assert_eq!(rejection.code, "teach-steps");
    assert_eq!(
        rejection.message,
        "a teach page needs 'worked_example.steps': the complete solution, one step per entry, \
ending with the final answer — a concept with no worked solution teaches nothing"
    );
}

/// A steps list with no step takes its own sentence.
#[test]
fn a_teach_body_with_an_empty_steps_list_is_rejected() {
    let exemplars = exemplars();
    let body = r#"{
        "concept": "Squaring a number multiplies it by itself.",
        "worked_example": {"problem": "Compute $6^2$.", "steps": []}
    }"#;

    let rejection = gate_teach(body, &spec(&exemplars)).expect_err("the list is empty");

    assert_eq!(rejection.code, "teach-steps");
    assert_eq!(
        rejection.message,
        "'worked_example.steps' is empty: the complete solution goes here, one step per entry, \
ending with the final answer"
    );
}

/// A step that is not a non-empty string is refused by position.
#[test]
fn a_step_that_is_not_text_is_rejected() {
    let exemplars = exemplars();
    let body = r#"{
        "concept": "Squaring a number multiplies it by itself.",
        "worked_example": {"problem": "Compute $6^2$.", "steps": ["Write $6 \\times 6$.", 36]}
    }"#;

    let rejection = gate_teach(body, &spec(&exemplars)).expect_err("step 1 is a number");

    assert_eq!(rejection.code, "teach-steps");
    assert_eq!(
        rejection.message,
        "step 1 of 'worked_example.steps' is not a non-empty string — every step is one line of \
the solution a learner reads"
    );
}

/// A page with no concept states no method.
#[test]
fn a_teach_body_with_no_concept_is_rejected() {
    let exemplars = exemplars();
    let body = r#"{
        "concept": "  ",
        "worked_example": {"problem": "Compute $6^2$.", "steps": ["$6 \\times 6 = 36$."]}
    }"#;

    let rejection = gate_teach(body, &spec(&exemplars)).expect_err("the concept is blank");

    assert_eq!(rejection.code, "teach-concept");
    assert_eq!(
        rejection.message,
        "a teach page needs a non-empty 'concept': one or two plain sentences that state the \
method or the rule, because the learner may never have seen this material"
    );
}

/// A page with no worked example at all takes the object sentence.
#[test]
fn a_teach_body_with_no_worked_example_is_rejected() {
    let exemplars = exemplars();
    let body = r#"{"concept": "Squaring a number multiplies it by itself."}"#;

    let rejection = gate_teach(body, &spec(&exemplars)).expect_err("there is no worked example");

    assert_eq!(rejection.code, "teach-worked-example");
    assert_eq!(
        rejection.message,
        "a teach page needs a 'worked_example' object with 'problem' and 'steps'"
    );
}

/// A6 serves the exemplars, so an exemplar worked out on the teach page hands
/// the learner an answer before the attempt (Hard Rule 1).
#[test]
fn a_worked_example_that_is_an_exemplar_is_rejected() {
    let exemplars = exemplars();
    let body = r#"{
        "concept": "Squaring a number multiplies it by itself.",
        "worked_example": {"problem": "Compute $7^2$.", "steps": ["$7 \\times 7 = 49$."]}
    }"#;

    let rejection = gate_teach(body, &spec(&exemplars)).expect_err("the problem is exemplar 0");

    assert_eq!(rejection.code, "teach-worked-example");
    assert_eq!(
        rejection.message,
        "'worked_example.problem' reads 'Compute $7^2$.', which is exemplar 0 — the server serves \
the exemplars, so work the method on DIFFERENT values, or the page answers a problem the learner \
has not attempted yet (Hard Rule 1)"
    );
}

/// The accepted page is the document the M5 teach route serves.
#[test]
fn a_complete_teach_page_is_accepted() {
    let exemplars = exemplars();
    let page = gate_teach(GOOD_TEACH, &spec(&exemplars)).expect("the page is complete");

    assert_eq!(
        page,
        TeachPage {
            concept: "Squaring a number multiplies it by itself.".to_owned(),
            worked_example: WorkedExample {
                problem: "Compute $6^2$.".to_owned(),
                steps: vec![
                    "Write $6^2$ as $6 \\times 6$.".to_owned(),
                    "Multiply: $6 \\times 6 = 36$.".to_owned(),
                ],
            },
        }
    );

    // The stored body is the document, written with no spare field: the serve
    // reader refuses one, so the gate's own output must read back through it.
    assert_eq!(
        serde_json::to_string(&page).unwrap(),
        r#"{"concept":"Squaring a number multiplies it by itself.","worked_example":{"problem":"Compute $6^2$.","steps":["Write $6^2$ as $6 \\times 6$.","Multiply: $6 \\times 6 = 36$."]}}"#
    );
}

/// The serve reader refuses an unknown field on the page and on the worked
/// example alike.
#[test]
fn a_teach_body_with_a_spare_field_is_rejected() {
    let exemplars = exemplars();
    let page = r#"{
        "concept": "Squaring a number multiplies it by itself.",
        "worked_example": {"problem": "Compute $6^2$.", "steps": ["$6 \\times 6 = 36$."]},
        "answer": "36"
    }"#;

    let rejection = gate_teach(page, &spec(&exemplars)).expect_err("answer is not a field");
    assert_eq!(rejection.code, "teach-unknown-field");
    assert_eq!(
        rejection.message,
        "the teach page carries the unknown field 'answer' — it holds 'concept' and \
'worked_example' and nothing else"
    );

    let nested = r#"{
        "concept": "Squaring a number multiplies it by itself.",
        "worked_example": {"problem": "Compute $6^2$.", "steps": ["$6 \\times 6 = 36$."], "answer": "36"}
    }"#;

    let rejection = gate_teach(nested, &spec(&exemplars)).expect_err("answer is not a field");
    assert_eq!(rejection.code, "teach-unknown-field");
    assert_eq!(
        rejection.message,
        "the worked example carries the unknown field 'answer' — it holds 'problem' and 'steps' \
and nothing else"
    );
}

// --------------------------------------------------------------------------- //
// C4: no body panics either gate
// --------------------------------------------------------------------------- //

/// A body that is not a document at all is a rejection, never a crash.
#[test]
fn no_body_panics_either_gate() {
    let exemplars = exemplars();
    let bodies = [
        "",
        "null",
        "[]",
        "3",
        "\"teach\"",
        "{",
        "{\"concept\": null}",
        "{\"hints\": {\"0\": \"a\"}}",
        "{\"worked_example\": []}",
    ];

    for body in bodies {
        let teach = gate_teach(body, &spec(&exemplars));
        assert!(teach.is_err(), "the teach gate accepted {body:?}");
        let hint = gate_hint_ladder(body, &spec(&exemplars));
        assert!(hint.is_err(), "the hint gate accepted {body:?}");
    }

    let not_json = gate_teach("{", &spec(&exemplars)).expect_err("a brace is not JSON");
    assert_eq!(not_json.code, "body");
    assert_eq!(
        not_json.message,
        "the teach body is not JSON: EOF while parsing an object at line 1 column 1"
    );

    let not_object = gate_hint_ladder("[]", &spec(&exemplars)).expect_err("a list is not a body");
    assert_eq!(not_object.code, "body");
    assert_eq!(not_object.message, "the hint body is not a JSON object");
}
