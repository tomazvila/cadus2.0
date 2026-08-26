//! U3 acceptance: the fold, the regrade pre-pass, and 1.0 parity (R5, D3, D4, C2, C4).
//!
//! Every expected value below is a LITERAL: a digest, a byte string, or a number that
//! `docs/reference/projector-1.0-spec.md` sections 8 and 9 pin, or that the comment
//! above it derives by hand from `config.yaml`. No expectation calls the code under
//! test.
//!
//! The stream fixture is `tests/fixtures/events/stream_1.jsonl` and its 1.0 fold is
//! `tests/fixtures/events/model_1.json`, both produced by `scripts/oracle`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use cadus_core::config::Config;
use cadus_core::curriculum::{Curriculum, load_curriculum};
use cadus_core::event::{Event, Timestamp, TopicStatus, WorkQuality};
use cadus_core::learner::{LearnerModel, TopicState, VelocityState};
use cadus_core::projector::{
    PROJECTOR_VERSION, ProjectionInput, Projector, apply_regrades, blob_digest, canonical_blob,
    project, project_incremental,
};

mod common;

/// The 1.0 fold of `stream_1.jsonl`, as the SHA-256 of the canonical blob with
/// `built_from_ts` removed (spec section 9).
const STREAM_1_DIGEST: &str = "ba128459985e0815db7446cb2af16452ec07d304b7efaa0952fc6567404245f5";

/// The `config_hash` of the default config (spec section 9).
const CONFIG_HASH: &str = "797575e985c12149";

/// The number of events in `stream_1.jsonl` (spec section 9).
const STREAM_1_EVENTS: usize = 47;

/// The build instant the oracle pins with `--now` (`dump_projector_1_0.py`).
///
/// Only `built_from_ts` carries it, and the parity blob drops that field.
const NOW: &str = "2000-01-01T00:00:00Z";

/// `T0` of the 1.0 regrade tests (`tests/test_regrade.py:44`).
const T0: &str = "2026-08-17T16:30:00Z";

/// `T` of the 1.0 peel-back and ability-seeding tests.
const T_SEED: &str = "2026-07-28T12:00:00Z";

// --------------------------------------------------------------------------- //
// Fixtures
// --------------------------------------------------------------------------- //

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/events")
        .join(name)
}

/// The checked-in curriculum tree, loaded once for the whole test binary.
fn tree() -> &'static Curriculum {
    static TREE: OnceLock<Curriculum> = OnceLock::new();
    TREE.get_or_init(|| {
        let (curriculum, _findings) =
            load_curriculum(&repo_root().join("curriculum")).expect("the tree loads");
        curriculum
    })
}

fn now() -> Timestamp {
    Timestamp::parse(NOW).unwrap()
}

/// One event from its wire JSON. The wire form is the literal; the test never builds a
/// body field by field.
fn event(json: &str) -> Event {
    Event::from_json(json).unwrap_or_else(|error| panic!("{json}: {error}"))
}

/// Every event of a JSONL fixture, in file order.
fn stream(name: &str) -> Vec<Event> {
    let text = std::fs::read_to_string(fixture(name)).expect("the fixture reads");
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(event)
        .collect()
}

/// The full replay of `events` over the checked-in tree, with the default config.
fn fold(events: &[Event]) -> LearnerModel {
    let cfg = Config::default();
    let input = ProjectionInput::new(tree(), &cfg, now());
    project(events, &input).expect("the fold succeeds")
}

/// The first differing window of two blobs, for a readable mismatch report.
fn first_difference(actual: &str, expected: &str) -> String {
    let left = actual.as_bytes();
    let right = expected.as_bytes();
    let at = left
        .iter()
        .zip(right.iter())
        .position(|(a, b)| a != b)
        .unwrap_or_else(|| left.len().min(right.len()));
    let from = at.saturating_sub(60);
    let to_actual = (at + 60).min(left.len());
    let to_expected = (at + 60).min(right.len());
    format!(
        "first difference at byte {at} (lengths {} and {})\n  actual   ...{}...\n  expected ...{}...",
        left.len(),
        right.len(),
        String::from_utf8_lossy(&left[from..to_actual]),
        String::from_utf8_lossy(&right[from..to_expected]),
    )
}

// --------------------------------------------------------------------------- //
// The fixture fold — the acceptance check
// --------------------------------------------------------------------------- //

#[test]
fn the_fixture_stream_holds_the_literal_event_count() {
    assert_eq!(stream("stream_1.jsonl").len(), STREAM_1_EVENTS);
}

#[test]
fn the_fold_of_stream_1_is_the_oracle_digest() {
    let model = fold(&stream("stream_1.jsonl"));
    assert_eq!(
        blob_digest(&model).unwrap(),
        STREAM_1_DIGEST,
        "the fold of stream_1.jsonl must reproduce the 1.0 blob digest"
    );
}

