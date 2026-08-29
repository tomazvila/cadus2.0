//! M5 U7 acceptance: `POST /api/task/{id}/serve`, `/teach`, and `/hint`.
//!
//! Requirements: A6, D5, D-O1, D-O3, D-S6, L4, L5, R4, T1. Spec
//! `docs/reference/web-service-1.0-spec.md` sections 2, 4.1, 5.6 and row U7 of
//! section 11; `docs/reference/serving-1.0-spec.md` sections 6 and 7.2.
//!
//! The four acceptance checks of row U7 land here:
//!
//! 1. a re-serve returns the same `problem_id` and a fresh `started_at` —
//!    [`a_re_serve_returns_the_same_problem_id_and_a_fresh_started_at`];
//! 2. a stale id 404s on both answer and hint —
//!    [`a_stale_problem_id_is_404_unknown_problem_on_hint_and_on_answer`];
//! 3. a hint body never contains `expected` —
//!    [`a_hint_never_carries_the_expected_answer`];
//! 4. teach on a review is `409 no_instruction` —
//!    [`teach_on_a_review_is_409_no_instruction`].
//!
//! Every expected value is a LITERAL: a literal status code, a literal error
//! code, a literal statement, a literal count.
//!
//! Every call presents a real session cookie, and `auth::layer::tenant_layer`
//! binds the tenant from it (FIX-M5-C). No test here writes a request extension
//! by hand.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use cadus_core::curriculum::{
    AnswerKind, Catalog, Course, Curriculum, Exemplar, KnowledgePoint, RawCurriculum, RawUnit,
    Slug, Topic, Unit,
};
use cadus_core::event::{Event, SchemaVersion, SessionStart, Timestamp};
use cadus_core::pool::{PoolAnswer, PoolProblem};
use cadus_store::pool::operator_flags;
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::state::{Content, ServedProblem, TaskProgress, WebState};
use cadus_web::{AppState, create_app};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::types::Uuid;
use sqlx::types::chrono::{DateTime, Utc};
use tower::ServiceExt;

/// The Unix microsecond instant of 2026-01-01T00:00:00Z.
const BASE_US: i64 = 1_767_225_600_000_000;

/// The session id every seeded log opens.
const SESSION: &str = "s_2026-01-01a";

/// The task id of the `addition` lesson (`assign_ids`: `{session}-{type}-{topic}`).
const LESSON: &str = "s_2026-01-01a-lesson-addition";

/// The task id of the `addition` review.
const REVIEW: &str = "s_2026-01-01a-review-addition";

/// The serving key of the first knowledge point of `addition`.
const KEY: &str = "addition/kp1";

/// The statement of the first authored exemplar of `addition/kp1`.
const EXEMPLAR_TEXT: &str = "Compute 8 + 5.5.";

/// The statement of the second authored exemplar of `addition/kp1`.
const EXEMPLAR_TEXT_2: &str = "Compute 9 + 4.25.";

/// The answer of the first exemplar. It must never appear in an HTTP body. Every
/// answer literal in this file carries a decimal point, which no `problem_id`
/// can hold, so a raw-body scan cannot match one by accident.
const EXEMPLAR_ANSWER: &str = "13.5";

/// The statement of the pool row the pop test seeds.
const POOL_TEXT: &str = "Compute 21 + 34.75.";

/// The answer of that pool row. It must never appear in an HTTP body.
const POOL_ANSWER: &str = "55.75";

// --------------------------------------------------------------------------- //
// The fixture curriculum
// --------------------------------------------------------------------------- //

/// One knowledge point with its authored exemplars.
fn kp(id: &str, exemplars: Vec<Exemplar>) -> KnowledgePoint {
    KnowledgePoint {
        id: Slug::new(id).unwrap(),
        name: format!("The {id} point"),
        key_prerequisites: Vec::new(),
        exemplars,
        constraints: None,
    }
}

/// One exemplar.
fn exemplar(problem: &str, answer: &str) -> Exemplar {
    Exemplar {
        problem: problem.to_string(),
        answer: answer.to_string(),
        solution_sketch: Some(format!("Add the parts to reach {answer}.")),
    }
}

