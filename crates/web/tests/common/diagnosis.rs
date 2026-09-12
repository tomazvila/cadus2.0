//! The fixtures of `tests/diagnosis_route.rs` and its parts.

use std::time::Duration;

use axum::http::header;
use cadus_core::curriculum::{
    AnswerKind, Catalog, Course, Curriculum, Exemplar, KnowledgePoint, RawCurriculum, RawUnit,
    Slug, Topic, Unit,
};
use cadus_core::event::{Event, SchemaVersion, SessionStart, Timestamp};
use cadus_core::pool::PoolAnswer;
use cadus_core::template::{GateSpec, gate_diagnosis_body};
use cadus_store::diagnosis::JobRow;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::diagnosis::{DiagnosisHub, job_view, match_distractor};
use cadus_web::state::{Content, ServedProblem, WebState};
use cadus_web::{AppState, create_app};
use http_body_util::BodyExt;
use tower::ServiceExt;

pub use super::prelude::*;
use super::*;

/// The `attempt_id` of the first answer of the lesson (`{task_id}-{n}`, n = 1).
pub const ATTEMPT_ID: &str = "s_2026-01-01a-lesson-addition-1";

/// The `attempt_id` of the second attempt of the lesson.
pub const ATTEMPT_ID_2: &str = "s_2026-01-01a-lesson-addition-2";

/// The `attempt_id` of the third attempt of the lesson.
pub const ATTEMPT_ID_3: &str = "s_2026-01-01a-lesson-addition-3";

/// The wrong answer the authored distractor names.
pub const DISTRACTOR_ANSWER: &str = "13";

/// The prose the authored distractor carries.
pub const DISTRACTOR_NOTE: &str = "You added the whole parts and dropped the half.";

/// A wrong answer NO authored distractor names.
pub const UNKNOWN_MISS: &str = "99";

// --------------------------------------------------------------------------- //
// The fixture curriculum
// --------------------------------------------------------------------------- //

/// The fixture curriculum: one course, one module, two topics.
pub fn graph() -> Curriculum {
    addition_curriculum(Vec::new())
}

// --------------------------------------------------------------------------- //
// The harness
// --------------------------------------------------------------------------- //

/// The router of a test, sharing `hub` with the caller.
pub fn app_with(db: &TestDb, hub: &Arc<DiagnosisHub>) -> Router {
    create_app(
        AppState::new(Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS))
            .with_content(Arc::new(super::open_content(graph())))
            .with_diagnosis(Arc::clone(hub)),
    )
}

/// The router of a test that needs no push.
pub fn app(db: &TestDb) -> Router {
    app_with(db, &Arc::new(DiagnosisHub::new()))
}

/// One request against the router, with the body read as JSON. `tenant` is the
/// bound learner.
pub async fn call(
    app: &Router,
    method: Method,
    uri: &str,
    tenant: Option<Uuid>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let (status, text) = super::call(app, method, uri, tenant, body).await;
    (status, serde_json::from_str(&text).unwrap_or(Value::Null))
}

/// Answer the lesson's live problem.
pub async fn answer(app: &Router, user: Uuid, given: &str) -> (StatusCode, Value) {
    call(
        app,
        Method::POST,
        &format!("/api/task/{LESSON}/answer"),
        Some(user),
        Some(json!({ "problem_id": PROBLEM_ID, "answer": given })),
    )
    .await
}

/// The problem that is live on the learner's screen, dealt twelve seconds ago.
pub fn live_problem() -> ServedProblem {
    lesson_problem(12.0, "kp1", Vec::new())
}

/// A D-S6 document holding one live lesson problem.
pub fn state_with_live_problem() -> WebState {
    lesson_state(live_problem(), 0, false)
}

/// Seed a learner with an open session and one live lesson problem.
pub async fn learner(db: &TestDb, email: &str) -> Uuid {
    let user = super::seed_learner(db, email).await;
    seed_open_session(db, user).await;
    put_state(db, user, &state_with_live_problem()).await;
    user
}

/// Put one approved kind-`diagnosis` document on `KEY` (spec section 6.2).
pub async fn seed_distractors(db: &TestDb, digest: &str, body: &Value) {
    sqlx::query!(
        r#"
        INSERT INTO content_store (digest, kp_id, kind, body, status, approved_at,
                                   approved_curriculum_digest, approved_review_engine_digest)
        VALUES ($1, $2, 'diagnosis', $3, 'approved', now(), $4, $5)
        "#,
        digest,
        KEY,
        body,
        "curriculum-v1",
        "engine-v1",
    )
    .execute(&db.admin)
    .await
    .unwrap();
}