#[test]
fn the_blob_of_stream_1_is_the_committed_model_byte_for_byte() {
    // `model_1.json` is the oracle's blob plus the newline `print` adds; the compared
    // bytes are the file with that newline removed. `built_from_ts` is already absent.
    let committed = std::fs::read_to_string(fixture("model_1.json")).expect("the model reads");
    let expected = committed.trim_end_matches('\n');
    let model = fold(&stream("stream_1.jsonl"));
    let actual = canonical_blob(&model).unwrap();
    assert!(
        actual == expected,
        "{}",
        first_difference(&actual, expected)
    );
}

#[test]
fn the_fold_stamps_the_projector_version_and_the_config_hash() {
    assert_eq!(PROJECTOR_VERSION, 3);
    let model = fold(&stream("stream_1.jsonl"));
    assert_eq!(model.projector_version, Some(3));
    assert_eq!(model.config_hash.as_deref(), Some(CONFIG_HASH));
}

#[test]
fn the_fold_reaches_forty_seven_topics_through_propagation() {
    // Five topics are named explicitly; encompassing propagation reaches the rest
    // (spec section 9, "Stream #1 coverage").
    let model = fold(&stream("stream_1.jsonl"));
    assert_eq!(model.topics.len(), 47);
    // A `review_result` on an untouched topic writes FIRe state and leaves the status
    // untouched (spec section 4.4).
    let state = &model.topics["absolute-value-inequalities"];
    assert_eq!(state.status, TopicStatus::Untouched);
    assert!(state.t0.is_some());
    // The mastery floor of `foundations` stamps `floor` on these three.
    for tid in [
        "place-value",
        "single-digit-addition",
        "multiplication-tables",
    ] {
        assert_eq!(model.topics[tid].status, TopicStatus::Floor, "{tid}");
    }
}

#[test]
fn incremental_equals_full_replay_at_every_split() {
    let events = stream("stream_1.jsonl");
    let cfg = Config::default();
    let input = ProjectionInput::new(tree(), &cfg, now());
    let full = canonical_blob(&project(&events, &input).unwrap()).unwrap();
    assert_eq!(events.len(), STREAM_1_EVENTS);

    for split in 0..=events.len() {
        let (prior, fresh) = events.split_at(split);
        let cached = project(prior, &input).unwrap();
        let incremental = project_incremental(&cached, prior, fresh, &input).unwrap();
        let actual = canonical_blob(&incremental).unwrap();
        assert!(
            actual == full,
            "split {split}: {}",
            first_difference(&actual, &full)
        );
    }
}

/// The 1.0 blob of one fixture stream, straight from the oracle, or `None` when
/// `CADUS_ORACLE_PYTHON` is unset.
fn live_oracle_blob(name: &str, tz: Option<&str>) -> Option<String> {
    let python = std::env::var("CADUS_ORACLE_PYTHON").ok()?;
    let mut command = Command::new(&python);
    command
        .arg(repo_root().join("scripts/oracle/dump_projector_1_0.py"))
        .arg(fixture(name))
        .arg("--curriculum")
        .arg(repo_root().join("curriculum"))
        .arg("--now")
        .arg("2000-01-01T00:00:00+00:00");
    if let Some(zone) = tz {
        command.arg("--tz").arg(zone);
    }
    // The 1.0 package imports from its own tree.
    let output = command
        .current_dir("/home/deploy/dev/cadus")
        .output()
        .expect("the oracle runs");
    assert!(
        output.status.success(),
        "the oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).expect("the oracle prints UTF-8");
    Some(text.trim_end_matches('\n').to_owned())
}

#[test]
fn the_live_oracle_agrees_with_the_fold() {
    let Some(expected) = live_oracle_blob("stream_1.jsonl", None) else {
        eprintln!("skipped: CADUS_ORACLE_PYTHON is not set");
        return;
    };
    let model = fold(&stream("stream_1.jsonl"));
    let actual = canonical_blob(&model).unwrap();
    assert!(
        actual == expected,
        "{}",
        first_difference(&actual, &expected)
    );
}

// --------------------------------------------------------------------------- //
// The coverage stream — the handlers stream_1.jsonl never reaches
// --------------------------------------------------------------------------- //

/// The 1.0 fold of `stream_u3_coverage.jsonl` in UTC, from `dump_projector_1_0.py`.
const COVERAGE_DIGEST: &str = "276de9945869db8221c90184b089215c6dec16f8fafcb3b29bd9c02766f3d8b4";

/// The same stream folded with `--tz America/New_York`.
const COVERAGE_DIGEST_NEW_YORK: &str =
    "873f2aad74421a005e5fc48abd74296ae793a3d8ed0fc76b6e80e8f6f9b444e4";

/// The number of events in `stream_u3_coverage.jsonl`.
const COVERAGE_EVENTS: usize = 33;

fn coverage_fold(tz: Option<&str>) -> LearnerModel {
    let cfg = Config::default();
    let input = ProjectionInput::new(tree(), &cfg, now()).with_timezone(tz);
    project(&stream("stream_u3_coverage.jsonl"), &input).expect("the fold succeeds")
}

#[test]
fn the_coverage_stream_folds_to_the_oracle_digest_and_bytes() {
    let events = stream("stream_u3_coverage.jsonl");
    assert_eq!(events.len(), COVERAGE_EVENTS);
    let model = coverage_fold(None);
    assert_eq!(blob_digest(&model).unwrap(), COVERAGE_DIGEST);

    let committed =
        std::fs::read_to_string(fixture("model_u3_coverage.json")).expect("the model reads");
    let expected = committed.trim_end_matches('\n');
    let actual = canonical_blob(&model).unwrap();
    assert!(
        actual == expected,
        "{}",
        first_difference(&actual, expected)
    );
}

#[test]
fn the_coverage_stream_folds_to_the_oracle_digest_in_new_york() {
    // The time zone enters only through `local_day` (trap T9), so only "today", the
    // streak, and the velocity window move with it.
    let model = coverage_fold(Some("America/New_York"));
    assert_eq!(blob_digest(&model).unwrap(), COVERAGE_DIGEST_NEW_YORK);

    let committed = std::fs::read_to_string(fixture("model_u3_coverage_new_york.json"))
        .expect("the model reads");
    let expected = committed.trim_end_matches('\n');
    let actual = canonical_blob(&model).unwrap();
    assert!(
        actual == expected,
        "{}",
        first_difference(&actual, expected)
    );
}

#[test]
fn the_whole_log_total_is_compensated_and_the_daily_total_is_naive() {
    // Traps T1 and T2 in one model. 25 XP records of 0.1 on one day:
    //   `xp.total`   goes through the compensated `sum()` site, which is EXACTLY 2.5,
    //                and Python `round(2.5)` is half-even, so it is 2.
    //   `xp.today`   goes through the NAIVE `daily_totals` loop, which is
    //                2.500000000000001, so it rounds to 3.
    //   `quiz.xp_since` is the other naive loop, so it is 3 as well.
    // A single summation rule anywhere in the fold collapses 2 and 3 onto one number.
    // Recorded from 1.0 `project` on the same 25 records.
    let events: Vec<Event> = (0..25)
        .map(|minute| {
            event(&format!(
                r#"{{"type":"review_result","ts":"2026-05-04T09:{minute:02}:00Z",
                    "topic":"absolute-value","passed":true,"weighted_score":1.0,"xp":0.1,
                    "quality_tier":"perfect"}}"#
            ))
        })
        .collect();
    assert_eq!(events.len(), 25);
    let model = fold(&events);
    assert_eq!(model.xp.total, 2, "the compensated whole-log total");
    assert_eq!(model.xp.today, 3, "the naive per-day total");
    assert_eq!(model.quiz.xp_since, 3, "the naive since-quiz total");
    assert_eq!(model.xp.streak_days, 0);
}

