//! M5 U8: the recorded event shapes of the grade path, diffed field by field
//! against the 1.0 shapes (oracle parity, HANDOVER section 2.5).
//!
//! Every key list here is the sorted key list of the 1.0 payload, written out.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axum::http::StatusCode;
use cadus_store::test_support::TestDb;
use common::{
    EXPECTED_ANSWER, LESSON, PROBLEM_ID, PROBLEM_TEXT, SESSION, answer_task, events_of_type,
    learner_with_kp1, lesson_app as app, lesson_problem, lesson_state, put_state, seed_four_misses,
    seed_learner, seed_open_session,
};
use serde_json::{Value, json};
use sqlx::types::Uuid;
use sqlx::types::chrono::DateTime;

/// The sorted key list of a 1.0 `attempt` event (`projector-1.0-spec.md:43`).
const ATTEMPT_KEYS: [&str; 19] = [
    "answer_kind",
    "assisted",
    "attempt_id",
    "correct",
    "error_tags",
    "given_answer",
    "grader_note",
    "kp",
    "problem",
    "secs",
    "session",
    "task_id",
    "task_type",
    "topic",
    "ts",
    "type",
    "v",
    "work",
    "work_quality",
];

/// The sorted key list of one event payload.
fn keys_of(event: &Value) -> Vec<String> {
    let mut keys: Vec<String> = event.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    keys
}

