//! Unit f4-outcome, step 1: the event contract v2, the v1 shim, and the fold rules
//! of the third outcome (D-F2, D-F9, D-F11).
//!
//! The v1 line the shim test reads is a LITERAL taken from
//! `tests/fixtures/events/one_per_type.jsonl`, the file the 1.0 models generated.
//! Nothing here re-derives an expected value from the code under test.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_core::event::{
    Attempt, AttemptOutcome, Event, Exposure, ItemSource, SchemaVersion, Slug,
};
use cadus_core::learner::{LearnerModel, UNGRADED_WINDOW};
use cadus_core::projector::{PROJECTOR_VERSION, project};
use common::events::{input, tree};

/// The `attempt` line of `one_per_type.jsonl`, written by the 1.0 models.
const V1_ATTEMPT: &str = r#"{"answer_kind":"numeric","assisted":true,"attempt_id":"t-1-1","correct":true,"error_tags":["notation","units"],"given_answer":"3","grader_note":"clean method","kp":"kp2","problem":{"expected":"3","text":" |-3| = ? — non-ASCII kept"},"secs":17,"session":"s_2026-05-04a","task_id":"t-1","task_type":"lesson","topic":"absolute-value","ts":"2026-05-04T13:45:06Z","type":"attempt","v":1,"work":"|-3| = 3","work_quality":"nearly_perfect"}"#;

/// The topic the fold tests practice. It is a real topic of the checked-in tree.
const TOPIC: &str = "absolute-value";

/// Read one event, or panic with the line that did not read.
fn read(text: &str) -> Event {
    Event::from_json(text).unwrap_or_else(|error| panic!("{text}: {error}"))
}

/// The `attempt` body of an event, or a panic.
fn attempt_of(event: &Event) -> &Attempt {
    match event {
        Event::Attempt(body) => body,
        other => panic!("expected an attempt, got a {}", other.type_name()),
    }
}

/// One attempt line on [`TOPIC`] with the given outcome key and `correct` value.
fn attempt_line(index: usize, outcome: &str, correct: bool) -> String {
    format!(
        r#"{{"type":"attempt","ts":"2026-07-14T12:0{index}:00Z","attempt_id":"a{index}",
           "task_id":"t1","topic":"{TOPIC}","task_type":"lesson",
           "problem":{{"text":"p{index}","expected":"3"}},"given_answer":"g",
           "correct":{correct},{outcome}"secs":5,"work_quality":"nearly_passable"}}"#
    )
    .replace('\n', "")
}

/// The full replay of `lines` over the checked-in tree.
fn fold(lines: &[String]) -> LearnerModel {
    let events: Vec<Event> = lines.iter().map(|line| read(line)).collect();
    project(&events, &input()).expect("the fold succeeds")
}

/// A stream that places `TOPIC` with conditional credit, then answers it once.
fn placed_then(attempt: &str) -> Vec<String> {
    vec![
        r#"{"type":"enrolled","ts":"2026-07-14T12:00:00Z","course":"foundations"}"#.to_owned(),
        format!(
            r#"{{"type":"diagnostic_placed","ts":"2026-07-14T12:01:00Z","balances":{{"{TOPIC}":2.0}},"conditional":["{TOPIC}"]}}"#
        ),
        attempt.to_owned(),
    ]
}

// --------------------------------------------------------------------------- //
// The v1 shim
// --------------------------------------------------------------------------- //

#[test]
fn a_v1_attempt_row_reads_as_v2_and_writes_its_own_bytes_back() {
    let event = read(V1_ATTEMPT);
    assert_eq!(event.v(), SchemaVersion::first());
    let body = attempt_of(&event);
    // The outcome comes from `correct`, and every new field takes its default.
    assert_eq!(body.outcome, AttemptOutcome::Correct);
    assert!(body.correct);
    assert_eq!(body.item_digest, None);
    assert_eq!(body.item_source, None);
    // The reliability of old evidence is not reconstructible, so neither is invented.
    assert_eq!(body.exposure, None);
    assert_eq!(body.timing_reliable, None);
    assert!(body.skills.is_empty());
    assert!(!body.independent_after_feedback);
    // An original row is never rewritten (C2).
    assert_eq!(event.to_canonical_json().unwrap(), V1_ATTEMPT);
}