#[test]
fn practice_at_the_trigger_instant_closes_a_remediation_target() {
    // The comparison is STRICTLY BEFORE (`projector.py:604-608`): a target stays open
    // only while its last practice is missing or earlier than the trigger. Practice at
    // exactly the trigger instant CLOSES it. Recorded from 1.0 `project`.
    let events = vec![
        event(
            r#"{"type":"remediation_triggered","ts":"2026-05-04T09:00:00Z","kind":"quiz-miss",
                "source_topic":"absolute-value",
                "targets":["adding-integers","two-step-equations"]}"#,
        ),
        event(
            r#"{"type":"lesson_result","ts":"2026-05-04T09:00:00Z","topic":"adding-integers",
                "passed":true,"xp":0.0,"quality_tier":"perfect"}"#,
        ),
    ];
    let model = fold(&events);
    assert_eq!(model.pending_remediation.len(), 1);
    assert_eq!(
        model.pending_remediation[0]
            .targets
            .iter()
            .map(|item| item.as_str())
            .collect::<Vec<_>>(),
        vec!["two-step-equations"]
    );
}

#[test]
fn the_coverage_stream_exercises_the_named_branches() {
    let model = coverage_fold(None);
    // The 45 XP record sits at 02:30 UTC on 2026-03-09, which is 2026-03-08 in
    // New York, so "today" moves with the zone while the whole-log total does not.
    assert_eq!(model.xp.total, 86);
    assert_eq!(model.xp.today, 45);
    assert_eq!(model.xp.streak_days, 1);
    let new_york = coverage_fold(Some("America/New_York"));
    assert_eq!(new_york.xp.total, 86);
    assert_eq!(new_york.xp.today, 0);
    assert_eq!(new_york.xp.streak_days, 1);

    // The quiz scored 0.75, below the 0.8 retake threshold.
    assert!(model.quiz.retake_pending);
    assert_eq!(
        model.quiz.last_at.map(|day| day.to_string()).as_deref(),
        Some("2026-03-06")
    );

    // The remediation trigger names three targets: one off the curriculum (dropped at
    // fold time), one practiced after the trigger (closed), one still open. The second,
    // identical trigger is deduped away.
    assert_eq!(model.pending_remediation.len(), 1);
    assert_eq!(model.pending_remediation[0].kind, "quiz-miss");
    assert_eq!(
        model.pending_remediation[0]
            .targets
            .iter()
            .map(|item| item.as_str())
            .collect::<Vec<_>>(),
        vec!["two-step-equations"]
    );

    // The refresh re-seeded `one-step-equations`: repNum is the mean of the placement
    // credit 2.0 and the refresh evidence min(1.0, 4.0), so 1.5.
    let one_step = &model.topics["one-step-equations"];
    assert_eq!(one_step.status, TopicStatus::Placed);
    assert!(
        (one_step.rep_num - 1.5).abs() < 1e-12,
        "{:?}",
        one_step.rep_num
    );
    // The H2 guard held: a non-positive refresh balance on never-learned material
    // leaves a default state, which `finalize` drops.
    assert!(!model.topics.contains_key("fraction-word-problems"));
    // An off-curriculum id is skipped everywhere it appears.
    assert!(!model.topics.contains_key("not-a-real-topic"));

    // The failed lesson stamped kp2, then the passing lesson marked every KP passed.
    let absolute_value = &model.topics["absolute-value"];
    assert_eq!(absolute_value.kp_progress.len(), 2);
    // The peel-back cleared the conditional flag set at placement.
    assert!(!absolute_value.conditional);
    // The dedup window holds one digest per attempt, in order.
    assert_eq!(absolute_value.last_problems.len(), 3);
}

