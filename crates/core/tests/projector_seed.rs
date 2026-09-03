//! U3 acceptance, part 3: the conditional peel-back, the ability seeding, and
//! the accumulator surface (spec section 8, `tests/test_peelback.py`,
//! `tests/test_ability_seeding.py`).
//!
//! Every expected value below is a LITERAL from the 1.0 test file or a number the
//! comment above it derives by hand from `config.yaml`. No expectation calls the
//! code under test.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use cadus_core::config::Config;
use cadus_core::curriculum::Curriculum;
use cadus_core::event::{Event, KpProgress, Timestamp, TopicStatus};
use cadus_core::learner::{LearnerModel, TopicState, VelocityState};
use cadus_core::projector::{ProjectionInput, Projector, blob_digest, canonical_blob, project};
use common::events::{event, now, regrade_graph};

/// `T` of the 1.0 peel-back and ability-seeding tests.
const T_SEED: &str = "2026-07-28T12:00:00Z";

/// Fold `events` over `graph` with the default config, built at `now_text`.
fn seed_fold(events: &[Event], graph: &Curriculum, now_text: &str) -> LearnerModel {
    let cfg = Config::default();
    let input = ProjectionInput::new(graph, &cfg, Timestamp::parse(now_text).unwrap());
    project(events, &input).expect("the fold succeeds")
}

/// One correct diagnostic answer on `topic` at `T_SEED`, then a placement at 2.0
/// one minute later.
fn answered_placement(topic: &str) -> Vec<Event> {
    vec![
        event(&format!(
            r#"{{"type":"diagnostic_answer","ts":"{T_SEED}","topic":"{topic}","correct":true,
                "secs":10,"weight":1.0}}"#
        )),
        event(&format!(
            r#"{{"type":"diagnostic_placed","ts":"2026-07-28T12:01:00Z","balances":{{"{topic}":2.0}}}}"#
        )),
    ]
}

// --------------------------------------------------------------------------- //
// Peel-back (spec section 8, tests/test_peelback.py)
// --------------------------------------------------------------------------- //

/// One missed `attempt` of the peel-back tests (`tests/test_peelback.py:56-68`).
fn miss(topic: &str, ts: &str) -> Event {
    event(&format!(
        r#"{{"type":"attempt","ts":"{ts}","attempt_id":"a-{topic}","task_id":"task",
            "topic":"{topic}","task_type":"review",
            "problem":{{"text":"q","expected":"a"}},"given_answer":"x","correct":false,
            "secs":40,"work_quality":"poor"}}"#
    ))
}

fn peelback_fold(events: &[Event], graph: &Curriculum) -> LearnerModel {
    seed_fold(events, graph, "2026-07-28T12:02:00Z")
}

#[test]
fn a_missed_prerequisite_peels_its_conditional_dependent() {
    // C depends on P; D is unrelated. Both C and D are placed conditional at 0.8.
    let graph = common::graph(vec![
        common::topic("P", &[], 0.3, &[]),
        common::topic("C", &[("P", 0.6, false)], 0.3, &[]),
        common::topic("D", &[], 0.3, &[]),
    ]);
    let events = vec![
        event(&format!(
            r#"{{"type":"diagnostic_placed","ts":"{T_SEED}","balances":{{"C":0.8,"D":0.8}},
                "conditional":["C","D"]}}"#
        )),
        miss("P", "2026-07-28T12:01:00Z"),
    ];
    let model = peelback_fold(&events, &graph);

    let c = &model.topics["C"];
    assert!(!c.conditional, "the dependent of a missed prereq is peeled");
    assert_eq!(c.status, TopicStatus::Placed, "halved, not un-mastered");
    // 0.8 / 2 = 0.4.
    assert!((c.rep_num - 0.4).abs() < 1e-12, "repNum {:?}", c.rep_num);
    // interval_for(0.4) with the table [2, 4.5, ...]: 2 + 0.4 x (4.5 - 2) = 3.0.
    assert!(
        (c.interval_days - 3.0).abs() < 1e-12,
        "interval {:?}",
        c.interval_days
    );

    let d = &model.topics["D"];
    assert!(d.conditional, "an unrelated conditional is untouched");
    assert!((d.rep_num - 0.8).abs() < 1e-12);
}

