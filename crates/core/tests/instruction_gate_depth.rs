#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The depth parts of a teach page: `picture`, `watch_out` and `second_example`.
mod common;
use cadus_core::instruction::gate_teach;
use common::instruction_fixtures::{GOOD_TEACH, INSTANCES, exemplars, spec, spec_with_instances};
use serde_json::{Value, json};

fn page(key: &str, value: Value) -> String {
    let mut body: Value = serde_json::from_str(GOOD_TEACH).unwrap();
    body[key] = value;
    body.to_string()
}

fn second(problem: &str, steps: Value) -> Value {
    json!({"problem": problem, "steps": steps})
}

fn code(body: &str) -> &'static str {
    let examples = exemplars();
    gate_teach(body, &spec(&examples)).unwrap_err().code
}

#[test]
fn a_page_with_the_three_parts_is_accepted() {
    let examples = exemplars();
    let mut body: Value = serde_json::from_str(GOOD_TEACH).unwrap();
    body["picture"] =
        json!("A square tile has the same length on each side. Count the tiles in rows.");
    body["watch_out"] = json!("The rule works for a number multiplied by itself only.");
    body["second_example"] = second(
        "A square room has an area of $25$ square meters.",
        json!([
            "The area is a side times a side.",
            "So each side is $5$ meters."
        ]),
    );
    let page = gate_teach(&body.to_string(), &spec(&examples)).unwrap();
    assert!(page.picture.is_some() && page.watch_out.is_some() && page.second_example.is_some());
    let written = serde_json::to_string(&page).unwrap();
    let after = written.split(r#""steps":["#).nth(1).unwrap();
    assert!(after.contains(r#""picture":"#));
    assert!(after.find("picture").unwrap() < after.find("watch_out").unwrap());
    assert!(after.find("watch_out").unwrap() < after.find("second_example").unwrap());
}

#[test]
fn a_page_without_the_parts_keeps_its_bytes() {
    let examples = exemplars();
    let page = gate_teach(GOOD_TEACH, &spec(&examples)).unwrap();
    assert_eq!(
        (page.picture, page.watch_out, page.second_example),
        (None, None, None)
    );
}

#[test]
fn a_picture_of_three_sentences_is_rejected() {
    assert_eq!(
        code(&page("picture", json!("One. Two. Three."))),
        "teach-picture"
    );
}

#[test]
fn a_picture_over_320_characters_is_rejected() {
    assert_eq!(
        code(&page("picture", json!("a".repeat(321)))),
        "teach-picture"
    );
}

#[test]
fn a_decimal_point_does_not_end_a_sentence() {
    let examples = exemplars();
    let body = page(
        "picture",
        json!("A tile costs 3.5 coins and 2.5 more for glue."),
    );
    assert!(gate_teach(&body, &spec(&examples)).is_ok());
}

#[test]
fn a_picture_that_names_a_served_answer_is_rejected() {
    let examples = exemplars();
    let served = spec_with_instances(&examples, &INSTANCES);
    let error = gate_teach(
        &page(
            "picture",
            json!("Eight tiles by eight tiles make 64 tiles."),
        ),
        &served,
    )
    .unwrap_err();
    assert_eq!(error.code, "teach-picture-answer");
    let body = page("picture", json!("Tiles in rows make a square."));
    assert!(gate_teach(&body, &served).is_ok());
}

#[test]
fn a_watch_out_that_repeats_the_concept_is_rejected() {
    let body = page(
        "watch_out",
        json!("squaring a number multiplies it by itself. "),
    );
    assert_eq!(code(&body), "teach-watch-out");
}

#[test]
fn a_watch_out_that_names_a_served_answer_is_rejected() {
    let examples = exemplars();
    let served = spec_with_instances(&examples, &INSTANCES);
    let error = gate_teach(
        &page("watch_out", json!("Do not confuse 81 with 18.")),
        &served,
    )
    .unwrap_err();
    assert_eq!(error.code, "teach-watch-out-answer");
}

#[test]
fn a_second_example_that_repeats_the_first_problem_is_rejected() {
    let body = page(
        "second_example",
        second("Compute $6^2$.", json!(["Read it."])),
    );
    assert_eq!(code(&body), "teach-second-example");
}

#[test]
fn a_second_example_on_an_exemplar_problem_is_rejected() {
    let body = page(
        "second_example",
        second("Compute $7^2$.", json!(["Read it."])),
    );
    assert_eq!(code(&body), "teach-worked-example");
}

#[test]
fn a_second_example_that_solves_a_served_problem_is_rejected() {
    let examples = exemplars();
    let served = spec_with_instances(&examples, &INSTANCES);
    let body = page(
        "second_example",
        second(
            "A tile grid is shown.",
            json!(["Compute $8^2$ and read: $8^2 = 64$."]),
        ),
    );
    assert_eq!(gate_teach(&body, &served).unwrap_err().code, "teach-answer");
}

#[test]
fn a_second_example_of_five_steps_is_rejected() {
    let body = page(
        "second_example",
        second("A room is shown.", json!(["a", "b", "c", "d", "e"])),
    );
    assert_eq!(code(&body), "teach-second-example");
}

#[test]
fn a_spare_field_in_the_second_example_is_rejected() {
    let body = page(
        "second_example",
        json!({"problem": "A room.", "steps": ["a"], "extra": 1}),
    );
    let examples = exemplars();
    let error = gate_teach(&body, &spec(&examples)).unwrap_err();
    assert_eq!(error.code, "teach-unknown-field");
    assert!(error.message.contains("second example"));
}
