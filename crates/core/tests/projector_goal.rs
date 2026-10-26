//! The learner sets the daily XP goal (D-XP1).
//!
//! The latest valid `goal_set` event sets the goal for the whole log. With no such
//! event, the goal is `ProjectionInput.goal`. A value outside 10 to 100 is ignored.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use cadus_core::event::Event;
use cadus_core::learner::LearnerModel;
use cadus_core::projector::{PROJECTOR_VERSION, ProjectionInput, project, project_incremental};
use common::events::{event, input};

/// One review result worth 50 XP on `day` (May 2026).
fn review_50(day: u32, topic: &str) -> Event {
    event(&format!(
        r#"{{"assisted":false,"passed":true,"quality_tier":"perfect","session":"s-goal","topic":"{topic}","ts":"2026-05-{day:02}T10:00:00Z","type":"review_result","v":1,"weighted_score":0.9,"xp":50.0}}"#
    ))
}

/// One review result worth `xp` XP on `day` (May 2026).
fn review_xp(day: u32, topic: &str, xp: u32) -> Event {
    event(&format!(
        r#"{{"assisted":false,"passed":true,"quality_tier":"perfect","session":"s-goal","topic":"{topic}","ts":"2026-05-{day:02}T10:00:00Z","type":"review_result","v":1,"weighted_score":0.9,"xp":{xp}.0}}"#
    ))
}

/// One `goal_set` event on `day` (May 2026) with the value `goal`.
fn goal_set(day: u32, goal: i64) -> Event {
    event(&format!(
        r#"{{"goal":{goal},"session":"s-goal","ts":"2026-05-{day:02}T11:00:00Z","type":"goal_set","v":1}}"#
    ))
}

/// Two days of 50 XP each.
fn two_days() -> Vec<Event> {
    vec![
        review_50(3, "absolute-value"),
        review_50(4, "adding-integers"),
    ]
}

fn fold(events: &[Event]) -> LearnerModel {
    project(events, &input()).expect("the fold succeeds")
}

#[test]
fn a_goal_set_of_60_sets_the_goal_and_a_50_xp_day_misses() {
    let mut events = two_days();
    events.push(goal_set(4, 60));
    let model = fold(&events);
    assert_eq!(model.xp.goal, 60);
    assert_eq!(model.xp.total, 100);
    assert_eq!(model.xp.streak_days, 0);
    assert_eq!(model.projector_version, Some(PROJECTOR_VERSION));
}

#[test]
fn the_same_days_hit_at_the_default_goal_of_40() {
    let model = fold(&two_days());
    assert_eq!(model.xp.goal, 40);
    assert_eq!(model.xp.streak_days, 2);
}

#[test]
fn the_new_goal_applies_to_the_days_before_it() {
    // Two days of 30 XP miss the default goal of 40. The goal_set sits after both days.
    // It still decides both of them: a goal of 20 gives a streak of 2.
    let mut events = vec![
        review_xp(3, "absolute-value", 30),
        review_xp(4, "adding-integers", 30),
    ];
    events.push(goal_set(5, 20));
    let model = fold(&events);
    assert_eq!(model.xp.goal, 20);
    assert_eq!(model.xp.streak_days, 2);
}

#[test]
fn the_latest_of_two_goal_set_events_wins() {
    let mut events = two_days();
    events.push(goal_set(4, 60));
    events.push(goal_set(4, 30));
    assert_eq!(fold(&events).xp.goal, 30);
    assert_eq!(fold(&events).xp.streak_days, 2);
}

#[test]
fn a_value_outside_10_to_100_keeps_the_earlier_goal() {
    let mut events = two_days();
    events.push(goal_set(4, 60));
    events.push(goal_set(4, 5));
    events.push(goal_set(4, 500));
    assert_eq!(fold(&events).xp.goal, 60);

    let mut only_bad = two_days();
    only_bad.push(goal_set(4, 5));
    assert_eq!(fold(&only_bad).xp.goal, 40);
}

#[test]
fn the_bounds_10_and_100_are_valid() {
    let mut low = two_days();
    low.push(goal_set(4, 10));
    assert_eq!(fold(&low).xp.goal, 10);
    let mut high = two_days();
    high.push(goal_set(4, 100));
    assert_eq!(fold(&high).xp.goal, 100);
}

#[test]
fn a_stream_with_no_goal_set_keeps_the_input_goal() {
    let base = input();
    let with_goal = ProjectionInput::new(base.graph, base.cfg, base.now).with_goal(25);
    let model = project(&two_days(), &with_goal).expect("the fold succeeds");
    assert_eq!(model.xp.goal, 25);
}

#[test]
fn the_incremental_fold_equals_the_full_fold_for_a_goal_set_in_either_half() {
    let mut events = two_days();
    events.push(goal_set(4, 60));
    events.push(review_50(5, "absolute-value"));
    let full = fold(&events);
    let base = input();
    for split in 0..=events.len() {
        let (prior, new) = events.split_at(split);
        let cached = project(prior, &base).expect("the prior fold succeeds");
        let resumed = project_incremental(&cached, prior, new, &base).expect("the resume succeeds");
        assert_eq!(resumed.xp, full.xp, "split {split}");
        assert_eq!(resumed.xp.goal, 60);
    }
}

#[test]
fn the_wire_form_round_trips_and_names_its_type() {
    let source = goal_set(4, 60);
    assert_eq!(source.type_name(), "goal_set");
    let text = serde_json::to_string(&source).expect("the event serializes");
    let back = Event::from_json(&text).expect("the event reads");
    assert_eq!(back, source);
    assert!(Event::TYPE_NAMES.contains(&"goal_set"));
}
