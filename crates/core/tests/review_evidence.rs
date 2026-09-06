//! D-F7 review evidence and D-F8 delayed confirmation regressions.
#![allow(clippy::unwrap_used)]
use cadus_core::{
    config::Config,
    event::{Attempt, Event},
    fire::assess_review,
};
use serde_json::json;
mod common;

fn attempt(correct: bool, skill: &str) -> Attempt {
    serde_json::from_value(json!({
        "ts":"2026-09-06T12:00:00Z", "attempt_id":"s-review-topic-1", "task_id":"s-review-topic",
        "topic":"topic", "kp":"kp1", "task_type":"review", "problem":{"text":"Give 2", "expected":"2"},
        "given_answer":if correct {"2"} else {"3"}, "correct":correct, "secs":10,
        "work_quality":"perfect", "skills":[skill]
    })).unwrap()
}

#[test]
fn conflicting_trajectory_requests_each_exercised_skill() {
    let items = [
        attempt(false, "topic/kp1"),
        attempt(false, "topic/kp2"),
        attempt(true, "topic/kp2"),
    ];
    let result = assess_review(&items.iter().collect::<Vec<_>>(), &Config::default());
    assert!(result.inconclusive && !result.passed);
    assert_eq!(result.score, 0.5);
    assert_eq!(result.confirmation_skills, ["topic/kp1", "topic/kp2"]);
}

#[test]
fn consistent_and_ungraded_evidence_have_separate_outcomes() {
    let mut item = attempt(true, "topic/kp1");
    assert!(assess_review(&[&item], &Config::default()).passed);
    item.assisted = true;
    assert!(assess_review(&[&item], &Config::default()).inconclusive);
    item.assisted = false;
    item.outcome = cadus_core::event::AttemptOutcome::Ungraded {
        reason: "unknown".to_owned(),
    };
    let result = assess_review(&[&item], &Config::default());
    assert!(result.inconclusive && !result.passed);
    assert_eq!(result.confirmation_skills, ["topic/kp1"]);
    assert!(assess_review(&[], &Config::default()).inconclusive);
}

#[test]
fn historic_review_round_trip_omits_new_optional_fields() {
    let old = r#"{"type":"review_result","ts":"2026-09-06T12:00:00Z","topic":"topic","passed":true,"weighted_score":1.0,"quality_tier":"perfect"}"#;
    let event = Event::from_json(old).unwrap();
    let value = serde_json::to_value(event).unwrap();
    assert!(value.get("confirmation_skills").is_none());
    assert!(value.get("inconclusive").is_none());
}

#[test]
fn one_skill_does_not_hide_another_skills_miss() {
    let items = [
        attempt(false, "topic/kp1"),
        attempt(true, "topic/kp2"),
        attempt(true, "topic/kp2"),
    ];
    let result = assess_review(&items.iter().collect::<Vec<_>>(), &Config::default());
    assert!(result.inconclusive);
    assert_eq!(result.confirmation_skills, ["topic/kp1"]);
}

#[test]
fn delayed_feedback_confirmation_waits_for_three_intervening_task_closes() {
    use cadus_core::event::Timestamp;
    use cadus_core::projector::Projector;
    let (graph, cfg) = projector_inputs();
    let mut projector = Projector::new(&graph, &cfg);
    let mut fresh = attempt(true, "topic/kp1");
    fresh.independent_after_feedback = true;
    projector.apply(&Event::Attempt(fresh), true);
    let mut early = attempt(true, "topic/kp1");
    early.task_id = "early-review-topic-confirm-kp1".to_owned();
    early.ts = Timestamp::from_micros(early.ts.micros() + 30_000_000);
    projector.apply(&Event::Attempt(early), true);
    // The original task closes first, followed by three intervening tasks.
    for index in 0..4 {
        assert!(projector.pending_remediation().is_empty());
        let close = Event::from_json(
            &json!({
                "type":"review_result", "ts":format!("2026-09-06T12:0{}:00Z", index+1),
                "topic":"topic", "task_id":format!("task-{index}"), "passed":true,
                "weighted_score":1.0, "quality_tier":"perfect", "xp":0.0
            })
            .to_string(),
        )
        .unwrap();
        projector.apply(&close, true);
        projector.apply(&close, true);
    }
    let queue = projector.pending_remediation();
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].kind, "review_confirmation:kp1");
    let mut confirmation = attempt(true, "topic/kp1");
    confirmation.task_id = "next-review-topic-confirm-kp1".to_owned();
    confirmation.ts = Timestamp::from_micros(confirmation.ts.micros() + 600_000_000);
    projector.apply(&Event::Attempt(confirmation), true);
    assert!(projector.pending_remediation().is_empty());
}

