//! U3 acceptance, part 2: the regrade pre-pass and the replayed model it buys
//! (spec section 8, `tests/test_regrade.py`, C2, C4).
//!
//! Every expected value below is a LITERAL from the 1.0 test file or a blob recorded
//! from 1.0. No expectation calls the code under test.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_core::config::Config;
use cadus_core::curriculum::Curriculum;
use cadus_core::event::{Event, Timestamp, WorkQuality};
use cadus_core::learner::LearnerModel;
use cadus_core::projector::{
    ProjectionInput, Projector, apply_regrades, canonical_blob, project, project_incremental,
};
use common::events::{assert_same_blob, event, oracle_stamp, regrade_graph, stream};

/// The number of events in `stream_1.jsonl` (spec section 9).
const STREAM_1_EVENTS: usize = 47;

/// `T0` of the 1.0 regrade tests (`tests/test_regrade.py:44`).
const T0: &str = "2026-08-17T16:30:00Z";

/// The instant the regrade folds build at.
const REGRADE_NOW: &str = "2026-08-19T16:30:00Z";

// --------------------------------------------------------------------------- //
// apply_regrades — the substitution (spec section 8, tests/test_regrade.py)
// --------------------------------------------------------------------------- //

/// One `attempt` of the regrade tests (`tests/test_regrade.py:68-88`).
fn regrade_attempt(aid: &str, ts: &str, task_id: &str, tier: &str, tags: &str) -> Event {
    event(&format!(
        r#"{{"type":"attempt","ts":"{ts}","attempt_id":"{aid}","task_id":"{task_id}",
            "topic":"subtraction-facts","kp":"kp1","task_type":"lesson",
            "problem":{{"text":"problem {aid}","expected":"7"}},"given_answer":"7",
            "correct":true,"secs":20,"error_tags":{tags},"work_quality":"{tier}"}}"#
    ))
}

/// One `lesson_result` of the regrade tests (`tests/test_regrade.py:91-95`).
fn regrade_lesson(ts: &str, tier: &str, xp: &str) -> Event {
    event(&format!(
        r#"{{"type":"lesson_result","ts":"{ts}","topic":"subtraction-facts","passed":true,
            "xp":{xp},"quality_tier":"{tier}"}}"#
    ))
}

/// The blown-off first task: an attempt `a1` with `tags` and the lesson that
/// closes it at -3.5 XP (`tests/test_regrade.py:9-11`).
fn blowoff_task(tags: &str) -> Vec<Event> {
    vec![
        regrade_attempt("a1", T0, "task-1", "blowoff", tags),
        regrade_lesson(T0, "blowoff", "-3.5"),
    ]
}

/// The correction of `task-1` at `ts`: `nearly_perfect` at 7.0 XP, with the
/// attempt `a1` regraded `nearly_perfect` and `extra` appended to it.
fn correction(ts: &str, extra: &str) -> Event {
    event(&format!(
        r#"{{"type":"regraded","ts":"{ts}","task_id":"task-1",
            "topic":"subtraction-facts","reason":"test","quality_tier":"nearly_perfect",
            "xp":7.0,"attempts":[{{"attempt_id":"a1","work_quality":"nearly_perfect"{extra}}}]}}"#
    ))
}

#[test]
fn a_correction_replaces_the_grade_fields_of_the_named_attempt() {
    let events = vec![
        regrade_attempt(
            "a1",
            T0,
            "task-1",
            "poor",
            r#"["incomplete","timing-unreliable"]"#,
        ),
        event(
            r#"{"type":"regraded","ts":"2026-08-18T16:30:00Z","task_id":"task-1",
                "topic":"subtraction-facts","reason":"test",
                "attempts":[{"attempt_id":"a1","work_quality":"nearly_perfect",
                             "error_tags":["timing-unreliable"],"grader_note":"regraded"}]}"#,
        ),
    ];
    let out = apply_regrades(&events);
    assert_eq!(out.len(), 1, "the correction itself is consumed");
    let Event::Attempt(fixed) = &out[0] else {
        panic!("expected an attempt, got {:?}", out[0].type_name());
    };
    assert_eq!(fixed.work_quality, WorkQuality::NearlyPerfect);
    // ORDER: the tag list is replaced wholesale, and its order is kept.
    assert_eq!(fixed.error_tags, vec!["timing-unreliable".to_string()]);
    assert_eq!(fixed.grader_note.as_deref(), Some("regraded"));
    // A correction restates how well the work was done, never whether it was right.
    assert!(fixed.correct);
    assert_eq!(fixed.given_answer, "7");
}

