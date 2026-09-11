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
//! The M6 review adds three more, one per finding:
//!
//! 3. a rung that names the answer of a rendered template instance is rejected
//!    (findings F2 and F15) —
//!    [`a_rung_that_names_a_template_instance_answer_is_rejected`];
//! 4. an exemplar whose own problem shows its answer takes no exemption any more
//!    (finding F25) —
//!    [`a_rung_that_names_an_answer_the_exemplar_problem_shows_is_still_rejected`];
//! 5. every shipped knowledge point that took that exemption now refuses a
//!    give-away rung, and the count is pinned (finding F25) —
//!    [`every_shipped_knowledge_point_whose_exemplar_shows_its_answer_gates`].
//!
//! The second M6 review adds three more, one per finding:
//!
//! 6. a give-away on the FIRST rung and on a MIDDLE rung is refused (finding
//!    V10) — [`a_give_away_on_the_first_rung_is_rejected`] and
//!    [`a_give_away_on_a_middle_rung_is_rejected`];
//! 7. the teach gate reads the served instances, so a last step that names the
//!    answer of ANOTHER served problem is refused (findings V2 and V11) —
//!    [`a_last_step_that_names_another_served_answer_is_rejected`];
//! 8. `regate` runs the gate of a stored document again, for the approve route
//!    (the FIX2-M6-A ruling, part 3) — [`the_regate_reads_the_stored_kind`].
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

mod common;

use cadus_core::curriculum::Exemplar;
use cadus_core::instruction::{HintLadder, TeachPage, WorkedExample, gate_hint_ladder, gate_teach};
use common::instruction_fixtures::{
    GOOD_LADDER, GOOD_TEACH, INSTANCES, exemplars, spec, spec_with_instances,
};

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
the answer '49' this knowledge point serves — a hint is a question, never the final step (Hard \
Rule 3)"
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

/// M6 review, findings F2 and F15. One ladder serves every instance of the
/// knowledge point, so an answer a rendered template instance carries is an
/// answer a learner reads. A rung that names it is refused, and the message
/// names the rung and the answer.
#[test]
fn a_rung_that_names_a_template_instance_answer_is_rejected() {
    let exemplars = exemplars();
    let body = r#"{"hints": [
        "What does the small 2 above the number ask you to do?",
        "Multiply the base by itself; the product is 81."
    ]}"#;

    let rejection = gate_hint_ladder(body, &spec_with_instances(&exemplars, &INSTANCES))
        .expect_err("the rung names the answer of a served instance");

    assert_eq!(rejection.code, "hint-answer");
    assert_eq!(
        rejection.message,
        "rung 1 reads 'Multiply the base by itself; the product is 81.', which names the answer \
'81' this knowledge point serves — a hint is a question, never the final step (Hard Rule 3)"
    );
}

/// The same set with no rung that names one of its answers is accepted, so the
/// wider set refuses give-away rungs and nothing else.
#[test]
fn a_ladder_that_names_no_instance_answer_is_accepted() {
    let exemplars = exemplars();
    let ladder = gate_hint_ladder(GOOD_LADDER, &spec_with_instances(&exemplars, &INSTANCES))
        .expect("no rung names 49, 64 or 81");

    assert_eq!(ladder.hints.len(), 3);
}

/// M6 review, finding F25. The exemption is gone: an exemplar whose own problem
/// shows its answer no longer takes its answer out of the give-away rule, because
/// the ladder serves the rendered instances of the same knowledge point too, and
/// their statements show nothing.
#[test]
fn a_rung_that_names_an_answer_the_exemplar_problem_shows_is_still_rejected() {
    let exemplars = vec![Exemplar {
        answer_contract: None,
        problem: "What is 49 divided by 7?".to_owned(),
        answer: "7".to_owned(),
        solution_sketch: None,
    }];
    let body = r#"{"hints": ["Count in sevens; the answer is 7."]}"#;

    let rejection =
        gate_hint_ladder(body, &spec(&exemplars)).expect_err("there is no exemption any more");

    assert_eq!(rejection.code, "hint-answer");
    assert_eq!(
        rejection.message,
        "rung 0 reads 'Count in sevens; the answer is 7.', which names the answer '7' this \
knowledge point serves — a hint is a question, never the final step (Hard Rule 3)"
    );
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
// The served answers and the step text
// --------------------------------------------------------------------------- //

#[test]
fn the_served_answers_hold_each_answer_once_and_no_empty_answer() {
    let exemplars = exemplars();
    let spec = spec_with_instances(
        &exemplars,
        &[
            ("Compute $7^2$ again.", "49"),
            ("Blank.", ""),
            ("Compute $9^2$.", "81"),
        ],
    );
    assert_eq!(spec.served_answers(), vec!["49", "81"]);
}

#[test]
fn a_blank_step_is_rejected() {
    let exemplars = exemplars();
    let body = r#"{
        "concept": "Squaring a number multiplies it by itself.",
        "worked_example": {"problem": "Compute $6^2$.", "steps": ["   "]}
    }"#;
    let rejection = gate_teach(body, &spec(&exemplars)).expect_err("a blank step is rejected");
    assert_eq!(rejection.code, "teach-steps");
    assert_eq!(
        rejection.message,
        "step 0 of 'worked_example.steps' is not a non-empty string — every step is one line of \
the solution a learner reads"
    );
}