#[test]
fn a_miss_on_the_conditional_topic_halves_it_and_keeps_it_placed() {
    let graph = common::graph(vec![
        common::topic("P", &[], 0.3, &[]),
        common::topic("C", &[("P", 0.6, false)], 0.3, &[]),
    ]);
    let events = vec![
        event(&format!(
            r#"{{"type":"diagnostic_placed","ts":"{T_SEED}","balances":{{"C":2.0}},
                "conditional":["C"]}}"#
        )),
        miss("C", "2026-07-28T12:01:00Z"),
    ];
    let model = peelback_fold(&events, &graph);
    let c = &model.topics["C"];
    // 2.0 halved, not zeroed and not revoked.
    assert!((c.rep_num - 1.0).abs() < 1e-12, "repNum {:?}", c.rep_num);
    assert!(!c.conditional);
    assert_eq!(c.status, TopicStatus::Placed);
    // interval_for(1.0) is the table entry at index 1: 4.5.
    assert!(
        (c.interval_days - 4.5).abs() < 1e-12,
        "interval {:?}",
        c.interval_days
    );
}

// --------------------------------------------------------------------------- //
// Ability seeding (spec section 8, tests/test_ability_seeding.py)
// --------------------------------------------------------------------------- //

#[test]
fn diagnostic_answers_feed_the_placed_ability_and_speed() {
    let graph = common::graph(vec![
        common::topic("fast", &[], 0.5, &[]),
        common::topic("slow", &[], 0.5, &[]),
    ]);
    let events = vec![
        event(&format!(
            r#"{{"type":"diagnostic_answer","ts":"{T_SEED}","topic":"fast","correct":true,
                "secs":10,"weight":1.0}}"#
        )),
        event(&format!(
            r#"{{"type":"diagnostic_answer","ts":"{T_SEED}","topic":"slow","correct":true,
                "secs":100,"weight":0.5}}"#
        )),
        event(
            r#"{"type":"diagnostic_placed","ts":"2026-07-28T12:01:00Z",
                "balances":{"fast":2.0,"slow":2.0}}"#,
        ),
    ];
    let model = seed_fold(&events, &graph, "2026-07-28T12:02:00Z");

    // The EWMA fold from the 0.5 prior with alpha 0.3:
    //   fast = 0.5 + 0.3 x 1.0 x (1.0 - 0.5) = 0.65
    //   slow = 0.5 + 0.3 x 0.5 x (1.0 - 0.5) = 0.575
    let fast = &model.topics["fast"];
    let slow = &model.topics["slow"];
    common::assert_approx(fast.ability, 0.65, "fast ability");
    common::assert_approx(slow.ability, 0.575, "slow ability");
    assert!(fast.ability > slow.ability && slow.ability > 0.0);
    // speed_for(0.65, 0.5) = clamp((0.5 + 0.65) / (0.5 + 0.5), 0.33, 3.0) = 1.15.
    common::assert_approx(fast.speed, 1.15, "fast speed");
}

#[test]
fn a_placed_topic_keeps_its_speed_through_its_first_review() {
    let graph = common::graph(vec![common::topic("t", &[], 0.5, &[])]);
    let mut events = answered_placement("t");
    events.push(event(
        r#"{"type":"review_result","ts":"2026-07-28T12:02:00Z","topic":"t","passed":true,
            "weighted_score":1.0,"quality_tier":"perfect"}"#,
    ));
    let model = seed_fold(&events, &graph, "2026-07-28T12:03:00Z");
    let t = &model.topics["t"];
    assert!(t.ability > 0.5, "ability {:?}", t.ability);
    assert!(t.speed >= 1.0, "speed {:?}", t.speed);
}

