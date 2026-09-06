//! U3 acceptance, part 1: the fixture fold, the coverage stream, the
//! incremental resume, and the live 1.0 oracle (R5, D3, D4, C2).
//!
//! Every expected value below is a LITERAL: a digest, a byte string, or a number that
//! `docs/reference/projector-1.0-spec.md` sections 8 and 9 pin, or that the comment
//! above it derives by hand from `config.yaml`. No expectation calls the code under
//! test.
//!
//! The stream fixture is `tests/fixtures/events/stream_1.jsonl` and its 1.0 fold is
//! `tests/fixtures/events/model_1.json`, both produced by `scripts/oracle`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::OnceLock;

use cadus_core::event::{Event, TopicStatus};
use cadus_core::learner::LearnerModel;
use cadus_core::projector::{
    PROJECTOR_VERSION, ProjectionInput, blob_digest, canonical_blob, project, project_incremental,
};
use common::events::{
    assert_same_blob, event, fixture, fold, input, live_oracle_blob, oracle_stamp, stream,
};

/// The 1.0 fold of `stream_1.jsonl`, as the SHA-256 of the canonical blob with
/// `built_from_ts` removed (spec section 9).
const STREAM_1_DIGEST: &str = "ba128459985e0815db7446cb2af16452ec07d304b7efaa0952fc6567404245f5";

/// The `config_hash` of the default config (spec section 9).
const CONFIG_HASH: &str = "797575e985c12149";

/// The number of events in `stream_1.jsonl` (spec section 9).
const STREAM_1_EVENTS: usize = 47;

/// Assert that the blob of `model` is the committed fixture `name`, byte for byte.
///
/// The fixture is the oracle's blob plus the newline `print` adds; the compared
/// bytes are the file with that newline removed. `built_from_ts` is already absent.
#[track_caller]
fn assert_blob_is_fixture(model: &LearnerModel, name: &str) {
    let committed = std::fs::read_to_string(fixture(name)).expect("the model reads");
    let expected = committed.trim_end_matches('\n');
    let actual = canonical_blob(model).unwrap();
    assert_same_blob(&actual, expected, name);
}

/// Run `check` on the incremental resume of `events` at every split, with the
/// full replay of the whole stream beside it.
fn for_each_split(
    events: &[Event],
    input: &ProjectionInput<'_>,
    mut check: impl FnMut(usize, &LearnerModel, &LearnerModel),
) {
    // The models the check reads carry the 1.0 stamp, because the pinned digests come
    // from the 1.0 oracle (D-F2). The cached seed keeps the 2.0 stamp.
    let full = oracle_stamp(project(events, input).unwrap());
    for split in 0..=events.len() {
        let (prior, fresh) = events.split_at(split);
        let cached = project(prior, input).unwrap();
        let incremental = oracle_stamp(project_incremental(&cached, prior, fresh, input).unwrap());
        check(split, &incremental, &full);
    }
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
    assert_blob_is_fixture(&fold(&stream("stream_1.jsonl")), "model_1.json");
}

#[test]
fn the_fold_stamps_the_projector_version_and_the_config_hash() {
    // The third attempt outcome changed the fold, so the stamp moved 3 to 4 (D-F2).
    assert_eq!(PROJECTOR_VERSION, 4);
    let events = stream("stream_1.jsonl");
    let model = project(&events, &input()).expect("the fold succeeds");
    assert_eq!(model.projector_version, Some(4));
    assert_eq!(model.config_hash.as_deref(), Some(CONFIG_HASH));
    // The parity comparison restamps the model with the 1.0 version and nothing else.
    assert_eq!(fold(&events).projector_version, Some(3));
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
    assert_eq!(events.len(), STREAM_1_EVENTS);
    for_each_split(&events, &input(), |split, incremental, full| {
        let actual = canonical_blob(incremental).unwrap();
        let expected = canonical_blob(full).unwrap();
        assert_same_blob(&actual, &expected, &format!("split {split}"));
    });
}

#[test]
fn the_live_oracle_agrees_with_the_fold() {
    let Some(expected) = live_oracle_blob("stream_1.jsonl", None) else {
        eprintln!("skipped: CADUS_ORACLE_PYTHON is not set");
        return;
    };
    let actual = canonical_blob(&fold(&stream("stream_1.jsonl"))).unwrap();
    assert_same_blob(&actual, &expected, "stream_1.jsonl");
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
    let input = input().with_timezone(tz);
    oracle_stamp(project(&stream("stream_u3_coverage.jsonl"), &input).expect("the fold succeeds"))
}

#[test]
fn the_coverage_stream_folds_to_the_oracle_digest_and_bytes() {
    let events = stream("stream_u3_coverage.jsonl");
    assert_eq!(events.len(), COVERAGE_EVENTS);
    let model = coverage_fold(None);
    assert_eq!(blob_digest(&model).unwrap(), COVERAGE_DIGEST);
    assert_blob_is_fixture(&model, "model_u3_coverage.json");
}

