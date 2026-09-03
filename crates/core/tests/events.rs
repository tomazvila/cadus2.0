//! M3 U1, part 1: the event wire shapes, the envelope, and the schema checks.
//!
//! Every expected value here is a LITERAL taken from a committed 1.0 oracle artifact
//! or from the port specification. Nothing is re-derived from the code under test:
//!
//! - `tests/fixtures/events/stream_1.jsonl` is the 47-event oracle stream. Its lines
//!   are already canonical JSON, so a byte comparison against the raw line IS the
//!   round-trip check.
//! - `tests/fixtures/events/one_per_type.jsonl` holds one full instance of each of
//!   the 16 types, built by `scripts/oracle/gen_one_per_type_1_0.py` through the 1.0
//!   pydantic models. `docs/reference/event-schemas-1.0.json` carries no `examples`,
//!   so this file is the per-type instance the acceptance check names.
//! - `tests/fixtures/events/model_1.json` is the 1.0 fold of `stream_1.jsonl`. Its
//!   canonical blob hashes to the digest the M3 plan pins.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::collections::BTreeSet;

use cadus_core::event::{Event, EventError, Slug, TaskType, Timestamp};

/// The 47-event oracle stream, in canonical JSON, one event per line.
const STREAM_1: &str = include_str!("fixtures/events/stream_1.jsonl");

/// One full instance of each of the 16 event types, in canonical JSON.
const ONE_PER_TYPE: &str = include_str!("fixtures/events/one_per_type.jsonl");

/// Read every line of a fixture, check that it writes its own bytes back, and
/// return the events in file order.
fn round_trip(fixture: &str) -> Vec<Event> {
    fixture
        .lines()
        .enumerate()
        .map(|(index, line)| {
            let event = Event::from_json(line)
                .unwrap_or_else(|error| panic!("line {index} does not parse: {error}"));
            assert_eq!(
                event.to_canonical_json().unwrap(),
                line,
                "line {index} does not re-serialize to its own bytes"
            );
            event
        })
        .collect()
}

// --------------------------------------------------------------------------- //
// Round-trip: every line of the oracle stream
// --------------------------------------------------------------------------- //

#[test]
fn every_line_of_stream_1_round_trips_byte_for_byte() {
    let events = round_trip(STREAM_1);
    assert_eq!(events.len(), 47, "the oracle stream holds 47 events");
}

#[test]
fn stream_1_holds_the_event_counts_the_specification_records() {
    // Spec section 9, "Stream #1 coverage".
    let mut counts = std::collections::BTreeMap::new();
    for line in STREAM_1.lines() {
        let event = Event::from_json(line).unwrap();
        *counts.entry(event.type_name()).or_insert(0_i32) += 1;
    }
    assert_eq!(counts.get("enrolled"), Some(&1));
    assert_eq!(counts.get("session_start"), Some(&2));
    assert_eq!(counts.get("session_end"), Some(&2));
    assert_eq!(counts.get("task_served"), Some(&10));
    assert_eq!(counts.get("attempt"), Some(&20));
    assert_eq!(counts.get("lesson_result"), Some(&5));
    assert_eq!(counts.get("review_result"), Some(&5));
    assert_eq!(counts.get("remediation_triggered"), Some(&1));
    assert_eq!(counts.get("regraded"), Some(&1));
}

// --------------------------------------------------------------------------- //
// Round-trip: one instance of every type
// --------------------------------------------------------------------------- //

#[test]
fn one_instance_of_every_type_round_trips_byte_for_byte() {
    let seen: BTreeSet<&str> = round_trip(ONE_PER_TYPE)
        .iter()
        .map(Event::type_name)
        .collect();
    let expected: BTreeSet<&str> = Event::TYPE_NAMES.into_iter().collect();
    assert_eq!(seen, expected, "the fixture must cover all 16 types");
}