/// One topic of the fixture curriculum.
fn topic(id: &str, points: Vec<KnowledgePoint>) -> Topic {
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

/// The fixture curriculum: one course, one module, two topics. `addition`
/// authors two knowledge points and two exemplars on the first one.
fn graph() -> Curriculum {
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
                            kp(
                                "kp1",
                                vec![
                                    exemplar(EXEMPLAR_TEXT, EXEMPLAR_ANSWER),
                                    exemplar(EXEMPLAR_TEXT_2, "13.25"),
                                ],
                            ),
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

/// The router of a test, with the fixture curriculum loaded.
fn app(db: &TestDb) -> Router {
    create_app(
        AppState::new(Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS))
            .with_content(Arc::new(Content::new(graph()))),
    )
}

/// One request against the router. `tenant` is the bound learner.
async fn call(
    app: &Router,
    method: Method,
    uri: &str,
    tenant: Option<Uuid>,
    body: Option<Value>,
) -> (StatusCode, String) {
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
        common::present_session(request.headers_mut(), user);
    }
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into())
}

/// The parsed JSON body of a call.
fn parse(body: &str) -> Value {
    serde_json::from_str(body).unwrap_or_else(|err| panic!("body is not JSON: {err}\n{body}"))
}

/// Open `SESSION` in the log of `user`.
async fn seed_open_session(db: &TestDb, user: Uuid) {
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

/// Put one unclaimed row into the pool of `(user, KEY)`.
async fn seed_pool_row(db: &TestDb, user: Uuid, text: &str, answer: &str, hash: &str) {
    let problem = PoolProblem {
        v: 1,
        text: text.to_string(),
        bindings: std::collections::BTreeMap::new(),
        seed: 7,
    };
    let expected = PoolAnswer {
        v: 1,
        answer: answer.to_string(),
    };
    sqlx::query!(
        r#"
        INSERT INTO serving_pool
            (user_id, kp_id, source, content_digest, problem, expected_answer, instance_hash)
        VALUES ($1, $2, 'template', NULL, $3::text::jsonb, $4::text::jsonb, $5)
        "#,
        user,
        KEY,
        problem.to_body().unwrap(),
        expected.to_body().unwrap(),
        hash,
    )
    .execute(&db.admin)
    .await
    .unwrap();
}

/// Approve one authored document for `KEY`.
async fn seed_content(db: &TestDb, kind: &str, digest: &str, body: Value) {
    sqlx::query!(
        r#"
        INSERT INTO content_store (digest, kp_id, kind, body, status, approved_at)
        VALUES ($1, $2, $3, $4, 'approved', now())
        "#,
        digest,
        KEY,
        kind,
        body
    )
    .execute(&db.admin)
    .await
    .unwrap();
}

/// The stored D-S6 document of `user`.
async fn stored_state(db: &TestDb, user: Uuid) -> WebState {
    let doc = sqlx::query_scalar!(
        r#"SELECT doc AS "doc!" FROM web_states WHERE user_id = $1"#,
        user
    )
    .fetch_one(&db.admin)
    .await
    .unwrap();
    WebState::from_doc(&doc).unwrap()
}

/// Write a D-S6 document for `user`.
async fn put_state(db: &TestDb, user: Uuid, scratch: &WebState) {
    sqlx::query!(
        r#"
        INSERT INTO web_states (user_id, doc) VALUES ($1, $2)
        ON CONFLICT (user_id) DO UPDATE SET doc = excluded.doc
        "#,
        user,
        scratch.to_doc().unwrap()
    )
    .execute(&db.admin)
    .await
    .unwrap();
}

/// Serve the lesson and give back the parsed body.
async fn serve_lesson(app: &Router, user: Uuid) -> Value {
    let (status, body) = call(
        app,
        Method::POST,
        &format!("/api/task/{LESSON}/serve"),
        Some(user),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    parse(&body)
}

// --------------------------------------------------------------------------- //
// The auth seam and the unknown task
// --------------------------------------------------------------------------- //

/// All three routes need a session. A request with no bound tenant is
/// `401 unauthorized` in the `{"error":{"code","message"}}` envelope.
#[tokio::test]
async fn every_u7_route_without_a_tenant_is_401_unauthorized() {
    TestDb::with(|db| async move {
        let app = app(&db);
        for suffix in ["serve", "teach", "hint"] {
            let uri = format!("/api/task/{LESSON}/{suffix}");
            let (status, body) = call(&app, Method::POST, &uri, None, Some(json!({}))).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{uri}: {body}");
            assert_eq!(parse(&body)["error"]["code"], "unauthorized", "{uri}");
        }
    })
    .await;
}

/// A task id that this session's plan does not hold is `404 unknown_task`, and a
/// route called with no session open is `409 no_open_session`.
#[tokio::test]
async fn an_unknown_task_is_404_and_a_closed_session_is_409() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "unknown@example.com").await;
        let app = app(&db);

        let (status, body) = call(
            &app,
            Method::POST,
            &format!("/api/task/{LESSON}/serve"),
            Some(user),
            Some(json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(parse(&body)["error"]["code"], "no_open_session");

        seed_open_session(&db, user).await;
        let (status, body) = call(
            &app,
            Method::POST,
            "/api/task/s_2026-01-01a-lesson-nowhere/serve",
            Some(user),
            Some(json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
        assert_eq!(parse(&body)["error"]["code"], "unknown_task");
    })
    .await;
}

// --------------------------------------------------------------------------- //
// Acceptance 1: the re-serve
// --------------------------------------------------------------------------- //

/// Section 5.6 and `_serve_live`. A second serve of a live problem hands back
/// the SAME `problem_id` and re-stamps `started_at`, because that is the moment
/// the problem goes on screen. It also serves no second problem: `served` is
/// keyed by task id, and `progress.served` stays 1.
#[tokio::test]
async fn a_re_serve_returns_the_same_problem_id_and_a_fresh_started_at() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "reserve@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;
        seed_pool_row(&db, user, POOL_TEXT, POOL_ANSWER, "hash-a").await;

        let first = serve_lesson(&app, user).await;
        assert_eq!(first["text"], POOL_TEXT);
        assert_eq!(first["index"], 1);
        assert_eq!(first["kp"], "kp1");
        assert_eq!(first["time_budget_secs"], 30);
        assert_eq!(first["countdown"], false);
        // A lesson names no problem count, so `total` is null, not 0
        // (`api.py:520`). The D-S6 row stores the absent count as 0, and the
        // payload must not read it back from there.
        assert_eq!(first["total"], Value::Null);
        let first_stamp = stored_state(&db, user).await.served[LESSON].started_at;

        let second = serve_lesson(&app, user).await;
        assert_eq!(
            second["problem_id"], first["problem_id"],
            "a re-serve dealt a different problem"
        );
        assert_eq!(second["text"], POOL_TEXT);
        assert_eq!(second["index"], 1);

        // The stamp MOVED. Section 5.6: the clock starts when the client asks
        // for the problem to put on screen, so a reload restarts it. Each serve
        // makes several database round trips, so the two instants are hundreds
        // of microseconds apart and the strict comparison is not a race.
        let after = stored_state(&db, user).await;
        assert!(
            after.served[LESSON].started_at > first_stamp,
            "started_at was not re-stamped: {} then {}",
            first_stamp,
            after.served[LESSON].started_at
        );
        assert_eq!(after.served.len(), 1, "served is keyed by task id");
        assert_eq!(after.tasks[LESSON].served, 1, "the re-serve counted twice");

        // The re-serve claimed no second pool row.
        let claimed = sqlx::query_scalar!(
            r#"SELECT count(*) AS "n!" FROM serving_pool
               WHERE user_id = $1 AND claimed_at IS NOT NULL"#,
            user
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(claimed, 1);
    })
    .await;
}

