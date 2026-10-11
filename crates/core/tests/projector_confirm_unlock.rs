//! A passed confirmation item of a failed knowledge point passes that point
//! (D-L2, projector version 9). The last open point also completes the topic.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use cadus_core::config::Config;
use cadus_core::curriculum::Curriculum;
use cadus_core::event::{Event, KpProgress, Timestamp, TopicStatus};
use cadus_core::learner::LearnerModel;
use cadus_core::projector::{ProjectionInput, project};
use cadus_core::selector::retry_available_at;
use common::events::event;
use common::{graph, knowledge_point, topic};

/// One topic `t` with three knowledge points.
fn three_point_graph() -> Curriculum {
    let mut t = topic("t", &[], 0.3, &[]);
    t.knowledge_points = vec![
        knowledge_point("kp1", &[]),
        knowledge_point("kp2", &[]),
        knowledge_point("kp3", &[]),
    ];
    graph(vec![t])
}

/// A failed lesson at `failed_kp`, then a passed confirmation item of that point.
fn stream(failed_kp: &str) -> Vec<Event> {
    let task_id = format!("s1-review-t-confirm-{failed_kp}");
    vec![
        event(&format!(
            r#"{{"type":"lesson_result","ts":"2026-10-11T10:00:00Z","topic":"t","passed":false,
                "failed_at_kp":"{failed_kp}","quality_tier":"poor"}}"#
        )),
        event(&format!(
            r#"{{"type":"task_served","ts":"2026-10-11T10:05:00Z","task_id":"{task_id}",
                "task_type":"review","topic":"t"}}"#
        )),
        event(&format!(
            r#"{{"type":"attempt","ts":"2026-10-11T10:06:00Z","attempt_id":"a1","task_id":"{task_id}",
                "topic":"t","task_type":"review","problem":{{"text":"q","expected":"a"}},
                "given_answer":"a","correct":true,"secs":20,"work_quality":"perfect"}}"#
        )),
        event(&format!(
            r#"{{"type":"review_result","ts":"2026-10-11T10:07:00Z","topic":"t","passed":true,
                "weighted_score":1.0,"quality_tier":"perfect","task_id":"{task_id}"}}"#
        )),
    ]
}

fn fold(events: &[Event]) -> (LearnerModel, Config) {
    let graph = three_point_graph();
    let cfg = Config::default();
    let now = Timestamp::parse("2026-10-11T11:00:00Z").unwrap();
    let input = ProjectionInput::new(&graph, &cfg, now);
    (project(events, &input).expect("the fold succeeds"), cfg)
}

#[test]
fn a_passed_confirmation_of_the_last_open_point_completes_the_topic() {
    let (model, cfg) = fold(&stream("kp3"));
    let state = &model.topics["t"];
    assert_eq!(state.kp_progress["kp3"], KpProgress::Passed);
    assert_eq!(state.status, TopicStatus::Learning);
    assert_eq!(retry_available_at(state, &cfg), None);
}

#[test]
fn a_passed_confirmation_of_an_early_point_leaves_the_topic_open() {
    let (model, cfg) = fold(&stream("kp1"));
    let state = &model.topics["t"];
    assert_eq!(state.kp_progress["kp1"], KpProgress::Passed);
    assert!(!state.kp_progress.contains_key("kp2"));
    assert_eq!(state.status, TopicStatus::Untouched);
    assert_eq!(retry_available_at(state, &cfg), None);
}