#[test]
fn a_stream_with_no_correction_comes_back_unchanged() {
    let events = blowoff_task("[]");
    assert_eq!(apply_regrades(&events), events);
}

#[test]
fn a_later_correction_supersedes_an_earlier_one() {
    let events = vec![
        regrade_attempt("a1", T0, "task-1", "poor", "[]"),
        event(
            r#"{"type":"regraded","ts":"2026-08-18T16:30:00Z","task_id":"task-1",
                "topic":"subtraction-facts","reason":"test",
                "attempts":[{"attempt_id":"a1","work_quality":"passable"}]}"#,
        ),
        event(
            r#"{"type":"regraded","ts":"2026-08-19T16:30:00Z","task_id":"task-1",
                "topic":"subtraction-facts","reason":"test",
                "attempts":[{"attempt_id":"a1","work_quality":"perfect"}]}"#,
        ),
    ];
    let out = apply_regrades(&events);
    assert_eq!(out.len(), 1);
    let Event::Attempt(fixed) = &out[0] else {
        panic!("expected an attempt");
    };
    assert_eq!(fixed.work_quality, WorkQuality::Perfect);
}

#[test]
fn a_correction_touches_only_the_attempt_it_names() {
    let events = vec![
        regrade_attempt("a1", T0, "task-1", "poor", "[]"),
        regrade_attempt("a2", "2026-08-17T16:31:00Z", "task-1", "passable", "[]"),
        event(
            r#"{"type":"regraded","ts":"2026-08-18T16:30:00Z","task_id":"task-1",
                "topic":"subtraction-facts","reason":"test",
                "attempts":[{"attempt_id":"a1","work_quality":"nearly_perfect"}]}"#,
        ),
    ];
    let out = apply_regrades(&events);
    assert_eq!(out.len(), 2);
    let (Event::Attempt(first), Event::Attempt(second)) = (&out[0], &out[1]) else {
        panic!("expected two attempts");
    };
    assert_eq!(first.work_quality, WorkQuality::NearlyPerfect);
    assert_eq!(second.work_quality, WorkQuality::Passable);
}

#[test]
fn a_lesson_result_is_matched_through_its_preceding_attempt() {
    // A 2-KP lesson: `blowoff` is 3.5 x 2 x -0.5 = -3.5, `nearly_perfect` is
    // 3.5 x 2 x 1.0 = 7.0 (tests/test_regrade.py:9-11).
    let mut events = blowoff_task("[]");
    events.push(correction("2026-08-18T16:30:00Z", ""));
    let out = apply_regrades(&events);
    assert_eq!(out.len(), 2);
    let Event::LessonResult(result) = &out[1] else {
        panic!("expected a lesson result");
    };
    assert_eq!(result.quality_tier, WorkQuality::NearlyPerfect);
    assert!((result.xp - 7.0).abs() < f64::EPSILON);
    assert!(result.passed);
}

#[test]
fn a_review_result_is_matched_by_its_own_task_id() {
    let events = vec![
        event(
            r#"{"type":"attempt","ts":"2026-08-17T16:30:00Z","attempt_id":"a1",
                "task_id":"review-9","topic":"subtraction-facts","kp":"kp1",
                "task_type":"review","problem":{"text":"problem a1","expected":"7"},
                "given_answer":"7","correct":true,"secs":20,"work_quality":"blowoff"}"#,
        ),
        event(
            r#"{"type":"review_result","ts":"2026-08-17T16:30:00Z","topic":"subtraction-facts",
                "passed":true,"weighted_score":1.0,"xp":0.0,"quality_tier":"poor",
                "task_id":"review-9"}"#,
        ),
        event(
            r#"{"type":"regraded","ts":"2026-08-18T16:30:00Z","task_id":"review-9",
                "topic":"subtraction-facts","reason":"test","quality_tier":"nearly_perfect",
                "xp":5.0,"attempts":[{"attempt_id":"a1","work_quality":"nearly_perfect"}]}"#,
        ),
    ];
    let out = apply_regrades(&events);
    assert_eq!(out.len(), 2);
    let Event::ReviewResult(result) = &out[1] else {
        panic!("expected a review result");
    };
    assert_eq!(result.quality_tier, WorkQuality::NearlyPerfect);
    assert!((result.xp - 5.0).abs() < f64::EPSILON);
}

