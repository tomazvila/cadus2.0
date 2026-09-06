//! D-F6: the mastery split, the honest progress numbers, and the confirmation
//! item of an inferred topic (audit finding l, book p.377).
//!
//! Every expectation here is a 2.0 behavior. The 1.0 rule stands beside it in
//! the tests that set `mastery.confirm_inferred` to false, and the last test
//! folds a committed 1.0 fixture with that flag off.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

mod common;

use std::collections::{BTreeMap, BTreeSet};

use cadus_core::config::Config;
use cadus_core::curriculum::Curriculum;
use cadus_core::event::{Event, TaskType, Timestamp, TopicStatus};
use cadus_core::learner::TopicState;
use cadus_core::projector::{ProjectionInput, blob_digest, project};
use cadus_core::selector::{
    REMEDIATION_CONFIRM_FAILED, SessionContext, SessionPlan, Task, compose_session, confirmations,
    is_course_complete, is_inferred, is_known, is_practiced, practiced_set,
};
use cadus_core::xp::{course_counts, course_progress};
use common::T_US;
use common::parity::{GOAL, assert_folds_to, row};
use common::selector::{LearnedSpec, graph_of, sampler, states_of, topic};

// --------------------------------------------------------------------------- //
// The predicates
// --------------------------------------------------------------------------- //

/// A state of `status`, with a memory of `memory` at `T`.
fn at(status: TopicStatus, memory: f64) -> TopicState {
    LearnedSpec::new(memory).status(status).build()
}

#[test]
fn the_predicates_split_practice_from_inference() {
    let cases = [
        (TopicStatus::Untouched, false, false, false),
        (TopicStatus::Frontier, false, false, false),
        (TopicStatus::Learning, true, true, false),
        (TopicStatus::Placed, false, true, true),
        (TopicStatus::Floor, false, true, true),
    ];
    for (status, practiced, known, inferred) in cases {
        let state = at(status, 0.5);
        assert_eq!(is_practiced(&state), practiced, "practiced {status:?}");
        assert_eq!(is_known(&state), known, "known {status:?}");
        assert_eq!(is_inferred(&state), inferred, "inferred {status:?}");
        // The deprecated alias answers exactly what `is_known` answers.
        #[expect(deprecated, reason = "the alias stands for one release (D-F6)")]
        let alias = cadus_core::xp::is_mastered(&state);
        assert_eq!(alias, known, "the alias of {status:?}");
    }
}

// --------------------------------------------------------------------------- //
// Completion and progress
// --------------------------------------------------------------------------- //

/// A three-topic course: one practiced, one placed, one on the floor.
fn mixed() -> (Curriculum, BTreeMap<String, TopicState>) {
    let graph = graph_of(
        vec![
            topic("done").build(),
            topic("placed").build(),
            topic("floored").build(),
        ],
        &[],
    );
    let states = states_of(vec![
        ("done", at(TopicStatus::Learning, 0.9)),
        ("placed", at(TopicStatus::Placed, 0.9)),
        ("floored", at(TopicStatus::Floor, 0.9)),
    ]);
    (graph, states)
}

#[test]
fn the_counts_report_practice_and_inference_apart() {
    let (graph, states) = mixed();
    let counts = course_counts(&states, &graph, "c");
    assert_eq!(counts.practiced, 1);
    assert_eq!(counts.inferred, 2);
    assert_eq!(counts.total, 3);
    assert_eq!(practiced_set(&states, &graph).sorted_ids(&graph), ["done"]);
}

#[test]
fn progress_counts_practice_only_and_the_flag_restores_the_1_0_rule() {
    let (graph, states) = mixed();
    let cfg = Config::default();
    assert!((course_progress(&states, &graph, "c", &cfg) - 1.0 / 3.0).abs() < 1e-12);

    let mut old_rule = Config::default();
    old_rule.mastery.confirm_inferred = false;
    assert!((course_progress(&states, &graph, "c", &old_rule) - 1.0).abs() < 1e-12);
}

#[test]
fn a_placed_course_is_not_complete_until_the_learner_practices_it() {
    let (graph, states) = mixed();
    let cfg = Config::default();
    assert!(!is_course_complete(&states, &graph, &cfg, Some("c"), None));

    let mut old_rule = Config::default();
    old_rule.mastery.confirm_inferred = false;
    assert!(is_course_complete(
        &states,
        &graph,
        &old_rule,
        Some("c"),
        None
    ));

    let all_done = states_of(vec![
        ("done", at(TopicStatus::Learning, 0.9)),
        ("placed", at(TopicStatus::Learning, 0.9)),
        ("floored", at(TopicStatus::Learning, 0.9)),
    ]);
    assert!(is_course_complete(&all_done, &graph, &cfg, Some("c"), None));
}

// --------------------------------------------------------------------------- //
// The confirmation schedule
// --------------------------------------------------------------------------- //

