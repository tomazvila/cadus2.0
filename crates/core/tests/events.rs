//! M3 U1: the event wire shapes and the learner-model wire shape.
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

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::OnceLock;

use cadus_core::config::Config;
use cadus_core::curriculum::{Curriculum, load_curriculum};
use cadus_core::event::{Event, EventError, Slug, Timestamp, TopicStatus};
use cadus_core::learner::{LearnerModel, TopicState};
use cadus_core::projector::{ProjectionInput, ProjectorError, blob_digest, project};

/// The 47-event oracle stream, in canonical JSON, one event per line.
const STREAM_1: &str = include_str!("fixtures/events/stream_1.jsonl");

/// One full instance of each of the 16 event types, in canonical JSON.
const ONE_PER_TYPE: &str = include_str!("fixtures/events/one_per_type.jsonl");

/// The 1.0 fold of `stream_1.jsonl`, as a canonical model blob with no newline.
const MODEL_1: &str = include_str!("fixtures/events/model_1.json");

/// The digest the M3 plan pins for the fold of `stream_1.jsonl`.
const MODEL_1_DIGEST: &str = "ba128459985e0815db7446cb2af16452ec07d304b7efaa0952fc6567404245f5";

/// The build instant the 1.0 oracle pins with `--now`.
const NOW: &str = "2000-01-01T00:00:00Z";

/// The daily XP goal the 1.0 oracle folds with.
const GOAL: i64 = 40;

/// The two-event stream of review finding #3, with a clean topic id.
const CLEAN_STREAM: &str = concat!(
    r#"{"course":"foundations","ts":"2026-03-01T09:00:00Z","type":"enrolled","v":1}"#,
    "\n",
    r#"{"assisted":false,"passed":true,"quality_tier":"perfect","topic":"absolute-value","ts":"2026-03-02T09:00:00Z","type":"lesson_result","v":1,"xp":10.0}"#,
);

/// The same stream with the topic id padded, which 1.0 strips.
const PADDED_STREAM: &str = concat!(
    r#"{"course":"foundations","ts":"2026-03-01T09:00:00Z","type":"enrolled","v":1}"#,
    "\n",
    r#"{"assisted":false,"passed":true,"quality_tier":"perfect","topic":" absolute-value ","ts":"2026-03-02T09:00:00Z","type":"lesson_result","v":1,"xp":10.0}"#,
);

/// The three-event stream of review finding #11: a lesson stamped 2060 and a review
/// of the same topic stamped 2026, so the decay exponent is about -2759.
const OVERFLOW_STREAM: &str = concat!(
    r#"{"course":"foundations","ts":"2026-01-01T09:00:00Z","type":"enrolled","v":1}"#,
    "\n",
    r#"{"assisted":false,"passed":true,"quality_tier":"perfect","topic":"absolute-value-inequalities","ts":"2060-01-01T09:00:00Z","type":"lesson_result","v":1,"xp":10.0}"#,
    "\n",
    r#"{"assisted":false,"passed":true,"quality_tier":"perfect","task_id":"t-1","topic":"absolute-value-inequalities","ts":"2026-01-02T09:00:00Z","type":"review_result","v":1,"weighted_score":1.0,"xp":5.0}"#,
);

/// The one-event stream of review finding #12: an XP value the `i64` range misses.
const HUGE_XP_STREAM: &str = r#"{"assisted":false,"passed":true,"quality_tier":"nearly_perfect","topic":"adding-subtracting-rational-expressions","ts":"2027-04-15T17:00:00Z","type":"review_result","v":1,"weighted_score":0.0,"xp":-1e+308}"#;

/// The checked-in curriculum tree, loaded once for the whole test binary.
fn tree() -> &'static Curriculum {
    static TREE: OnceLock<Curriculum> = OnceLock::new();
    TREE.get_or_init(|| {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let (curriculum, _findings) =
            load_curriculum(&root.join("curriculum")).expect("the tree loads");
        curriculum
    })
}

/// Fold a JSONL stream with the oracle defaults: UTC, `now` at [`NOW`], goal 40.
fn fold(stream: &str) -> Result<LearnerModel, ProjectorError> {
    let events: Vec<Event> = stream
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| Event::from_json(line).unwrap_or_else(|error| panic!("{line}: {error}")))
        .collect();
    let cfg = Config::default();
    let input = ProjectionInput::new(tree(), &cfg, Timestamp::parse(NOW).unwrap()).with_goal(GOAL);
    project(&events, &input)
}

