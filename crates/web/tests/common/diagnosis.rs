//! The fixtures of `tests/diagnosis_route.rs` and its parts.

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use cadus_core::curriculum::{
    AnswerKind, Catalog, Course, Curriculum, Exemplar, KnowledgePoint, RawCurriculum, RawUnit,
    Slug, Topic, Unit,
};
use cadus_core::event::{Event, SchemaVersion, SessionStart, Timestamp};
use cadus_core::pool::PoolAnswer;
use cadus_core::template::{GateSpec, gate_diagnosis_body};
use cadus_store::diagnosis::JobRow;
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::diagnosis::{DiagnosisHub, job_view, match_distractor};
use cadus_web::state::{Content, ServedProblem, TaskProgress, WebState};
use cadus_web::{AppState, create_app};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::types::Uuid;
use sqlx::types::chrono::{DateTime, Utc};
use tower::ServiceExt;

use super::*;

/// The Unix microsecond instant of 2026-01-01T00:00:00Z.
pub const BASE_US: i64 = 1_767_225_600_000_000;

/// The session id every seeded log opens.
pub const SESSION: &str = "s_2026-01-01a";

/// The task id of the `addition` lesson (`assign_ids`: `{session}-{type}-{topic}`).
pub const LESSON: &str = "s_2026-01-01a-lesson-addition";

/// The serving key of the first knowledge point of `addition`.
pub const KEY: &str = "addition/kp1";

/// The statement of the served problem.
pub const PROBLEM_TEXT: &str = "Compute 8 + 5.5.";

/// The authored answer of the served problem.
pub const EXPECTED_ANSWER: &str = "13.5";

/// The `problem_id` every seeded state hands the client.
pub const PROBLEM_ID: &str = "p0000000000000000000000000000001";

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

/// One knowledge point with its authored exemplars.
pub fn kp(id: &str, exemplars: Vec<Exemplar>) -> KnowledgePoint {
    KnowledgePoint {
        id: Slug::new(id).unwrap(),
        name: format!("The {id} point"),
        key_prerequisites: Vec::new(),
        exemplars,
        constraints: None,
    }
}

/// One exemplar.
pub fn exemplar(problem: &str, answer: &str) -> Exemplar {
    Exemplar {
        problem: problem.to_string(),
        answer: answer.to_string(),
        solution_sketch: Some(format!("Add the parts to reach {answer}.")),
    }
}

/// One topic of the fixture curriculum.
pub fn topic(id: &str, points: Vec<KnowledgePoint>) -> Topic {
    Topic {
        id: Slug::new(id).unwrap(),
        name: format!("The {id} topic"),
        core: true,
        difficulty: 0.3,
        drill: false,
        answer_kind: AnswerKind::Numeric,
        expected_time_secs: 30,
        prerequisites: Vec::new(),
        encompassings_extra: Vec::new(),
        knowledge_points: points,
        diagnostic_exemplar: None,
        anki_seeds: Vec::new(),
    }
}

/// The fixture curriculum: one course, one module, two topics.
pub fn graph() -> Curriculum {
    let catalog = Catalog {
        courses: vec![Course {
            id: Slug::new("c1").unwrap(),
            name: "Foundations".to_string(),
            order: 0,
            mastery_floor: Vec::new(),
            mastery_floor_course: None,
        }],
    };
    Curriculum::build(RawCurriculum {
        catalog,
        units: vec![RawUnit {
            course_id: "c1".to_string(),
            file_name: "00-M1.yaml".to_string(),
            unit: Unit {
                unit: "M1".to_string(),
                course: Slug::new("c1").unwrap(),
                module: "M1".to_string(),
                topics: vec![
                    topic(
                        "addition",
                        vec![
                            kp("kp1", vec![exemplar(PROBLEM_TEXT, EXPECTED_ANSWER)]),
                            kp("kp2", vec![exemplar("Compute 40 + 2.5.", "42.5")]),
                        ],
                    ),
                    topic("subtraction", vec![kp("kp1", vec![])]),
                ],
            },
            first_load_index: 0,
        }],
    })
    .unwrap()
}