/// A four-topic course of placed topics, weakest memory first: `p1` 0.70,
/// `p2` 0.80, `p3` 0.90, `p4` 0.95.
///
/// Every memory stays above the nearly-due band, so no topic owes a plain
/// review and the confirmation item is the only task the topic owes.
fn four_placed() -> (Curriculum, BTreeMap<String, TopicState>) {
    let graph = graph_of(
        vec![
            topic("p1").build(),
            topic("p2").build(),
            topic("p3").build(),
            topic("p4").build(),
        ],
        &[],
    );
    let states = states_of(vec![
        ("p1", at(TopicStatus::Placed, 0.70)),
        ("p2", at(TopicStatus::Placed, 0.80)),
        ("p3", at(TopicStatus::Placed, 0.90)),
        ("p4", at(TopicStatus::Placed, 0.95)),
    ]);
    (graph, states)
}

#[test]
fn two_confirmations_per_session_go_to_the_nearest_due_memories() {
    let (graph, states) = four_placed();
    let cfg = Config::default();
    let none: BTreeSet<String> = BTreeSet::new();
    assert_eq!(
        confirmations(&states, &graph, &cfg, T_US, Some("c"), &none),
        ["p1", "p2"]
    );
    // A topic the plan already serves waits for the next session.
    let busy: BTreeSet<String> = ["p1".to_owned()].into();
    assert_eq!(
        confirmations(&states, &graph, &cfg, T_US, Some("c"), &busy),
        ["p2", "p3"]
    );
}

#[test]
fn the_flag_off_owes_no_confirmation_and_a_zero_rate_owes_none() {
    let (graph, states) = four_placed();
    let none: BTreeSet<String> = BTreeSet::new();
    let mut off = Config::default();
    off.mastery.confirm_inferred = false;
    assert!(confirmations(&states, &graph, &off, T_US, Some("c"), &none).is_empty());

    let mut zero = Config::default();
    zero.mastery.max_per_session = 0;
    assert!(confirmations(&states, &graph, &zero, T_US, Some("c"), &none).is_empty());
}

/// Compose one session over `states` and `graph` with `cfg`.
fn compose(states: &BTreeMap<String, TopicState>, graph: &Curriculum, cfg: &Config) -> SessionPlan {
    let ctx = SessionContext::default()
        .with_session_id("s1")
        .with_course(Some("c"));
    compose_session(states, graph, cfg, T_US, &mut sampler(1), &ctx)
}

/// The confirmation tasks of a plan, in serve order.
fn confirm_tasks(plan: &SessionPlan) -> Vec<&Task> {
    plan.tasks.iter().filter(|task| task.confirm).collect()
}

#[test]
fn the_plan_serves_the_confirmation_as_a_one_item_review() {
    let (graph, states) = four_placed();
    let plan = compose(&states, &graph, &Config::default());
    let confirm = confirm_tasks(&plan);
    assert_eq!(confirm.len(), 2);
    assert_eq!(confirm[0].topic.as_deref(), Some("p1"));
    assert_eq!(confirm[0].task_type, TaskType::Review);
    assert_eq!(confirm[0].n_problems, Some(1));
    assert_eq!(confirm[0].task_id, "s1-review-p1");
    assert!(confirm[0].why.starts_with("confirmation;"));
    // The plan of a placed-only course reports no completion (D-F6).
    assert!(!plan.course_complete);

    let mut off = Config::default();
    off.mastery.confirm_inferred = false;
    assert!(confirm_tasks(&compose(&states, &graph, &off)).is_empty());
}

// --------------------------------------------------------------------------- //
// The two outcomes, in the fold
// --------------------------------------------------------------------------- //

/// The curriculum the fold tests read: course `c` with the floor topic `base`
/// and one plain topic `p`.
fn fold_tree() -> Curriculum {
    graph_of(vec![topic("base").build(), topic("p").build()], &[])
}

/// One event from its wire JSON.
fn ev(json: &str) -> Event {
    Event::from_json(json).expect("the test event reads")
}

