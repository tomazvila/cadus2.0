//! Delay, freshness, and replay keep integrated transfer evidence independent.
#![allow(clippy::unwrap_used)]
use super::*;
use crate::event::Event;
use crate::fire::testing::{graph, topic};
use crate::projector::{ProjectionInput, project, project_incremental};
use serde_json::json;

fn item(id: &str) -> super::super::IntegratedItem {
    serde_json::from_value(json!({
        "id":id,"title":id,"course":"c1","topic":"a","component_topics":["a"],
        "domain":"rates_units","scenario":format!("Scenario {id}"),
        "given":[{"label":"Count","value":"2"}],
        "steps":[{"id":"work","ask":{"prompt":"How much?","answer":"2"},"skills":["a/kp1"]},{"id":"capacity","ask":{"prompt":"How many?","answer":"2"},"skills":["a/kp1"]}],
        "final":{"ask":{"prompt":"Final?","answer":"2"},"interpretation":"Two.","skills":["a/kp1"]}
    })).unwrap()
}
fn attempt() -> IntegratedAttempt {
    serde_json::from_value(json!({
        "ts":Timestamp::from_micros(DAY_US), "session":"s1", "task_id":"t1", "attempt_id":"a1",
        "item_id":"source", "item_digest":item("source").digest(),"topic":"a","steps":[],
        "final_field":{"id":"final","answer":"2","contract":{"kind":"exact"},"outcome":"correct","assisted":false,"skills":["a/kp1"]},
        "solved":true,"assisted":false,"instruction_kp":"a/kp1"
    })).unwrap()
}
#[test]
fn delayed_transfer_waits_seven_days_and_requires_an_unseen_authored_item() {
    let mut state = JourneyState::default();
    state.attempted(&attempt());
    let items = IntegratedSet::from_items(vec![item("source"), item("fresh")]);
    assert!(
        state
            .task(&items, "s2", Timestamp::from_micros(8 * DAY_US - 1))
            .is_none()
    );
    let due = state
        .task(&items, "s2", Timestamp::from_micros(8 * DAY_US))
        .unwrap();
    assert_eq!(due.integrated_item_id.as_deref(), Some("fresh"));
    assert_eq!(due.integrated_assessment_of.as_deref(), Some("source"));
    assert_eq!(due.probe_delay_days, Some(7));
    state.seen.insert(item("fresh").digest());
    assert!(
        state
            .task(&items, "s2", Timestamp::from_micros(8 * DAY_US))
            .is_none()
    );
}
#[test]
fn instruction_and_independence_are_required_before_scheduling() {
    for (instruction, solved, assisted) in [
        (None, true, false),
        (Some("a/kp1"), false, false),
        (Some("a/kp1"), true, true),
    ] {
        let mut event = attempt();
        event.instruction_kp = instruction.map(str::to_owned);
        event.solved = solved;
        event.assisted = assisted;
        let mut state = JourneyState::default();
        state.attempted(&event);
        assert!(state.learned.is_empty());
    }
}
#[test]
fn full_and_incremental_replay_preserve_pinned_assessment_and_receipt() {
    let items = IntegratedSet::from_items(vec![item("source"), item("fresh")]);
    let served = IntegratedServed {
        assessment_of: Some("source".into()),
        assessment_delay_days: Some(7),
        ts: Timestamp::from_micros(8 * DAY_US),
        session: Some("s2".into()),
        v: crate::event::SchemaVersion::current(),
        task_id: "t2".into(),
        item_id: "fresh".into(),
        item_digest: item("fresh").digest(),
        topic: "a".into(),
        skills: vec![],
    };
    let mut answer = attempt();
    answer.session = Some("s2".into());
    answer.task_id = "t2".into();
    answer.attempt_id = "a2".into();
    answer.item_id = "fresh".into();
    answer.item_digest = item("fresh").digest();
    answer.assessment_of = Some("source".into());
    answer.assessment_delay_days = Some(7);
    answer.instruction_kp = None;
    answer.ts = Timestamp::from_micros(8 * DAY_US);
    let events = vec![
        Event::IntegratedAttempt(attempt()),
        Event::IntegratedServed(served),
        Event::IntegratedAttempt(answer),
    ];
    let tree = graph(vec![topic("a", &[])]);
    let cfg = crate::config::Config::default();
    let input = ProjectionInput::new(&tree, &cfg, Timestamp::from_micros(8 * DAY_US));
    let full = project(&events, &input).unwrap();
    for split in 0..=events.len() {
        let cached = project(&events[..split], &input).unwrap();
        let restored =
            project_incremental(&cached, &events[..split], &events[split..], &input).unwrap();
        assert_eq!(restored.integrated_journey, full.integrated_journey);
    }
    let task = full
        .integrated_journey
        .task(&items, "s2", input.now)
        .unwrap();
    assert_eq!(task.task_id, "t2");
    assert!(
        full.integrated_journey
            .task(&items, "s3", input.now)
            .is_none()
    );
}
