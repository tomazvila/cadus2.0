//! M6 R6 acceptance, the gate half, part 2: the served material (M6 review 2,
//! findings V2, V10 and V11; M6 review, finding F25), the panic sweep, and the
//! shipped curriculum.
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

use cadus_core::instruction::{
    INSTANCE_SAMPLES, ServedInstance, gate_hint_ladder, gate_teach, regate, template_instances,
};
use common::instruction_fixtures::{GOOD_TEACH, INSTANCES, exemplars, spec, spec_with_instances};
use common::paths::curriculum_root;

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

/// A worked example must not reveal an exact practice problem before its attempt.
#[test]
fn a_page_that_repeats_a_served_template_problem_is_refused() {
    let exemplars = exemplars();
    let body = r#"{
        "concept": "Squaring a number multiplies it by itself.",
        "worked_example": {
            "problem": "Compute $9^2$.",
            "steps": ["Write $9$ twice.", "The product is 81."]
        }
    }"#;
    let refusal = gate_teach(body, &spec_with_instances(&exemplars, &INSTANCES)).unwrap_err();
    assert_eq!(refusal.code, "teach-worked-example");
}

/// Equal answers from distinct arithmetic problems do not disclose problem identity.
#[test]
fn a_distinct_addition_example_survives_an_equal_served_sum() {
    let exemplars = [];
    let instances = [("Compute $4 + 1$.", "5")];
    let body = r#"{
        "concept": "Addition combines two quantities.",
        "worked_example": {
            "problem": "Compute $2 + 3$.",
            "steps": ["Start at two and count three more.", "The sum is 5."]
        }
    }"#;
    gate_teach(body, &spec_with_instances(&exemplars, &instances)).unwrap();
    let dense = [
        ("Another problem with answer two", "2"),
        ("Another problem with answer three", "3"),
        ("Compute $4 + 1$.", "5"),
    ];
    let equation = body.replace("The sum is 5.", "$2 + 3 = 5$.");
    gate_teach(&equation, &spec_with_instances(&exemplars, &dense)).unwrap();
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
// The served instances of a stored template
// --------------------------------------------------------------------------- //

/// The instances one template body serves: every pinned sample and the
/// [`INSTANCE_SAMPLES`] draws from the gate seed, each answer once.
#[test]
fn a_template_body_serves_its_samples_and_its_seeded_draws() {
    let body = r#"{
      "v": 1,
      "topic_id": "perfect-squares",
      "answer_kind": "numeric",
      "statement": "Compute ${a}^{{2}}$.",
      "params": {"a": {"kind": "int", "low": 1, "high": 2}},
      "answer_expr": "a**2",
      "hints": ["What does squaring a number mean?"],
      "samples": [{"params": {"a": 2}, "expected": "4"}]
    }"#;
    let served = template_instances(body);
    assert_eq!(INSTANCE_SAMPLES, 8);
    assert_eq!(
        served[0],
        ServedInstance {
            problem: "Compute $2^{2}$.".to_owned(),
            answer: "4".to_owned(),
        },
        "the pinned sample comes first"
    );
    assert_eq!(
        served.len(),
        2,
        "two values of a give two distinct instances"
    );
    assert_eq!(served[1].answer, "1");

    assert_eq!(template_instances("not json"), Vec::new());
    let uncompilable = r#"{
      "v": 1,
      "topic_id": "perfect-squares",
      "answer_kind": "numeric",
      "statement": "Compute ${a}$.",
      "params": {"a": {"kind": "int", "low": 1, "high": 2}},
      "answer_expr": "a +",
      "hints": ["Read it again."]
    }"#;
    assert_eq!(template_instances(uncompilable), Vec::new());
}

/// The teach gate refuses a worked example with no problem, and the regate
/// runs the teach gate for the `teach` kind.
#[test]
fn a_worked_example_with_no_problem_is_rejected_and_regated() {
    let body = r#"{
        "concept": "Squaring a number multiplies it by itself.",
        "worked_example": {"problem": "  ", "steps": ["Multiply."]}
    }"#;
    let refused = gate_teach(body, &spec(&exemplars())).expect_err("no problem");
    assert_eq!(refused.code, "teach-problem");
    assert_eq!(
        refused.message,
        "a teach page needs a non-empty 'worked_example.problem': the concept states the method, \
and the worked problem is where the learner sees it done"
    );
    assert_eq!(regate("teach", GOOD_TEACH, &spec(&exemplars())), None);
    assert_eq!(
        regate("teach", body, &spec(&exemplars())).map(|rejection| rejection.code),
        Some("teach-problem")
    );
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
/// and served through the L5 hint route. Every currently matching exemplar must
/// refuse a give-away rung. The final census is only a review tripwire: authored
/// tuple and structured answers can legitimately stop appearing verbatim in
/// their problems, but a census change must still run this complete live sweep.
#[test]
fn every_shipped_knowledge_point_whose_exemplar_shows_its_answer_gates() {
    let (curriculum, _) = cadus_core::curriculum::load_raw_curriculum(&curriculum_root())
        .expect("the shipped tree loads");

    let mut exempted = 0_usize;
    let mut blind = 0_usize;
    let mut foundations_exempted = 0_usize;
    let mut foundations_blind = 0_usize;
    for topic in curriculum.topics() {
        for kp in &topic.topic.knowledge_points {
            let shown: Vec<&cadus_core::curriculum::Exemplar> = kp
                .exemplars
                .iter()
                .filter(|exemplar| stands_alone(&exemplar.problem, &exemplar.answer))
                .collect();
            if shown.is_empty() {
                continue;
            }
            exempted += 1;
            if topic.course_dir == "foundations" {
                foundations_exempted += 1;
            }
            if shown.len() == kp.exemplars.len() {
                blind += 1;
                if topic.course_dir == "foundations" {
                    foundations_blind += 1;
                }
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

    // `exempted` counts knowledge points with at least one formerly exempted
    // exemplar. `blind` counts knowledge points whose exemplars were all
    // formerly exempted. The first census covers every shipped course; the
    // second makes the Foundations-only scope explicit. The loop above is the
    // fail-closed invariant for every member of both cohorts.
    assert_eq!((exempted, blind), (363, 60));
    assert_eq!((foundations_exempted, foundations_blind), (157, 30));
}

/// A sample that does not instantiate and a draw that does not evaluate
/// contribute nothing, and the rest of the document still serves.
#[test]
fn an_instance_that_does_not_evaluate_contributes_nothing() {
    let body = r#"{
      "v": 1,
      "topic_id": "divide",
      "answer_kind": "numeric",
      "statement": "Compute $6 / ({a} - 2)$.",
      "params": {"a": {"kind": "int", "low": 1, "high": 3}},
      "answer_expr": "6/(a-2)",
      "hints": ["What is a - 2?"],
      "samples": [{"params": {"a": 2}, "expected": "x"}]
    }"#;
    let served = template_instances(body);
    let answers: Vec<&str> = served.iter().map(|s| s.answer.as_str()).collect();
    assert!(!answers.is_empty(), "the values 1 and 3 still serve");
    assert!(
        answers
            .iter()
            .all(|answer| *answer == "-6" || *answer == "6"),
        "no instance of a = 2 serves, it served {answers:?}"
    );
}