/// The stream that places `p`, serves its confirmation, and closes the review
/// with `passed`.
fn confirmation_stream(passed: bool) -> Vec<Event> {
    vec![
        ev(r#"{"type":"diagnostic_placed","ts":"2026-07-14T12:00:00Z","balances":{"p":2.0}}"#),
        ev(
            r#"{"type":"task_served","ts":"2026-07-14T12:01:00Z","task_id":"s1-review-p","task_type":"review","topic":"p","confirm":true}"#,
        ),
        ev(&format!(
            r#"{{"type":"review_result","ts":"2026-07-14T12:02:00Z","task_id":"s1-review-p","topic":"p","passed":{passed},"weighted_score":{score},"quality_tier":"{tier}","xp":5.0}}"#,
            passed = passed,
            score = if passed { 1.0 } else { 0.0 },
            tier = if passed { "perfect" } else { "poor" },
        )),
    ]
}

/// Fold `events` over [`fold_tree`] at the default config.
fn fold(events: &[Event]) -> cadus_core::learner::LearnerModel {
    let graph = fold_tree();
    let cfg = Config::default();
    let now = Timestamp::parse("2026-07-15T00:00:00Z").expect("the instant parses");
    let input = ProjectionInput::new(&graph, &cfg, now);
    project(events, &input).expect("the fold succeeds")
}

#[test]
fn a_passed_confirmation_moves_the_topic_to_learning() {
    let model = fold(&confirmation_stream(true));
    let state = model.topics.get("p").expect("the topic folded");
    assert_eq!(state.status, TopicStatus::Learning);
    assert!(is_practiced(state));
    assert!(model.pending_remediation.is_empty());
}

#[test]
fn a_failed_confirmation_keeps_placed_and_schedules_the_lesson() {
    let model = fold(&confirmation_stream(false));
    let state = model.topics.get("p").expect("the topic folded");
    assert_eq!(state.status, TopicStatus::Placed);
    assert!(is_inferred(state));
    let queued = &model.pending_remediation;
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].kind, REMEDIATION_CONFIRM_FAILED);
    assert_eq!(queued[0].targets[0].as_str(), "p");

    // The queue serves the LESSON, not another review: the inference just failed.
    let graph = fold_tree();
    let ctx = SessionContext::default()
        .with_session_id("s1")
        .with_course(Some("c"))
        .with_pending_remediation(queued);
    let plan = compose_session(
        &model.topics,
        &graph,
        &Config::default(),
        T_US,
        &mut sampler(1),
        &ctx,
    );
    let first = &plan.tasks[0];
    assert!(first.is_remediation);
    assert_eq!(first.task_type, TaskType::Lesson);
    assert_eq!(first.topic.as_deref(), Some("p"));
    // The topic waits behind its own lesson: no second confirmation this session.
    assert!(confirm_tasks(&plan).is_empty());
}

#[test]
fn a_closed_confirmation_leaves_the_re_served_plan() {
    let (graph, states) = four_placed();
    let cfg = Config::default();
    let open = compose(&states, &graph, &cfg);
    assert_eq!(confirm_tasks(&open).len(), 2);

    // The learner answered `s1-review-p1`, so the item is closed and the
    // re-serve drops it. `p2` is still open and keeps its place and its id.
    let closed: BTreeSet<String> = ["s1-review-p1".to_owned()].into();
    let ctx = SessionContext::default()
        .with_session_id("s1")
        .with_course(Some("c"))
        .with_open_plan(Some(&open))
        .with_multistep(0, &closed);
    let again = compose_session(&states, &graph, &cfg, T_US, &mut sampler(1), &ctx);
    let kept = confirm_tasks(&again);
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].task_id, "s1-review-p2");
}

#[test]
fn a_review_that_is_not_a_confirmation_still_changes_no_status() {
    let plain = vec![
        ev(r#"{"type":"diagnostic_placed","ts":"2026-07-14T12:00:00Z","balances":{"p":2.0}}"#),
        ev(
            r#"{"type":"task_served","ts":"2026-07-14T12:01:00Z","task_id":"s1-review-p","task_type":"review","topic":"p"}"#,
        ),
        ev(
            r#"{"type":"review_result","ts":"2026-07-14T12:02:00Z","task_id":"s1-review-p","topic":"p","passed":true,"weighted_score":1.0,"quality_tier":"perfect","xp":5.0}"#,
        ),
    ];
    let model = fold(&plain);
    let state = model.topics.get("p").expect("the topic folded");
    assert_eq!(state.status, TopicStatus::Placed);
}

// --------------------------------------------------------------------------- //
// The parity fixture with the flag off
// --------------------------------------------------------------------------- //

#[test]
fn a_1_0_stream_folds_to_its_committed_digest_with_the_flag_off() {
    // `common::events::cfg` carries `mastery.confirm_inferred = false`, which is
    // the 1.0 progress rule the committed digest holds.
    let entry = row(1);
    assert_folds_to(&entry.stream, None, &entry.digests["UTC"]);
    assert_eq!(GOAL, 40);
}

#[test]
fn the_same_stream_folds_to_a_different_progress_with_the_flag_on() {
    let events = common::events::stream("stream_1.jsonl");
    let graph = common::events::tree();
    let now = Timestamp::parse("2026-08-30T09:00:00Z").expect("the instant parses");

    let mut old_rule = Config::default();
    old_rule.mastery.confirm_inferred = false;
    let one_zero = project(
        &events,
        &ProjectionInput::new(graph, &old_rule, now).with_goal(GOAL),
    )
    .expect("the fold succeeds");

    let new_rule = Config::default();
    let two_zero = project(
        &events,
        &ProjectionInput::new(graph, &new_rule, now).with_goal(GOAL),
    )
    .expect("the fold succeeds");

    assert!(two_zero.velocity.course_progress <= one_zero.velocity.course_progress);
    assert_ne!(
        blob_digest(&one_zero).unwrap(),
        blob_digest(&two_zero).unwrap()
    );
}