/// The splits of `stream_u3_coverage.jsonl` where 1.0's own incremental fold differs
/// from its full replay, recorded from `project_incremental`.
///
/// The cause is the `profile_reset` at index 20. `finalize` DROPS a topic state that
/// equals a default one, so the cached model carries no `number-line-integers` key
/// after the reset. `ability_update` and `apply_attempt` skip a target that is ABSENT
/// from the states, while a present-and-default target takes a delta — so a resume that
/// seeds from the filtered cache stops propagating onto the reset topic. Splits 21 to
/// 31 put the reset in the light half and a graded event after it. 1.0 answers this at
/// the service layer, which sends such a stream down the full-replay path
/// (`service.py:257-280`). The port reproduces the divergence rather than hides it.
const COVERAGE_DIVERGING_SPLITS: [usize; 11] = [21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31];

#[test]
fn incremental_matches_full_replay_at_the_same_splits_as_one_point_zero() {
    let events = stream("stream_u3_coverage.jsonl");
    let cfg = Config::default();
    let input = ProjectionInput::new(tree(), &cfg, now());
    let full = canonical_blob(&project(&events, &input).unwrap()).unwrap();

    let mut diverging: Vec<usize> = Vec::new();
    for split in 0..=events.len() {
        let (prior, fresh) = events.split_at(split);
        let cached = project(prior, &input).unwrap();
        let incremental = project_incremental(&cached, prior, fresh, &input).unwrap();
        if canonical_blob(&incremental).unwrap() != full {
            diverging.push(split);
        }
    }
    assert_eq!(diverging, COVERAGE_DIVERGING_SPLITS.to_vec());
}

#[test]
fn the_live_oracle_agrees_on_the_coverage_stream() {
    let Some(expected) = live_oracle_blob("stream_u3_coverage.jsonl", None) else {
        eprintln!("skipped: CADUS_ORACLE_PYTHON is not set");
        return;
    };
    let actual = canonical_blob(&coverage_fold(None)).unwrap();
    assert!(
        actual == expected,
        "{}",
        first_difference(&actual, &expected)
    );

    let expected = live_oracle_blob("stream_u3_coverage.jsonl", Some("America/New_York"))
        .expect("the oracle runs");
    let actual = canonical_blob(&coverage_fold(Some("America/New_York"))).unwrap();
    assert!(
        actual == expected,
        "{}",
        first_difference(&actual, &expected)
    );
}

// --------------------------------------------------------------------------- //
// The no-op event types
// --------------------------------------------------------------------------- //

#[test]
fn the_six_no_op_types_move_nothing() {
    // `task_served`, `session_start`, `session_end`, `anki_card_created`,
    // `config_changed`, and `curriculum_changed` carry no derived state
    // (spec section 4). The whole `one_per_type.jsonl` fixture holds one of each of
    // the 16 types; folding the six no-ops alone must give a default model.
    let all = stream("one_per_type.jsonl");
    let no_ops: Vec<Event> = all
        .into_iter()
        .filter(|item| {
            matches!(
                item.type_name(),
                "task_served"
                    | "session_start"
                    | "session_end"
                    | "anki_card_created"
                    | "config_changed"
                    | "curriculum_changed"
            )
        })
        .collect();
    assert_eq!(no_ops.len(), 6);
    let model = fold(&no_ops);
    assert!(model.topics.is_empty());
    assert_eq!(model.xp.total, 0);
    assert_eq!(model.xp.today, 0);
    assert_eq!(model.xp.streak_days, 0);
    assert_eq!(model.quiz.xp_since, 0);
    assert!(model.pending_remediation.is_empty());
}

// --------------------------------------------------------------------------- //
// apply_regrades — the substitution (spec section 8, tests/test_regrade.py)
// --------------------------------------------------------------------------- //

/// The regrade graph of `tests/test_regrade.py:54-65`: one topic, two knowledge points.
fn regrade_graph() -> Curriculum {
    let mut topic = common::topic("subtraction-facts", &[], 0.2, &[]);
    topic.knowledge_points = vec![
        common::knowledge_point("kp1", &[]),
        common::knowledge_point("kp2", &[]),
    ];
    common::graph(vec![topic])
}

