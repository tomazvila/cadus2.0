#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The figure of a teach page: one index into the `visuals` of the knowledge point.
mod common;
use cadus_core::curriculum::Exemplar;
use cadus_core::instruction::{InstructionSpec, ServedInstance, gate_teach};
use cadus_core::visual::VisualSpec;
use common::instruction_fixtures::{GOOD_TEACH, exemplars};
use serde_json::{Value, json};

fn spec_of<'a>(
    exemplars: &'a [Exemplar],
    visuals: &'a [VisualSpec],
    served: &[(&str, &str)],
) -> InstructionSpec<'a> {
    InstructionSpec {
        exemplars,
        instance_answers: served
            .iter()
            .map(|(problem, answer)| ServedInstance {
                problem: (*problem).to_owned(),
                answer: (*answer).to_owned(),
            })
            .collect(),
        visuals,
    }
}

fn line(caption: &str) -> VisualSpec {
    serde_json::from_value(json!({
        "kind": "number_line", "min": "-10", "max": "10", "tick": "1",
        "points": [{"at": "6", "filled": true}],
        "caption": caption,
    }))
    .unwrap()
}

fn plane(label: &str) -> VisualSpec {
    serde_json::from_value(json!({
        "kind": "coordinate", "x_min": "-4", "x_max": "6", "y_min": "0", "y_max": "4",
        "x_tick": "1", "y_tick": "1",
        "points": [{"x": "2", "y": "2", "label": label}],
        "caption": "A coordinate grid with one labelled point.",
    }))
    .unwrap()
}

fn page(figure: Option<Value>) -> String {
    let mut body: Value = serde_json::from_str(GOOD_TEACH).unwrap();
    if let Some(figure) = figure {
        body["figure"] = figure;
    }
    body.to_string()
}

fn code(body: &str, spec: &InstructionSpec<'_>) -> &'static str {
    gate_teach(body, spec).unwrap_err().code
}

#[test]
fn a_reference_figure_the_example_states_is_accepted() {
    let examples = exemplars();
    let visuals = [line("A number line with a filled point at 6.")];
    let spec = spec_of(&examples, &visuals, &[]);
    let page = gate_teach(&page(Some(json!(0))), &spec).unwrap();
    assert_eq!(page.figure, Some(0));
    assert!(
        serde_json::to_string(&page)
            .unwrap()
            .contains("\"figure\":0")
    );
}

#[test]
fn a_page_without_figure_keeps_the_bytes_of_today() {
    let examples = exemplars();
    let visuals = [line("A number line with a filled point at 6.")];
    let spec = spec_of(&examples, &visuals, &[]);
    let page = gate_teach(&page(None), &spec).unwrap();
    assert_eq!(page.figure, None);
    let expected = r#"{"concept":"Squaring a number multiplies it by itself.","worked_example":{"problem":"Compute $6^2$.","steps":["Write $6^2$ as $6 \\times 6$.","Multiply: $6 \\times 6 = 36$."]}}"#;
    assert_eq!(serde_json::to_string(&page).unwrap(), expected);
}

#[test]
fn a_figure_that_is_not_an_index_into_the_list_is_refused() {
    let examples = exemplars();
    let visuals = [line("c"), line("d")];
    let spec = spec_of(&examples, &visuals, &[]);
    for bad in [json!(5), json!(-1), json!("0"), json!(1.5), json!(null)] {
        assert_eq!(code(&page(Some(bad)), &spec), "teach-figure");
    }
}

#[test]
fn a_figure_that_fails_validate_is_refused() {
    let examples = exemplars();
    let broken: VisualSpec = serde_json::from_value(json!({
        "kind": "number_line", "min": "5", "max": "-5", "tick": "1",
        "points": [{"at": "6", "filled": true}],
    }))
    .unwrap();
    let visuals = [broken];
    let spec = spec_of(&examples, &visuals, &[]);
    let error = gate_teach(&page(Some(json!(0))), &spec).unwrap_err();
    assert_eq!(error.code, "teach-figure");
    assert!(error.message.contains("does not hold"), "{}", error.message);
}

#[test]
fn a_figure_that_an_exemplar_names_is_refused() {
    let mut examples = exemplars();
    examples[0].visual = Some(0);
    let visuals = [line("A number line with a filled point at 6.")];
    let spec = spec_of(&examples, &visuals, &[]);
    let error = gate_teach(&page(Some(json!(0))), &spec).unwrap_err();
    assert_eq!(error.code, "teach-figure");
    assert!(error.message.contains("serves"), "{}", error.message);
}

#[test]
fn a_figure_label_the_example_never_states_is_refused() {
    let examples = exemplars();
    let visuals = [plane("B")];
    let spec = spec_of(&examples, &visuals, &[]);
    let error = gate_teach(&page(Some(json!(0))), &spec).unwrap_err();
    assert_eq!(error.code, "teach-figure");
    assert!(error.message.contains('B'), "{}", error.message);
}

#[test]
fn a_figure_label_that_is_a_quantity_of_the_worked_problem_is_refused() {
    let examples = exemplars();
    let visuals = [plane("h")];
    let spec = spec_of(&examples, &visuals, &[]);
    let body = json!({
        "concept": "The area of a triangle is half the base times the height.",
        "worked_example": {
            "problem": "A triangle has height $h = 3$ and base 4. Find the area.",
            "steps": ["Half of 4 times $h$ is 6."],
        },
        "figure": 0,
    })
    .to_string();
    let error = gate_teach(&body, &spec).unwrap_err();
    assert_eq!(error.code, "teach-figure");
    assert!(error.message.contains("quantity"), "{}", error.message);
}

#[test]
fn a_caption_that_solves_a_served_problem_is_refused() {
    let examples = exemplars();
    let visuals = [line("Note: (8^2) equals 64")];
    let served = [("Compute $8^2$.", "64")];
    let spec = spec_of(&examples, &visuals, &served);
    let error = gate_teach(&page(Some(json!(0))), &spec).unwrap_err();
    assert_eq!(error.code, "teach-figure");
    assert!(error.message.contains("gives away"), "{}", error.message);
    assert!(gate_teach(&page(None), &spec).is_ok());
}

#[test]
fn the_older_rules_still_hold_with_a_figure_on_the_list() {
    let examples = exemplars();
    let visuals = [line("A number line with a filled point at 6.")];
    let spec = spec_of(&examples, &visuals, &[("Compute $9^2$.", "81")]);
    let unknown =
        json!({"concept": "c", "extra": 1, "worked_example": {"problem": "p", "steps": ["s"]}});
    assert_eq!(code(&unknown.to_string(), &spec), "teach-unknown-field");
    let same =
        json!({"concept": "c", "worked_example": {"problem": "Compute $7^2$.", "steps": ["49"]}});
    assert_eq!(code(&same.to_string(), &spec), "teach-worked-example");
    let last = json!({"concept": "c", "worked_example": {"problem": "Compute $6^2$.", "steps": ["Likewise $9^2$ is 81."]}});
    assert_eq!(code(&last.to_string(), &spec), "teach-answer");
}