// --------------------------------------------------------------------------- //
// Hard Rule 1: the serve payload
// --------------------------------------------------------------------------- //

/// Trap W7: scan the RAW JSON. A serve carries neither `expected` nor the
/// solution sketch, and the answer text never appears in it.
#[tokio::test]
async fn a_serve_never_carries_the_expected_answer_or_the_sketch() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "secrecy@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;
        seed_pool_row(&db, user, POOL_TEXT, POOL_ANSWER, "hash-a").await;

        let (status, body) = call(
            &app,
            Method::POST,
            &format!("/api/task/{LESSON}/serve"),
            Some(user),
            Some(json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(
            !body.contains("expected"),
            "the serve leaked expected: {body}"
        );
        assert!(
            !body.contains("solution_sketch"),
            "the serve leaked the sketch: {body}"
        );
        assert!(
            !body.contains(POOL_ANSWER),
            "the serve leaked the answer text: {body}"
        );
        // The payload carries these seven fields and no eighth
        // (`_serve_payload`, `api.py:501-527`).
        let payload = parse(&body);
        let keys: Vec<&str> = payload
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            vec![
                "countdown",
                "index",
                "kp",
                "problem_id",
                "text",
                "time_budget_secs",
                "total",
            ]
        );

        // The document the client cannot read DOES hold it, so the grade path of
        // unit U8 finds it.
        let stored = stored_state(&db, user).await;
        assert_eq!(stored.served[LESSON].expected.answer, POOL_ANSWER);
    })
    .await;
}