/// One `attempt` of the regrade tests (`tests/test_regrade.py:68-88`).
fn regrade_attempt(aid: &str, ts: &str, task_id: &str, tier: &str, tags: &str) -> Event {
    event(&format!(
        r#"{{"type":"attempt","ts":"{ts}","attempt_id":"{aid}","task_id":"{task_id}",
            "topic":"subtraction-facts","kp":"kp1","task_type":"lesson",
            "problem":{{"text":"problem {aid}","expected":"7"}},"given_answer":"7",
            "correct":true,"secs":20,"error_tags":{tags},"work_quality":"{tier}"}}"#
    ))
}

/// One `lesson_result` of the regrade tests (`tests/test_regrade.py:91-95`).
fn regrade_lesson(ts: &str, tier: &str, xp: &str) -> Event {
    event(&format!(
        r#"{{"type":"lesson_result","ts":"{ts}","topic":"subtraction-facts","passed":true,
            "xp":{xp},"quality_tier":"{tier}"}}"#
    ))
}

#[test]
fn a_correction_replaces_the_grade_fields_of_the_named_attempt() {
    let events = vec![
        regrade_attempt(
            "a1",
            T0,
            "task-1",
            "poor",
            r#"["incomplete","timing-unreliable"]"#,
        ),
        event(
            r#"{"type":"regraded","ts":"2026-08-18T16:30:00Z","task_id":"task-1",
                "topic":"subtraction-facts","reason":"test",
                "attempts":[{"attempt_id":"a1","work_quality":"nearly_perfect",
                             "error_tags":["timing-unreliable"],"grader_note":"regraded"}]}"#,
        ),
    ];
    let out = apply_regrades(&events);
    assert_eq!(out.len(), 1, "the correction itself is consumed");
    let Event::Attempt(fixed) = &out[0] else {
        panic!("expected an attempt, got {:?}", out[0].type_name());
    };
    assert_eq!(fixed.work_quality, WorkQuality::NearlyPerfect);
    // ORDER: the tag list is replaced wholesale, and its order is kept.
    assert_eq!(fixed.error_tags, vec!["timing-unreliable".to_string()]);
    assert_eq!(fixed.grader_note.as_deref(), Some("regraded"));
    // A correction restates how well the work was done, never whether it was right.
    assert!(fixed.correct);
    assert_eq!(fixed.given_answer, "7");
}

#[test]
fn a_stream_with_no_correction_comes_back_unchanged() {
    let events = vec![
        regrade_attempt("a1", T0, "task-1", "blowoff", "[]"),
        regrade_lesson(T0, "blowoff", "-3.5"),
    ];
    assert_eq!(apply_regrades(&events), events);
}

#[test]
fn a_later_correction_supersedes_an_earlier_one() {
    let events = vec![
        regrade_attempt("a1", T0, "task-1", "poor", "[]"),
        event(
            r#"{"type":"regraded","ts":"2026-08-18T16:30:00Z","task_id":"task-1",
                "topic":"subtraction-facts","reason":"test",
                "attempts":[{"attempt_id":"a1","work_quality":"passable"}]}"#,
        ),
        event(
            r#"{"type":"regraded","ts":"2026-08-19T16:30:00Z","task_id":"task-1",
                "topic":"subtraction-facts","reason":"test",
                "attempts":[{"attempt_id":"a1","work_quality":"perfect"}]}"#,
        ),
    ];
    let out = apply_regrades(&events);
    assert_eq!(out.len(), 1);
    let Event::Attempt(fixed) = &out[0] else {
        panic!("expected an attempt");
    };
    assert_eq!(fixed.work_quality, WorkQuality::Perfect);
}

#[test]
fn a_correction_touches_only_the_attempt_it_names() {
    let events = vec![
        regrade_attempt("a1", T0, "task-1", "poor", "[]"),
        regrade_attempt("a2", "2026-08-17T16:31:00Z", "task-1", "passable", "[]"),
        event(
            r#"{"type":"regraded","ts":"2026-08-18T16:30:00Z","task_id":"task-1",
                "topic":"subtraction-facts","reason":"test",
                "attempts":[{"attempt_id":"a1","work_quality":"nearly_perfect"}]}"#,
        ),
    ];
    let out = apply_regrades(&events);
    assert_eq!(out.len(), 2);
    let (Event::Attempt(first), Event::Attempt(second)) = (&out[0], &out[1]) else {
        panic!("expected two attempts");
    };
    assert_eq!(first.work_quality, WorkQuality::NearlyPerfect);
    assert_eq!(second.work_quality, WorkQuality::Passable);
}

