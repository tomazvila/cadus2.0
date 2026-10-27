#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The term list of a teach page: at most six rows of a word and its meaning.
mod common;
use cadus_core::instruction::{Term, gate_teach};
use common::instruction_fixtures::{GOOD_TEACH, INSTANCES, exemplars, spec, spec_with_instances};
use serde_json::{Value, json};

fn page(terms: Value) -> String {
    let mut body: Value = serde_json::from_str(GOOD_TEACH).unwrap();
    body["terms"] = terms;
    body.to_string()
}

fn row(term: &str, meaning: &str) -> Value {
    json!({"term": term, "meaning": meaning})
}

#[test]
fn a_teach_page_with_terms_is_accepted() {
    let examples = exemplars();
    let body = page(json!([
        row("Base", "The number that is multiplied by itself."),
        row("Square", "A number multiplied by itself.")
    ]));
    let page = gate_teach(&body, &spec(&examples)).unwrap();
    assert_eq!(
        page.terms,
        vec![
            Term {
                term: "Base".to_owned(),
                meaning: "The number that is multiplied by itself.".to_owned()
            },
            Term {
                term: "Square".to_owned(),
                meaning: "A number multiplied by itself.".to_owned()
            },
        ]
    );
    let written = serde_json::to_string(&page).unwrap();
    assert!(written.contains(r#""steps":["Write $6^2$ as $6 \\times 6$.","Multiply: $6 \\times 6 = 36$."]},"terms":[{"term":"Base""#));
}

#[test]
fn a_page_without_terms_serializes_as_before() {
    let examples = exemplars();
    let page = gate_teach(GOOD_TEACH, &spec(&examples)).unwrap();
    assert!(page.terms.is_empty());
    assert!(!serde_json::to_string(&page).unwrap().contains("terms"));
    let empty = gate_teach(&page_of_empty(), &spec(&examples)).unwrap();
    assert_eq!(empty, page);
}

fn page_of_empty() -> String {
    page(json!([]))
}

#[test]
fn a_seventh_term_is_rejected() {
    let examples = exemplars();
    let rows: Vec<Value> = (0..7)
        .map(|n| row(&format!("Word {n}"), "A plain word."))
        .collect();
    let error = gate_teach(&page(json!(rows)), &spec(&examples)).unwrap_err();
    assert_eq!(error.code, "teach-terms");
}

#[test]
fn a_terms_value_that_is_not_a_list_is_rejected() {
    let examples = exemplars();
    let error = gate_teach(&page(json!("Base")), &spec(&examples)).unwrap_err();
    assert_eq!(error.code, "teach-terms");
}

#[test]
fn a_term_row_with_an_empty_meaning_is_rejected() {
    let examples = exemplars();
    let error = gate_teach(&page(json!([row("Base", " ")])), &spec(&examples)).unwrap_err();
    assert_eq!(error.code, "teach-term");
}

#[test]
fn a_meaning_of_201_characters_is_rejected() {
    let examples = exemplars();
    let long = "a".repeat(201);
    let error = gate_teach(&page(json!([row("Base", &long)])), &spec(&examples)).unwrap_err();
    assert_eq!(error.code, "teach-term");
}

#[test]
fn a_repeated_term_is_rejected() {
    let examples = exemplars();
    let body = page(json!([
        row("Square", "A product."),
        row("square ", "Again.")
    ]));
    let error = gate_teach(&body, &spec(&examples)).unwrap_err();
    assert_eq!(error.code, "teach-term-repeat");
}

#[test]
fn a_meaning_that_names_a_served_answer_is_rejected() {
    let examples = exemplars();
    let served = spec_with_instances(&examples, &INSTANCES);
    let error =
        gate_teach(&page(json!([row("Square", "The result is 64.")])), &served).unwrap_err();
    assert_eq!(error.code, "teach-term-answer");
    let body = page(json!([row("Square", "The number multiplied by itself.")]));
    assert!(gate_teach(&body, &served).is_ok());
}

#[test]
fn a_term_row_with_a_spare_field_is_rejected() {
    let examples = exemplars();
    let body = page(json!([{"term": "Base", "meaning": "A number.", "extra": 1}]));
    let error = gate_teach(&body, &spec(&examples)).unwrap_err();
    assert_eq!(error.code, "teach-unknown-field");
    assert!(error.message.contains("term row"));
}