// --------------------------------------------------------------------------- //
// Acceptance 2: the stale id
// --------------------------------------------------------------------------- //

/// A `problem_id` that is not the task's current one is `404 unknown_problem` on
/// the hint route, and the SAME rule refuses it on the answer route: both call
/// `WebState::validate`, which unit U8 wires into `POST /answer`.
#[tokio::test]
async fn a_stale_problem_id_is_404_unknown_problem_on_hint_and_on_answer() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "stale@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;
        seed_pool_row(&db, user, POOL_TEXT, POOL_ANSWER, "hash-a").await;
        seed_content(
            &db,
            "hint_ladder",
            "digest-hints",
            json!({"hints": ["Line the digits up.", "Add the ones first."]}),
        )
        .await;

        let served = serve_lesson(&app, user).await;
        let live = served["problem_id"].as_str().unwrap().to_string();

        let (status, body) = call(
            &app,
            Method::POST,
            &format!("/api/task/{LESSON}/hint"),
            Some(user),
            Some(json!({"problem_id": "0123456789abcdef0123456789abcdef"})),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
        assert_eq!(parse(&body)["error"]["code"], "unknown_problem");

        // The answer path takes the same refusal from the same function.
        let scratch = stored_state(&db, user).await;
        assert_eq!(
            scratch
                .validate(LESSON, "0123456789abcdef0123456789abcdef")
                .unwrap_err(),
            cadus_web::state::ValidateError::UnknownProblem
        );
        assert!(scratch.validate(LESSON, &live).is_ok());

        // A hint with no `problem_id` at all is `422 invalid_request`.
        let (status, body) = call(
            &app,
            Method::POST,
            &format!("/api/task/{LESSON}/hint"),
            Some(user),
            Some(json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert_eq!(parse(&body)["error"]["code"], "invalid_request");
    })
    .await;
}

