//! Narrow integrated credit, duplicate protection, and cached-replay evidence.

use super::*;
use crate::config::Config;
use crate::curriculum::Curriculum;
use crate::event::{Event, IntegratedField, SchemaVersion, Timestamp};
use crate::fire::testing::{T_US, graph, knowledge_point, topic};
use crate::learner::{LearnerModel, TopicState};
use crate::projector::{ProjectionInput, canonical_blob, project, project_incremental};

fn tree() -> Curriculum {
    let mut a = topic("a", &[]);
    a.knowledge_points.push(knowledge_point("kp2", &[]));
    graph(vec![a, topic("b", &[("a", 0.8, true)])])
}

fn field(id: &str, skill: &str, outcome: AttemptOutcome) -> IntegratedField {
    IntegratedField {
        id: id.into(),
        answer: "2".into(),
        contract: AnswerContract::Exact,
        outcome,
        assisted: false,
        skills: vec![skill.into()],
    }
}

fn attempt() -> IntegratedAttempt {
    IntegratedAttempt {
        ts: Timestamp::from_micros(T_US),
        session: Some("s1".into()),
        v: SchemaVersion::current(),
        attempt_id: "attempt1".into(),
        task_id: "task1".into(),
        item_id: "item1".into(),
        item_digest: "digest".into(),
        topic: "a".into(),
        method: None,
        method_correct: None,
        steps: vec![field("step", "a/kp1", AttemptOutcome::Correct)],
        final_field: field("final", "a/kp2", AttemptOutcome::Incorrect),
        skills_credited: vec!["a/kp1".into(), "a/kp2".into(), "b/kp1".into()],
        solved: false,
        assisted: false,
        grade: None,
        reasoning_ungraded: Some("ungraded prose".into()),
    }
}

#[test]
fn integrated_partial_credit_persists_only_the_decided_credited_kp() {
    let graph = tree();
    let cfg = Config::default();
    let input = ProjectionInput::new(&graph, &cfg, Timestamp::from_micros(T_US));
    let event = Event::IntegratedAttempt(attempt());
    let model = project(&[event], &input).unwrap();
    let state = &model.topics["a"];
    assert_eq!(state.kp_progress.len(), 1);
    assert_eq!(state.kp_progress["kp1"], KpProgress::Passed);
    assert!(!model.topics.contains_key("b"));
    let mut without_kp = state.clone();
    without_kp.kp_progress.clear();
    assert_eq!(without_kp, TopicState::default());
    assert_eq!(model.xp.total, 0);
    let restored: LearnerModel =
        serde_json::from_str(&serde_json::to_string(&model).unwrap()).unwrap();
    assert_eq!(restored.topics, model.topics);
}

#[test]
fn integrated_ungraded_incorrect_and_uncredited_fields_change_no_progression() {
    let graph = tree();
    let cfg = Config::default();
    for outcome in [
        AttemptOutcome::Incorrect,
        AttemptOutcome::Ungraded {
            reason: "outside grammar".into(),
        },
    ] {
        let mut body = attempt();
        body.steps[0].outcome = outcome;
        let mut projector = Projector::new(&graph, &cfg);
        projector.apply(&Event::IntegratedAttempt(body), true);
        assert!(projector.topics.is_empty());
        assert!(projector.last_practice.is_empty());
        assert_eq!(projector.completed_tasks, 1);
    }
    let mut body = attempt();
    body.skills_credited.clear();
    let mut projector = Projector::new(&graph, &cfg);
    projector.apply(&Event::IntegratedAttempt(body), true);
    assert!(projector.topics.is_empty());
}

#[test]
fn integrated_repeated_task_keeps_the_first_receipt_credit_and_one_completion() {
    let graph = tree();
    let cfg = Config::default();
    let body = attempt();
    let mut forged_retry = body.clone();
    forged_retry.final_field.outcome = AttemptOutcome::Correct;
    forged_retry.attempt_id = "different-id-same-task".into();
    let mut projector = Projector::new(&graph, &cfg);
    projector.apply(&Event::IntegratedAttempt(body.clone()), true);
    projector.apply(&Event::IntegratedAttempt(body), true);
    projector.apply(&Event::IntegratedAttempt(forged_retry), true);
    assert_eq!(projector.completed_tasks, 1);
    assert_eq!(projector.topics["a"].kp_progress.len(), 1);
}

