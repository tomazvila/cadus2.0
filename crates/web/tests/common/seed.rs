//! The admin-pool seeds of the log, the pool and the D-S6 row, and the reads
//! that check them.
//!
//! Every query text here is one the `.sqlx` offline cache already holds.

use std::collections::BTreeMap;

use cadus_core::config::Config;
use cadus_core::event::Event;
use cadus_core::learner::LearnerModel;
use cadus_core::pool::{PoolAnswer, PoolProblem};
use cadus_store::test_support::TestDb;
use cadus_web::state::{ServedProblem, TaskProgress, WebState};
use serde_json::{Value, json};
use sqlx::types::Uuid;
use sqlx::types::chrono::{DateTime, Utc};

use super::{
    BASE_US, EXPECTED_ANSWER, LESSON, PROBLEM_ID, PROBLEM_TEXT, SESSION, SOLUTION, seed_learner,
};

/// Put one event row of `user` into the log, at `seq`.
pub async fn seed_event_row(
    db: &TestDb,
    user: Uuid,
    seq: i64,
    ts_us: i64,
    kind: &str,
    session: Option<&str>,
    payload: &Value,
) {
    let ts = DateTime::<Utc>::from_timestamp_micros(ts_us).unwrap();
    sqlx::query!(
        r#"
        INSERT INTO events (user_id, seq, ts, type, session_id, v, payload)
        VALUES ($1, $2, $3, $4, $5, 1, $6)
        "#,
        user,
        seq,
        ts,
        kind,
        session,
        payload
    )
    .execute(&db.admin)
    .await
    .unwrap();
}

/// Put one literal event of `user` into the log, at `seq`.
pub async fn seed_event(
    db: &TestDb,
    user: Uuid,
    seq: i64,
    ts_us: i64,
    session: &str,
    payload: Value,
) {
    let kind = payload["type"].as_str().unwrap().to_owned();
    seed_event_row(db, user, seq, ts_us, &kind, Some(session), &payload).await;
}

/// Put one typed event of `user` into the log, at `seq`, at its own `ts`.
pub async fn seed_typed_event(db: &TestDb, user: Uuid, seq: i64, event: &Event) {
    seed_event_row(
        db,
        user,
        seq,
        event.ts().micros(),
        event.type_name(),
        event.session(),
        &serde_json::to_value(event).unwrap(),
    )
    .await;
}

