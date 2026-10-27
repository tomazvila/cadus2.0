//! D-TM1 and D-TD1: the stored teach body keeps the optional parts that the check accepts.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_worker::authoring::job::verify_kind;
use cadus_worker::authoring::prompt::Kind;
use common::{squares_spec, teach_arguments};
use serde_json::{Value, json};

#[test]
fn verify_teach_keeps_the_terms_and_the_three_depth_parts() {
    let mut arguments = teach_arguments();
    arguments["terms"] =
        json!([{"term": "Base", "meaning": "The number that is multiplied by itself."}]);
    arguments["picture"] = json!("A square tile has the same length on each side.");
    arguments["watch_out"] = json!("The rule needs the same number twice.");
    arguments["second_example"] = json!({
        "problem": "A square room has an area of $25$ square meters.",
        "steps": ["The area is a side times a side.", "So each side is $5$ meters."]
    });
    let body = verify_kind(Kind::Teach, &squares_spec(), &arguments, &[]).unwrap();
    let stored: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(stored["terms"][0]["term"], "Base");
    assert_eq!(stored["picture"], arguments["picture"]);
    assert_eq!(stored["watch_out"], arguments["watch_out"]);
    assert_eq!(stored["second_example"], arguments["second_example"]);
}

#[test]
fn a_teach_page_without_the_parts_stores_none_of_the_keys() {
    let body = verify_kind(Kind::Teach, &squares_spec(), &teach_arguments(), &[]).unwrap();
    let stored: Value = serde_json::from_str(&body).unwrap();
    for key in ["terms", "picture", "watch_out", "second_example"] {
        assert!(stored.get(key).is_none(), "the body holds {key}");
    }
}
