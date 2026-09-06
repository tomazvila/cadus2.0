//! Unit f19: the delayed retention probe, end to end (D-F11, D-F12).
//!
//! Every event here is a WIRE LITERAL, the way the other fold tests write one, so
//! the test reads the same bytes a client sends. The clock is explicit: the
//! schedule takes `now_us` from the test and never a wall clock.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeMap;

use cadus_core::config::Config;
use cadus_core::event::Event;
use cadus_core::learner::{LAST_PROBLEMS_WINDOW, TopicState, problem_text_hash};
use cadus_core::projector::{PROJECTOR_VERSION, project, project_incremental};
use cadus_core::retention::{RetentionState, due_probe};
use common::events::{event, input, tree};

/// The topic the probes test, and its two knowledge points.
const TOPIC: &str = "absolute-value";

/// The microseconds of one day.
const DAY_US: i64 = 86_400_000_000;

/// The instant the lesson passed: 2026-01-01T00:00:00Z.
const LESSON_US: i64 = 1_767_225_600_000_000;

/// One `retention_probe` line.
fn probe_line(
    day: &str,
    session: &str,
    kp: &str,
    delay_days: u32,
    outcome: &str,
    assisted: bool,
    exposure: &str,
) -> String {
    format!(
        r#"{{"type":"retention_probe","ts":"{day}T09:00:00Z","session":"{session}",
           "kp":"{kp}","topic":"{TOPIC}","delay_days":{delay_days},
           "item_digest":"d-{kp}-{delay_days}","outcome":{outcome},
           "assisted":{assisted},"exposure":"{exposure}","secs":40}}"#
    )
    .replace('\n', "")
    .replace("           ", "")
}

/// The stream every test folds: an enrollment and then the probe lines.
fn stream(probes: &[String]) -> Vec<Event> {
    let mut lines = vec![
        r#"{"type":"enrolled","ts":"2026-01-01T00:00:00Z","course":"foundations"}"#.to_owned(),
    ];
    lines.extend_from_slice(probes);
    lines.iter().map(|line| event(line)).collect()
}

