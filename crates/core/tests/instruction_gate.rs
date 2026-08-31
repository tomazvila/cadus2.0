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

use std::path::Path;

use cadus_core::curriculum::{Exemplar, load_raw_curriculum};
use cadus_core::instruction::{
    HintLadder, InstructionSpec, ServedInstance, TeachPage, WorkedExample, gate_hint_ladder,
    gate_teach, regate,
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

/// The spec of the knowledge point under test: no approved template, so the
/// exemplar answers are the whole served set.
fn spec(exemplars: &[Exemplar]) -> InstructionSpec<'_> {
    InstructionSpec {
        exemplars,
        instance_answers: Vec::new(),
    }
}

/// The spec of a knowledge point whose stored templates render these instances.
///
/// Each pair is one served problem and the answer that problem expects. The
/// teach gate reads the pair; the hint gate reads the answer alone.
fn spec_with_instances<'a>(
    exemplars: &'a [Exemplar],
    instances: &[(&str, &str)],
) -> InstructionSpec<'a> {
    InstructionSpec {
        exemplars,
        instance_answers: instances
            .iter()
            .map(|(problem, answer)| ServedInstance {
                problem: (*problem).to_owned(),
                answer: (*answer).to_owned(),
            })
            .collect(),
    }
}

/// The two instances of every test that judges against served material: the
/// squares of 8 and of 9.
const INSTANCES: [(&str, &str); 2] = [("Compute $8^2$.", "64"), ("Compute $9^2$.", "81")];

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
// M6 review 2: the FIX2-M6-A findings
// --------------------------------------------------------------------------- //

/// FIX2-M6-A, finding V10. Every earlier test of the give-away rule put the
/// answer on the LAST rung, so a gate that read the last rung alone passed all of
/// them and no test saw the difference. The FIRST rung is refused too, and
/// the message names rung 0.
#[test]
fn a_give_away_on_the_first_rung_is_rejected() {
    let exemplars = exemplars();
    let body = r#"{"hints": [
        "A square is the number multiplied by itself, so $7^2$ is 49.",
        "What does the small 2 above the number ask you to do?",
        "Write the base twice with a multiplication sign between them, then multiply."
    ]}"#;

    let rejection = gate_hint_ladder(body, &spec(&exemplars)).expect_err("rung 0 names 49");

    assert_eq!(rejection.code, "hint-answer");
    assert_eq!(
        rejection.message,
        "rung 0 reads 'A square is the number multiplied by itself, so $7^2$ is 49.', which names \
the answer '49' this knowledge point serves — a hint is a question, never the final step (Hard \
Rule 3)"
    );
}

/// FIX2-M6-A, finding V10, the other half. A MIDDLE rung of a three-rung ladder
/// is refused, and the answer here is an instance answer, so the rule reads the
/// whole served set on every rung and not on the last one.
#[test]
fn a_give_away_on_a_middle_rung_is_rejected() {
    let exemplars = exemplars();
    let body = r#"{"hints": [
        "What does the small 2 above the number ask you to do?",
        "For a base of 9 the product is 81.",
        "Write the base twice with a multiplication sign between them, then multiply."
    ]}"#;

    let rejection = gate_hint_ladder(body, &spec_with_instances(&exemplars, &INSTANCES))
        .expect_err("rung 1 of three names 81");

    assert_eq!(rejection.code, "hint-answer");
    assert_eq!(
        rejection.message,
        "rung 1 reads 'For a base of 9 the product is 81.', which names the answer '81' this \
knowledge point serves — a hint is a question, never the final step (Hard Rule 3)"
    );
}