/// The blob digest of a folded stream.
fn fold_digest(stream: &str) -> String {
    blob_digest(&fold(stream).expect("the fold succeeds")).unwrap()
}

/// The error a folded stream reports.
fn fold_error(stream: &str) -> ProjectorError {
    fold(stream).expect_err("the fold reports an error")
}

// --------------------------------------------------------------------------- //
// Round-trip: every line of the oracle stream
// --------------------------------------------------------------------------- //

#[test]
fn every_line_of_stream_1_round_trips_byte_for_byte() {
    let mut count = 0;
    for (index, line) in STREAM_1.lines().enumerate() {
        let event = Event::from_json(line)
            .unwrap_or_else(|error| panic!("line {index} does not parse: {error}"));
        assert_eq!(
            event.to_canonical_json().unwrap(),
            line,
            "line {index} does not re-serialize to its own bytes"
        );
        count += 1;
    }
    assert_eq!(count, 47, "the oracle stream holds 47 events");
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
    let mut seen = BTreeSet::new();
    for (index, line) in ONE_PER_TYPE.lines().enumerate() {
        let event = Event::from_json(line)
            .unwrap_or_else(|error| panic!("line {index} does not parse: {error}"));
        assert_eq!(
            event.to_canonical_json().unwrap(),
            line,
            "line {index} does not re-serialize to its own bytes"
        );
        seen.insert(event.type_name());
    }
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

// --------------------------------------------------------------------------- //
// Review 1 findings #3 and #5: the pydantic `strip_whitespace` of every Slug
// --------------------------------------------------------------------------- //

#[test]
fn a_slug_loses_its_outer_whitespace() {
    // 1.0 `model.py:23`:
    // `Slug = Annotated[str, StringConstraints(min_length=1, strip_whitespace=True)]`.
    assert_eq!(
        Slug::new(" absolute-value ").unwrap().as_str(),
        "absolute-value"
    );
    assert_eq!(
        Slug::new("absolute-value\t").unwrap().as_str(),
        "absolute-value"
    );
    assert_eq!(
        Slug::new("absolute-value\n").unwrap().as_str(),
        "absolute-value"
    );
    // The trim runs FIRST, so a whitespace-only id has nothing left and is an error,
    // the way pydantic applies `min_length=1` to the stripped value.
    assert!(Slug::new("   ").is_err());
    assert_eq!(
        Slug::new(" ").unwrap_err().to_string(),
        "event JSON is invalid: a curriculum id must not be empty"
    );
    // An id with an inner space keeps it: only the OUTER whitespace goes away.
    assert_eq!(Slug::new("two words").unwrap().as_str(), "two words");
}

#[test]
fn a_padded_topic_id_parses_to_the_real_topic() {
    let text = r#"{"assisted":false,"passed":true,"quality_tier":"perfect","topic":" absolute-value ","ts":"2026-03-02T09:00:00Z","type":"lesson_result","v":1,"xp":10.0}"#;
    let Event::LessonResult(result) = Event::from_json(text).unwrap() else {
        panic!("expected a lesson_result event");
    };
    assert_eq!(result.topic.as_str(), "absolute-value");
    // A whitespace-only id is a validation error, as it is in 1.0.
    let text = r#"{"assisted":false,"passed":true,"quality_tier":"perfect","topic":" ","ts":"2026-03-02T09:00:00Z","type":"lesson_result","v":1,"xp":10.0}"#;
    assert!(Event::from_json(text).is_err());
}

#[test]
fn the_padded_stream_folds_to_the_1_0_digest_of_the_clean_stream() {
    // Both streams below fold to this digest in 1.0, run on this box:
    //   scripts/oracle/dump_projector_1_0.py <stream> --curriculum curriculum
    // Review round 1, finding #3, names the same value.
    const DIGEST: &str = "af3cc77f069edf252691c90d0f32fcfbc6cf95d9882fdf677b3b6b256a2101e7";
    assert_eq!(fold_digest(CLEAN_STREAM), DIGEST);
    assert_eq!(fold_digest(PADDED_STREAM), DIGEST);
}

// --------------------------------------------------------------------------- //
// Review 1 findings #11 and #12: what the fold reports instead of a wrong model
// --------------------------------------------------------------------------- //

#[test]
fn a_non_finite_decay_stops_the_fold() {
    // 1.0 raises `OverflowError: (34, 'Numerical result out of range')` at
    // `cadus/fire.py:179` on this stream and builds NO model. Verified on this box
    // with scripts/oracle/dump_projector_1_0.py. The raise happens at the third
    // event (index 2), in the explicit `memory_at` of `apply_attempt`.
    let error = fold_error(OVERFLOW_STREAM);
    assert_eq!(
        error,
        ProjectorError::NonFinite {
            topic: "absolute-value-inequalities".to_owned(),
            event_index: 2,
        }
    );
    assert_eq!(
        error.to_string(),
        "the decay of topic `absolute-value-inequalities` at event 2 is not a finite number"
    );
}

#[test]
fn an_xp_total_outside_the_i64_range_stops_the_fold() {
    // A DOCUMENTED divergence (spec section 7, trap T22). 1.0 folds this one event
    // into a model whose `quiz.xp_since` is the exact 309-digit Python integer
    // `-100000000000000001097906362944045541740492309677311846336810682903...`,
    // verified on this box. An `i64` does not hold it, so the 2.0 fold reports it.
    let error = fold_error(HUGE_XP_STREAM);
    assert_eq!(
        error.to_string(),
        "the rounded value -1e+308 is outside the i64 range"
    );
    assert!(matches!(error, ProjectorError::OutOfRange(_)));
}

#[test]
fn a_diagnostic_placed_keeps_the_insertion_order_of_its_balances() {
    // Spec trap T6: 1.0 iterates `balances.items()` in dict order, and the refresh
    // path is sensitive to it. Parsing must not sort the keys.
    let text = r#"{"type":"diagnostic_placed","ts":"2026-03-02T09:00:00Z","balances":{"zebra":1.0,"alpha":2.0,"middle":3.0}}"#;
    let Event::DiagnosticPlaced(placed) = Event::from_json(text).unwrap() else {
        panic!("expected a diagnostic_placed event");
    };
    let order: Vec<&str> = placed.balances.keys().map(String::as_str).collect();
    assert_eq!(order, vec!["zebra", "alpha", "middle"]);
    assert_eq!(placed.balances.get("alpha"), Some(&2.0));
}

#[test]
fn a_regraded_attempt_carries_no_correct_field() {
    // C4: a correction restates how well the work was done, never whether the answer
    // was right (`model.py:449-452`).
    let text = r#"{"type":"regraded","ts":"2026-03-02T09:00:00Z","task_id":"t","topic":"a","reason":"r","attempts":[{"attempt_id":"a1","work_quality":"poor","correct":false}]}"#;
    assert!(Event::from_json(text).is_err());
}

#[test]
fn a_float_reads_back_to_the_same_bits() {
    // `serde_json` needs its `float_roundtrip` feature for this. Without it the
    // parser is off by one unit in the last place and the fold would diverge.
    let text = r#"{"type":"review_result","ts":"2026-03-02T09:00:00Z","topic":"a","passed":true,"weighted_score":0.36925854946048403,"quality_tier":"perfect"}"#;
    let Event::ReviewResult(result) = Event::from_json(text).unwrap() else {
        panic!("expected a review_result event");
    };
    assert_eq!(result.weighted_score.to_bits(), 0x3fd7_a1ee_9c6c_e017);
    assert_eq!(
        result.weighted_score,
        "0.36925854946048403".parse::<f64>().unwrap()
    );
}

// --------------------------------------------------------------------------- //
// The learner model
// --------------------------------------------------------------------------- //

#[test]
fn the_oracle_model_round_trips_to_its_pinned_digest() {
    // The M3 acceptance digest: `stream_1.jsonl` folds to this blob. U1 does not
    // fold; it pins that the MODEL SHAPE writes those exact bytes.
    let model = LearnerModel::from_json(MODEL_1).unwrap();
    assert_eq!(model.parity_blob().unwrap(), MODEL_1.trim_end());
    assert_eq!(model.parity_digest().unwrap(), MODEL_1_DIGEST);
    assert_eq!(model.topics.len(), 47);
    assert_eq!(model.projector_version, Some(3));
    assert_eq!(model.config_hash.as_deref(), Some("797575e985c12149"));
}

#[test]
fn the_parity_blob_drops_built_from_ts_and_through_seq() {
    // Trap T10: `built_from_ts` is wall-clock `now` and is excluded from every parity
    // comparison. D4's `through_seq` is new in 2.0 and never reaches the wire.
    let mut model = LearnerModel::from_json(MODEL_1).unwrap();
    model.built_from_ts = Some(Timestamp::from_micros(1_777_902_306_123_456));
    model.through_seq = Some(4711);
    let blob = model.parity_blob().unwrap();
    assert!(!blob.contains("built_from_ts"));
    assert!(!blob.contains("through_seq"));
    assert_eq!(model.parity_digest().unwrap(), MODEL_1_DIGEST);
}

#[test]
fn the_camel_case_topic_state_spellings_survive_the_dump() {
    // Spec section 3: `repNum` and `memoryBase` are load-bearing on the wire
    // (`tests/test_model.py:242-246`).
    let state = TopicState {
        rep_num: 3.5,
        memory_base: 1.25,
        ..TopicState::default()
    };
    let text = serde_json::to_string(&state).unwrap();
    assert!(text.contains(r#""repNum":3.5"#), "got {text}");
    assert!(text.contains(r#""memoryBase":1.25"#), "got {text}");
    assert!(!text.contains("rep_num"));
    assert!(!text.contains("memory_base"));
}

#[test]
fn a_default_topic_state_holds_the_pinned_initial_values() {
    // Spec section 3, the initial column.
    let state = TopicState::default();
    assert_eq!(state.status, TopicStatus::Untouched);
    assert_eq!(state.rep_num, 0.0);
    assert_eq!(state.memory_base, 0.0);
    assert_eq!(state.t0, None);
    assert_eq!(state.interval_days, 0.0);
    assert_eq!(state.ability, 0.0);
    assert_eq!(state.speed, 1.0);
    assert!(!state.conditional);
    assert!(!state.explicit_only);
    assert!(state.last_problems.is_empty());
    assert!(state.kp_progress.is_empty());
    assert!(state.is_default());

    // `finalize` drops every topic that still equals a default state.
    let touched = TopicState {
        speed: 1.000_000_1,
        ..TopicState::default()
    };
    assert!(!touched.is_default());
}

#[test]
fn a_topic_state_from_the_oracle_model_reads_every_field() {
    let model = LearnerModel::from_json(MODEL_1).unwrap();
    let state = model.topics.get("absolute-value-inequalities").unwrap();
    // Spec section 4.4: a `review_result` on an untouched topic leaves it untouched
    // while writing `repNum`, `memoryBase`, and `t0`.
    assert_eq!(state.status, TopicStatus::Untouched);
    assert_eq!(state.memory_base, 0.85);
    assert_eq!(state.rep_num, 1.127_185_631_752_380_7);
    assert_eq!(
        state.t0,
        Some(Timestamp::parse("2026-03-10T22:53:40Z").unwrap())
    );
    assert_eq!(state.last_problems.len(), 4);
    assert_eq!(state.kp_progress.len(), 3);
}

#[test]
fn the_learner_model_rejects_an_unknown_key() {
    let text = r#"{"topics":{},"nope":1}"#;
    assert!(LearnerModel::from_json(text).is_err());
    let text = r#"{"topics":{"a":{"repNum":1.0,"nope":1}}}"#;
    assert!(LearnerModel::from_json(text).is_err());
}

#[test]
fn an_empty_learner_model_holds_the_pinned_defaults() {
    let model = LearnerModel::from_json("{}").unwrap();
    assert_eq!(model.xp.total, 0);
    assert_eq!(model.xp.today, 0);
    assert_eq!(model.xp.goal, 40);
    assert_eq!(model.xp.streak_days, 0);
    assert_eq!(model.quiz.last_at, None);
    assert_eq!(model.quiz.xp_since, 0);
    assert!(!model.quiz.retake_pending);
    assert_eq!(model.velocity.xp_per_day_28d, 0.0);
    assert_eq!(model.velocity.eta, None);
    assert_eq!(model.config_hash, None);
    assert_eq!(model.projector_version, None);
    assert_eq!(model.through_seq, None);
    assert_eq!(model, LearnerModel::default());
    assert_eq!(
        model.parity_blob().unwrap(),
        r#"{"config_hash":null,"pending_remediation":[],"projector_version":null,"quiz":{"last_at":null,"retake_pending":false,"xp_since":0},"topics":{},"velocity":{"course_progress":0.0,"eta":null,"topics_per_week_28d":0.0,"xp_per_day_28d":0.0},"xp":{"goal":40,"streak_days":0,"today":0,"total":0}}"#
    );
}