#[test]
fn inconclusive_review_does_not_change_memory_or_award_xp() {
    use cadus_core::projector::Projector;
    let (graph, cfg) = projector_inputs();
    let mut projector = Projector::new(&graph, &cfg);
    let event = Event::from_json(r#"{"type":"review_result","ts":"2026-09-06T12:00:00Z","topic":"topic","passed":false,"weighted_score":0.5,"quality_tier":"perfect","inconclusive":true,"xp":100}"#).unwrap();
    projector.apply(&event, true);
    let model = projector
        .finalize(cadus_core::event::Timestamp::from_micros(0))
        .unwrap();
    assert!(model.topics.is_empty());
    assert_eq!(model.xp.total, 0);
}

#[test]
fn each_uncertain_kp_gets_a_distinct_one_item_task() {
    use cadus_core::event::Slug;
    use cadus_core::learner::PendingRemediation;
    use cadus_core::selector::{assign_ids, remediation_tasks};
    use common::selector::{graph_of, kp, topic};
    use std::collections::BTreeMap;
    let graph = graph_of(
        vec![
            topic("topic")
                .kps(vec![kp("kp1", &[]), kp("kp2", &[])])
                .build(),
        ],
        &[],
    );
    let pending: Vec<_> = ["kp1", "kp2"]
        .iter()
        .map(|kp| PendingRemediation {
            kind: format!("review_confirmation:{kp}"),
            targets: vec![Slug::new("topic").unwrap()],
        })
        .collect();
    let mut tasks = remediation_tasks(&pending, &BTreeMap::new(), &graph, &Config::default());
    assign_ids(&mut tasks, "next");
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0].task_id, "next-review-topic-confirm-kp1");
    assert_eq!(tasks[1].task_id, "next-review-topic-confirm-kp2");
    assert!(
        tasks
            .iter()
            .all(|task| task.n_problems == Some(1) && task.mix.is_empty())
    );
}

#[test]
fn a_final_miss_does_not_override_a_passing_weighted_score() {
    let items: Vec<_> = [true, true, true, true, false]
        .iter()
        .map(|correct| attempt(*correct, "topic/kp1"))
        .collect();
    let result = assess_review(&items.iter().collect::<Vec<_>>(), &Config::default());
    assert!(result.score >= Config::default().review.pass_weighted);
    assert!(result.inconclusive && !result.passed);
    assert_eq!(result.confirmation_skills, ["topic/kp1"]);
}

fn projector_inputs() -> (cadus_core::curriculum::Curriculum, Config) {
    use common::selector::{graph_of, topic};
    (
        graph_of(vec![topic("topic").build()], &[]),
        Config::default(),
    )
}

#[test]
fn supplemental_practice_never_changes_the_original_review_score() {
    let original = attempt(true, "topic/kp1");
    let mut practice = attempt(false, "topic/kp2");
    practice.feedback_practice = true;
    practice.assisted = true;
    let result = assess_review(&[&original, &practice], &Config::default());
    assert!(result.passed && !result.inconclusive);
    assert_eq!(result.score, 1.0);
    assert!(result.confirmation_skills.is_empty());
}