#[test]
fn a_lesson_result_is_matched_through_its_preceding_attempt() {
    // A 2-KP lesson: `blowoff` is 3.5 x 2 x -0.5 = -3.5, `nearly_perfect` is
    // 3.5 x 2 x 1.0 = 7.0 (tests/test_regrade.py:9-11).
    let events = vec![
        regrade_attempt("a1", T0, "task-1", "blowoff", "[]"),
        regrade_lesson(T0, "blowoff", "-3.5"),
        event(
            r#"{"type":"regraded","ts":"2026-08-18T16:30:00Z","task_id":"task-1",
                "topic":"subtraction-facts","reason":"test","quality_tier":"nearly_perfect",
                "xp":7.0,"attempts":[{"attempt_id":"a1","work_quality":"nearly_perfect"}]}"#,
        ),
    ];
    let out = apply_regrades(&events);
    assert_eq!(out.len(), 2);
    let Event::LessonResult(result) = &out[1] else {
        panic!("expected a lesson result");
    };
    assert_eq!(result.quality_tier, WorkQuality::NearlyPerfect);
    assert!((result.xp - 7.0).abs() < f64::EPSILON);
    assert!(result.passed);
}

#[test]
fn a_review_result_is_matched_by_its_own_task_id() {
    let events = vec![
        event(
            r#"{"type":"attempt","ts":"2026-08-17T16:30:00Z","attempt_id":"a1",
                "task_id":"review-9","topic":"subtraction-facts","kp":"kp1",
                "task_type":"review","problem":{"text":"problem a1","expected":"7"},
                "given_answer":"7","correct":true,"secs":20,"work_quality":"blowoff"}"#,
        ),
        event(
            r#"{"type":"review_result","ts":"2026-08-17T16:30:00Z","topic":"subtraction-facts",
                "passed":true,"weighted_score":1.0,"xp":0.0,"quality_tier":"poor",
                "task_id":"review-9"}"#,
        ),
        event(
            r#"{"type":"regraded","ts":"2026-08-18T16:30:00Z","task_id":"review-9",
                "topic":"subtraction-facts","reason":"test","quality_tier":"nearly_perfect",
                "xp":5.0,"attempts":[{"attempt_id":"a1","work_quality":"nearly_perfect"}]}"#,
        ),
    ];
    let out = apply_regrades(&events);
    assert_eq!(out.len(), 2);
    let Event::ReviewResult(result) = &out[1] else {
        panic!("expected a review result");
    };
    assert_eq!(result.quality_tier, WorkQuality::NearlyPerfect);
    assert!((result.xp - 5.0).abs() < f64::EPSILON);
}

#[test]
fn the_result_of_an_uncorrected_task_keeps_its_own_grade() {
    let later = "2026-08-17T17:30:00Z";
    let events = vec![
        regrade_attempt("a1", T0, "task-1", "blowoff", "[]"),
        regrade_lesson(T0, "blowoff", "-3.5"),
        regrade_attempt("a2", later, "task-2", "passable", "[]"),
        // 3.5 x 2 x 0.85 = 5.95.
        regrade_lesson(later, "passable", "5.95"),
        event(
            r#"{"type":"regraded","ts":"2026-08-18T16:30:00Z","task_id":"task-1",
                "topic":"subtraction-facts","reason":"test","quality_tier":"nearly_perfect",
                "xp":7.0,"attempts":[{"attempt_id":"a1","work_quality":"nearly_perfect"}]}"#,
        ),
    ];
    let out = apply_regrades(&events);
    assert_eq!(out.len(), 4);
    let (Event::LessonResult(first), Event::LessonResult(second)) = (&out[1], &out[3]) else {
        panic!("expected two lesson results");
    };
    assert!((first.xp - 7.0).abs() < f64::EPSILON);
    assert!((second.xp - 5.95).abs() < f64::EPSILON);
    assert_eq!(second.quality_tier, WorkQuality::Passable);
}

#[test]
fn apply_regrades_is_idempotent() {
    let events = vec![
        regrade_attempt("a1", T0, "task-1", "blowoff", r#"["incomplete"]"#),
        regrade_lesson(T0, "blowoff", "-3.5"),
        event(
            r#"{"type":"regraded","ts":"2026-08-18T16:30:00Z","task_id":"task-1",
                "topic":"subtraction-facts","reason":"test","quality_tier":"nearly_perfect",
                "xp":7.0,"attempts":[{"attempt_id":"a1","work_quality":"nearly_perfect",
                                      "error_tags":["timing-unreliable"]}]}"#,
        ),
    ];
    let once = apply_regrades(&events);
    let twice = apply_regrades(&once);
    assert_eq!(twice, once);

    // The same holds on the committed stream, which carries one `regraded` event.
    let fixture_events = stream("stream_1.jsonl");
    let once = apply_regrades(&fixture_events);
    assert_eq!(once.len(), STREAM_1_EVENTS - 1);
    assert_eq!(apply_regrades(&once), once);
}

// --------------------------------------------------------------------------- //
// What the correction buys — the replayed model
// --------------------------------------------------------------------------- //

/// The regrade tests fold over their own one-topic graph, not over the tree.
fn regrade_fold(events: &[Event], graph: &Curriculum) -> LearnerModel {
    let cfg = Config::default();
    let input = ProjectionInput::new(
        graph,
        &cfg,
        Timestamp::parse("2026-08-19T16:30:00Z").unwrap(),
    );
    project(events, &input).expect("the fold succeeds")
}