#[test]
fn an_upward_penalty_leaves_an_untouched_dependent_seedable() {
    // A depends on B. B is placed high, a failed review penalizes upward, then A's own
    // lesson must still seed A's ability from the neighborhood.
    let graph = common::graph(vec![
        common::topic("B", &[], 0.5, &[]),
        common::topic("A", &[("B", 0.6, false)], 0.5, &[]),
    ]);
    let mut events = answered_placement("B");
    events.push(event(
        r#"{"type":"review_result","ts":"2026-07-28T12:02:00Z","topic":"B","passed":false,
            "weighted_score":0.0,"quality_tier":"poor"}"#,
    ));
    events.push(event(
        r#"{"type":"lesson_result","ts":"2026-07-28T12:03:00Z","topic":"A","passed":true,
            "quality_tier":"perfect"}"#,
    ));
    let model = seed_fold(&events, &graph, "2026-07-28T12:04:00Z");
    let a = &model.topics["A"];
    assert!(a.ability > 0.5, "ability {:?}", a.ability);
    assert!(a.speed >= 1.0, "speed {:?}", a.speed);
}

#[test]
fn a_refresh_never_promotes_a_never_learned_topic() {
    // H2: a non-positive refresh balance on an untouched topic is skipped.
    let graph = common::graph(vec![
        common::topic("known", &[], 0.3, &[]),
        common::topic("never_seen", &[], 0.3, &[]),
    ]);
    let events = vec![event(&format!(
        r#"{{"type":"diagnostic_placed","ts":"{T_SEED}",
            "balances":{{"known":2.0,"never_seen":-1.0}},"refresh":true}}"#
    ))];
    let model = seed_fold(&events, &graph, "2026-07-28T12:01:00Z");
    assert_eq!(model.topics["known"].status, TopicStatus::Placed);
    // The untouched topic keeps a default state, so `finalize` drops it entirely.
    assert!(!model.topics.contains_key("never_seen"));
}

#[test]
fn a_refresh_folds_only_its_own_sessions_answers() {
    let graph = common::graph(vec![common::topic("t", &[], 0.5, &[])]);
    let mut base = answered_placement("t");
    for day in ["2026-07-29", "2026-08-03"] {
        base.push(event(&format!(
            r#"{{"type":"review_result","ts":"{day}T12:00:00Z","topic":"t","passed":true,
                "weighted_score":1.0,"quality_tier":"perfect"}}"#
        )));
    }
    let mut refreshed = base.clone();
    refreshed.push(event(
        r#"{"type":"diagnostic_placed","ts":"2027-02-13T12:00:00Z","balances":{"t":2.0},
            "refresh":true}"#,
    ));
    let without = seed_fold(&base, &graph, "2027-02-14T12:00:00Z").topics["t"].ability;
    let with = seed_fold(&refreshed, &graph, "2027-02-14T12:00:00Z").topics["t"].ability;
    // The refresh re-asked nothing about `t`, so its ability is unchanged.
    common::assert_approx(with, without, "ability across a refresh");
}

#[test]
fn an_untouched_topic_is_not_reseeded_on_every_event() {
    // Two misses must drive the ability strictly LOWER than one. A re-seed on the
    // second event re-seeded, both land on the same neighborhood value.
    let graph = common::graph(vec![common::topic("t", &[], 0.5, &[])]);
    let misses = vec![
        event(&format!(
            r#"{{"type":"review_result","ts":"{T_SEED}","topic":"t","passed":false,
                "weighted_score":0.0,"quality_tier":"poor"}}"#
        )),
        event(
            r#"{"type":"review_result","ts":"2026-07-29T12:00:00Z","topic":"t","passed":false,
                "weighted_score":0.0,"quality_tier":"poor"}"#,
        ),
    ];
    let one = seed_fold(&misses[..1], &graph, "2026-07-30T12:00:00Z").topics["t"].ability;
    let two = seed_fold(&misses, &graph, "2026-07-30T12:00:00Z").topics["t"].ability;
    assert!(two < one, "one miss {one:?}, two misses {two:?}");
}

// --------------------------------------------------------------------------- //
// Topics outside the curriculum
// --------------------------------------------------------------------------- //