/// A closed task refuses a serve with `409 task_complete`, and a hint against it
/// takes the same refusal (section 4.2).
#[tokio::test]
async fn a_closed_task_refuses_a_serve_and_a_hint_with_409_task_complete() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "closed@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;

        let mut scratch = WebState::for_session(SESSION);
        scratch.tasks.insert(
            LESSON.to_string(),
            TaskProgress {
                task_id: LESSON.to_string(),
                task_type: "lesson".to_string(),
                total: 3,
                served: 3,
                answered: 3,
                done: true,
                current_kp: Some("kp1".to_string()),
            },
        );
        scratch.served.insert(
            LESSON.to_string(),
            ServedProblem {
                problem_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
                task_id: LESSON.to_string(),
                topic: Some("addition".to_string()),
                serve_topic: Some("addition".to_string()),
                kp: Some("kp1".to_string()),
                answer_kind: Some("numeric".to_string()),
                text: POOL_TEXT.to_string(),
                expected: PoolAnswer {
                    v: 1,
                    answer: POOL_ANSWER.to_string(),
                },
                solution_sketch: None,
                started_at: 1.0,
                hints_given: Vec::new(),
                index: 2,
                rework: None,
            },
        );
        put_state(&db, user, &scratch).await;

        let (status, body) = call(
            &app,
            Method::POST,
            &format!("/api/task/{LESSON}/serve"),
            Some(user),
            Some(json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(parse(&body)["error"]["code"], "task_complete");

        let (status, body) = call(
            &app,
            Method::POST,
            &format!("/api/task/{LESSON}/hint"),
            Some(user),
            Some(json!({"problem_id": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"})),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(parse(&body)["error"]["code"], "task_complete");
    })
    .await;
}

// --------------------------------------------------------------------------- //
// Acceptance 3: the hint ladder
// --------------------------------------------------------------------------- //

/// The hint comes from the authored ladder, rung by rung, and its body carries
/// no `expected` and no answer text (trap W7). The third hint on a REVIEW adds
/// the reference lesson (`api.py:1856-1864`); a lesson never gets one.
#[tokio::test]
async fn a_hint_never_carries_the_expected_answer() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "hint@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;
        seed_pool_row(&db, user, POOL_TEXT, POOL_ANSWER, "hash-a").await;
        seed_content(
            &db,
            "hint_ladder",
            "digest-hints",
            json!({"hints": ["Line the digits up.", "Add the ones first."]}),
        )
        .await;

        let served = serve_lesson(&app, user).await;
        let problem_id = served["problem_id"].as_str().unwrap().to_string();
        let uri = format!("/api/task/{LESSON}/hint");

        let (status, body) = call(
            &app,
            Method::POST,
            &uri,
            Some(user),
            Some(json!({"problem_id": problem_id})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(
            !body.contains("expected"),
            "the hint leaked expected: {body}"
        );
        assert!(
            !body.contains(POOL_ANSWER),
            "the hint leaked the answer text: {body}"
        );
        let first = parse(&body);
        assert_eq!(first["hint"], "Line the digits up.");
        assert_eq!(first["hint_number"], 1);
        assert_eq!(first["reference_lesson"], Value::Null);

        let (status, body) = call(
            &app,
            Method::POST,
            &uri,
            Some(user),
            Some(json!({"problem_id": problem_id})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let second = parse(&body);
        assert_eq!(second["hint"], "Add the ones first.");
        assert_eq!(second["hint_number"], 2);

        // A ladder that runs out repeats its last rung, and no model is asked.
        let (status, body) = call(
            &app,
            Method::POST,
            &uri,
            Some(user),
            Some(json!({"problem_id": problem_id})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let third = parse(&body);
        assert_eq!(third["hint"], "Add the ones first.");
        assert_eq!(third["hint_number"], 3);
        // A LESSON never escalates; only a review and a multi-step part do.
        assert_eq!(third["reference_lesson"], Value::Null);

        // The three hints are recorded, so the H3 rule of unit U8 sees them.
        let stored = stored_state(&db, user).await;
        assert_eq!(stored.served[LESSON].hints_given.len(), 3);
    })
    .await;
}

/// The third hint on a review points the learner at the reference lesson, with
/// the topic id and the topic name (`api.py:1856-1864`).
#[tokio::test]
async fn the_third_hint_on_a_review_escalates_to_the_reference_lesson() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "escalate@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;
        seed_content(
            &db,
            "hint_ladder",
            "digest-hints",
            json!({"hints": ["One."]}),
        )
        .await;

        let mut scratch = WebState::for_session(SESSION);
        scratch.served.insert(
            REVIEW.to_string(),
            ServedProblem {
                problem_id: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
                task_id: REVIEW.to_string(),
                topic: Some("addition".to_string()),
                serve_topic: Some("addition".to_string()),
                kp: Some("kp1".to_string()),
                answer_kind: Some("numeric".to_string()),
                text: POOL_TEXT.to_string(),
                expected: PoolAnswer {
                    v: 1,
                    answer: POOL_ANSWER.to_string(),
                },
                solution_sketch: None,
                started_at: 1.0,
                // Two hints already taken: the next one is the third.
                hints_given: vec!["One.".to_string(), "One.".to_string()],
                index: 0,
                rework: None,
            },
        );
        put_state(&db, user, &scratch).await;
        seed_due_review(&db, user).await;

        let (status, body) = call(
            &app,
            Method::POST,
            &format!("/api/task/{REVIEW}/hint"),
            Some(user),
            Some(json!({"problem_id": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let third = parse(&body);
        assert_eq!(third["hint_number"], 3);
        assert_eq!(third["reference_lesson"]["topic"], "addition");
        assert_eq!(third["reference_lesson"]["name"], "The addition topic");
    })
    .await;
}

/// A hint inside a quiz is `409 no_hints_in_quiz`, and a knowledge point with no
/// approved ladder is `409 no_hint_ladder`: 2.0 asks no model for either (T1).
#[tokio::test]
async fn a_knowledge_point_with_no_approved_ladder_refuses_the_hint() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "noladder@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;
        seed_pool_row(&db, user, POOL_TEXT, POOL_ANSWER, "hash-a").await;

        let served = serve_lesson(&app, user).await;
        let problem_id = served["problem_id"].as_str().unwrap().to_string();

        let (status, body) = call(
            &app,
            Method::POST,
            &format!("/api/task/{LESSON}/hint"),
            Some(user),
            Some(json!({"problem_id": problem_id})),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(parse(&body)["error"]["code"], "no_hint_ladder");

        // A `pending` ladder is not an approved one (C6).
        sqlx::query!(
            r#"
            INSERT INTO content_store (digest, kp_id, kind, body, status)
            VALUES ('digest-pending', $1, 'hint_ladder', $2, 'pending')
            "#,
            KEY,
            json!({"hints": ["Never served."]})
        )
        .execute(&db.admin)
        .await
        .unwrap();

        let (status, body) = call(
            &app,
            Method::POST,
            &format!("/api/task/{LESSON}/hint"),
            Some(user),
            Some(json!({"problem_id": problem_id})),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(parse(&body)["error"]["code"], "no_hint_ladder");
        assert!(!body.contains("Never served."), "a pending rung was served");
    })
    .await;
}

// --------------------------------------------------------------------------- //
// Acceptance 4: teach
// --------------------------------------------------------------------------- //

/// Only a lesson teaches. A review is `409 no_instruction`, and so is a lesson
/// whose knowledge point has no APPROVED teach page: in both cases the server
/// has no worked example, and it never asks a model for one (L4, T1).
#[tokio::test]
async fn teach_on_a_review_is_409_no_instruction() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "teachreview@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;
        seed_due_review(&db, user).await;
        seed_content(
            &db,
            "teach",
            "digest-teach",
            json!({
                "concept": "Addition combines two counts.",
                "worked_example": {"problem": "Compute 2 + 3.", "steps": ["2 + 3 = 5."]}
            }),
        )
        .await;

        let (status, body) = call(
            &app,
            Method::POST,
            &format!("/api/task/{REVIEW}/teach"),
            Some(user),
            Some(json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(parse(&body)["error"]["code"], "no_instruction");
    })
    .await;
}

/// A lesson serves the authored page verbatim, and a lesson with no approved
/// page takes the same `409 no_instruction`.
#[tokio::test]
async fn teach_on_a_lesson_serves_the_authored_page() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "teach@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;
        let uri = format!("/api/task/{LESSON}/teach");

        let (status, body) = call(&app, Method::POST, &uri, Some(user), Some(json!({}))).await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(parse(&body)["error"]["code"], "no_instruction");

        seed_content(
            &db,
            "teach",
            "digest-teach",
            json!({
                "concept": "Addition combines two counts.",
                "worked_example": {"problem": "Compute 2 + 3.", "steps": ["2 + 3 = 5."]}
            }),
        )
        .await;

        let (status, body) = call(&app, Method::POST, &uri, Some(user), Some(json!({}))).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let page = parse(&body);
        assert_eq!(page["kp"], "kp1");
        assert_eq!(page["concept"], "Addition combines two counts.");
        assert_eq!(page["worked_example"]["problem"], "Compute 2 + 3.");
        assert_eq!(page["worked_example"]["steps"][0], "2 + 3 = 5.");

        // D-O3: the teach read writes nothing.
        let rows = sqlx::query_scalar!(
            r#"SELECT count(*) AS "n!" FROM web_states WHERE user_id = $1"#,
            user
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(rows, 0, "the teach read installed a state row");
    })
    .await;
}

// --------------------------------------------------------------------------- //
// A6: the pool miss
// --------------------------------------------------------------------------- //

/// `serving-1.0-spec.md` section 7.2. An empty pool must NOT generate. The serve
/// instantiates the knowledge point's authored exemplars in process, writes the
/// first pool rows of the pair with `source = 'exemplar'`, claims one, and the
/// A6 operator view then reports the fallback.
#[tokio::test]
async fn a_pool_miss_instantiates_an_exemplar_and_raises_the_a6_flag() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "fallback@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;

        let served = serve_lesson(&app, user).await;
        // `addition/kp1` authors two exemplars. One batch takes one
        // `created_at`, so the pop orders the two by their random ids: the rule
        // is that ONE of the authored statements is served, never that a
        // position wins (`insert_batch`, "the order inside one batch").
        let text = served["text"].as_str().unwrap();
        assert!(
            text == EXEMPLAR_TEXT || text == EXEMPLAR_TEXT_2,
            "the fallback served {text:?}, which is not an authored exemplar"
        );
        assert!(
            !serde_json::to_string(&served)
                .unwrap()
                .contains(EXEMPLAR_ANSWER),
            "the fallback leaked the exemplar answer"
        );

        let rows = sqlx::query!(
            r#"
            SELECT source AS "source!", content_digest,
                   (claimed_at IS NOT NULL) AS "claimed!"
            FROM serving_pool WHERE user_id = $1 AND kp_id = $2
            ORDER BY claimed_at NULLS LAST, id
            "#,
            user,
            KEY
        )
        .fetch_all(&db.admin)
        .await
        .unwrap();
        assert_eq!(rows.len(), 2, "the whole exemplar list did not go in");
        assert!(rows.iter().all(|row| row.source == "exemplar"));
        assert!(rows.iter().all(|row| row.content_digest.is_none()));
        assert_eq!(
            rows.iter().filter(|row| row.claimed).count(),
            1,
            "the serve claimed more than one row"
        );

        let flags = operator_flags(&db.admin).await.unwrap();
        let flag = flags
            .iter()
            .find(|flag| flag.kp_id == KEY)
            .unwrap_or_else(|| panic!("the A6 view lost {KEY}"));
        assert_eq!(flag.approved_templates, 0);
        assert!(flag.needs_template, "the A6 flag did not rise");
        assert!(
            flag.last_exemplar_at.is_some(),
            "the A6 view records no exemplar serve"
        );
        assert_eq!(flag.pool_depth, 1);

        // The D5 windows recorded the instance, so the next serve avoids it.
        let stored = stored_state(&db, user).await;
        assert_eq!(stored.ring("addition").len(), 1);
        assert_eq!(stored.memory(LESSON).len(), 1);
    })
    .await;
}