#[test]
fn the_result_of_an_uncorrected_task_keeps_its_own_grade() {
    let later = "2026-08-17T17:30:00Z";
    let mut events = blowoff_task("[]");
    events.push(regrade_attempt("a2", later, "task-2", "passable", "[]"));
    // 3.5 x 2 x 0.85 = 5.95.
    events.push(regrade_lesson(later, "passable", "5.95"));
    events.push(correction("2026-08-18T16:30:00Z", ""));
    let out = apply_regrades(&events);
    assert_eq!(out.len(), 4);
    let (Event::LessonResult(first), Event::LessonResult(second)) = (&out[1], &out[3]) else {
        panic!("expected two lesson results");
    };
    assert!((first.xp - 7.0).abs() < f64::EPSILON);
    assert!((second.xp - 5.95).abs() < f64::EPSILON);
    assert_eq!(second.quality_tier, WorkQuality::Passable);
}

#[test]
fn apply_regrades_is_idempotent() {
    let mut events = blowoff_task(r#"["incomplete"]"#);
    events.push(correction(
        "2026-08-18T16:30:00Z",
        r#","error_tags":["timing-unreliable"]"#,
    ));
    let once = apply_regrades(&events);
    let twice = apply_regrades(&once);
    assert_eq!(twice, once);

    // The same holds on the committed stream, which carries one `regraded` event.
    let fixture_events = stream("stream_1.jsonl");
    let once = apply_regrades(&fixture_events);
    assert_eq!(once.len(), STREAM_1_EVENTS - 1);
    assert_eq!(apply_regrades(&once), once);
}

// --------------------------------------------------------------------------- //
// What the correction buys — the replayed model
// --------------------------------------------------------------------------- //

/// The regrade tests fold over their own one-topic graph, not over the tree.
fn regrade_fold(events: &[Event], graph: &Curriculum) -> LearnerModel {
    let cfg = Config::default();
    let input = ProjectionInput::new(graph, &cfg, Timestamp::parse(REGRADE_NOW).unwrap());
    project(events, &input).expect("the fold succeeds")
}

#[test]
fn a_correction_turns_the_recorded_failure_into_a_pass() {
    let graph = regrade_graph();
    let broken = blowoff_task(r#"["incomplete"]"#);
    let before = regrade_fold(&broken, &graph);
    // Python `round(-3.5)` is half-even, so -3.5 rounds to -4, never to -3 (trap T3).
    assert_eq!(before.xp.total, -4);

    let mut fixed = broken.clone();
    fixed.push(correction("2026-08-18T16:30:00Z", ""));
    let after = regrade_fold(&fixed, &graph);
    assert_eq!(after.xp.total, 7);
    // FIRe read a pass, not a miss.
    assert!(
        after.topics["subtraction-facts"].interval_days
            > before.topics["subtraction-facts"].interval_days
    );
}

#[test]
fn a_correction_written_forty_days_later_lands_on_the_graded_day() {
    let graph = regrade_graph();
    let mut events = blowoff_task("[]");
    events.push(correction("2026-09-26T16:30:00Z", ""));
    // 7.0 lands on 2026-08-17, the graded day, not on the day the correction was written:
    // `apply_regrades` consumes the correction before the fold, so it never moves `t_ref`.
    let model = regrade_fold(&events, &graph);
    assert_eq!(model.xp.today, 7);
    assert_eq!(model.xp.total, 7);
}

/// The 1.0 FULL replay of the late-correction stream, recorded from `project`.
const LATE_CORRECTION_FULL: &str = r#"{"config_hash":"797575e985c12149","pending_remediation":[],"projector_version":3,"quiz":{"last_at":null,"retake_pending":false,"xp_since":7},"topics":{"subtraction-facts":{"ability":0.65,"conditional":false,"explicit_only":false,"interval_days":6.680357142857142,"kp_progress":{"kp1":"passed","kp2":"passed"},"last_problems":["80baf8b31b56"],"memoryBase":0.85,"repNum":1.3964285714285714,"speed":1.6428571428571428,"status":"learning","t0":"2026-08-17T16:30:00Z"}},"velocity":{"course_progress":0.0,"eta":null,"topics_per_week_28d":0.0,"xp_per_day_28d":0.0},"xp":{"goal":40,"streak_days":0,"today":7,"total":7}}"#;

/// The 1.0 INCREMENTAL replay of the same stream, recorded from `project_incremental`.
///
/// It differs from [`LATE_CORRECTION_FULL`] in the FIRe fields, and 1.0 differs the
/// same way: the correction lands in `new_events`, so the corrected `lesson_result`
/// re-splits into the PRIOR half and replays with FIRe off, on top of a cache built
/// before the correction. 1.0 answers this at the service layer, which sends a stream
/// carrying a new correction down the FULL-replay path instead
/// (`projector.py:812-820`, `service.py:257-280`). The 1.0 test asserts only the XP
/// (`tests/test_regrade.py:364-384`); the two blobs below were recorded from 1.0 so
/// the port pins the divergence rather than hides it.
const LATE_CORRECTION_INCREMENTAL: &str = r#"{"config_hash":"797575e985c12149","pending_remediation":[],"projector_version":3,"quiz":{"last_at":null,"retake_pending":false,"xp_since":7},"topics":{"subtraction-facts":{"ability":0.65,"conditional":false,"explicit_only":false,"interval_days":2.0,"kp_progress":{"kp1":"passed","kp2":"passed"},"last_problems":["80baf8b31b56"],"memoryBase":0.0,"repNum":0.0,"speed":1.6428571428571428,"status":"learning","t0":"2026-08-17T16:30:00Z"}},"velocity":{"course_progress":0.0,"eta":null,"topics_per_week_28d":0.0,"xp_per_day_28d":0.0},"xp":{"goal":40,"streak_days":0,"today":7,"total":7}}"#;

#[test]
fn incremental_carries_the_corrected_xp_when_the_correction_arrives_late() {
    let graph = regrade_graph();
    let cfg = Config::default();
    let input = ProjectionInput::new(&graph, &cfg, Timestamp::parse(REGRADE_NOW).unwrap());
    let prior = blowoff_task("[]");
    let fresh = vec![correction("2026-08-18T16:30:00Z", "")];
    let mut whole = prior.clone();
    whole.extend(fresh.clone());

    // The two blobs below are 1.0 literals, so both models take the 1.0 stamp (D-F2).
    let full = oracle_stamp(project(&whole, &input).unwrap());
    let cached = project(&prior, &input).unwrap();
    let incremental = oracle_stamp(project_incremental(&cached, &prior, &fresh, &input).unwrap());
    // The prior half is not inert: its result XP is tallied with FIRe off, so the
    // correction reaches both paths.
    assert_eq!(full.xp.total, 7);
    assert_eq!(incremental.xp.total, 7);

    let actual_full = canonical_blob(&full).unwrap();
    assert_same_blob(&actual_full, LATE_CORRECTION_FULL, "the full replay");
    let actual_incremental = canonical_blob(&incremental).unwrap();
    assert_same_blob(
        &actual_incremental,
        LATE_CORRECTION_INCREMENTAL,
        "the incremental replay",
    );
}

#[test]
fn the_projector_version_is_stamped_on_a_minimal_fold() {
    let graph = regrade_graph();
    let model = regrade_fold(
        &[regrade_attempt("a1", T0, "task-1", "blowoff", "[]")],
        &graph,
    );
    assert_eq!(model.projector_version, Some(6));
}

#[test]
fn a_light_replay_writes_no_topic_state() {
    // `apply_fire = false` skips every TopicState write while it still tallies XP.
    let graph = regrade_graph();
    let cfg = Config::default();
    let mut proj = Projector::new(&graph, &cfg);
    for item in blowoff_task("[]") {
        proj.apply(&item, false);
    }
    assert!(proj.topics().is_empty());
    let model = proj
        .finalize(Timestamp::parse(REGRADE_NOW).unwrap())
        .unwrap();
    assert!(model.topics.is_empty());
    assert_eq!(model.xp.total, -4);
}

#[test]
fn a_correction_carries_only_the_fields_it_names() {
    // A correction with a tier and no XP leaves the XP alone, and one with XP
    // and no tier leaves the tier alone. Both still match through the task.
    let mut events = blowoff_task("[]");
    events.push(event(
        r#"{"type":"regraded","ts":"2026-08-18T16:30:00Z","task_id":"task-1",
            "topic":"subtraction-facts","reason":"test","quality_tier":"passable"}"#,
    ));
    let out = apply_regrades(&events);
    let Event::LessonResult(tier_only) = &out[1] else {
        panic!("expected a lesson result");
    };
    assert_eq!(tier_only.quality_tier, WorkQuality::Passable);
    assert!((tier_only.xp + 3.5).abs() < f64::EPSILON);

    let mut events = blowoff_task("[]");
    events.push(event(
        r#"{"type":"regraded","ts":"2026-08-18T16:30:00Z","task_id":"task-1",
            "topic":"subtraction-facts","reason":"test","xp":2.0}"#,
    ));
    let out = apply_regrades(&events);
    let Event::LessonResult(xp_only) = &out[1] else {
        panic!("expected a lesson result");
    };
    assert_eq!(xp_only.quality_tier, WorkQuality::Blowoff);
    assert!((xp_only.xp - 2.0).abs() < f64::EPSILON);
}