#[test]
fn the_union_declares_exactly_the_sixteen_1_0_types() {
    // Spec section 2: the 16 types of the 1.0 event union.
    assert_eq!(
        Event::TYPE_NAMES,
        [
            "session_start",
            "session_end",
            "enrolled",
            "task_served",
            "attempt",
            "lesson_result",
            "review_result",
            "quiz_result",
            "remediation_triggered",
            "diagnostic_answer",
            "diagnostic_placed",
            "profile_reset",
            "regraded",
            "anki_card_created",
            "config_changed",
            "curriculum_changed",
        ]
    );
    let unique: BTreeSet<&str> = Event::TYPE_NAMES.into_iter().collect();
    assert_eq!(unique.len(), 16);
}

// --------------------------------------------------------------------------- //
// The envelope
// --------------------------------------------------------------------------- //

#[test]
fn a_timestamp_with_microseconds_writes_six_fractional_digits() {
    // 1.0 writes `2026-05-04T13:45:06.123456Z` for that instant, and
    // `2026-05-04T13:45:06Z` when the microsecond part is zero.
    let with_micros = Timestamp::parse("2026-05-04T13:45:06.123456Z").unwrap();
    assert_eq!(with_micros.micros(), 1_777_902_306_123_456);
    assert_eq!(
        with_micros.to_wire_string().unwrap(),
        "2026-05-04T13:45:06.123456Z"
    );

    let whole = Timestamp::parse("2026-05-04T13:45:06Z").unwrap();
    assert_eq!(whole.micros(), 1_777_902_306_000_000);
    assert_eq!(whole.to_wire_string().unwrap(), "2026-05-04T13:45:06Z");

    // A trailing zero in the fractional part still writes six digits.
    let tenth = Timestamp::parse("2026-05-04T13:45:06.100000Z").unwrap();
    assert_eq!(tenth.micros(), 1_777_902_306_100_000);
    assert_eq!(
        tenth.to_wire_string().unwrap(),
        "2026-05-04T13:45:06.100000Z"
    );
}

#[test]
fn a_naive_timestamp_is_read_as_utc() {
    // Spec trap T8: a naive `ts` is silently read as UTC.
    let naive = Timestamp::parse("2026-03-02T09:00:00").unwrap();
    let zulu = Timestamp::parse("2026-03-02T09:00:00Z").unwrap();
    assert_eq!(naive, zulu);
    assert_eq!(naive.micros(), 1_772_442_000_000_000);
    // 2.0 normalizes the naive form to the `Z` form on the way out.
    assert_eq!(naive.to_wire_string().unwrap(), "2026-03-02T09:00:00Z");
}

#[test]
fn an_offset_timestamp_is_converted_to_utc() {
    let offset = Timestamp::parse("2026-03-02T11:00:00+02:00").unwrap();
    assert_eq!(offset.micros(), 1_772_442_000_000_000);
    assert_eq!(offset.to_wire_string().unwrap(), "2026-03-02T09:00:00Z");
    let epoch = Timestamp::parse("1970-01-01T00:00:00Z").unwrap();
    assert_eq!(epoch.micros(), 0);
}

#[test]
fn a_malformed_timestamp_is_an_error_value_not_a_panic() {
    assert_eq!(
        Timestamp::parse("not a date").unwrap_err(),
        EventError::InvalidTimestamp("not a date".to_owned())
    );
    assert!(Timestamp::parse("").is_err());
    assert!(Timestamp::parse("2026-13-45T99:99:99Z").is_err());
    let text = r#"{"type":"session_start","ts":"not a date","session":null,"v":1}"#;
    assert!(Event::from_json(text).is_err());
}

#[test]
fn a_version_other_than_one_is_an_error() {
    // 1.0 holds an empty shim table, so only v1 reads. `v: 0` raises there.
    for version in ["0", "2", "-1"] {
        let text = format!(
            r#"{{"type":"session_start","ts":"2026-03-02T09:00:00Z","session":null,"v":{version}}}"#
        );
        let error = Event::from_json(&text).unwrap_err();
        assert!(
            error.to_string().contains(&format!("v{version}")),
            "the error must name the rejected version, got: {error}"
        );
    }
}