#[test]
fn integrated_malformed_or_unknown_keys_and_none_contract_are_never_credit() {
    let graph = tree();
    let cfg = Config::default();
    let mut body = attempt();
    body.steps[0].skills = [
        "a",
        "/kp1",
        "a/",
        "a/kp1/extra",
        " a/kp1",
        "ghost/kp1",
        "a/ghost",
    ]
    .map(str::to_owned)
    .to_vec();
    body.skills_credited = body.steps[0].skills.clone();
    let mut projector = Projector::new(&graph, &cfg);
    projector.apply(&Event::IntegratedAttempt(body), true);
    assert!(projector.topics.is_empty());
    let mut body = attempt();
    body.task_id = "task2".into();
    body.steps[0].contract = AnswerContract::None;
    projector.apply(&Event::IntegratedAttempt(body), true);
    assert!(projector.topics.is_empty());
}

#[test]
fn integrated_assistance_grants_only_the_existing_lesson_kp_marker() {
    let graph = tree();
    let cfg = Config::default();
    for (overall, field_assisted) in [(true, true), (true, false), (false, true)] {
        let mut body = attempt();
        body.assisted = overall;
        body.steps[0].assisted = field_assisted;
        let mut projector = Projector::new(&graph, &cfg);
        projector.apply(&Event::IntegratedAttempt(body), true);
        assert_eq!(projector.topics["a"].kp_progress["kp1"], KpProgress::Passed);
        assert_eq!(projector.topics["a"].rep_num, 0.0);
        assert_eq!(projector.topics["a"].ability, 0.0);
        assert!(projector.last_practice.is_empty());
        let mut lesson = Projector::new(&graph, &cfg);
        lesson.mark_kps_passed("a");
        assert_eq!(
            projector.topics["a"].kp_progress["kp1"],
            lesson.topics["a"].kp_progress["kp1"]
        );
    }
}

#[test]
fn integrated_light_replay_rebuilds_completion_and_practice_without_kp_writes() {
    let graph = tree();
    let cfg = Config::default();
    let mut projector = Projector::new(&graph, &cfg);
    let event = Event::IntegratedAttempt(attempt());
    projector.apply(&event, false);
    projector.apply(&event, false);
    assert_eq!(projector.completed_tasks, 1);
    assert_eq!(projector.last_practice["a/kp1"], T_US);
    assert!(projector.topics.is_empty());
}

#[test]
fn integrated_full_replay_matches_every_incremental_split_including_duplicates() {
    let graph = tree();
    let cfg = Config::default();
    let input = ProjectionInput::new(&graph, &cfg, Timestamp::from_micros(T_US));
    let first = attempt();
    let mut second = first.clone();
    second.task_id = "task2".into();
    second.final_field.outcome = AttemptOutcome::Correct;
    second.assisted = true;
    let events = vec![
        Event::IntegratedAttempt(first.clone()),
        Event::IntegratedAttempt(second),
        Event::IntegratedAttempt(first),
    ];
    let full = project(&events, &input).unwrap();
    for split in 0..=events.len() {
        let cached = project(&events[..split], &input).unwrap();
        let resumed =
            project_incremental(&cached, &events[..split], &events[split..], &input).unwrap();
        assert_eq!(
            canonical_blob(&full).unwrap(),
            canonical_blob(&resumed).unwrap(),
            "split {split}"
        );
    }
}

#[test]
fn integrated_final_credit_resolves_its_own_topic_without_implicit_neighbors() {
    let graph = tree();
    let cfg = Config::default();
    let mut body = attempt();
    body.final_field = field("final", "b/kp1", AttemptOutcome::Correct);
    body.skills_credited = vec!["b/kp1".into()];
    let mut projector = Projector::new(&graph, &cfg);
    projector.apply(&Event::IntegratedAttempt(body), true);
    assert_eq!(projector.topics.len(), 1);
    assert_eq!(projector.topics["b"].kp_progress["kp1"], KpProgress::Passed);
    assert_eq!(projector.topics["b"].rep_num, 0.0);
    assert!(!projector.last_practice.contains_key("a/kp1"));
}