#[test]
fn a_correction_turns_the_recorded_failure_into_a_pass() {
    let graph = regrade_graph();
    let broken = vec![
        regrade_attempt("a1", T0, "task-1", "blowoff", r#"["incomplete"]"#),
        regrade_lesson(T0, "blowoff", "-3.5"),
    ];
    let before = regrade_fold(&broken, &graph);
    // Python `round(-3.5)` is half-even, so -3.5 rounds to -4, never to -3 (trap T3).
    assert_eq!(before.xp.total, -4);

    let mut fixed = broken.clone();
    fixed.push(event(
        r#"{"type":"regraded","ts":"2026-08-18T16:30:00Z","task_id":"task-1",
            "topic":"subtraction-facts","reason":"test","quality_tier":"nearly_perfect",
            "xp":7.0,"attempts":[{"attempt_id":"a1","work_quality":"nearly_perfect"}]}"#,
    ));
    let after = regrade_fold(&fixed, &graph);
    assert_eq!(after.xp.total, 7);
    // FIRe read a pass, not a miss.
    assert!(
        after.topics["subtraction-facts"].interval_days
            > before.topics["subtraction-facts"].interval_days
    );
}

#[test]
fn a_correction_written_forty_days_later_lands_on_the_graded_day() {
    let graph = regrade_graph();
    let events = vec![
        regrade_attempt("a1", T0, "task-1", "blowoff", "[]"),
        regrade_lesson(T0, "blowoff", "-3.5"),
        event(
            r#"{"type":"regraded","ts":"2026-09-26T16:30:00Z","task_id":"task-1",
                "topic":"subtraction-facts","reason":"test","quality_tier":"nearly_perfect",
                "xp":7.0,"attempts":[{"attempt_id":"a1","work_quality":"nearly_perfect"}]}"#,
        ),
    ];
    // 7.0 lands on 2026-08-17, the graded day, not on the day the correction was written:
    // `apply_regrades` consumes the correction before the fold, so it never moves `t_ref`.
    let model = regrade_fold(&events, &graph);
    assert_eq!(model.xp.today, 7);
    assert_eq!(model.xp.total, 7);
}

/// The 1.0 FULL replay of the late-correction stream, recorded from `project`.
const LATE_CORRECTION_FULL: &str = r#"{"config_hash":"797575e985c12149","pending_remediation":[],"projector_version":3,"quiz":{"last_at":null,"retake_pending":false,"xp_since":7},"topics":{"subtraction-facts":{"ability":0.65,"conditional":false,"explicit_only":false,"interval_days":6.680357142857142,"kp_progress":{"kp1":"passed","kp2":"passed"},"last_problems":["80baf8b31b56"],"memoryBase":0.85,"repNum":1.3964285714285714,"speed":1.6428571428571428,"status":"learning","t0":"2026-08-17T16:30:00Z"}},"velocity":{"course_progress":0.0,"eta":null,"topics_per_week_28d":0.0,"xp_per_day_28d":0.0},"xp":{"goal":40,"streak_days":0,"today":7,"total":7}}"#;

/// The 1.0 INCREMENTAL replay of the same stream, recorded from `project_incremental`.
///
/// It differs from [`LATE_CORRECTION_FULL`] in the FIRe fields, and 1.0 differs the
/// same way: the correction lands in `new_events`, so the corrected `lesson_result`
/// re-splits into the PRIOR half and replays with FIRe off, on top of a cache built
/// before the correction. 1.0 answers this at the service layer, which sends a stream
/// carrying a new correction down the FULL-replay path instead
/// (`projector.py:812-820`, `service.py:257-280`). The 1.0 test asserts only the XP
/// (`tests/test_regrade.py:364-384`); the two blobs below were recorded from 1.0 so
/// the port pins the divergence rather than hides it.
const LATE_CORRECTION_INCREMENTAL: &str = r#"{"config_hash":"797575e985c12149","pending_remediation":[],"projector_version":3,"quiz":{"last_at":null,"retake_pending":false,"xp_since":7},"topics":{"subtraction-facts":{"ability":0.65,"conditional":false,"explicit_only":false,"interval_days":2.0,"kp_progress":{"kp1":"passed","kp2":"passed"},"last_problems":["80baf8b31b56"],"memoryBase":0.0,"repNum":0.0,"speed":1.6428571428571428,"status":"learning","t0":"2026-08-17T16:30:00Z"}},"velocity":{"course_progress":0.0,"eta":null,"topics_per_week_28d":0.0,"xp_per_day_28d":0.0},"xp":{"goal":40,"streak_days":0,"today":7,"total":7}}"#;