/// Every event of `user`, oldest first, as `(seq, type, payload)`.
async fn event_stream(db: &TestDb, user: Uuid) -> Vec<(i64, String, Value)> {
    sqlx::query!(
        r#"
        SELECT seq AS "seq!", type AS "type!", payload AS "payload!"
        FROM events WHERE user_id = $1 ORDER BY seq
        "#,
        user
    )
    .fetch_all(&db.admin)
    .await
    .unwrap()
    .into_iter()
    .map(|row| (row.seq, row.r#type, row.payload))
    .collect()
}

/// Answer the lesson's live problem with `14` and the shown work `8 + 5.5 = 14`.
async fn answer_with_work(app: &axum::Router, user: Uuid) -> Value {
    let (status, body) = answer_task(
        app,
        user,
        LESSON,
        json!({"problem_id": PROBLEM_ID, "answer": "14", "work": "8 + 5.5 = 14"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

/// The field-by-field check of one recorded `attempt` against the 1.0 shape.
fn assert_attempt_shape(event: &Value) {
    assert_eq!(keys_of(event), ATTEMPT_KEYS);
    assert_eq!(event["type"], "attempt");
    assert_eq!(event["task_id"], LESSON);
    assert_eq!(event["topic"], "addition");
    assert_eq!(event["kp"], "kp1");
    assert_eq!(event["task_type"], "lesson");
    assert_eq!(event["answer_kind"], "numeric");
    assert_eq!(event["problem"]["text"], PROBLEM_TEXT);
    assert_eq!(event["problem"]["expected"], EXPECTED_ANSWER);
    assert_eq!(event["given_answer"], "14");
    assert_eq!(event["work"], "8 + 5.5 = 14");
    assert_eq!(event["correct"], false);
    assert_eq!(event["work_quality"], "nearly_passable");
    assert_eq!(event["error_tags"], json!([]));
    assert_eq!(event["grader_note"], "deterministic");
    assert_eq!(event["assisted"], false);
    assert_eq!(event["session"], SESSION);
    assert_eq!(event["v"], 1);
}

/// The recorded `attempt` event, diffed field by field against the 1.0 shape
/// (`projector-1.0-spec.md:43`). `attempt_id` is excepted: 2.0 defines its own
/// deterministic rule (trap T12).
#[tokio::test]
async fn a_recorded_attempt_matches_the_1_0_event_shape() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_with_kp1(&db, "shape@example.com", 7.0).await;

        answer_with_work(&app, user).await;

        let recorded = events_of_type(&db, user, "attempt").await;
        assert_eq!(recorded.len(), 1);
        assert_attempt_shape(&recorded[0]);
        // The row's own columns carry the idempotency key and the type.
        let key = sqlx::query_scalar!(
            r#"SELECT attempt_id FROM events WHERE user_id = $1 AND type = 'attempt'"#,
            user
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(key.as_deref(), Some("s_2026-01-01a-lesson-addition-1"));
    })
    .await;
}

/// The WHOLE stream one recorded session writes, diffed field by field against
/// the 1.0 shape (`docs/reference/projector-1.0-spec.md:34-46`).
///
/// The closing miss appends three events in one transaction, so this case reads
/// all three 1.0 shapes the grade path produces: `attempt`, `lesson_result` and
/// `remediation_triggered`. The envelope of every one of them is `ts`, `session`
/// and `v` (`model.py:212-218`), and `type` is the union discriminator.
/// `attempt_id` is excepted from the diff: 2.0 defines its own deterministic
/// rule (trap T12).
#[tokio::test]
async fn the_recorded_session_stream_matches_the_1_0_event_shapes() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = seed_learner(&db, "stream@example.com").await;
        seed_open_session(&db, user).await;
        put_state(
            &db,
            user,
            &lesson_state(lesson_problem(20.0, "kp1", Vec::new()), 4, false),
        )
        .await;
        seed_four_misses(&db, user, 2).await;

        answer_with_work(&app, user).await;

        let stream = event_stream(&db, user).await;
        let shape: Vec<(i64, &str)> = stream
            .iter()
            .map(|(seq, kind, _)| (*seq, kind.as_str()))
            .collect();
        assert_eq!(
            shape,
            vec![
                (1, "session_start"),
                (2, "attempt"),
                (3, "attempt"),
                (4, "attempt"),
                (5, "attempt"),
                (6, "attempt"),
                (7, "lesson_result"),
                (8, "remediation_triggered"),
            ]
        );

        // The three events this ONE request appended, in the order it appended
        // them.
        let attempt = &stream[5].2;
        let result = &stream[6].2;
        let remediation = &stream[7].2;

        assert_attempt_shape(attempt);

        assert_eq!(
            keys_of(result),
            vec![
                "assisted",
                "failed_at_kp",
                "passed",
                "quality_tier",
                "session",
                "topic",
                "ts",
                "type",
                "v",
                "xp",
            ]
        );
        assert_eq!(result["type"], "lesson_result");
        assert_eq!(result["topic"], "addition");
        assert_eq!(result["passed"], false);
        assert_eq!(result["failed_at_kp"], "kp1");
        assert_eq!(result["xp"], 1.05);
        assert_eq!(result["quality_tier"], "nearly_passable");
        assert_eq!(result["assisted"], false);
        assert_eq!(result["session"], SESSION);
        assert_eq!(result["v"], 1);

        assert_eq!(
            keys_of(remediation),
            vec![
                "kind",
                "session",
                "source_topic",
                "targets",
                "ts",
                "type",
                "v",
            ]
        );
        assert_eq!(remediation["type"], "remediation_triggered");
        assert_eq!(remediation["kind"], "lesson_fail");
        assert_eq!(remediation["source_topic"], "addition");
        assert_eq!(remediation["targets"], json!([]));
        assert_eq!(remediation["session"], SESSION);
        assert_eq!(remediation["v"], 1);

        // The envelope instant of all three is the RFC 3339 `Z` spelling of 1.0.
        for event in [attempt, result, remediation] {
            let ts = event["ts"].as_str().unwrap();
            assert!(ts.ends_with('Z'), "{ts}");
            DateTime::parse_from_rfc3339(ts).unwrap();
        }

        // The three rows carry the session on the COLUMN too, so the session
        // reader of unit U6 finds them without reading the payload.
        let tagged = sqlx::query_scalar!(
            r#"SELECT count(*) AS "n!" FROM events WHERE user_id = $1 AND session_id = $2"#,
            user,
            SESSION
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(tagged, 8);
    })
    .await;
}
