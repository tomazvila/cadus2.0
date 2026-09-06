//! Integrated report uses whole-item evidence and preserves unknown outcomes.
#![allow(clippy::unwrap_used)]
use super::*;
use crate::event::{
    AttemptOutcome, ReviewResult, SchemaVersion, Slug, TaskServed, TaskType, Timestamp, WorkQuality,
};
use serde_json::json;

/// One served multi-step task `task_id`.
fn served(task_id: &str) -> Event {
    Event::TaskServed(TaskServed {
        ts: Timestamp::from_micros(0),
        session: None,
        v: SchemaVersion::current(),
        task_id: task_id.to_owned(),
        task_type: TaskType::MultiStep,
        topic: None,
        kp: None,
        problems: Vec::new(),
        component_topics: Vec::new(),
        seed: None,
        probe_delay_days: None,
        confirm: false,
    })
}

/// One review result that closes `task_id`.
fn closed(task_id: Option<&str>, passed: bool, inconclusive: bool) -> Event {
    Event::ReviewResult(ReviewResult {
        ts: Timestamp::from_micros(0),
        session: None,
        v: SchemaVersion::current(),
        topic: Slug::new("t1").expect("a slug"),
        passed,
        weighted_score: 1.0,
        xp: 0.0,
        quality_tier: WorkQuality::NearlyPerfect,
        assisted: false,
        task_id: task_id.map(std::borrow::ToOwned::to_owned),
        inconclusive,
        confirmation_skills: Vec::new(),
    })
}

fn integrated_served(session: &str, task: &str) -> Event {
    serde_json::from_value(json!({
        "type":"integrated_served", "ts":Timestamp::from_micros(0), "session":session,
        "task_id":task,"item_id":"whole","item_digest":"digest","topic":"t1"
    }))
    .unwrap()
}

fn attempt(session: &str, task: &str, outcome: AttemptOutcome) -> Event {
    serde_json::from_value(json!({
        "type":"integrated_attempt", "ts":Timestamp::from_micros(0), "session":session,
        "attempt_id":format!("{session}:{task}"), "task_id":task,
        "item_id":"whole","item_digest":"digest","topic":"t1", "steps":[],
        "final_field":{"id":"final","answer":"4","contract":{"kind":"exact"},"outcome":outcome,"assisted":false},
        "solved":outcome == AttemptOutcome::Correct, "assisted":false
    }))
    .unwrap()
}

#[test]
fn component_fallback_tasks_never_count_as_integrated_performance() {
    let report = IntegratedPerformance::of_events(&[
        served("fallback"),
        closed(Some("fallback"), true, false),
        closed(Some("fallback"), false, true),
    ]);
    assert_eq!(report, IntegratedPerformance::default());
    assert_eq!(report.pass_rate(), None);
}

#[test]
fn integrated_attempts_decide_outcomes_and_serve_only_items_stay_open() {
    let events = [
        integrated_served("s1", "pass"),
        integrated_served("s1", "pass"),
        integrated_served("s1", "open"),
        attempt("s1", "pass", AttemptOutcome::Correct),
        attempt("s1", "pass", AttemptOutcome::Correct),
        attempt("s2", "pass", AttemptOutcome::Incorrect),
        attempt(
            "s1",
            "unknown",
            AttemptOutcome::Ungraded {
                reason: "undecidable".to_owned(),
            },
        ),
    ];
    let report = IntegratedPerformance::of_events(&events);
    assert_eq!(
        report,
        IntegratedPerformance {
            served: 4,
            passed: 1,
            failed: 1,
            inconclusive: 1,
            open: 1
        }
    );
    assert_eq!(report.pass_rate(), Some(0.5));
    let replay: Vec<Event> =
        serde_json::from_value(serde_json::to_value(&events).unwrap()).unwrap();
    assert_eq!(IntegratedPerformance::of_events(&replay), report);
}

#[test]
fn unknown_integrated_result_has_no_pass_rate() {
    let report = IntegratedPerformance::of_events(&[attempt(
        "s1",
        "unknown",
        AttemptOutcome::Ungraded {
            reason: "undecidable".to_owned(),
        },
    )]);
    assert_eq!(report.inconclusive, 1);
    assert_eq!(report.failed, 0);
    assert_eq!(report.pass_rate(), None);
}