/// FIX2-M6-A, findings V2 and V11. The teach gate READS the served instances.
///
/// The page works `Compute $15^2$.`, which no template of this knowledge point
/// renders, so 225 is no served answer. Its last step also states 81, the answer
/// of the served instance `Compute $9^2$.`, and that answer belongs to a problem
/// the learner has not attempted yet.
#[test]
fn a_last_step_that_names_another_served_answer_is_rejected() {
    let exemplars = exemplars();
    let body = r#"{
        "concept": "Squaring a number multiplies it by itself.",
        "worked_example": {
            "problem": "Compute $15^2$.",
            "steps": [
                "Write the base twice with a multiplication sign between them.",
                "The product is 225, the same way $9^2$ is 81."
            ]
        }
    }"#;

    let rejection = gate_teach(body, &spec_with_instances(&exemplars, &INSTANCES))
        .expect_err("the last step names the answer of a served instance");

    assert_eq!(rejection.code, "teach-answer");
    assert_eq!(
        rejection.message,
        "the last step of 'worked_example.steps' reads 'The product is 225, the same way $9^2$ is \
81.', which names '81', the answer of 'Compute $9^2$.' — this knowledge point serves that problem \
too, and the page works 'Compute $15^2$.', so the step hands the learner an answer before the \
attempt (Hard Rule 1)"
    );
}

/// The same page with no second answer in its last step is accepted, so the rule
/// refuses a give-away and nothing else.
#[test]
fn a_last_step_that_names_only_its_own_answer_is_accepted() {
    let exemplars = exemplars();
    let body = r#"{
        "concept": "Squaring a number multiplies it by itself.",
        "worked_example": {
            "problem": "Compute $15^2$.",
            "steps": [
                "Write the base twice with a multiplication sign between them.",
                "The product is 225."
            ]
        }
    }"#;

    let page = gate_teach(body, &spec_with_instances(&exemplars, &INSTANCES))
        .expect("no served answer stands in the last step");

    assert_eq!(page.worked_example.problem, "Compute $15^2$.");
}

/// The page works its OWN problem to its own answer, and the gate accepts that.
/// The worked problem here is the served instance `Compute $9^2$.`, so 81 is the
/// answer of the problem the page works and not the answer of another one.
#[test]
fn a_page_that_works_a_served_problem_states_that_answer() {
    let exemplars = exemplars();
    let body = r#"{
        "concept": "Squaring a number multiplies it by itself.",
        "worked_example": {
            "problem": "Compute $9^2$.",
            "steps": ["Write $9$ twice.", "The product is 81."]
        }
    }"#;

    let page = gate_teach(body, &spec_with_instances(&exemplars, &INSTANCES))
        .expect("81 is the answer of the problem the page works");

    assert_eq!(page.worked_example.steps.len(), 2);
}

/// An EARLIER step is not the answer of the page, so a numeral on the way to it
/// is arithmetic and not a give-away.
#[test]
fn an_earlier_step_that_names_a_served_answer_is_accepted() {
    let exemplars = exemplars();
    let body = r#"{
        "concept": "Squaring a number multiplies it by itself.",
        "worked_example": {
            "problem": "Compute $15^2$.",
            "steps": [
                "Split 15 into 8 and 7, and remember that $8^2$ is 64.",
                "The product is 225."
            ]
        }
    }"#;

    let page = gate_teach(body, &spec_with_instances(&exemplars, &INSTANCES))
        .expect("only the last step carries the answer of the page");

    assert_eq!(page.worked_example.steps.len(), 2);
}

