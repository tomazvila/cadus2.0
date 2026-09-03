//! M5 U8: the lesson advance of the grade path (`service.advance_task`), its
//! remediation, and the fold the path runs.
//!
//! Every XP total here is a literal of the 1.0 price table.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeMap;

use axum::Router;
use cadus_core::curriculum::Slug;
use cadus_core::learner::{LearnerModel, TopicState};
use cadus_store::test_support::TestDb;
use common::{
    BASE_US, LESSON, SESSION, Verdict, addition_curriculum, answer_lesson_ok, app_with_content,
    attempt_payload, events_of_type, learner_at_the_fifth_miss, lesson_app as app, lesson_learner,
    lesson_problem, lesson_state, put_state, seed_attempt_row, seed_cached_model, seed_event,
    seed_four_misses, seed_learner, seed_open_session, stored_state,
};
use serde_json::{Value, json};
use sqlx::types::Uuid;

/// The router of a test, with `subtraction` a KEY prerequisite of
/// `addition/kp1`.
///
/// The repeat-fail peel-back queues the key prerequisites of the failed
/// knowledge point, so a fixture with none can never show the event.
fn app_with_key_prereq(db: &TestDb) -> Router {
    app_with_content(
        db,
        addition_curriculum(vec![Slug::new("subtraction").unwrap()]),
    )
}

/// Seed a learner whose lesson stands at `kp` with one correct answer at `kp`
/// already in the log at `seq` 2, so the next correct answer passes the point.
/// `assisted` is the H3 flag of that earlier answer.
async fn learner_at_the_second_pass(db: &TestDb, email: &str, kp: &str, assisted: bool) -> Uuid {
    let user = seed_learner(db, email).await;
    seed_open_session(db, user).await;
    put_state(
        db,
        user,
        &lesson_state(lesson_problem(5.0, kp, Vec::new()), 1, false),
    )
    .await;
    let mut prior = attempt_payload(
        LESSON,
        "s_2026-01-01a-lesson-addition-0",
        kp,
        ("Compute 40 + 2.5.", "42.5"),
        &Verdict {
            given_answer: "42.5",
            correct: true,
            work_quality: "nearly_perfect",
            error_tags: json!([]),
            secs: 9,
        },
    );
    prior["assisted"] = json!(assisted);
    seed_attempt_row(db, user, 2, "s_2026-01-01a-lesson-addition-0", &prior).await;
    user
}

/// The reply of a fifth miss whose lesson had not failed before: the task
/// closed `task_failed` and one plain `lesson_fail` remediation is queued.
fn assert_plain_lesson_fail(body: &Value) {
    assert_eq!(body["task_status"], "task_failed");
    assert_eq!(
        body["remediation"],
        json!([{"kind": "lesson_fail", "targets": []}])
    );
}

// --------------------------------------------------------------------------- //
// The lesson advance
// --------------------------------------------------------------------------- //

/// Two correct answers in a row pass a knowledge point (`2consec`). The lesson
/// stands at its LAST knowledge point, so it closes: `task_passed`, one
/// `lesson_result` event, and the XP the core priced.
///
/// The literal XP: a lesson's base is 3.5 per knowledge point and `addition`
/// authors two, so the base is 7.0; `nearly_perfect` multiplies by 1.0.
#[tokio::test]
async fn a_second_correct_answer_at_the_last_kp_passes_the_lesson() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_at_the_second_pass(&db, "pass@example.com", "kp2", false).await;

        let body = answer_lesson_ok(&app, user, "13.5").await;
        assert_eq!(body["task_status"], "task_passed");
        assert_eq!(body["xp"], 7.0);
        assert_eq!(body["next"], Value::Null);

        let closes = events_of_type(&db, user, "lesson_result").await;
        assert_eq!(closes.len(), 1);
        assert_eq!(closes[0]["topic"], "addition");
        assert_eq!(closes[0]["passed"], true);
        assert_eq!(closes[0]["xp"], 7.0);
        assert_eq!(closes[0]["quality_tier"], "nearly_perfect");

        // The closed task drops its whole scratch.
        let scratch = stored_state(&db, user).await;
        assert!(scratch.tasks[LESSON].done);
        assert!(!scratch.served.contains_key(LESSON));
    })
    .await;
}

