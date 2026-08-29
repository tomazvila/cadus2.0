//! FIX-M5-B acceptance: the authored solution and the serve topic of one serve.
//!
//! Requirements: A4, A6, D-M5-3, D-O3, D-S6, L5, C6. Spec
//! `docs/reference/web-service-1.0-spec.md` sections 2.1, 5.5 and 6.2;
//! `docs/reviews/M5-review-1.md`, unit FIX-M5-B.
//!
//! Two defects of the M5 review land here:
//!
//! 1. F2 and F11 — `install_next` set `solution_sketch: None` for every serve,
//!    so no graded reply carried the worked solution the stock re-solve text
//!    tells the learner to study:
//!    [`a_graded_miss_carries_the_authored_exemplar_solution`] and
//!    [`a_graded_miss_carries_the_rendered_template_solution`];
//! 2. F10 and F16 — a review that micro-interleaves a component skill drew the
//!    statement from the component and then read the hint ladder and the
//!    pre-authored diagnosis of the PARENT topic:
//!    [`a_component_review_question_reads_the_components_hint_ladder`] and
//!    [`a_component_review_question_reads_the_components_diagnosis`].
//!
//! Every expected value is a LITERAL: a literal status code, a literal statement,
//! a literal hint, a literal solution.
//!
//! Unit U2 writes the credential reader, so this file puts the `Tenant` into the
//! request extensions the way the auth layer will.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use cadus_core::curriculum::{
    AnswerKind, Catalog, Course, Curriculum, Exemplar, KnowledgePoint, PrereqEdge, RawCurriculum,
    RawUnit, Slug, Topic, Unit,
};
use cadus_core::event::{Event, SchemaVersion, SessionStart, Timestamp, TopicStatus};
use cadus_core::learner::{LearnerModel, TopicState};
use cadus_core::pool::{PoolAnswer, PoolProblem};
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::diagnosis::DiagnosisHub;
use cadus_web::state::{Content, TaskProgress, Tenant, WebState};
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

/// The task id of the `counting` lesson (`assign_ids`: `{session}-{type}-{topic}`).
const LESSON: &str = "s_2026-01-01a-lesson-counting";

/// The task id of the `addition` review.
const REVIEW: &str = "s_2026-01-01a-review-addition";

/// The serving key of the component skill.
const COMPONENT_KEY: &str = "counting/kp1";

/// The serving key of the parent topic of the review.
const PARENT_KEY: &str = "addition/kp1";

/// The statement of the authored exemplar of `counting/kp1`.
const COMPONENT_TEXT: &str = "Count on from 3 by 4.";

/// The answer of that exemplar.
const COMPONENT_ANSWER: &str = "7";

/// The authored solution of that exemplar.
const COMPONENT_SOLUTION: &str = "Count 4, 5, 6, 7.";

/// The statement of the authored exemplar of `addition/kp1`.
const PARENT_TEXT: &str = "Compute 8 + 5.5.";

/// The answer of that exemplar.
const PARENT_ANSWER: &str = "13.5";

/// The authored solution of that exemplar.
const PARENT_SOLUTION: &str = "Add 8 and 5.5 to reach 13.5.";

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

/// One exemplar with its authored solution.
fn exemplar(problem: &str, answer: &str, solution: &str) -> Exemplar {
    Exemplar {
        problem: problem.to_string(),
        answer: answer.to_string(),
        solution_sketch: Some(solution.to_string()),
    }
}

/// One topic of the fixture curriculum.
fn topic(id: &str, prerequisites: Vec<PrereqEdge>, points: Vec<KnowledgePoint>) -> Topic {
    Topic {
        id: Slug::new(id).unwrap(),
        name: format!("The {id} topic"),
        core: true,
        difficulty: 0.3,
        drill: false,
        answer_kind: AnswerKind::Numeric,
        expected_time_secs: 30,
        prerequisites,
        encompassings_extra: Vec::new(),
        knowledge_points: points,
        diagnostic_exemplar: None,
        anki_seeds: Vec::new(),
    }
}