// --------------------------------------------------------------------------- //
// The harness
// --------------------------------------------------------------------------- //

/// The router of a test, sharing `hub` with the caller.
pub fn app_with(db: &TestDb, hub: &Arc<DiagnosisHub>) -> Router {
    create_app(
        AppState::new(Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS))
            .with_content(Arc::new(Content::new(graph())))
            .with_diagnosis(Arc::clone(hub)),
    )
}

/// The router of a test that needs no push.
pub fn app(db: &TestDb) -> Router {
    app_with(db, &Arc::new(DiagnosisHub::new()))
}

/// One request against the router. `tenant` is the bound learner.
pub async fn call(
    app: &Router,
    method: Method,
    uri: &str,
    tenant: Option<Uuid>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    let payload = match body {
        Some(value) => {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            Body::from(value.to_string())
        }
        None => Body::empty(),
    };
    let mut request = builder.body(payload).unwrap();
    if let Some(user) = tenant {
        super::present_session(request.headers_mut(), user);
    }
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let body = serde_json::from_str(&text).unwrap_or(Value::Null);
    (status, body)
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

/// Open `SESSION` in the log of `user`.
pub async fn seed_open_session(db: &TestDb, user: Uuid) {
    let event = Event::SessionStart(SessionStart {
        ts: Timestamp::from_micros(BASE_US),
        session: Some(SESSION.to_string()),
        v: SchemaVersion,
    });
    let ts = DateTime::<Utc>::from_timestamp_micros(BASE_US).unwrap();
    sqlx::query!(
        r#"
        INSERT INTO events (user_id, seq, ts, type, session_id, v, payload)
        VALUES ($1, 1, $2, $3, $4, 1, $5)
        "#,
        user,
        ts,
        event.type_name(),
        event.session(),
        serde_json::to_value(&event).unwrap()
    )
    .execute(&db.admin)
    .await
    .unwrap();
}

/// The problem that is live on the learner's screen.
pub fn live_problem() -> ServedProblem {
    let now = Utc::now().timestamp_micros() as f64 / 1_000_000.0;
    ServedProblem {
        problem_id: PROBLEM_ID.to_string(),
        task_id: LESSON.to_string(),
        topic: Some("addition".to_string()),
        serve_topic: Some("addition".to_string()),
        kp: Some("kp1".to_string()),
        answer_kind: Some("numeric".to_string()),
        text: PROBLEM_TEXT.to_string(),
        expected: PoolAnswer {
            v: 1,
            answer: EXPECTED_ANSWER.to_string(),
        },
        solution_sketch: Some("Add the parts to reach 13.5.".to_string()),
        started_at: now - 12.0,
        hints_given: Vec::new(),
        index: 0,
        rework: None,
    }
}

/// A D-S6 document holding one live lesson problem.
pub fn state_with_live_problem() -> WebState {
    let mut scratch = WebState::for_session(SESSION);
    scratch.tasks.insert(
        LESSON.to_string(),
        TaskProgress {
            task_id: LESSON.to_string(),
            task_type: "lesson".to_string(),
            total: 0,
            served: 1,
            answered: 0,
            done: false,
            current_kp: Some("kp1".to_string()),
        },
    );
    scratch.served.insert(LESSON.to_string(), live_problem());
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
        INSERT INTO content_store (digest, kp_id, kind, body, status, approved_at)
        VALUES ($1, $2, 'diagnosis', $3, 'approved', now())
        "#,
        digest,
        KEY,
        body,
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
    let payload = json!({
        "type": "attempt",
        "ts": "2026-01-01T00:00:10Z",
        "session": SESSION,
        "v": 1,
        "attempt_id": attempt_id,
        "task_id": LESSON,
        "topic": "addition",
        "kp": "kp1",
        "task_type": "lesson",
        "problem": {"text": PROBLEM_TEXT, "expected": EXPECTED_ANSWER},
        "given_answer": UNKNOWN_MISS,
        "correct": false,
        "secs": 20,
        "error_tags": [],
        "work_quality": "nearly_passable",
    });
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