/// Open `SESSION` in the log of `user`, at `seq` 1.
pub async fn seed_open_session(db: &TestDb, user: Uuid) {
    seed_event(
        db,
        user,
        1,
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
}

/// Put one unclaimed template row into the pool of `(user, key)`.
///
/// `curriculum_digest` and `review_engine_digest` supply the generation context
/// required by migration 0019. Without them the pop succeeds but the hand-off
/// refuses the template row.
pub async fn seed_pool_row(
    db: &TestDb,
    user: Uuid,
    key: &str,
    text: &str,
    answer: &str,
    hash: &str,
    digests: (&str, &str),
) {
    let (curriculum_digest, review_engine_digest) = digests;
    let problem = PoolProblem {
        v: 1,
        text: text.to_string(),
        bindings: BTreeMap::new(),
        seed: 7,
    };
    let expected = PoolAnswer {
        answer_contract: None,
        v: 1,
        answer: answer.to_string(),
    };
    sqlx::query!(
        r#"
        INSERT INTO serving_pool
            (user_id, kp_id, source, content_digest, problem, expected_answer, instance_hash,
             source_curriculum_digest, source_review_engine_digest)
        VALUES ($1, $2, 'template', NULL, $3::text::jsonb, $4::text::jsonb, $5, $6, $7)
        "#,
        user,
        key,
        problem.to_body().unwrap(),
        expected.to_body().unwrap(),
        hash,
        curriculum_digest,
        review_engine_digest,
    )
    .execute(&db.admin)
    .await
    .unwrap();
}

/// Cache `model` for `user` at `through_seq`, under the default config hash.
pub async fn seed_cached_model(db: &TestDb, user: Uuid, model: &LearnerModel, through_seq: i64) {
    sqlx::query!(
        r#"
        INSERT INTO learner_models
            (user_id, model, through_seq, projector_version, config_hash)
        VALUES ($1, $2, $3, $4, $5)
        "#,
        user,
        serde_json::to_value(model).unwrap(),
        through_seq,
        // The seeded cache must be a VALID cache, so it carries the version this
        // build folds with. A stale version forces the full replay and the test
        // then measures the replay it did not mean to measure.
        i32::try_from(cadus_core::projector::PROJECTOR_VERSION).unwrap(),
        Config::default().config_hash().unwrap()
    )
    .execute(&db.admin)
    .await
    .unwrap();
}

/// The Unix seconds of now, in the time base of `ServedProblem::started_at`.
pub fn now_secs() -> f64 {
    Utc::now().timestamp_micros() as f64 / 1_000_000.0
}

/// One live problem of the `addition` lesson, `age_secs` old, at `kp`.
pub fn lesson_problem(age_secs: f64, kp: &str, hints: Vec<String>) -> ServedProblem {
    ServedProblem {
        timing_interrupted: false,
        problem_id: PROBLEM_ID.to_string(),
        task_id: LESSON.to_string(),
        topic: Some("addition".to_string()),
        serve_topic: Some("addition".to_string()),
        kp: Some(kp.to_string()),
        answer_kind: Some("numeric".to_string()),
        text: PROBLEM_TEXT.to_string(),
        expected: PoolAnswer {
            answer_contract: None,
            v: 1,
            answer: EXPECTED_ANSWER.to_string(),
        },
        solution_sketch: Some(SOLUTION.to_string()),
        started_at: now_secs() - age_secs,
        hints_given: hints,
        index: 0,
        rework: None,
        handoff: None,
    }
}

/// A D-S6 document holding one live lesson problem.
pub fn lesson_state(live: ServedProblem, answered: i64, done: bool) -> WebState {
    let mut scratch = WebState::for_session(SESSION);
    scratch.tasks.insert(
        live.task_id.clone(),
        TaskProgress {
            task_id: live.task_id.clone(),
            task_type: "lesson".to_string(),
            total: 0,
            served: 1,
            answered,
            done,
            current_kp: live.kp.clone(),
        },
    );
    scratch.served.insert(live.task_id.clone(), live);
    scratch
}

/// Write a D-S6 document for `user`.
pub async fn put_state(db: &TestDb, user: Uuid, scratch: &WebState) {
    sqlx::query!(
        r#"
        INSERT INTO web_states (user_id, doc) VALUES ($1, $2)
        ON CONFLICT (user_id) DO UPDATE SET doc = excluded.doc
        "#,
        user,
        scratch.to_doc()
    )
    .execute(&db.admin)
    .await
    .unwrap();
}

/// The stored D-S6 document of `user`.
pub async fn stored_state(db: &TestDb, user: Uuid) -> WebState {
    let doc = sqlx::query_scalar!(
        r#"SELECT doc AS "doc!" FROM web_states WHERE user_id = $1"#,
        user
    )
    .fetch_one(&db.admin)
    .await
    .unwrap();
    WebState::from_doc(&doc).unwrap()
}

/// Every event of `user` of one type, oldest first.
pub async fn events_of_type(db: &TestDb, user: Uuid, kind: &str) -> Vec<Value> {
    sqlx::query_scalar!(
        r#"SELECT payload AS "payload!" FROM events WHERE user_id = $1 AND type = $2 ORDER BY seq"#,
        user,
        kind
    )
    .fetch_all(&db.admin)
    .await
    .unwrap()
}

/// Seed a learner with an open session and one live lesson problem.
pub async fn lesson_learner(db: &TestDb, email: &str, live: ServedProblem) -> Uuid {
    let user = seed_learner(db, email).await;
    seed_open_session(db, user).await;
    put_state(db, user, &lesson_state(live, 0, false)).await;
    user
}

/// Seed a learner whose live `kp1` problem of the lesson went on screen
/// `age_secs` ago.
pub async fn learner_with_kp1(db: &TestDb, email: &str, age_secs: f64) -> Uuid {
    lesson_learner(db, email, lesson_problem(age_secs, "kp1", Vec::new())).await
}

/// The verdict one seeded attempt carries. Every field is a literal of the
/// caller, so a test pins what the stored event holds and reads it back from a
/// reply.
pub struct Verdict<'a> {
    pub given_answer: &'a str,
    pub correct: bool,
    pub work_quality: &'a str,
    pub error_tags: Value,
    pub secs: i64,
}