/// The D-S6 row of one serve names both topics and holds the authored solution.
///
/// `serve_topic` is the topic the STATEMENT came from and `topic` is the topic
/// the attempt records against. They are the same topic for a lesson, and the
/// hint ladder and the pre-authored diagnosis key on the first one (M5 review 1,
/// findings F10 and F16). `solution_sketch` is the exemplar's own worked
/// solution, which the grade reply of unit U8 reveals after the attempt commits
/// (findings F2 and F11).
///
/// Hard Rule 1 still holds: neither value leaves the D-S6 row on this route.
#[tokio::test]
async fn a_serve_stores_the_serve_topic_and_the_authored_solution() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "sketch@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;

        let served = serve_lesson(&app, user).await;
        let text = served["text"].as_str().unwrap().to_string();
        // One batch takes one `created_at`, so either authored exemplar may win
        // the pop. The solution is the one the WINNER authored.
        let solution = if text == EXEMPLAR_TEXT {
            "Add the parts to reach 13.5."
        } else {
            "Add the parts to reach 13.25."
        };

        let stored = stored_state(&db, user).await;
        let live = &stored.served[LESSON];
        assert_eq!(live.topic.as_deref(), Some("addition"));
        assert_eq!(live.serve_topic.as_deref(), Some("addition"));
        assert_eq!(live.kp.as_deref(), Some("kp1"));
        assert_eq!(
            live.solution_sketch.as_deref(),
            Some(solution),
            "the D-S6 row holds no authored solution"
        );

        // Trap W7: scan the RAW payload. The serve reveals neither.
        let raw = serde_json::to_string(&served).unwrap();
        assert!(
            !raw.contains("solution"),
            "the serve leaked a solution: {raw}"
        );
        assert!(
            !raw.contains("serve_topic"),
            "the serve leaked the serve topic: {raw}"
        );
    })
    .await;
}