#[test]
fn a_missing_version_defaults_to_one_and_writes_back_as_one() {
    let text = r#"{"type":"session_start","ts":"2026-03-02T09:00:00Z"}"#;
    let event = Event::from_json(text).unwrap();
    assert_eq!(event.v().get(), 1);
    assert_eq!(
        event.to_canonical_json().unwrap(),
        r#"{"session":null,"ts":"2026-03-02T09:00:00Z","type":"session_start","v":1}"#
    );
}

#[test]
fn the_envelope_accessors_read_every_member() {
    for line in ONE_PER_TYPE.lines() {
        let event = Event::from_json(line).unwrap();
        assert_eq!(event.v().get(), 1);
        assert_eq!(event.session(), Some("s_2026-05-04a"));
        assert!(event.ts().micros() > 0);
        assert!(Event::TYPE_NAMES.contains(&event.type_name()));
    }
}

// --------------------------------------------------------------------------- //
// Schema validation: every rejection 1.0 makes
// --------------------------------------------------------------------------- //

#[test]
fn an_unknown_key_is_rejected_on_every_type() {
    // 1.0 declares `extra="forbid"` on every model.
    for line in ONE_PER_TYPE.lines() {
        let mut value: serde_json::Value = serde_json::from_str(line).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("no_such_key".to_owned(), serde_json::Value::from(1));
        let text = serde_json::to_string(&value).unwrap();
        let error = Event::from_json(&text).unwrap_err();
        assert!(
            error.to_string().contains("no_such_key"),
            "an unknown key must be named in the error, got: {error}"
        );
    }
}

#[test]
fn an_unknown_key_inside_a_nested_body_is_rejected() {
    let text = r#"{"type":"quiz_result","ts":"2026-03-02T09:00:00Z","quiz_id":"q","score":1.0,"per_topic":[{"topic":"a","correct":true,"secs":1,"nope":2}]}"#;
    assert!(Event::from_json(text).is_err());
}

#[test]
fn an_unknown_type_is_rejected() {
    let text = r#"{"type":"no_such_event","ts":"2026-03-02T09:00:00Z"}"#;
    assert!(Event::from_json(text).is_err());
    let text = r#"{"ts":"2026-03-02T09:00:00Z"}"#;
    assert!(Event::from_json(text).is_err());
}