/// A wrong answer of the lesson at the `nearly_passable` tier.
pub fn a_miss() -> Verdict<'static> {
    Verdict {
        given_answer: "14",
        correct: false,
        work_quality: "nearly_passable",
        error_tags: json!([]),
        secs: 20,
    }
}

/// The payload of one `attempt` event of `task_id` at `kp`, in the 1.0 shape.
pub fn attempt_payload(
    task_id: &str,
    attempt_id: &str,
    kp: &str,
    problem: (&str, &str),
    verdict: &Verdict<'_>,
) -> Value {
    json!({
        "type": "attempt",
        "ts": "2026-01-01T00:00:10Z",
        "session": SESSION,
        "v": 1,
        "attempt_id": attempt_id,
        "task_id": task_id,
        "topic": "addition",
        "kp": kp,
        "task_type": "lesson",
        "problem": {"text": problem.0, "expected": problem.1},
        "given_answer": verdict.given_answer,
        "correct": verdict.correct,
        "secs": verdict.secs,
        "error_tags": verdict.error_tags,
        "work_quality": verdict.work_quality,
    })
}

/// Put one attempt row into the log at `seq`, ten seconds into `SESSION`, with
/// `attempt_id` on the idempotency column.
pub async fn seed_attempt_row(
    db: &TestDb,
    user: Uuid,
    seq: i64,
    attempt_id: &str,
    payload: &Value,
) {
    let ts = DateTime::<Utc>::from_timestamp_micros(BASE_US + 10_000_000).unwrap();
    sqlx::query!(
        r#"
        INSERT INTO events (user_id, seq, ts, type, session_id, v, attempt_id, payload)
        VALUES ($1, $2, $3, 'attempt', $4, 1, $5, $6)
        "#,
        user,
        seq,
        ts,
        SESSION,
        attempt_id,
        payload
    )
    .execute(&db.admin)
    .await
    .unwrap();
}

/// Put one attempt of `task_id` at `kp1` into the log, at `seq`, under `attempt_id`.
pub async fn seed_task_attempt(
    db: &TestDb,
    user: Uuid,
    seq: i64,
    task_id: &str,
    attempt_id: &str,
    verdict: Verdict<'_>,
) {
    let payload = attempt_payload(
        task_id,
        attempt_id,
        "kp1",
        (PROBLEM_TEXT, EXPECTED_ANSWER),
        &verdict,
    );
    seed_attempt_row(db, user, seq, attempt_id, &payload).await;
}

/// Put one attempt of the lesson into the log, at `seq`, under `attempt_id`.
pub async fn seed_attempt(
    db: &TestDb,
    user: Uuid,
    seq: i64,
    attempt_id: &str,
    verdict: Verdict<'_>,
) {
    seed_task_attempt(db, user, seq, LESSON, attempt_id, verdict).await;
}

/// Seed a learner whose lesson stands at `kp1` with four misses behind it, so
/// the next miss is the fifth.
pub async fn learner_at_the_fifth_miss(db: &TestDb, email: &str) -> Uuid {
    let user = seed_learner(db, email).await;
    seed_open_session(db, user).await;
    seed_four_misses(db, user, 2).await;
    put_state(
        db,
        user,
        &lesson_state(lesson_problem(20.0, "kp1", Vec::new()), 4, false),
    )
    .await;
    user
}

/// Put four wrong answers at `kp1` of the lesson into the log, at `seq`
/// `first_seq` to `first_seq + 3`.
pub async fn seed_four_misses(db: &TestDb, user: Uuid, first_seq: i64) {
    for n in 0..4_i64 {
        seed_attempt(
            db,
            user,
            first_seq + n,
            &format!("{LESSON}-seed-{n}"),
            a_miss(),
        )
        .await;
    }
}
