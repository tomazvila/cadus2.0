//! Historical observations remain immutable; explicit uncertainty survives every replay.
#![allow(clippy::unwrap_used, clippy::panic)]
mod common;

use cadus_core::answer::{AnswerContract, Outcome, check_contract};
use cadus_core::event::{AttemptOutcome, Event};
use cadus_core::projector::{canonical_blob, project, project_incremental};
use common::events::input;
use serde_json::json;

const REASON: &str =
    "Historical answer is approximate prose; no deterministic verdict was recorded.";

fn attempt(version: i64, explicit: bool) -> Event {
    let mut row = json!({
        "type":"attempt","v":version,"ts":"2026-07-14T12:02:00Z","session":"s1",
        "attempt_id":"legacy-ambiguous","task_id":"t1","topic":"absolute-value","task_type":"review",
        "problem":{"text":"Find the value.","expected":"2"},"given_answer":"about two",
        "correct":false,"secs":5,"work_quality":"nearly_passable"
    });
    if explicit {
        row["outcome"] = json!({"ungraded":{"reason":REASON}});
    }
    Event::from_json(&row.to_string()).unwrap()
}

fn placed() -> Event {
    Event::from_json(r#"{"type":"diagnostic_placed","ts":"2026-07-14T12:01:00Z","balances":{"absolute-value":2.0},"conditional":["absolute-value"]}"#).unwrap()
}

#[test]
fn explicit_uncertainty_survives_v1_v2_normalization_serialization_and_every_replay_split() {
    for version in [1, 2] {
        let original = attempt(version, true);
        let bytes = original.to_canonical_json().unwrap();
        let mut migrated = Event::from_json(&bytes).unwrap();
        migrated.normalize();
        migrated.normalize();
        assert_eq!(migrated.to_canonical_json().unwrap(), bytes);
        let events = vec![placed(), migrated];
        let full = project(&events, &input()).unwrap();
        assert_eq!(full.ungraded.len(), 1);
        assert_eq!(full.ungraded[0].reason, REASON);
        assert!(full.topics["absolute-value"].conditional);
        assert_eq!(full.topics["absolute-value"].rep_num, 2.0);
        assert_eq!(full.xp.total, 0);
        for split in 0..=events.len() {
            let cached = project(&events[..split], &input()).unwrap();
            let resumed =
                project_incremental(&cached, &events[..split], &events[split..], &input()).unwrap();
            assert_eq!(
                canonical_blob(&resumed).unwrap(),
                canonical_blob(&full).unwrap()
            );
        }
    }
}

#[test]
fn an_ambiguous_v1_miss_keeps_its_observation_until_an_explicit_review_correction() {
    let original = attempt(1, false);
    let original_bytes = original.to_canonical_json().unwrap();
    let Event::Attempt(body) = &original else {
        panic!("attempt fixture");
    };
    assert_eq!(body.outcome, AttemptOutcome::Incorrect);
    assert!(matches!(
        check_contract(
            &body.problem.expected,
            &body.given_answer,
            AnswerContract::Exact
        ),
        Outcome::Undecidable(_)
    ));
    let before = project(&[placed(), original.clone()], &input()).unwrap();
    assert!(
        before.ungraded.is_empty(),
        "v1 stores no uncertainty; the migration invents none"
    );
    assert!(!before.topics["absolute-value"].conditional);

    let correction = Event::from_json(&json!({
        "type":"regraded","ts":"2026-07-14T12:03:00Z","task_id":"t1","topic":"absolute-value",
        "reason":"Review of the preserved historical response found unresolved ambiguity.",
        "attempts":[{"attempt_id":"legacy-ambiguous","work_quality":"nearly_passable","outcome":{"ungraded":{"reason":REASON}}}]
    }).to_string()).unwrap();
    let history = [placed(), original.clone(), correction];
    // A correction rebuilds the whole projection; it never edits the original log row.
    let replayed: Vec<Event> = history
        .iter()
        .map(|e| Event::from_json(&e.to_canonical_json().unwrap()).unwrap())
        .collect();
    let after = project(&replayed, &input()).unwrap();
    assert_eq!(after.ungraded.len(), 1);
    assert_eq!(after.ungraded[0].attempt_id, "legacy-ambiguous");
    assert_eq!(after.ungraded[0].reason, REASON);
    assert!(after.topics["absolute-value"].conditional);
    assert_eq!(after.topics["absolute-value"].rep_num, 2.0);
    assert_eq!(original.to_canonical_json().unwrap(), original_bytes);
}