#[test]
fn an_event_on_an_unknown_topic_leaves_the_graph_alone() {
    // A miss on an unknown topic peels nothing, a failed lesson on it stamps its
    // knowledge point with no graph walk, and a passed lesson on it marks no
    // knowledge point. A failed lesson with no knowledge point stamps `t0` only.
    let graph = common::graph(vec![common::topic("t", &[], 0.5, &[])]);
    let events = vec![
        miss("ghost", "2026-07-28T12:01:00Z"),
        event(
            r#"{"type":"lesson_result","ts":"2026-07-28T12:02:00Z","topic":"ghost","passed":false,
                "failed_at_kp":"kp9","quality_tier":"poor"}"#,
        ),
        event(
            r#"{"type":"lesson_result","ts":"2026-07-28T12:03:00Z","topic":"ghost","passed":true,
                "quality_tier":"perfect"}"#,
        ),
        event(
            r#"{"type":"lesson_result","ts":"2026-07-28T12:04:00Z","topic":"t","passed":false,
                "quality_tier":"poor"}"#,
        ),
    ];
    let model = seed_fold(&events, &graph, "2026-07-28T12:05:00Z");
    let ghost = &model.topics["ghost"];
    assert_eq!(ghost.kp_progress.get("kp9"), Some(&KpProgress::FailedOnce));
    assert_eq!(ghost.status, TopicStatus::Learning);
    let t = &model.topics["t"];
    assert!(t.kp_progress.is_empty());
    assert_eq!(
        t.t0,
        Some(Timestamp::parse("2026-07-28T12:04:00Z").unwrap())
    );
}

// --------------------------------------------------------------------------- //
// The accumulator surface
// --------------------------------------------------------------------------- //

#[test]
fn the_blob_writes_a_float_the_way_python_repr_writes_it() {
    // `json.dumps` writes a float with `repr`, which pads the exponent to two digits
    // and signs it. `serde_json` writes `1e-5` and `1e16` for the same two values, so a
    // blob built with `serde_json::to_string` diverges as soon as a topic's ability or
    // memory falls below 1e-4. Recorded from
    // `json.dumps({"a":1e-05,"b":1e+16,"c":-0.0,"d":1.0}, sort_keys=True,
    //              separators=(",",":"))`.
    let mut model = LearnerModel::default();
    let tiny = TopicState {
        ability: 1e-05,
        memory_base: 1e16,
        rep_num: -0.0,
        interval_days: 1.0,
        ..TopicState::default()
    };
    model.topics.insert("t".to_owned(), tiny);

    let blob = canonical_blob(&model).unwrap();
    assert!(blob.contains("\"ability\":1e-05"), "{blob}");
    assert!(blob.contains("\"memoryBase\":1e+16"), "{blob}");
    assert!(blob.contains("\"repNum\":-0.0"), "{blob}");
    assert!(blob.contains("\"interval_days\":1.0"), "{blob}");
    // `built_from_ts` never enters the compared bytes (trap T10).
    assert!(!blob.contains("built_from_ts"), "{blob}");
    // The keys are sorted and the separators are compact.
    assert!(
        blob.starts_with("{\"config_hash\":null,\"pending_remediation\":[],"),
        "{blob}"
    );

    // `LearnerModel::parity_blob` is the same one definition, not a second spelling.
    assert_eq!(model.parity_blob().unwrap(), blob);
    assert_eq!(model.parity_digest().unwrap(), blob_digest(&model).unwrap());
}

#[test]
fn an_empty_log_takes_now_as_the_reference_instant() {
    let graph = regrade_graph();
    let cfg = Config::default();
    let proj = Projector::new(&graph, &cfg);
    assert_eq!(proj.last_ts(), None);
    let model = proj.finalize(now()).unwrap();
    assert_eq!(model.built_from_ts, Some(now()));
    assert!(model.topics.is_empty());
    assert_eq!(model.xp.goal, 40);
    assert_eq!(model.velocity, VelocityState::default());
}