/// The passing lesson is reference-assisted when ANY of its attempts was: the
/// earlier answer took a hint, so the close carries `assisted: true` even
/// though the closing answer did not.
#[tokio::test]
async fn a_lesson_passed_with_an_earlier_assisted_answer_closes_assisted() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_at_the_second_pass(&db, "assisted-pass@example.com", "kp2", true).await;

        let body = answer_lesson_ok(&app, user, "13.5").await;
        assert_eq!(body["task_status"], "task_passed");

        let closes = events_of_type(&db, user, "lesson_result").await;
        assert_eq!(closes.len(), 1);
        assert_eq!(closes[0]["passed"], true);
        assert_eq!(closes[0]["assisted"], true);
    })
    .await;
}

/// Two correct answers at the FIRST knowledge point pass it, and the lesson
/// moves on: `kp_advance`, no close event, no XP, the progress row at `kp2`,
/// and a next problem drawn for it.
#[tokio::test]
async fn a_second_correct_answer_at_the_first_kp_advances_the_lesson() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_at_the_second_pass(&db, "advance@example.com", "kp1", false).await;

        let body = answer_lesson_ok(&app, user, "13.5").await;
        assert_eq!(body["task_status"], "kp_advance");
        assert_eq!(body.get("xp"), None);
        assert_eq!(body["remediation"], json!([]));
        assert_eq!(body["next"]["text"], "Compute 40 + 2.5.");
        assert_eq!(events_of_type(&db, user, "lesson_result").await.len(), 0);

        let scratch = stored_state(&db, user).await;
        assert!(!scratch.tasks[LESSON].done);
        assert_eq!(scratch.tasks[LESSON].current_kp.as_deref(), Some("kp2"));
        assert_eq!(scratch.served[LESSON].kp.as_deref(), Some("kp2"));
    })
    .await;
}

/// Five wrong answers fail the knowledge point (`lesson.fail_after` is 5). The
/// lesson closes `task_failed`, prices 0.0 XP at the `nearly_passable` tier
/// (3.5 × 1 knowledge point × 0.3 is 1.05), and queues one lesson-fail
/// remediation.
#[tokio::test]
async fn the_fifth_wrong_answer_fails_the_lesson_and_queues_remediation() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_at_the_fifth_miss(&db, "fail@example.com").await;

        let body = answer_lesson_ok(&app, user, "14").await;
        assert_plain_lesson_fail(&body);
        assert_eq!(body["xp"], 1.05);

        let closes = events_of_type(&db, user, "lesson_result").await;
        assert_eq!(closes.len(), 1);
        assert_eq!(closes[0]["passed"], false);
        assert_eq!(closes[0]["failed_at_kp"], "kp1");
        assert_eq!(
            events_of_type(&db, user, "remediation_triggered")
                .await
                .len(),
            1
        );
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The repeat-fail peel-back reads HISTORY (M5 review 2, finding V2)
// --------------------------------------------------------------------------- //

/// The session that closed one day before [`SESSION`].
const EARLIER: &str = "s_2025-12-31a";

/// The Unix microsecond instant of 2025-12-31T00:00:00Z.
const EARLIER_US: i64 = BASE_US - 86_400_000_000;