#[test]
fn a_missing_required_field_is_rejected() {
    // 1.0 requires `attempt_id` on an attempt (`tests/test_model.py:211-224`).
    let complete = ONE_PER_TYPE
        .lines()
        .find(|line| line.contains(r#""type":"attempt""#))
        .unwrap();
    assert!(Event::from_json(complete).is_ok());
    for required in [
        "attempt_id",
        "task_id",
        "topic",
        "correct",
        "secs",
        "work_quality",
    ] {
        let mut value: serde_json::Value = serde_json::from_str(complete).unwrap();
        value.as_object_mut().unwrap().remove(required);
        let text = serde_json::to_string(&value).unwrap();
        assert!(
            Event::from_json(&text).is_err(),
            "an attempt without `{required}` must be rejected"
        );
    }
}

#[test]
fn a_bad_enumeration_value_is_rejected() {
    // 1.0 rejects a bad `work_quality` (`tests/test_model.py:211-224`).
    let text = r#"{"type":"lesson_result","ts":"2026-03-02T09:00:00Z","topic":"a","passed":true,"quality_tier":"excellent"}"#;
    assert!(Event::from_json(text).is_err());
    let text = r#"{"type":"task_served","ts":"2026-03-02T09:00:00Z","task_id":"t","task_type":"homework"}"#;
    assert!(Event::from_json(text).is_err());
}

#[test]
fn an_out_of_range_number_is_rejected() {
    // 1.0 declares `secs >= 0`, `weight` in 0.0 to 1.0, and `expected_time_secs > 0`
    // (`tests/test_model.py:227-239`).
    let text = r#"{"type":"diagnostic_answer","ts":"2026-03-02T09:00:00Z","topic":"a","correct":true,"secs":-1,"weight":0.5}"#;
    assert!(Event::from_json(text).is_err());
    let text = r#"{"type":"diagnostic_answer","ts":"2026-03-02T09:00:00Z","topic":"a","correct":true,"secs":1,"weight":1.5}"#;
    assert!(Event::from_json(text).is_err());
    let text = r#"{"type":"task_served","ts":"2026-03-02T09:00:00Z","task_id":"t","task_type":"lesson","problems":[{"id":"p","text_hash":"h","expected_time_secs":0}]}"#;
    assert!(Event::from_json(text).is_err());
}

#[test]
fn an_empty_curriculum_id_is_rejected() {
    // 1.0 declares `min_length=1` on every slug.
    let text = r#"{"type":"enrolled","ts":"2026-03-02T09:00:00Z","course":""}"#;
    assert!(Event::from_json(text).is_err());
    assert!(Slug::new("").is_err());
    assert_eq!(
        Slug::new("absolute-value").unwrap().as_str(),
        "absolute-value"
    );
}

#[test]
fn a_scalar_of_the_wrong_json_type_is_rejected() {
    // Each checked scalar reads its raw wire value first, so a value of another
    // JSON type is an error before the range check runs.
    for text in [
        r#"{"type":"session_start","ts":5}"#,
        r#"{"type":"session_start","ts":"2026-03-02T09:00:00Z","v":"one"}"#,
        r#"{"type":"enrolled","ts":"2026-03-02T09:00:00Z","course":7}"#,
        r#"{"type":"diagnostic_answer","ts":"2026-03-02T09:00:00Z","topic":"a","correct":true,"secs":"9","weight":0.5}"#,
        r#"{"type":"diagnostic_answer","ts":"2026-03-02T09:00:00Z","topic":"a","correct":true,"secs":9,"weight":"half"}"#,
        r#"{"type":"task_served","ts":"2026-03-02T09:00:00Z","task_id":"t","task_type":"lesson","problems":[{"id":"p","text_hash":"h","expected_time_secs":"9"}]}"#,
    ] {
        assert!(Event::from_json(text).is_err(), "{text} must be rejected");
    }
}

#[test]
fn an_instant_outside_the_chrono_range_writes_no_wire_form() {
    let far = Timestamp::from_micros(i64::MAX);
    assert!(far.to_wire_string().is_err());
    // `Display` falls back to the raw microseconds where the wire form fails.
    assert_eq!(far.to_string(), "9223372036854775807us");
    assert_eq!(
        Timestamp::from_micros(0).to_string(),
        "1970-01-01T00:00:00Z"
    );
    let text = r#"{"type":"session_start","ts":"2026-03-02T09:00:00Z"}"#;
    let Event::SessionStart(mut body) = Event::from_json(text).unwrap() else {
        panic!("expected a session_start event");
    };
    body.ts = far;
    let error = Event::SessionStart(body).to_canonical_json().unwrap_err();
    assert!(matches!(error, EventError::Serialize(_)), "{error}");
}

#[test]
fn a_diagnostic_task_spells_its_type() {
    let text = r#"{"type":"task_served","ts":"2026-03-02T09:00:00Z","task_id":"t","task_type":"diagnostic"}"#;
    let Event::TaskServed(body) = Event::from_json(text).unwrap() else {
        panic!("expected a task_served event");
    };
    assert_eq!(body.task_type, TaskType::Diagnostic);
    assert_eq!(body.task_type.as_str(), "diagnostic");
}
