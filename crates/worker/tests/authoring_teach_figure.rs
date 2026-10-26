//! Brief 09 part A: the teach prompt offers the reference figures of a knowledge
//! point, the schema takes an optional `figure`, and the teach check keeps the index.
//!
//! The digest test rebuilds the teach material as it was before the figure, so
//! the old value is a literal of the old text and not a copy of the new code.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_core::visual::VisualSpec;
use cadus_worker::authoring::job::verify_teach;
use cadus_worker::authoring::prompt::{
    Kind, digest_material, digest_of, prompt_digest, tool_schema, user_message,
};
use common::{squares_spec, teach_arguments};
use serde_json::{Value, json};

fn figure(caption: &str) -> VisualSpec {
    serde_json::from_value(json!({
        "kind": "number_line", "min": "-10", "max": "10", "tick": "1",
        "points": [{"at": "6", "filled": true}],
        "caption": caption,
    }))
    .unwrap()
}

#[test]
fn the_teach_message_lists_each_reference_figure() {
    let mut spec = squares_spec();
    spec.visuals = vec![figure("A number line with a filled point at 6.")];
    let message = user_message(Kind::Teach, &spec, None);
    assert!(message.contains("Reference figures"), "{message}");
    assert!(message.contains("Figure 0 (number_line): A number line with a filled point at 6."));
    assert!(message.contains(&format!(
        "Text description: {}",
        spec.visuals[0].text_equivalent()
    )));
    for kind in [Kind::Template, Kind::HintLadder, Kind::Diagnosis] {
        assert!(!user_message(kind, &spec, None).contains("Reference figures"));
    }
}

#[test]
fn a_message_with_no_reference_figure_is_the_message_of_today() {
    let plain = squares_spec();
    let before = user_message(Kind::Teach, &plain, None);
    assert!(!before.contains("Figure "));
    // A figure that an exemplar names is not a reference figure.
    let mut named = squares_spec();
    named.visuals = vec![figure("A number line.")];
    named.exemplars[0].visual = Some(0);
    assert_eq!(user_message(Kind::Teach, &named, None), before);
    assert!(before.ends_with(Kind::Teach.ask()));
}

#[test]
fn the_schema_has_an_optional_figure() {
    let schema = tool_schema(Kind::Teach);
    assert_eq!(
        schema["properties"]["figure"],
        json!({
            "type": "integer",
            "minimum": 0,
            "description": schema["properties"]["figure"]["description"],
        })
    );
    assert_eq!(schema["required"], json!(["concept", "worked_example"]));
    for kind in [Kind::HintLadder, Kind::Diagnosis] {
        assert!(tool_schema(kind)["properties"].get("figure").is_none());
    }
}

/// The teach digest material as it was before the figure.
fn old_teach_material() -> Value {
    let mut material = digest_material(Kind::Teach);
    let paragraph = "The user message lists reference figures when the knowledge point has some. \
If one figure fits this knowledge point, set 'figure' to its number. Then write the worked \
example about that figure. Use its labels and its numbers. If no figure fits, leave 'figure' \
out. Never describe a figure in the steps.\n\n";
    let system = material["system"].as_str().unwrap().replace(paragraph, "");
    material["system"] = Value::String(system);
    material["schema"]["properties"]
        .as_object_mut()
        .unwrap()
        .remove("figure");
    material
}

#[test]
fn the_teach_digest_moves_and_the_other_kinds_keep_theirs() {
    let before = digest_of(&old_teach_material());
    assert_eq!(before, "dfc6329034bf030f");
    assert_ne!(before, prompt_digest(Kind::Teach));
    assert_eq!(prompt_digest(Kind::Template), "c0ef8977d579adc8");
    assert_eq!(prompt_digest(Kind::HintLadder), "c8e040f53fce1881");
    assert_eq!(prompt_digest(Kind::Diagnosis), "164baef458d55e4f");
}

#[test]
fn verify_teach_writes_the_figure_index_into_the_body() {
    let mut spec = squares_spec();
    spec.visuals = vec![figure("A number line with a filled point at 6.")];
    let mut arguments = teach_arguments();
    arguments["figure"] = json!(0);
    let body = verify_teach(&spec, &arguments, &[]).unwrap();
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["figure"], json!(0));
    let plain = verify_teach(&spec, &teach_arguments(), &[]).unwrap();
    assert!(!plain.contains("figure"));
    arguments["figure"] = json!(3);
    assert_eq!(
        verify_teach(&spec, &arguments, &[]).unwrap_err().code,
        "teach-figure"
    );
}