/// Seed a learner whose lesson failed at `kp1` in [`EARLIER`], and who stands
/// at the fifth miss of the same lesson in [`SESSION`].
async fn learner_at_the_second_failure(db: &TestDb, email: &str) -> Uuid {
    let user = seed_learner(db, email).await;

    // Session one: the lesson failed at `kp1`, and the session closed.
    seed_event(
        db,
        user,
        1,
        EARLIER_US,
        EARLIER,
        json!({
            "type": "session_start",
            "ts": "2025-12-31T00:00:00Z",
            "session": EARLIER,
            "v": 1,
        }),
    )
    .await;
    seed_event(
        db,
        user,
        2,
        EARLIER_US + 60_000_000,
        EARLIER,
        json!({
            "type": "lesson_result",
            "ts": "2025-12-31T00:01:00Z",
            "session": EARLIER,
            "v": 1,
            "topic": "addition",
            "passed": false,
            "failed_at_kp": "kp1",
            "xp": 1.05,
            "quality_tier": "nearly_passable",
            "assisted": false,
        }),
    )
    .await;
    seed_event(
        db,
        user,
        3,
        EARLIER_US + 120_000_000,
        EARLIER,
        json!({
            "type": "session_end",
            "ts": "2025-12-31T00:02:00Z",
            "session": EARLIER,
            "v": 1,
            "xp_earned": 1.05,
            "minutes": 2.0,
        }),
    )
    .await;

    // Session two: the same lesson, with four misses at `kp1` behind it.
    seed_event(
        db,
        user,
        4,
        BASE_US,
        SESSION,
        json!({
            "type": "session_start",
            "ts": "2026-01-01T00:00:00Z",
            "session": SESSION,
            "v": 1,
        }),
    )
    .await;
    seed_four_misses(db, user, 5).await;
    put_state(
        db,
        user,
        &lesson_state(lesson_problem(20.0, "kp1", Vec::new()), 4, false),
    )
    .await;
    user
}

/// V2. A lesson that failed at `kp1` in an EARLIER session peels back to the key
/// prerequisites of that knowledge point when it fails a second time.
///
/// The second failure of one lesson is only reachable in a later session: a
/// lesson task id is `{session}-lesson-{topic}`, and the failed task is `done`
/// in the D-S6 row for the rest of its own session. The open-session window
/// therefore never holds the earlier `lesson_result`, and the repeat test has to
/// read the whole-log map of the session view.
///
/// The literals: the reply queues `repeat_fail` on `subtraction`, the log holds
/// ONE `remediation_triggered` of that kind, and the second `lesson_result`
/// stands beside the first.
#[tokio::test]
async fn a_lesson_failed_in_an_earlier_session_peels_back_on_the_second_failure() {
    TestDb::with(|db| async move {
        let app = app_with_key_prereq(&db);
        let user = learner_at_the_second_failure(&db, "repeat-fail@example.com").await;

        // The fifth miss fails `kp1` a SECOND time.
        let body = answer_lesson_ok(&app, user, "14").await;
        assert_eq!(body["task_status"], "task_failed");
        assert_eq!(
            body["remediation"],
            json!([{"kind": "repeat_fail", "targets": ["subtraction"]}])
        );

        let queued = events_of_type(&db, user, "remediation_triggered").await;
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0]["kind"], "repeat_fail");
        assert_eq!(queued[0]["source_topic"], "addition");
        assert_eq!(queued[0]["targets"], json!(["subtraction"]));

        let closes = events_of_type(&db, user, "lesson_result").await;
        assert_eq!(closes.len(), 2);
        assert_eq!(closes[1]["passed"], false);
        assert_eq!(closes[1]["failed_at_kp"], "kp1");
    })
    .await;
}

/// A second failure of a knowledge point that names NO key prerequisite has
/// nothing to peel back to: the lesson closes `task_failed`, and no
/// remediation is queued at all.
#[tokio::test]
async fn a_second_failure_with_no_key_prerequisite_queues_nothing() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_at_the_second_failure(&db, "repeat-bare@example.com").await;

        let body = answer_lesson_ok(&app, user, "14").await;
        assert_eq!(body["task_status"], "task_failed");
        assert_eq!(body["xp"], 1.05);
        assert_eq!(body["remediation"], json!([]));

        assert_eq!(
            events_of_type(&db, user, "remediation_triggered")
                .await
                .len(),
            0
        );
        assert_eq!(events_of_type(&db, user, "lesson_result").await.len(), 2);
    })
    .await;
}