/// The rotation never runs dry. A pair whose whole authored list is claimed
/// serves the least recently served exemplar again, and `last_exemplar_at` moves
/// with it, so the A6 view keeps showing a knowledge point that still needs a
/// template.
#[tokio::test]
async fn the_exemplar_rotation_serves_the_list_again_when_it_is_exhausted() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "rotate@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;

        let mut texts = Vec::new();
        for _ in 0..4 {
            let served = serve_lesson(&app, user).await;
            texts.push(served["text"].as_str().unwrap().to_string());
            // Close the live problem so the next call serves a new one.
            let mut scratch = stored_state(&db, user).await;
            scratch.served.remove(LESSON);
            put_state(&db, user, &scratch).await;
        }

        // The first two serves cover both authored statements: the ring blocks
        // the one already served.
        for text in &texts {
            assert!(
                text == EXEMPLAR_TEXT || text == EXEMPLAR_TEXT_2,
                "the rotation served {text:?}, which is not an authored exemplar"
            );
        }
        assert_ne!(texts[0], texts[1], "the second serve repeated the first");
        // Both statements are used now, so the rotation repeats the LEAST
        // recently served one and keeps alternating.
        assert_eq!(texts[2], texts[0]);
        assert_eq!(texts[3], texts[1]);

        // The pool still holds exactly the two authored rows: the rotation
        // writes no duplicate (the A5 unique index).
        let rows = sqlx::query_scalar!(
            r#"SELECT count(*) AS "n!" FROM serving_pool WHERE user_id = $1 AND kp_id = $2"#,
            user,
            KEY
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(rows, 2);
    })
    .await;
}