/// The fixture curriculum: one course, one module, two topics.
///
/// `counting` is a KEY prerequisite of `addition`, so `review_mix` of `addition`
/// is `["kp1", "component:counting"]` and serve index 1 of the review draws its
/// statement from `counting` while the attempt records against `addition`.
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
                        "counting",
                        Vec::new(),
                        vec![kp(
                            "kp1",
                            vec![exemplar(
                                COMPONENT_TEXT,
                                COMPONENT_ANSWER,
                                COMPONENT_SOLUTION,
                            )],
                        )],
                    ),
                    topic(
                        "addition",
                        vec![PrereqEdge {
                            id: Slug::new("counting").unwrap(),
                            weight: 1.0,
                            key: true,
                        }],
                        vec![kp(
                            "kp1",
                            vec![exemplar(PARENT_TEXT, PARENT_ANSWER, PARENT_SOLUTION)],
                        )],
                    ),
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
            .with_content(Arc::new(Content::new(graph())))
            .with_diagnosis(Arc::new(DiagnosisHub::new())),
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
        request.extensions_mut().insert(Tenant(user));
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

/// Serve one task and give back the parsed body.
async fn serve(app: &Router, user: Uuid, task_id: &str) -> Value {
    let (status, body) = call(
        app,
        Method::POST,
        &format!("/api/task/{task_id}/serve"),
        Some(user),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    parse(&body)
}

/// Answer one task and give back the parsed body.
async fn answer(app: &Router, user: Uuid, task_id: &str, body: Value) -> Value {
    let (status, raw) = call(
        app,
        Method::POST,
        &format!("/api/task/{task_id}/answer"),
        Some(user),
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{raw}");
    parse(&raw)
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

/// Make `addition` a due review, so the plan holds `REVIEW`.
async fn seed_due_review(db: &TestDb, user: Uuid) {
    let mut topics: BTreeMap<String, TopicState> = BTreeMap::new();
    topics.insert(
        "addition".to_string(),
        TopicState {
            status: TopicStatus::Learning,
            rep_num: 1.0,
            memory_base: 1.0,
            t0: Some(Timestamp::from_micros(BASE_US - 400 * 86_400_000_000)),
            interval_days: 1.0,
            ability: 0.6,
            ..TopicState::default()
        },
    );
    let model = LearnerModel {
        topics,
        ..LearnerModel::default()
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

/// Put one unclaimed template row into the pool of `(user, key)`.
async fn seed_template_row(
    db: &TestDb,
    user: Uuid,
    key: &str,
    digest: &str,
    bindings: BTreeMap<String, String>,
    text: &str,
    answer: &str,
) {
    let problem = PoolProblem {
        v: 1,
        text: text.to_string(),
        bindings,
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
        VALUES ($1, $2, 'template', $3, $4::text::jsonb, $5::text::jsonb, 'hash-template')
        "#,
        user,
        key,
        digest,
        problem.to_body().unwrap(),
        expected.to_body().unwrap(),
    )
    .execute(&db.admin)
    .await
    .unwrap();
}

/// Approve one authored document for `key`.
async fn seed_content(db: &TestDb, key: &str, kind: &str, digest: &str, body: Value) {
    sqlx::query!(
        r#"
        INSERT INTO content_store (digest, kp_id, kind, body, status, approved_at)
        VALUES ($1, $2, $3, $4, 'approved', now())
        "#,
        digest,
        key,
        kind,
        body
    )
    .execute(&db.admin)
    .await
    .unwrap();
}

/// Approve one authored document for `key` a day before the others.
///
/// `approved_template` takes the NEWEST approval, so this document is the one a
/// later approval supersedes.
async fn seed_older_content(db: &TestDb, key: &str, kind: &str, digest: &str, body: Value) {
    sqlx::query!(
        r#"
        INSERT INTO content_store (digest, kp_id, kind, body, status, approved_at)
        VALUES ($1, $2, $3, $4, 'approved', now() - interval '1 day')
        "#,
        digest,
        key,
        kind,
        body
    )
    .execute(&db.admin)
    .await
    .unwrap();
}

/// The body of the `counting/kp1` template, with the sketch its author wrote.
fn template_body(sketch: &str) -> Value {
    json!({
        "v": 1,
        "topic_id": "counting",
        "answer_kind": "numeric",
        "statement": "Count on from {a} by {b}.",
        "params": {
            "a": {"kind": "int", "low": 1, "high": 12},
            "b": {"kind": "int", "low": -9, "high": -1}
        },
        "answer_expr": "a + b",
        "solution_sketch": sketch
    })
}

/// The bindings of the seeded template row: `a = 8` and `b = -3`.
fn seeded_bindings() -> BTreeMap<String, String> {
    let mut bindings: BTreeMap<String, String> = BTreeMap::new();
    bindings.insert("a".to_string(), "8".to_string());
    bindings.insert("b".to_string(), "-3".to_string());
    bindings
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

/// A learner whose review stands at serve index 1: the COMPONENT question.
///
/// `review_mix("addition")` is `["kp1", "component:counting"]` and a review
/// indexes by served count, so the next serve of `REVIEW` draws from `counting`
/// and records against `addition`.
async fn learner_at_the_component_question(db: &TestDb, email: &str) -> Uuid {
    let user = db.seed_user(email).await;
    seed_open_session(db, user).await;
    seed_due_review(db, user).await;
    let mut scratch = WebState::for_session(SESSION);
    scratch.tasks.insert(
        REVIEW.to_string(),
        TaskProgress {
            task_id: REVIEW.to_string(),
            task_type: "review".to_string(),
            total: 4,
            served: 1,
            answered: 1,
            done: false,
            current_kp: None,
        },
    );
    put_state(db, user, &scratch).await;
    user
}

// --------------------------------------------------------------------------- //
// F2 and F11: the graded reply carries the authored solution
// --------------------------------------------------------------------------- //

/// The A6 exemplar path. An empty pool falls back to the authored exemplars, and
/// the row the fallback serves carries the exemplar's own `solution_sketch`, so
/// the graded miss reveals it beside the stock re-solve instruction (A4, A6,
/// D-M5-3, spec section 5.5).
#[tokio::test]
async fn a_graded_miss_carries_the_authored_exemplar_solution() {
    TestDb::with(|db| async move {
        let user = db.seed_user("sketch-exemplar@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;

        let served = serve(&app, user, LESSON).await;
        assert_eq!(served["text"], COMPONENT_TEXT);
        // Hard Rule 1: the serve itself reveals nothing.
        assert_eq!(served.get("solution"), None);

        let problem_id = served["problem_id"].as_str().unwrap().to_string();
        let graded = answer(
            &app,
            user,
            LESSON,
            json!({"problem_id": problem_id, "answer": "6"}),
        )
        .await;
        assert_eq!(graded["correct"], false);
        assert_eq!(
            graded["solution"], COMPONENT_SOLUTION,
            "the graded miss carried no authored solution: {graded}"
        );
    })
    .await;
}

/// The A1 template path. A pool row names its `content_store` document by
/// digest, and the document's `solution_sketch` is a statement over the same
/// parameters, so the row's own bindings render it. `b` binds `-3`, and the
/// renderer brackets a negative value exactly as it brackets one in a statement.
#[tokio::test]
async fn a_graded_miss_carries_the_rendered_template_solution() {
    TestDb::with(|db| async move {
        let user = db.seed_user("sketch-template@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;
        seed_content(
            &db,
            COMPONENT_KEY,
            "template",
            "digest-template",
            template_body("Start at {a} and step {b} to reach 5."),
        )
        .await;
        seed_template_row(
            &db,
            user,
            COMPONENT_KEY,
            "digest-template",
            seeded_bindings(),
            "Count on from 8 by -3.",
            "5",
        )
        .await;

        let served = serve(&app, user, LESSON).await;
        assert_eq!(served["text"], "Count on from 8 by -3.");
        let problem_id = served["problem_id"].as_str().unwrap().to_string();

        let graded = answer(
            &app,
            user,
            LESSON,
            json!({"problem_id": problem_id, "answer": "11"}),
        )
        .await;
        assert_eq!(graded["correct"], false);
        assert_eq!(
            graded["solution"], "Start at 8 and step (-3) to reach 5.",
            "the graded miss carried no rendered solution: {graded}"
        );
    })
    .await;
}

/// C6: the row was drawn from ONE digest. A document approved after the draw is
/// a different problem's solution, so the reply carries none at all.
#[tokio::test]
async fn a_row_that_a_later_approval_superseded_carries_no_solution() {
    TestDb::with(|db| async move {
        let user = db.seed_user("sketch-stale@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;
        seed_older_content(
            &db,
            COMPONENT_KEY,
            "template",
            "digest-superseded",
            template_body("The first author wrote this."),
        )
        .await;
        seed_content(
            &db,
            COMPONENT_KEY,
            "template",
            "digest-newer",
            template_body("A later author wrote this."),
        )
        .await;
        seed_template_row(
            &db,
            user,
            COMPONENT_KEY,
            "digest-superseded",
            seeded_bindings(),
            "Count on from 8 by -3.",
            "5",
        )
        .await;

        let served = serve(&app, user, LESSON).await;
        assert_eq!(served["text"], "Count on from 8 by -3.");
        let problem_id = served["problem_id"].as_str().unwrap().to_string();
        let graded = answer(
            &app,
            user,
            LESSON,
            json!({"problem_id": problem_id, "answer": "11"}),
        )
        .await;
        assert_eq!(graded["correct"], false);
        assert_eq!(
            graded.get("solution"),
            None,
            "a superseded digest revealed another document's solution: {graded}"
        );
    })
    .await;
}

// --------------------------------------------------------------------------- //
// F10 and F16: the serving key of a component review question
// --------------------------------------------------------------------------- //

/// The review draws serve index 1 from the COMPONENT skill, so the hint comes
/// from the component's authored ladder. The parent topic's ladder is approved
/// too, and it must not be read (L5, D-O3).
#[tokio::test]
async fn a_component_review_question_reads_the_components_hint_ladder() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_at_the_component_question(&db, "ladder-component@example.com").await;
        seed_content(
            &db,
            COMPONENT_KEY,
            "hint_ladder",
            "digest-component-hints",
            json!({"hints": ["Count the steps one at a time."]}),
        )
        .await;
        seed_content(
            &db,
            PARENT_KEY,
            "hint_ladder",
            "digest-parent-hints",
            json!({"hints": ["Line the decimal points up."]}),
        )
        .await;

        // The statement comes from the component, and the attempt still records
        // against the parent topic.
        let served = serve(&app, user, REVIEW).await;
        assert_eq!(served["text"], COMPONENT_TEXT);
        assert_eq!(served["index"], 2);
        let problem_id = served["problem_id"].as_str().unwrap().to_string();

        let (status, body) = call(
            &app,
            Method::POST,
            &format!("/api/task/{REVIEW}/hint"),
            Some(user),
            Some(json!({"problem_id": problem_id})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let hint = parse(&body);
        assert_eq!(
            hint["hint"], "Count the steps one at a time.",
            "the hint came from the wrong knowledge point: {body}"
        );
        assert_eq!(hint["hint_number"], 1);
    })
    .await;
}

/// The same key rule on the A4 pre-authored path. The distractor is authored on
/// the COMPONENT, so a matching wrong answer is `ready` inline and writes no job
/// row (spec section 6.2). Before the fix the lookup read the parent topic, found
/// no distractor, and enqueued a model call.
#[tokio::test]
async fn a_component_review_question_reads_the_components_diagnosis() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_at_the_component_question(&db, "diagnosis-component@example.com").await;
        seed_content(
            &db,
            COMPONENT_KEY,
            "diagnosis",
            "digest-component-diagnosis",
            json!({
                "v": 1,
                "distractors": [
                    {"answer": "8",
                     "error_tag": "arithmetic-slip",
                     "note": "You counted the starting number as one step."}
                ]
            }),
        )
        .await;

        let served = serve(&app, user, REVIEW).await;
        assert_eq!(served["text"], COMPONENT_TEXT);
        let problem_id = served["problem_id"].as_str().unwrap().to_string();

        let graded = answer(
            &app,
            user,
            REVIEW,
            json!({"problem_id": problem_id, "answer": "8"}),
        )
        .await;
        assert_eq!(graded["correct"], false);
        assert_eq!(
            graded["diagnosis"],
            json!({
                "status": "ready",
                "error_tags": ["arithmetic-slip"],
                "prose": "You counted the starting number as one step.",
            }),
            "the pre-authored lookup read the wrong knowledge point: {graded}"
        );
        let jobs = sqlx::query_scalar!(
            r#"SELECT count(*) AS "n!" FROM diagnosis_jobs WHERE user_id = $1"#,
            user
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(jobs, 0, "a pre-authored hit must write no job row");
    })
    .await;
}

/// The attempt of a component question records against the PARENT topic, and the
/// D-S6 row keeps both topics: the review's FIRe applies to `addition`, and the
/// component gets credit by encompassing propagation (`api.py:352-357`).
#[tokio::test]
async fn a_component_review_question_records_against_the_parent_topic() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_at_the_component_question(&db, "record-parent@example.com").await;

        let served = serve(&app, user, REVIEW).await;
        let problem_id = served["problem_id"].as_str().unwrap().to_string();
        answer(
            &app,
            user,
            REVIEW,
            json!({"problem_id": problem_id, "answer": COMPONENT_ANSWER}),
        )
        .await;

        let payload = sqlx::query_scalar!(
            r#"
            SELECT payload AS "payload!" FROM events
            WHERE user_id = $1 AND type = 'attempt' ORDER BY seq LIMIT 1
            "#,
            user
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(payload["topic"], "addition");
        assert_eq!(payload["kp"], "kp1");
        assert_eq!(payload["correct"], true);
    })
    .await;
}
