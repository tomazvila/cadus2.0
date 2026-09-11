//! Placement evidence excludes ungraded responses and preserves legacy rows.
#![allow(clippy::unwrap_used)]
mod common;
use cadus_core::event::Event;
use cadus_core::projector::project;
use common::events::input;

#[test]
fn legacy_diagnostic_rows_keep_their_original_fields() {
    let text = r#"{"correct":true,"secs":3,"session":"s","topic":"x","ts":"2026-03-02T09:00:00Z","type":"diagnostic_answer","v":1,"weight":1.0}"#;
    let event = Event::from_json(text).unwrap();
    assert_eq!(event.to_canonical_json().unwrap(), text);
}

#[test]
fn an_ungraded_diagnostic_has_no_effect_on_projected_placement() {
    let correct = Event::from_json(r#"{"type":"diagnostic_answer","ts":"2026-07-14T12:00:00Z","topic":"absolute-value","correct":true,"secs":5,"weight":1.0}"#).unwrap();
    let ungraded = Event::from_json(r#"{"type":"diagnostic_answer","ts":"2026-07-14T12:01:00Z","topic":"absolute-value","correct":true,"outcome":{"ungraded":{"reason":"unsupported notation"}},"secs":5,"weight":1.0}"#).unwrap();
    let placed = Event::from_json(r#"{"type":"diagnostic_placed","ts":"2026-07-14T12:02:00Z","balances":{"absolute-value":1.0},"conditional":["absolute-value"]}"#).unwrap();
    let original = project(&[correct.clone(), placed.clone()], &input()).unwrap();
    let after = project(&[correct, ungraded.clone(), placed], &input()).unwrap();
    assert_eq!(after.topics, original.topics);
    let wire: serde_json::Value =
        serde_json::from_str(&ungraded.to_canonical_json().unwrap()).unwrap();
    assert_eq!(wire["correct"], false);
    assert_eq!(wire["weight"], 0.0);
}