#[test]
fn the_coverage_stream_folds_to_the_oracle_digest_in_new_york() {
    // The time zone enters only through `local_day` (trap T9), so only "today", the
    // streak, and the velocity window move with it.
    let model = coverage_fold(Some("America/New_York"));
    assert_eq!(blob_digest(&model).unwrap(), COVERAGE_DIGEST_NEW_YORK);
    assert_blob_is_fixture(&model, "model_u3_coverage_new_york.json");
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
/// 31 put the reset in the light half and a graded event after it.
///
/// **1.0 does NOT route such a stream away from the incremental path.**
/// `service.py:272` forces a full replay on a `Regraded` event or on a
/// `projector_version` mismatch, and on nothing else; a `ProfileReset` matches neither
/// arm, and `service.py` never imports `ProfileReset`. REQUIREMENTS.md D4 reserves the
/// full replay for the same two triggers in 2.0, so the reset class stays on the
/// incremental path on BOTH sides. The port therefore reproduces 1.0 here rather than
/// hides the divergence: it leaves the full replay at these splits and at no other, and
/// it lands on the SAME model 1.0 lands on (`incremental_1_0.json`; finding #4 of the
/// M3 review round 1).
const COVERAGE_DIVERGING_SPLITS: [usize; 11] = [21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31];

/// The coverage stream's row of `incremental_1_0.json`
/// (`scripts/oracle/incremental_splits_1_0.py`), read once for the test binary.
fn coverage_incremental_row() -> &'static serde_json::Value {
    static ROW: OnceLock<serde_json::Value> = OnceLock::new();
    ROW.get_or_init(|| {
        let text =
            std::fs::read_to_string(fixture("incremental_1_0.json")).expect("the index reads");
        let index: serde_json::Value = serde_json::from_str(&text).expect("the index parses");
        index["streams"]
            .as_array()
            .expect("the index holds an array of streams")
            .iter()
            .find(|entry| entry["stream"] == "stream_u3_coverage.jsonl")
            .expect("the coverage stream has a row")
            .clone()
    })
}

/// The 1.0 incremental digest of the coverage stream at `split`.
///
/// `None` when 1.0 agrees with its own full replay at that split.
fn coverage_incremental_digest(split: usize) -> Option<String> {
    coverage_incremental_row()["mismatching_digests"][split.to_string()]
        .as_str()
        .map(ToOwned::to_owned)
}

/// The 1.0 full-replay digest of the coverage stream.
fn coverage_full_digest() -> String {
    coverage_incremental_row()["full_digest"]
        .as_str()
        .expect("the row holds a full digest")
        .to_owned()
}

#[test]
fn incremental_matches_full_replay_at_the_same_splits_as_one_point_zero() {
    let events = stream("stream_u3_coverage.jsonl");
    assert_eq!(blob_digest(&fold(&events)).unwrap(), coverage_full_digest());

    let mut diverging: Vec<usize> = Vec::new();
    for_each_split(&events, &input(), |split, incremental, full| {
        if canonical_blob(incremental).unwrap() != canonical_blob(full).unwrap() {
            diverging.push(split);
        }
    });
    assert_eq!(diverging, COVERAGE_DIVERGING_SPLITS.to_vec());
}

#[test]
fn the_diverging_splits_land_on_the_same_model_one_point_zero_lands_on() {
    // The `profile_reset` divergence stays on the incremental path in 1.0
    // (`service.py:272`), so the port must reproduce 1.0's OWN divergent model at every
    // one of these splits, not merely leave the full replay there.
    let events = stream("stream_u3_coverage.jsonl");
    assert_eq!(events.len(), COVERAGE_EVENTS);
    let full = coverage_full_digest();

    for_each_split(&events, &input(), |split, incremental, _| {
        let actual = blob_digest(incremental).unwrap();
        let expected = coverage_incremental_digest(split).unwrap_or_else(|| full.clone());
        assert_eq!(
            actual, expected,
            "split {split}: the resume gives a different model than 1.0 gives"
        );
    });

    // Every recorded divergent split carries a digest of its own. One equal to the full
    // replay would make the comparison above vacuous.
    for split in COVERAGE_DIVERGING_SPLITS {
        let divergent = coverage_incremental_digest(split)
            .unwrap_or_else(|| panic!("split {split} has no 1.0 digest"));
        assert_ne!(divergent, full, "split {split}");
    }
}

#[test]
fn the_live_oracle_agrees_on_the_coverage_stream() {
    let Some(expected) = live_oracle_blob("stream_u3_coverage.jsonl", None) else {
        eprintln!("skipped: CADUS_ORACLE_PYTHON is not set");
        return;
    };
    let actual = canonical_blob(&coverage_fold(None)).unwrap();
    assert_same_blob(&actual, &expected, "the coverage stream in UTC");

    let expected = live_oracle_blob("stream_u3_coverage.jsonl", Some("America/New_York"))
        .expect("the oracle runs");
    let actual = canonical_blob(&coverage_fold(Some("America/New_York"))).unwrap();
    assert_same_blob(&actual, &expected, "the coverage stream in New York");
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
// The mastery floor stamps an untouched topic only
// --------------------------------------------------------------------------- //

#[test]
fn a_floor_topic_with_a_history_keeps_its_status_on_enrollment() {
    // `place-value` is on the mastery floor of `foundations`. A lesson passed
    // BEFORE the enrollment leaves it `learning`; the floor stamp skips it.
    let events = vec![
        event(
            r#"{"type":"lesson_result","ts":"2026-05-04T09:00:00Z","topic":"place-value",
                "passed":true,"xp":10.0,"quality_tier":"perfect"}"#,
        ),
        event(r#"{"type":"enrolled","ts":"2026-05-04T09:01:00Z","course":"foundations"}"#),
    ];
    let model = fold(&events);
    assert_eq!(model.topics["place-value"].status, TopicStatus::Learning);
    assert_eq!(
        model.topics["single-digit-addition"].status,
        TopicStatus::Floor
    );
}