#[test]
fn the_fold_tallies_a_probe_with_its_provenance() {
    let events = stream(&[
        probe_line("2026-01-08", "s1", "kp1", 7, r#""correct""#, false, "first"),
        probe_line("2026-01-09", "s2", "kp2", 7, r#""correct""#, true, "first"),
        probe_line(
            "2026-02-01",
            "s3",
            "kp1",
            30,
            r#""incorrect""#,
            false,
            "first",
        ),
    ]);
    let model = project(&events, &input()).expect("the fold succeeds");
    let seven = &model.retention.by_delay[&7];
    assert_eq!(seven.probes, 2);
    assert_eq!(seven.correct, 2);
    assert_eq!(seven.assisted, 1);
    assert_eq!(
        seven.independent, 1,
        "the assisted answer is no independent evidence"
    );
    assert_eq!(seven.retained_accuracy(), Some(1.0));
    assert_eq!(seven.assistance_dependence(), Some(0.5));
    let thirty = &model.retention.by_delay[&30];
    assert_eq!(thirty.independent, 1);
    assert_eq!(thirty.independent_correct, 0);
    assert_eq!(thirty.retained_accuracy(), Some(0.0));
    assert!(model.retention.is_done(TOPIC, "kp1", 7));
    assert!(!model.retention.is_done(TOPIC, "kp2", 30));
}

#[test]
fn an_ungraded_probe_enters_no_accuracy() {
    let ungraded = r#"{"ungraded":{"reason":"model-unavailable"}}"#;
    let events = stream(&[probe_line(
        "2026-01-08",
        "s1",
        "kp1",
        7,
        ungraded,
        false,
        "first",
    )]);
    let model = project(&events, &input()).expect("the fold succeeds");
    let seven = &model.retention.by_delay[&7];
    assert_eq!(seven.probes, 1);
    assert_eq!(seven.ungraded, 1);
    assert_eq!(seven.independent, 0);
    assert_eq!(seven.retained_accuracy(), None, "no evidence is not a zero");
}

#[test]
fn a_log_with_no_probe_keeps_the_1_0_wire_shape() {
    let model = project(&stream(&[]), &input()).expect("the fold succeeds");
    assert!(model.retention.is_empty());
    let json = serde_json::to_string(&model).expect("the model serializes");
    assert!(
        !json.contains("retention"),
        "the writer skips an empty state: {json}"
    );
    assert_eq!(
        PROJECTOR_VERSION, 6,
        "the current fold stamp invalidates pre-integration caches"
    );
}

#[test]
fn a_resume_over_a_probe_equals_the_full_replay() {
    let events = stream(&[
        probe_line("2026-01-08", "s1", "kp1", 7, r#""correct""#, false, "first"),
        probe_line(
            "2026-02-01",
            "s2",
            "kp1",
            30,
            r#""correct""#,
            false,
            "repeat",
        ),
    ]);
    let whole = project(&events, &input()).expect("the fold succeeds");
    let (prior, new) = events.split_at(2);
    let cached = project(prior, &input()).expect("the fold succeeds");
    let resumed = project_incremental(&cached, prior, new, &input()).expect("the resume succeeds");
    assert_eq!(
        resumed.retention, whole.retention,
        "the retention state is a light index; a resume rebuilds it whole"
    );
}

#[test]
fn the_schedule_serves_7_then_30_then_90_and_one_probe_per_session() {
    let cfg = Config::default();
    let graph = tree();
    let mut state = TopicState::default();
    for kp in ["kp1", "kp2"] {
        state
            .kp_progress
            .insert(kp.to_owned(), cadus_core::event::KpProgress::Passed);
    }
    let topics = BTreeMap::from([(TOPIC.to_owned(), state)]);
    let learned = BTreeMap::from([(TOPIC.to_owned(), LESSON_US)]);
    let mut retention = RetentionState::default();
    let mut served: Vec<(String, u32)> = Vec::new();

    // One session per day, from day 1 to day 100.
    for day in 1..=100_i64 {
        let session = format!("s{day}");
        let now = LESSON_US + day * DAY_US;
        let Some(plan) = due_probe(
            graph,
            &topics,
            &retention,
            &cfg.retention,
            &learned,
            &session,
            now,
        ) else {
            continue;
        };
        served.push((plan.kp.clone(), plan.delay_days));
        // The answer of the probe lands, and the rate rule then closes the session.
        let event = event(&probe_line(
            "2026-01-08",
            &session,
            &plan.kp,
            plan.delay_days,
            r#""correct""#,
            false,
            "first",
        ));
        let Event::RetentionProbe(body) = event else {
            panic!("a retention probe");
        };
        retention.apply(&body, &cfg.retention);
        assert_eq!(
            due_probe(
                graph,
                &topics,
                &retention,
                &cfg.retention,
                &learned,
                &session,
                now
            ),
            None,
            "the session already carried its probe"
        );
    }

    assert_eq!(
        served,
        vec![
            ("kp1".to_owned(), 7),
            ("kp2".to_owned(), 7),
            ("kp1".to_owned(), 30),
            ("kp2".to_owned(), 30),
            ("kp1".to_owned(), 90),
            ("kp2".to_owned(), 90),
        ],
        "each knowledge point runs each configured delay once, most overdue first"
    );
}

#[test]
fn the_probe_never_repeats_an_item_the_learner_saw() {
    let mut topics = BTreeMap::new();
    topics.insert(
        TOPIC.to_owned(),
        TopicState {
            last_problems: vec!["seen-1".to_owned()],
            ..TopicState::default()
        },
    );
    let retention = RetentionState {
        digests: vec!["probed-1".to_owned()],
        ..RetentionState::default()
    };
    let seen = cadus_core::retention::seen_digests(&topics, &retention, TOPIC);
    let candidates = [
        "seen-1".to_owned(),
        "probed-1".to_owned(),
        "fresh-1".to_owned(),
    ];
    assert_eq!(
        cadus_core::retention::unseen_item(&candidates, &seen),
        Some("fresh-1")
    );
}

#[test]
fn an_item_older_than_the_recent_window_is_still_refused() {
    // `TopicState::last_problems` keeps a bounded recent window, so a window alone
    // cannot answer "ever seen". The lifetime exposure index of the fold does.
    let mut lines = vec![
        r#"{"type":"enrolled","ts":"2026-01-01T00:00:00Z","course":"foundations"}"#.to_owned(),
    ];
    let old_problem = "the-first-problem";
    for index in 0..(LAST_PROBLEMS_WINDOW + 5) {
        let text = if index == 0 {
            old_problem.to_owned()
        } else {
            format!("problem-{index}")
        };
        lines.push(
            format!(
                r#"{{"type":"attempt","ts":"2026-01-01T00:0{}:00Z","attempt_id":"a{index}",
                   "task_id":"t1","topic":"{TOPIC}","task_type":"review",
                   "problem":{{"text":"{text}","expected":"3"}},"given_answer":"3",
                   "correct":true,"secs":5,"work_quality":"nearly_passable"}}"#,
                index % 10
            )
            .replace('\n', "")
            .replace("                   ", ""),
        );
    }
    let events: Vec<Event> = lines.iter().map(|line| event(line)).collect();
    let model = project(&events, &input()).expect("the fold succeeds");
    let old_digest = problem_text_hash(old_problem);

    let state = &model.topics[TOPIC];
    assert!(
        !state.last_problems.contains(&old_digest),
        "the recent window already forgot the item"
    );
    assert!(
        model.retention.is_exposed(&old_digest),
        "the lifetime index still holds it"
    );
    let seen = cadus_core::retention::seen_digests(&model.topics, &model.retention, TOPIC);
    assert_eq!(
        cadus_core::retention::unseen_item(std::slice::from_ref(&old_digest), &seen),
        None,
        "a probe never serves an item the learner met, however long ago"
    );
}

#[test]
fn a_served_probe_uses_up_the_session_before_any_answer() {
    // The rate rule counts SERVES. A refresh, an abandoned probe, and a second tab
    // therefore never buy the session a second probe.
    let served = format!(
        r#"{{"type":"task_served","ts":"2026-01-08T09:00:00Z","session":"s1",
           "task_id":"s1-review-1","task_type":"review","topic":"{TOPIC}","kp":"kp1",
           "probe_delay_days":7}}"#
    )
    .replace('\n', "")
    .replace("           ", "");
    let events = stream(&[served]);
    let model = project(&events, &input()).expect("the fold succeeds");
    assert_eq!(model.retention.probes_in_session("s1"), 1);
    assert!(
        model.retention.by_delay.is_empty(),
        "no answer arrived, so no tally moved"
    );

    let cfg = Config::default();
    let mut state = TopicState::default();
    state
        .kp_progress
        .insert("kp1".to_owned(), cadus_core::event::KpProgress::Passed);
    let topics = BTreeMap::from([(TOPIC.to_owned(), state)]);
    let learned = BTreeMap::from([(TOPIC.to_owned(), LESSON_US)]);
    assert_eq!(
        due_probe(
            tree(),
            &topics,
            &model.retention,
            &cfg.retention,
            &learned,
            "s1",
            LESSON_US + 40 * DAY_US
        ),
        None,
        "the session already served its probe"
    );
}