#[test]
fn incremental_carries_the_corrected_xp_when_the_correction_arrives_late() {
    let graph = regrade_graph();
    let cfg = Config::default();
    let input = ProjectionInput::new(
        &graph,
        &cfg,
        Timestamp::parse("2026-08-19T16:30:00Z").unwrap(),
    );
    let prior = vec![
        regrade_attempt("a1", T0, "task-1", "blowoff", "[]"),
        regrade_lesson(T0, "blowoff", "-3.5"),
    ];
    let fresh = vec![event(
        r#"{"type":"regraded","ts":"2026-08-18T16:30:00Z","task_id":"task-1",
            "topic":"subtraction-facts","reason":"test","quality_tier":"nearly_perfect",
            "xp":7.0,"attempts":[{"attempt_id":"a1","work_quality":"nearly_perfect"}]}"#,
    )];
    let mut whole = prior.clone();
    whole.extend(fresh.clone());

    let full = project(&whole, &input).unwrap();
    let cached = project(&prior, &input).unwrap();
    let incremental = project_incremental(&cached, &prior, &fresh, &input).unwrap();
    // The prior half is not inert: its result XP is tallied with FIRe off, so the
    // correction reaches both paths.
    assert_eq!(full.xp.total, 7);
    assert_eq!(incremental.xp.total, 7);

    let actual_full = canonical_blob(&full).unwrap();
    assert!(
        actual_full == LATE_CORRECTION_FULL,
        "{}",
        first_difference(&actual_full, LATE_CORRECTION_FULL)
    );
    let actual_incremental = canonical_blob(&incremental).unwrap();
    assert!(
        actual_incremental == LATE_CORRECTION_INCREMENTAL,
        "{}",
        first_difference(&actual_incremental, LATE_CORRECTION_INCREMENTAL)
    );
}

#[test]
fn the_projector_version_is_stamped_on_a_minimal_fold() {
    let graph = regrade_graph();
    let model = regrade_fold(
        &[regrade_attempt("a1", T0, "task-1", "blowoff", "[]")],
        &graph,
    );
    assert_eq!(model.projector_version, Some(3));
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
    let cfg = Config::default();
    let input = ProjectionInput::new(
        graph,
        &cfg,
        Timestamp::parse("2026-07-28T12:02:00Z").unwrap(),
    );
    project(events, &input).expect("the fold succeeds")
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

fn seed_fold(events: &[Event], graph: &Curriculum, now_text: &str) -> LearnerModel {
    let cfg = Config::default();
    let input = ProjectionInput::new(graph, &cfg, Timestamp::parse(now_text).unwrap());
    project(events, &input).expect("the fold succeeds")
}

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
    let events = vec![
        event(&format!(
            r#"{{"type":"diagnostic_answer","ts":"{T_SEED}","topic":"t","correct":true,
                "secs":10,"weight":1.0}}"#
        )),
        event(r#"{"type":"diagnostic_placed","ts":"2026-07-28T12:01:00Z","balances":{"t":2.0}}"#),
        event(
            r#"{"type":"review_result","ts":"2026-07-28T12:02:00Z","topic":"t","passed":true,
                "weighted_score":1.0,"quality_tier":"perfect"}"#,
        ),
    ];
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
    let events = vec![
        event(&format!(
            r#"{{"type":"diagnostic_answer","ts":"{T_SEED}","topic":"B","correct":true,
                "secs":10,"weight":1.0}}"#
        )),
        event(r#"{"type":"diagnostic_placed","ts":"2026-07-28T12:01:00Z","balances":{"B":2.0}}"#),
        event(
            r#"{"type":"review_result","ts":"2026-07-28T12:02:00Z","topic":"B","passed":false,
                "weighted_score":0.0,"quality_tier":"poor"}"#,
        ),
        event(
            r#"{"type":"lesson_result","ts":"2026-07-28T12:03:00Z","topic":"A","passed":true,
                "quality_tier":"perfect"}"#,
        ),
    ];
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
    let base = vec![
        event(&format!(
            r#"{{"type":"diagnostic_answer","ts":"{T_SEED}","topic":"t","correct":true,
                "secs":10,"weight":1.0}}"#
        )),
        event(r#"{"type":"diagnostic_placed","ts":"2026-07-28T12:01:00Z","balances":{"t":2.0}}"#),
        event(
            r#"{"type":"review_result","ts":"2026-07-29T12:00:00Z","topic":"t","passed":true,
                "weighted_score":1.0,"quality_tier":"perfect"}"#,
        ),
        event(
            r#"{"type":"review_result","ts":"2026-08-03T12:00:00Z","topic":"t","passed":true,
                "weighted_score":1.0,"quality_tier":"perfect"}"#,
        ),
    ];
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
// The accumulator surface
// --------------------------------------------------------------------------- //

#[test]
fn a_light_replay_writes_no_topic_state() {
    // `apply_fire = false` skips every TopicState write while it still tallies XP.
    let graph = regrade_graph();
    let cfg = Config::default();
    let mut proj = Projector::new(&graph, &cfg);
    for item in [
        regrade_attempt("a1", T0, "task-1", "blowoff", "[]"),
        regrade_lesson(T0, "blowoff", "-3.5"),
    ] {
        proj.apply(&item, false);
    }
    assert!(proj.topics().is_empty());
    let model = proj
        .finalize(Timestamp::parse("2026-08-19T16:30:00Z").unwrap())
        .unwrap();
    assert!(model.topics.is_empty());
    assert_eq!(model.xp.total, -4);
}

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