/// FIX2-M6-A, part 3. `regate` runs the gate of a STORED document again, so the
/// approve route judges a pending ladder against material that reached the table
/// after the ladder did. A kind that is not an instruction document is never
/// judged.
#[test]
fn the_regate_reads_the_stored_kind() {
    let exemplars = exemplars();
    let ladder = r#"{"hints": ["For a base of 9 the product is 81."]}"#;

    assert_eq!(regate("hint_ladder", ladder, &spec(&exemplars)), None);

    let refused = regate(
        "hint_ladder",
        ladder,
        &spec_with_instances(&exemplars, &INSTANCES),
    )
    .expect("the served instance answer is now in the set");
    assert_eq!(refused.code, "hint-answer");
    assert_eq!(
        refused.message,
        "rung 0 reads 'For a base of 9 the product is 81.', which names the answer '81' this \
knowledge point serves — a hint is a question, never the final step (Hard Rule 3)"
    );

    // A template body is not an instruction document, so the re-gate answers
    // None for it and the approve route leaves the row alone.
    assert_eq!(
        regate(
            "template",
            ladder,
            &spec_with_instances(&exemplars, &INSTANCES)
        ),
        None
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

// --------------------------------------------------------------------------- //
// M6 review, finding F25: the shipped curriculum
// --------------------------------------------------------------------------- //

/// Whether `token` stands in `text` as a run of its own.
///
/// The rule is written out here, so the count below is this test's own number and
/// never a number the code under test handed back. A letter, a digit, or an
/// underscore beside the run makes the run part of a longer word or number. A
/// point or a slash breaks the run only when a digit stands beyond it, so `16`
/// stands at the end of `is 16.` and `2` does not stand inside `1/2`.
fn stands_alone(text: &str, token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    let characters: Vec<char> = text.chars().collect();
    let needle: Vec<char> = token.chars().collect();
    let free = |near: Option<&char>, far: Option<&char>| -> bool {
        match near {
            None => true,
            Some(near) if near.is_ascii_alphanumeric() || *near == '_' => false,
            Some(near) if *near == '.' || *near == '/' => !far.is_some_and(char::is_ascii_digit),
            Some(_) => true,
        }
    };
    let last = characters.len().saturating_sub(needle.len());
    for start in 0..=last {
        if characters.get(start..start + needle.len()) != Some(needle.as_slice()) {
            continue;
        }
        let after = start + needle.len();
        let free_before = start.checked_sub(1).is_none_or(|index| {
            free(
                characters.get(index),
                index.checked_sub(1).and_then(|far| characters.get(far)),
            )
        });
        if free_before && free(characters.get(after), characters.get(after + 1)) {
            return true;
        }
    }
    false
}

/// The give-away ladder of one answer: one rung, and it states the answer.
fn give_away(answer: &str) -> String {
    let rung = serde_json::to_string(&format!("The answer is {answer}."))
        .expect("a string writes as JSON");
    format!(r#"{{"hints": [{rung}]}}"#)
}

/// Finding F25. Before this fix the gate skipped an exemplar outright whenever
/// the exemplar's own problem carried its answer, so for those knowledge points a
/// rung that stated the answer verbatim passed the gate, was stored `pending`,
/// and served through the L5 hint route. The count of shipped knowledge points
/// that took the exemption is pinned here, and every one of them now refuses a
/// give-away rung.
#[test]
fn every_shipped_knowledge_point_whose_exemplar_shows_its_answer_gates() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../curriculum");
    let (curriculum, _) = load_raw_curriculum(&root).expect("the shipped tree loads");

    let mut exempted = 0_usize;
    let mut blind = 0_usize;
    for topic in curriculum.topics() {
        for kp in &topic.topic.knowledge_points {
            let shown: Vec<&Exemplar> = kp
                .exemplars
                .iter()
                .filter(|exemplar| stands_alone(&exemplar.problem, &exemplar.answer))
                .collect();
            if shown.is_empty() {
                continue;
            }
            exempted += 1;
            if shown.len() == kp.exemplars.len() {
                blind += 1;
            }
            let key = format!("{}/{}", topic.topic.id, kp.id);
            for exemplar in shown {
                match gate_hint_ladder(&give_away(&exemplar.answer), &spec(&kp.exemplars)) {
                    Ok(_) => panic!(
                        "{key} accepted a rung that states the answer {:?}",
                        exemplar.answer
                    ),
                    Err(rejection) => assert_eq!(rejection.code, "hint-answer", "{key}"),
                }
            }
        }
    }

    // `exempted` counts the knowledge points that held at least one exempted
    // exemplar. `blind` counts the ones whose exemplars were ALL exempted: a
    // ladder that stated every answer of those passed the whole gate.
    assert_eq!((exempted, blind), (307, 62));
}