/// The FIRST failure of a lesson still queues the plain `lesson_fail`, even when
/// the failed knowledge point names a key prerequisite. The peel-back is the
/// SECOND failure and nothing else.
#[tokio::test]
async fn a_first_failure_queues_the_plain_lesson_fail() {
    TestDb::with(|db| async move {
        let app = app_with_key_prereq(&db);
        let user = learner_at_the_fifth_miss(&db, "first-fail@example.com").await;

        let body = answer_lesson_ok(&app, user, "14").await;
        assert_plain_lesson_fail(&body);
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The fold: incremental, and the full replay a `regraded` forces
// --------------------------------------------------------------------------- //

/// The second topic of the fixture curriculum. No event of these two logs ever
/// names it, so a saved model that carries it proves the fold started FROM the
/// cache, and a saved model without it proves the fold threw the cache away and
/// replayed the whole log.
const SENTINEL_TOPIC: &str = "subtraction";

/// The ability the cached model stamps on [`SENTINEL_TOPIC`]. It is a value no
/// fold of these logs can produce.
const SENTINEL_ABILITY: f64 = 0.75;

/// Cache a learner model at `through_seq` that carries [`SENTINEL_TOPIC`].
async fn poison_cache(db: &TestDb, user: Uuid, through_seq: i64) {
    let mut topics = BTreeMap::new();
    topics.insert(
        SENTINEL_TOPIC.to_string(),
        TopicState {
            ability: SENTINEL_ABILITY,
            ..TopicState::default()
        },
    );
    let model = LearnerModel {
        topics,
        ..LearnerModel::default()
    };
    seed_cached_model(db, user, &model, through_seq).await;
}

/// The cached model of `user` and the cursor it stands at.
async fn cached_model(db: &TestDb, user: Uuid) -> (Value, i64) {
    let row = sqlx::query!(
        r#"
        SELECT model AS "model!", through_seq AS "through_seq!"
        FROM learner_models WHERE user_id = $1
        "#,
        user
    )
    .fetch_one(&db.admin)
    .await
    .unwrap();
    (row.model, row.through_seq)
}

/// One `regraded` of an earlier attempt, at `seq`.
async fn seed_regraded(db: &TestDb, user: Uuid, seq: i64, attempt_id: &str) {
    let payload = json!({
        "type": "regraded",
        "ts": "2026-01-01T00:00:20Z",
        "session": SESSION,
        "v": 1,
        "task_id": LESSON,
        "topic": "addition",
        "attempts": [{
            "attempt_id": attempt_id,
            "work_quality": "poor",
            "error_tags": ["arithmetic-slip"],
            "grader_note": "operator repair",
        }],
        "reason": "an operator repair",
    });
    seed_event(db, user, seq, BASE_US + 20_000_000, SESSION, payload).await;
}

/// A learner with a poisoned cache at `seq` 1 and one live lesson problem.
async fn learner_with_poisoned_cache(db: &TestDb, email: &str) -> Uuid {
    let user = lesson_learner(db, email, lesson_problem(5.0, "kp1", Vec::new())).await;
    poison_cache(db, user, 1).await;
    user
}

/// Spec section 4.3 step 7. The grade path folds ONE event forward from the
/// cache, and it replays the WHOLE log when the events after the cursor hold a
/// `regraded`.
///
/// Both learners start from the same poisoned cache at `seq` 1. The learner
/// whose log holds no `regraded` keeps the sentinel topic, because the fold
/// started from the cache. The learner whose log holds one loses it, because the
/// fold threw the cache away.
#[tokio::test]
async fn a_regraded_in_the_log_makes_the_grade_path_replay_the_whole_fold() {
    TestDb::with(|db| async move {
        let app = app(&db);

        // The incremental arm: session_start at 1, the new attempt at 2.
        let plain = learner_with_poisoned_cache(&db, "fold-plain@example.com").await;
        answer_lesson_ok(&app, plain, "13.5").await;
        let (model, cursor) = cached_model(&db, plain).await;
        assert_eq!(cursor, 2);
        assert_eq!(
            model["topics"][SENTINEL_TOPIC]["ability"],
            json!(0.75),
            "the incremental fold must start from the cached model: {model}"
        );

        // The replay arm: session_start at 1, a `regraded` at 2, the new attempt
        // at 3.
        let repaired = learner_with_poisoned_cache(&db, "fold-regrade@example.com").await;
        seed_regraded(&db, repaired, 2, "s_2026-01-01a-lesson-addition-0").await;
        answer_lesson_ok(&app, repaired, "13.5").await;
        let (model, cursor) = cached_model(&db, repaired).await;
        assert_eq!(cursor, 3);
        assert!(
            model["topics"].get(SENTINEL_TOPIC).is_none(),
            "a regraded must force the full replay, which drops the cache: {model}"
        );
    })
    .await;
}