/// The pop wins over the fallback: a pair with an unclaimed pool row serves that
/// row and writes no exemplar row at all.
#[tokio::test]
async fn the_serve_pops_the_pool_before_it_falls_back() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "pop@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;
        seed_pool_row(&db, user, POOL_TEXT, POOL_ANSWER, "hash-a").await;

        let served = serve_lesson(&app, user).await;
        assert_eq!(served["text"], POOL_TEXT);

        let sources = sqlx::query_scalar!(
            r#"SELECT source AS "source!" FROM serving_pool WHERE user_id = $1"#,
            user
        )
        .fetch_all(&db.admin)
        .await
        .unwrap();
        assert_eq!(sources, vec!["template".to_string()]);
    })
    .await;
}

// --------------------------------------------------------------------------- //
// Seeding a due review
// --------------------------------------------------------------------------- //

/// Write a learner model whose `addition` topic is a review that came due.
async fn seed_due_review(db: &TestDb, user: Uuid) {
    let mut topics: std::collections::BTreeMap<String, cadus_core::learner::TopicState> =
        std::collections::BTreeMap::new();
    topics.insert(
        "addition".to_string(),
        cadus_core::learner::TopicState {
            status: cadus_core::event::TopicStatus::Learning,
            rep_num: 1.0,
            memory_base: 1.0,
            t0: Some(Timestamp::from_micros(BASE_US - 400 * 86_400_000_000)),
            interval_days: 1.0,
            ability: 0.6,
            ..cadus_core::learner::TopicState::default()
        },
    );
    let model = cadus_core::learner::LearnerModel {
        topics,
        ..cadus_core::learner::LearnerModel::default()
    };
    sqlx::query!(
        r#"
        INSERT INTO learner_models
            (user_id, model, through_seq, projector_version, config_hash)
        VALUES ($1, $2, 1, 3, $3)
        "#,
        user,
        serde_json::to_value(&model).unwrap(),
        cadus_core::config::Config::default().config_hash().unwrap()
    )
    .execute(&db.admin)
    .await
    .unwrap();
}