#[test]
fn a_v1_miss_reads_as_the_incorrect_outcome() {
    let text = V1_ATTEMPT.replace(r#""correct":true"#, r#""correct":false"#);
    let event = read(&text);
    assert_eq!(attempt_of(&event).outcome, AttemptOutcome::Incorrect);
    assert_eq!(event.to_canonical_json().unwrap(), text);
}

#[test]
fn a_v2_ungraded_row_keeps_its_reason_and_claims_no_correctness() {
    let line = attempt_line(
        1,
        r#""outcome":{"ungraded":{"reason":"no deterministic verdict for a proof"}},"#,
        true,
    );
    let event = read(&line);
    let body = attempt_of(&event);
    // `correct` equals `outcome == Correct`, so an ungraded row never claims a pass.
    assert!(!body.correct);
    assert_eq!(
        body.outcome.reason(),
        Some("no deterministic verdict for a proof")
    );
    let written = event.to_canonical_json().unwrap();
    assert!(written.contains(r#""outcome":{"ungraded":"#));
    assert_eq!(read(&written), event);
}

#[test]
fn the_shim_step_is_idempotent() {
    let event = read(V1_ATTEMPT);
    let mut once = event.clone();
    once.normalize();
    let mut twice = once.clone();
    twice.normalize();
    assert_eq!(once, event);
    assert_eq!(twice, once);
}

#[test]
fn the_evidence_fields_survive_a_round_trip() {
    let line = attempt_line(
        2,
        r#""outcome":"incorrect","item_digest":"d1","item_source":"template","exposure":"repeat","timing_reliable":true,"skills":["kp1","kp2"],"independent_after_feedback":true,"#,
        false,
    );
    let event = read(&line);
    let body = attempt_of(&event);
    assert_eq!(body.item_digest.as_deref(), Some("d1"));
    assert_eq!(body.item_source, Some(ItemSource::Template));
    assert_eq!(body.exposure, Some(Exposure::Repeat));
    assert_eq!(body.timing_reliable, Some(true));
    assert_eq!(body.skills, ["kp1", "kp2"]);
    assert!(body.independent_after_feedback);
    assert_eq!(read(&event.to_canonical_json().unwrap()), event);
}

// --------------------------------------------------------------------------- //
// The fold
// --------------------------------------------------------------------------- //

#[test]
fn an_ungraded_attempt_moves_no_mastery_and_peels_back_no_credit() {
    let reason = r#""outcome":{"ungraded":{"reason":"the answer left the grammar"}},"#;
    let ungraded = fold(&placed_then(&attempt_line(2, reason, false)));
    let missed = fold(&placed_then(&attempt_line(
        2,
        r#""outcome":"incorrect","#,
        false,
    )));

    let after_ungraded = &ungraded.topics[TOPIC];
    let after_miss = &missed.topics[TOPIC];
    // A miss halves the conditional credit and clears the flag. Ungraded does neither.
    assert!(after_ungraded.conditional);
    assert!(!after_miss.conditional);
    assert!(after_ungraded.rep_num > after_miss.rep_num);
    assert_eq!(after_ungraded.status, after_miss.status);
    // No XP, and no streak effect.
    assert_eq!(ungraded.xp.total, 0);
    assert_eq!(ungraded.xp.streak_days, 0);
    // No knowledge point steps: only a `lesson_result` writes `kp_progress`.
    assert!(after_ungraded.kp_progress.is_empty());
    // The anti-repeat window still holds the problem the learner saw (A5).
    assert_eq!(after_ungraded.last_problems.len(), 1);
}

#[test]
fn an_ungraded_attempt_counts_on_the_topic_and_enters_the_recovery_list() {
    let reason = r#""outcome":{"ungraded":{"reason":"the answer left the grammar"}},"#;
    let model = fold(&placed_then(&attempt_line(2, reason, false)));
    assert_eq!(model.topics[TOPIC].ungraded_attempts, 1);
    assert_eq!(model.ungraded.len(), 1);
    let entry = &model.ungraded[0];
    assert_eq!(entry.attempt_id, "a2");
    assert_eq!(entry.topic, TOPIC);
    assert_eq!(entry.reason, "the answer left the grammar");
    // A decided attempt adds nothing to either place.
    let decided = fold(&placed_then(&attempt_line(
        2,
        r#""outcome":"incorrect","#,
        false,
    )));
    assert_eq!(decided.topics[TOPIC].ungraded_attempts, 0);
    assert!(decided.ungraded.is_empty());
}

#[test]
fn the_recovery_list_keeps_the_last_twenty_ungraded_attempts() {
    let mut lines = vec![
        r#"{"type":"enrolled","ts":"2026-07-14T11:00:00Z","course":"foundations"}"#.to_owned(),
    ];
    let count = UNGRADED_WINDOW + 5;
    for index in 0..count {
        lines.push(
            format!(
                r#"{{"type":"attempt","ts":"2026-07-14T12:00:00Z","attempt_id":"a{index}","task_id":"t1","topic":"{TOPIC}","task_type":"lesson","problem":{{"text":"p{index}","expected":"3"}},"given_answer":"g","correct":false,"outcome":{{"ungraded":{{"reason":"r{index}"}}}},"secs":5,"work_quality":"nearly_passable"}}"#
            ),
        );
    }
    let model = fold(&lines);
    assert_eq!(model.ungraded.len(), UNGRADED_WINDOW);
    // The list keeps the NEWEST entries, oldest first.
    assert_eq!(
        model.ungraded[0].attempt_id,
        format!("a{}", count - UNGRADED_WINDOW)
    );
    assert_eq!(
        model.ungraded[UNGRADED_WINDOW - 1].attempt_id,
        format!("a{}", count - 1)
    );
    assert_eq!(
        u32::try_from(count).expect("the count fits"),
        model.topics[TOPIC].ungraded_attempts
    );
}

#[test]
fn a_model_with_no_ungraded_attempt_keeps_the_1_0_wire_shape() {
    let model = fold(&placed_then(&attempt_line(
        2,
        r#""outcome":"incorrect","#,
        false,
    )));
    let blob = model.parity_blob().unwrap();
    assert!(!blob.contains("ungraded"));
    assert_eq!(model.projector_version, Some(PROJECTOR_VERSION));
    assert_eq!(PROJECTOR_VERSION, 6);
}

// --------------------------------------------------------------------------- //
// The retention probe
// --------------------------------------------------------------------------- //

#[test]
fn a_retention_probe_reads_writes_and_folds_into_the_retention_state() {
    let line = r#"{"assisted":false,"delay_days":7,"exposure":"first","item_digest":"d9","kp":"kp1","outcome":"correct","secs":12,"session":null,"topic":"absolute-value","ts":"2026-07-21T09:00:00Z","type":"retention_probe","v":2}"#;
    let event = read(line);
    assert_eq!(event.type_name(), "retention_probe");
    assert_eq!(event.v(), SchemaVersion::current());
    assert_eq!(
        event.to_canonical_json().unwrap(),
        line,
        "the f19 fields are optional, so a probe without them keeps its bytes"
    );
    // Unit f19 gave the event its handler (D-F11). The tally carries the
    // provenance, and a log with NO probe still writes the 1.0 wire shape.
    let with_probe = project(&[event], &input()).expect("the fold succeeds");
    let empty = project(&[], &input()).expect("the fold succeeds");
    let tally = &with_probe.retention.by_delay[&7];
    assert_eq!(tally.probes, 1);
    assert_eq!(tally.independent_correct, 1);
    assert!(with_probe.retention.is_done(TOPIC, "kp1", 7));
    assert!(!empty.parity_blob().unwrap().contains("retention"));
    assert!(tree().idx_of(TOPIC).is_some());
    assert!(Slug::new(TOPIC).is_ok());
}