/// Every job row of `user`, oldest first.
pub async fn jobs_of(db: &TestDb, user: Uuid) -> Vec<(Uuid, String, Value)> {
    sqlx::query!(
        r#"
        SELECT id AS "id!", attempt_id AS "attempt_id!", payload AS "payload!"
        FROM diagnosis_jobs WHERE user_id = $1 ORDER BY created_at, id
        "#,
        user
    )
    .fetch_all(&db.admin)
    .await
    .unwrap()
    .into_iter()
    .map(|row| (row.id, row.attempt_id, row.payload))
    .collect()
}

/// Put one wrong attempt of the lesson into the log, at `seq`, under `attempt_id`.
pub async fn seed_attempt(db: &TestDb, user: Uuid, seq: i64, attempt_id: &str) {
    let verdict = Verdict {
        given_answer: UNKNOWN_MISS,
        correct: false,
        work_quality: "nearly_passable",
        error_tags: json!([]),
        secs: 20,
    };
    super::seed_attempt(db, user, seq, attempt_id, verdict).await;
}

/// Seed one pending job row and return its id.
pub async fn seed_pending_job(db: &TestDb, user: Uuid, attempt_id: &str) -> Uuid {
    sqlx::query_scalar!(
        r#"
        INSERT INTO diagnosis_jobs (user_id, attempt_id, status, payload)
        VALUES ($1, $2, 'pending', '{}'::jsonb)
        RETURNING id AS "id!"
        "#,
        user,
        attempt_id,
    )
    .fetch_one(&db.admin)
    .await
    .unwrap()
}

/// Seed one finished job row and return its id.
pub async fn seed_done_job(db: &TestDb, user: Uuid, attempt_id: &str, result: &Value) -> Uuid {
    sqlx::query_scalar!(
        r#"
        INSERT INTO diagnosis_jobs (user_id, attempt_id, status, payload, result, finished_at)
        VALUES ($1, $2, 'done', '{}'::jsonb, $3, now())
        RETURNING id AS "id!"
        "#,
        user,
        attempt_id,
        result,
    )
    .fetch_one(&db.admin)
    .await
    .unwrap()
}

/// The 11 tags of spec section 5.3, spelled out.
pub const VOCABULARY: [&str; 11] = [
    "sign-error",
    "arithmetic-slip",
    "algebra-slip",
    "wrong-method",
    "formula-recall",
    "misread-problem",
    "incomplete",
    "notation",
    "units",
    "timing-unreliable",
    "blowoff",
];

/// Put the authored `arithmetic-slip` distractor of [`DISTRACTOR_ANSWER`] on
/// `KEY`, under `digest`.
pub async fn seed_slip_distractor(db: &TestDb, digest: &str) {
    seed_distractors(
        db,
        digest,
        &json!({
            "v": 1,
            "distractors": [
                { "answer": DISTRACTOR_ANSWER,
                  "error_tag": "arithmetic-slip",
                  "note": DISTRACTOR_NOTE }
            ]
        }),
    )
    .await;
}

/// The inline `ready` diagnosis of the `arithmetic-slip` distractor.
pub fn ready_slip() -> Value {
    json!({
        "status": "ready",
        "error_tags": ["arithmetic-slip"],
        "prose": "You added the whole parts and dropped the half.",
    })
}

// --------------------------------------------------------------------------- //
// The push channel
// --------------------------------------------------------------------------- //

/// Read the next non-empty data frame of a streaming body, or `None`.
pub async fn next_frame(body: &mut Body, within: Duration) -> Option<String> {
    let deadline = tokio::time::Instant::now() + within;
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            return None;
        }
        let frame = match tokio::time::timeout(left, body.frame()).await {
            Ok(Some(Ok(frame))) => frame,
            _ => return None,
        };
        let Some(bytes) = frame.data_ref() else {
            continue;
        };
        let text = String::from_utf8_lossy(bytes).into_owned();
        if !text.trim().is_empty() {
            return Some(text);
        }
    }
}

/// Open one `/api/diagnosis/stream` as `user`.
pub async fn open_stream(app: &Router, user: Uuid) -> Body {
    let mut request = Request::builder()
        .method(Method::GET)
        .uri("/api/diagnosis/stream")
        .body(Body::empty())
        .unwrap();
    super::present_session(request.headers_mut(), user);
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/event-stream")
    );
    response.into_body()
}
