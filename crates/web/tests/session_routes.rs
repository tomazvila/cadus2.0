//! M5 U6 acceptance, the HTTP routes: `/api/status`, `/api/graph`,
//! `/api/enroll`, `/api/modules`, `/api/export`, and the session cycle.
//!
//! Requirements: C2, C3, D-S6, R4. Spec
//! `docs/reference/web-service-1.0-spec.md` section 2 (the route table) and
//! section 11 row U6.
//!
//! The four acceptance checks of row U6 land here:
//!
//! 1. the second concurrent tab — `tests/session_state.rs`;
//! 2. listing the plan writes nothing — [`listing_the_plan_writes_nothing`];
//! 3. `progress.done` is true for a recomposed failed review —
//!    [`progress_done_is_true_for_a_recomposed_failed_review`];
//! 4. the export round-trips through the event reader —
//!    [`the_export_round_trips_through_the_event_reader`].
//!
//! Every expected value is a LITERAL: a literal status code, a literal error
//! code, a literal session id, a literal node count.
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

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use cadus_core::config::Config;
use cadus_core::curriculum::{
    AnswerKind, Catalog, Course, Curriculum, PrereqEdge, RawCurriculum, RawUnit, Slug, Topic, Unit,
};
use cadus_core::event::{Event, SchemaVersion, SessionStart, Timestamp, TopicStatus};
use cadus_core::learner::{LearnerModel, TopicState};
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::state::{Content, TaskProgress, WebState};
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

/// One topic of the fixture curriculum.
fn topic(id: &str, prereq: Option<&str>) -> Topic {
    Topic {
        id: Slug::new(id).unwrap(),
        name: format!("The {id} topic"),
        core: true,
        difficulty: 0.3,
        drill: false,
        answer_kind: AnswerKind::Numeric,
        expected_time_secs: 30,
        prerequisites: prereq
            .map(|pid| {
                vec![PrereqEdge {
                    id: Slug::new(pid).unwrap(),
                    weight: 0.8,
                    key: true,
                }]
            })
            .unwrap_or_default(),
        encompassings_extra: Vec::new(),
        knowledge_points: Vec::new(),
        diagnostic_exemplar: None,
        anki_seeds: Vec::new(),
    }
}

/// The fixture curriculum: course `c1` (module `M1`, two topics) and course
/// `c2` (module `M2`, one topic that needs `addition`).
fn graph() -> Curriculum {
    let catalog = Catalog {
        courses: vec![
            Course {
                id: Slug::new("c1").unwrap(),
                name: "Foundations".to_string(),
                order: 0,
                mastery_floor: vec![Slug::new("addition").unwrap()],
                mastery_floor_course: None,
            },
            Course {
                id: Slug::new("c2").unwrap(),
                name: "Proofs".to_string(),
                order: 1,
                mastery_floor: Vec::new(),
                mastery_floor_course: None,
            },
        ],
    };
    Curriculum::build(RawCurriculum {
        catalog,
        units: vec![
            RawUnit {
                course_id: "c1".to_string(),
                file_name: "00-M1.yaml".to_string(),
                unit: Unit {
                    unit: "M1".to_string(),
                    course: Slug::new("c1").unwrap(),
                    module: "M1".to_string(),
                    topics: vec![topic("addition", None), topic("subtraction", None)],
                },
                first_load_index: 0,
            },
            RawUnit {
                course_id: "c2".to_string(),
                file_name: "01-M2.yaml".to_string(),
                unit: Unit {
                    unit: "M2".to_string(),
                    course: Slug::new("c2").unwrap(),
                    module: "M2".to_string(),
                    topics: vec![topic("fractions", Some("addition"))],
                },
                first_load_index: 2,
            },
        ],
    })
    .unwrap()
}

/// The router of a test, with the fixture curriculum loaded.
fn app(db: &TestDb) -> Router {
    create_app(
        AppState::new(Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS))
            .with_content(Arc::new(Content::new(graph()))),
    )
}

/// The router of a test with NO curriculum loaded.
fn app_without_content(db: &TestDb) -> Router {
    create_app(AppState::new(Db::new(
        db.app.clone(),
        DEFAULT_CLIENT_TIMEOUT_MS,
    )))
}

/// One request against the router. `tenant` is the bound learner, or `None` for
/// an unauthenticated call.
async fn call(
    app: &Router,
    method: Method,
    uri: &str,
    tenant: Option<Uuid>,
    body: Option<Value>,
) -> (StatusCode, String, String) {
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
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, content_type, String::from_utf8_lossy(&bytes).into())
}

/// The parsed JSON body of a call.
fn parse(body: &str) -> Value {
    serde_json::from_str(body).unwrap_or_else(|err| panic!("body is not JSON: {err}\n{body}"))
}

/// Insert one event as the superuser, at line `seq`.
async fn seed_event(db: &TestDb, user: Uuid, seq: i64, event: &Event) {
    let ts = DateTime::<Utc>::from_timestamp_micros(event.ts().micros()).unwrap();
    sqlx::query!(
        r#"
        INSERT INTO events (user_id, seq, ts, type, session_id, v, payload)
        VALUES ($1, $2, $3, $4, $5, 1, $6)
        "#,
        user,
        seq,
        ts,
        event.type_name(),
        event.session(),
        serde_json::to_value(event).unwrap()
    )
    .execute(&db.admin)
    .await
    .unwrap();
}

/// Open `SESSION` in the log of `user`.
async fn seed_open_session(db: &TestDb, user: Uuid) {
    seed_event(
        db,
        user,
        1,
        &Event::SessionStart(SessionStart {
            ts: Timestamp::from_micros(BASE_US),
            session: Some(SESSION.to_string()),
            v: SchemaVersion,
        }),
    )
    .await;
}

/// Write a learner model whose `addition` topic is a REVIEW that came due:
/// `Learning`, decayed far past its interval, so `memory_at` is far below the
/// 0.5 due threshold (`fire.rs`, `review_state`).
async fn seed_due_review(db: &TestDb, user: Uuid, through_seq: i64) {
    let mut topics: BTreeMap<String, TopicState> = BTreeMap::new();
    topics.insert(
        "addition".to_string(),
        TopicState {
            status: TopicStatus::Learning,
            rep_num: 1.0,
            memory_base: 1.0,
            // 400 days before the request instant, with a 1-day interval.
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
        VALUES ($1, $2, $3, 3, $4)
        "#,
        user,
        serde_json::to_value(&model).unwrap(),
        through_seq,
        Config::default().config_hash().unwrap()
    )
    .execute(&db.admin)
    .await
    .unwrap();
}

// --------------------------------------------------------------------------- //
// Auth seam and the curriculum guard
// --------------------------------------------------------------------------- //

/// Every U6 route needs a session. A request with no bound tenant is
/// `401 unauthorized` in the `{"error":{"code","message"}}` envelope.
#[tokio::test]
async fn every_u6_route_without_a_tenant_is_401_unauthorized() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let routes = [
            (Method::GET, "/api/status"),
            (Method::GET, "/api/graph"),
            (Method::GET, "/api/modules"),
            (Method::GET, "/api/export"),
            (Method::POST, "/api/enroll"),
            (Method::POST, "/api/session/start"),
            (Method::POST, "/api/session/end"),
            (Method::GET, "/api/session/plan"),
        ];
        for (method, uri) in routes {
            let (status, _, body) = call(&app, method.clone(), uri, None, None).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri}: {body}");
            let value = parse(&body);
            assert_eq!(value["error"]["code"], "unauthorized", "{method} {uri}");
            assert!(
                value["error"]["message"].is_string(),
                "{method} {uri} carries no message"
            );
        }
    })
    .await;
}

/// A process with no curriculum serves `503 curriculum_unavailable`, never an
/// empty graph. The binary loads the tree at boot and exits 2 when it does not.
#[tokio::test]
async fn a_route_that_needs_the_curriculum_is_503_without_one() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "nocontent@example.com").await;
        let app = app_without_content(&db);
        let (status, _, body) = call(&app, Method::GET, "/api/status", Some(user), None).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(parse(&body)["error"]["code"], "curriculum_unavailable");
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The session cycle
// --------------------------------------------------------------------------- //

/// `POST /api/session/start` is idempotent: the second call resumes the SAME
/// session and appends nothing. `POST /api/session/end` then closes it, and a
/// second end is `409 no_open_session`.
#[tokio::test]
async fn the_session_cycle_opens_once_resumes_and_refuses_a_second_end() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "cycle@example.com").await;
        let app = app(&db);

        let (status, _, body) =
            call(&app, Method::POST, "/api/session/start", Some(user), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let first = parse(&body);
        assert_eq!(first["reopened"], false);
        let session = first["session"].as_str().unwrap().to_string();
        assert!(
            session.starts_with("s_") && session.ends_with('a'),
            "the first session of a day is `s_<date>a`, got {session}"
        );
        assert_eq!(first["frontier"], 2);
        assert_eq!(first["due_reviews"], 0);

        // A second start resumes: the same id, `reopened: true`.
        let (status, _, body) =
            call(&app, Method::POST, "/api/session/start", Some(user), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let second = parse(&body);
        assert_eq!(second["reopened"], true);
        assert_eq!(second["session"].as_str().unwrap(), session);

        // Exactly ONE session_start reached the append-only log.
        let starts: i64 = sqlx::query_scalar!(
            r#"SELECT count(*) AS "n!" FROM events WHERE user_id = $1 AND type = 'session_start'"#,
            user
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(starts, 1);

        // The end takes the minutes the client sends.
        let (status, _, body) = call(
            &app,
            Method::POST,
            "/api/session/end",
            Some(user),
            Some(json!({"minutes": 12.5})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let closed = parse(&body);
        assert_eq!(closed["session"].as_str().unwrap(), session);
        assert_eq!(closed["minutes"], 12.5);
        assert_eq!(closed["xp_earned"], 0.0);
        assert_eq!(closed["anki"]["pending"], 0);

        // The scratch row went away with the session.
        let scratch: i64 = sqlx::query_scalar!(
            r#"SELECT count(*) AS "n!" FROM web_states WHERE user_id = $1"#,
            user
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(scratch, 0);

        // A second end has no session to close.
        let (status, _, body) =
            call(&app, Method::POST, "/api/session/end", Some(user), None).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(parse(&body)["error"]["code"], "no_open_session");

        // The plan needs an open session too.
        let (status, _, body) =
            call(&app, Method::GET, "/api/session/plan", Some(user), None).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(parse(&body)["error"]["code"], "no_open_session");
    })
    .await;
}

// --------------------------------------------------------------------------- //
// Acceptance 2: listing the plan writes nothing
// --------------------------------------------------------------------------- //

/// Trap W3. `GET /api/session/plan` appends no event, installs no
/// `TaskProgress`, and touches neither the `web_states` row nor the
/// `learner_models` row.
#[tokio::test]
async fn listing_the_plan_writes_nothing() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "plan@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;
        seed_due_review(&db, user, 1).await;

        let scratch = WebState::for_session(SESSION);
        sqlx::query!(
            "INSERT INTO web_states (user_id, doc) VALUES ($1, $2)",
            user,
            scratch.to_doc().unwrap()
        )
        .execute(&db.admin)
        .await
        .unwrap();

        let before = sqlx::query!(
            r#"
            SELECT (SELECT count(*) FROM events WHERE user_id = $1) AS "events!",
                   (SELECT doc FROM web_states WHERE user_id = $1) AS "doc!",
                   (SELECT updated_at FROM web_states WHERE user_id = $1) AS "touched!",
                   (SELECT built_at FROM learner_models WHERE user_id = $1) AS "built!",
                   (SELECT through_seq FROM learner_models WHERE user_id = $1) AS "cursor!"
            "#,
            user
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();

        let (status, _, body) =
            call(&app, Method::GET, "/api/session/plan", Some(user), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let plan = parse(&body);
        assert_eq!(plan["session"].as_str().unwrap(), SESSION);
        assert!(
            plan["tasks"]
                .as_array()
                .is_some_and(|tasks| !tasks.is_empty()),
            "the plan composed no task: {body}"
        );

        let after = sqlx::query!(
            r#"
            SELECT (SELECT count(*) FROM events WHERE user_id = $1) AS "events!",
                   (SELECT doc FROM web_states WHERE user_id = $1) AS "doc!",
                   (SELECT updated_at FROM web_states WHERE user_id = $1) AS "touched!",
                   (SELECT built_at FROM learner_models WHERE user_id = $1) AS "built!",
                   (SELECT through_seq FROM learner_models WHERE user_id = $1) AS "cursor!"
            "#,
            user
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();

        assert_eq!(before.events, 1);
        assert_eq!(after.events, 1, "the plan appended an event");
        assert_eq!(after.doc, before.doc, "the plan rewrote the D-S6 document");
        assert_eq!(
            after.touched, before.touched,
            "the plan touched the D-S6 row"
        );
        assert_eq!(
            after.built, before.built,
            "the plan rewrote the learner model"
        );
        assert_eq!(after.cursor, 1);
    })
    .await;
}

// --------------------------------------------------------------------------- //
// Acceptance 3: progress.done for a recomposed failed review
// --------------------------------------------------------------------------- //

/// Trap W3. A failed review stays DUE, so the composer builds it again under the
/// SAME task id while the state row still has it closed. The plan must report
/// `progress.done: true`, or a client that tracks completion itself asks to
/// serve a closed task and gets `409 task_complete` with nothing it can do.
#[tokio::test]
async fn progress_done_is_true_for_a_recomposed_failed_review() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "recompose@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;
        // Enroll in `c1`, so `fractions` stays out of scope. An in-scope lesson
        // that encompasses `addition` at weight 0.8 would compress the review
        // out of the plan before this test could look at it.
        seed_event(
            &db,
            user,
            2,
            &Event::Enrolled(cadus_core::event::Enrolled {
                ts: Timestamp::from_micros(BASE_US + 1),
                session: Some(SESSION.to_string()),
                v: SchemaVersion,
                course: cadus_core::event::Slug::new("c1").unwrap(),
                reason: None,
                return_to: None,
            }),
        )
        .await;
        seed_due_review(&db, user, 2).await;

        // The task id the composer assigns is `{session}-{type}-{topic}`
        // (`selector.rs`, `assign_ids`).
        let task_id = format!("{SESSION}-review-addition");
        let mut scratch = WebState::for_session(SESSION);
        scratch.tasks.insert(
            task_id.clone(),
            TaskProgress {
                task_id: task_id.clone(),
                task_type: "review".to_string(),
                total: 3,
                served: 3,
                answered: 3,
                done: true,
                current_kp: None,
            },
        );
        sqlx::query!(
            "INSERT INTO web_states (user_id, doc) VALUES ($1, $2)",
            user,
            scratch.to_doc().unwrap()
        )
        .execute(&db.admin)
        .await
        .unwrap();

        let (status, _, body) =
            call(&app, Method::GET, "/api/session/plan", Some(user), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let plan = parse(&body);

        let review = plan["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|task| task["task_id"] == json!(task_id.clone()))
            .unwrap_or_else(|| panic!("the failed review did not recompose: {body}"));

        assert_eq!(review["task_type"], "review");
        assert_eq!(review["topic"]["id"], "addition");
        assert_eq!(review["topic"]["module"], "M1");
        assert_eq!(review["progress"]["done"], true);
        assert_eq!(review["progress"]["answered"], 3);

        // A task the plan merely LISTED and the state row does not know reports
        // the untouched pair, and no row was installed for it.
        let fresh = plan["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|task| task["task_id"] != json!(task_id.clone()));
        if let Some(task) = fresh {
            assert_eq!(task["progress"]["done"], false);
            assert_eq!(task["progress"]["answered"], 0);
        }
        let stored: Value = sqlx::query_scalar!(
            r#"SELECT doc AS "doc!" FROM web_states WHERE user_id = $1"#,
            user
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(
            WebState::from_doc(&stored).unwrap().tasks.len(),
            1,
            "the plan installed a TaskProgress row"
        );
    })
    .await;
}

// --------------------------------------------------------------------------- //
// Acceptance 4: the export round-trips
// --------------------------------------------------------------------------- //

/// `GET /api/export` streams one canonical event per line, and every line reads
/// back through `Event::from_json` as the event that was stored.
#[tokio::test]
async fn the_export_round_trips_through_the_event_reader() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "export@example.com").await;
        let stranger = common::seed_learner(&db, "stranger@example.com").await;
        let app = app(&db);

        let written = vec![
            Event::SessionStart(SessionStart {
                ts: Timestamp::from_micros(BASE_US),
                session: Some(SESSION.to_string()),
                v: SchemaVersion,
            }),
            Event::Enrolled(cadus_core::event::Enrolled {
                ts: Timestamp::from_micros(BASE_US + 1),
                session: Some(SESSION.to_string()),
                v: SchemaVersion,
                course: cadus_core::event::Slug::new("c1").unwrap(),
                reason: None,
                return_to: None,
            }),
        ];
        for (index, event) in written.iter().enumerate() {
            seed_event(&db, user, index as i64 + 1, event).await;
        }
        // The other tenant's log must never appear in this export (C3).
        seed_event(
            &db,
            stranger,
            1,
            &Event::SessionStart(SessionStart {
                ts: Timestamp::from_micros(BASE_US),
                session: Some("s_2026-01-01z".to_string()),
                v: SchemaVersion,
            }),
        )
        .await;

        let (status, content_type, body) =
            call(&app, Method::GET, "/api/export", Some(user), None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(content_type, "application/x-ndjson");

        let lines: Vec<&str> = body.lines().collect();
        assert_eq!(lines.len(), 2, "the export carried {} lines", lines.len());
        for (line, original) in lines.iter().zip(&written) {
            let parsed = Event::from_json(line).unwrap();
            assert_eq!(&parsed, original, "the export line did not round-trip");
        }
        assert!(
            !body.contains("s_2026-01-01z"),
            "the export carried another tenant's session"
        );

        // Nothing was written by the export.
        let total: i64 = sqlx::query_scalar!(r#"SELECT count(*) AS "n!" FROM events"#)
            .fetch_one(&db.admin)
            .await
            .unwrap();
        assert_eq!(total, 3);
    })
    .await;
}

// --------------------------------------------------------------------------- //
// enroll, status, graph, modules
// --------------------------------------------------------------------------- //

/// `POST /api/enroll` refuses a missing course with `422 invalid_request` and an
/// unknown one with `404 unknown_course`. A good one appends `enrolled`, reports
/// the mastery floor, and clears the scratch.
#[tokio::test]
async fn enroll_refuses_a_missing_or_unknown_course_and_clears_the_scratch() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "enroll@example.com").await;
        let app = app(&db);

        let (status, _, body) = call(
            &app,
            Method::POST,
            "/api/enroll",
            Some(user),
            Some(json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(parse(&body)["error"]["code"], "invalid_request");

        let (status, _, body) = call(
            &app,
            Method::POST,
            "/api/enroll",
            Some(user),
            Some(json!({"course": "no-such-course"})),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(parse(&body)["error"]["code"], "unknown_course");

        // A stale scratch row stands before the switch.
        sqlx::query!(
            "INSERT INTO web_states (user_id, doc) VALUES ($1, $2)",
            user,
            WebState::for_session(SESSION).to_doc().unwrap()
        )
        .execute(&db.admin)
        .await
        .unwrap();

        let (status, _, body) = call(
            &app,
            Method::POST,
            "/api/enroll",
            Some(user),
            Some(json!({"course": "c1"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let enrolled = parse(&body);
        assert_eq!(enrolled["enrolled"], "c1");
        assert_eq!(enrolled["mastery_floor"], json!(["addition"]));
        assert_eq!(enrolled["floor_size"], 1);

        let events: i64 = sqlx::query_scalar!(
            r#"SELECT count(*) AS "n!" FROM events WHERE user_id = $1 AND type = 'enrolled'"#,
            user
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(events, 1);

        let scratch: i64 = sqlx::query_scalar!(
            r#"SELECT count(*) AS "n!" FROM web_states WHERE user_id = $1"#,
            user
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(scratch, 0, "the enroll left a stale served problem behind");
    })
    .await;
}

/// `GET /api/status` reports the enrolled course, the journey, and the counts.
#[tokio::test]
async fn status_reports_the_enrolled_course_and_the_counts() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "status@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;
        seed_due_review(&db, user, 1).await;
        seed_event(
            &db,
            user,
            2,
            &Event::Enrolled(cadus_core::event::Enrolled {
                ts: Timestamp::from_micros(BASE_US + 1),
                session: Some(SESSION.to_string()),
                v: SchemaVersion,
                course: cadus_core::event::Slug::new("c1").unwrap(),
                reason: None,
                return_to: None,
            }),
        )
        .await;

        let (status, _, body) = call(&app, Method::GET, "/api/status", Some(user), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let value = parse(&body);

        assert_eq!(value["course"]["id"], "c1");
        assert_eq!(value["course"]["name"], "Foundations");
        assert_eq!(
            value["courses"],
            json!([
                {"id": "c1", "name": "Foundations", "current": true},
                {"id": "c2", "name": "Proofs", "current": false},
            ])
        );
        assert_eq!(value["test_prep"], Value::Null);
        assert_eq!(value["placed"], true);
        assert_eq!(value["due_reviews"], 1);
        // `subtraction` is the only unmastered `c1` topic with no prerequisite.
        assert_eq!(value["frontier"], 1);
        assert_eq!(value["quiz_due"], false);
        assert_eq!(value["drill_due"], false);
        assert_eq!(value["xp"]["goal"], 40);
        assert_eq!(value["pending_remediation"], json!([]));
    })
    .await;
}

/// `GET /api/graph` filters by scope, `404`s an unknown course, and never
/// leaves the request tenant.
#[tokio::test]
async fn graph_filters_by_scope_and_404s_an_unknown_course() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "graph@example.com").await;
        let app = app(&db);

        let (status, _, body) =
            call(&app, Method::GET, "/api/graph?scope=all", Some(user), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let all = parse(&body);
        assert_eq!(all["scope"], "all");
        assert_eq!(all["counts"]["nodes"], 3);
        assert_eq!(all["counts"]["edges"], 1);
        assert_eq!(all["counts"]["mastered"], 0);
        assert_eq!(all["modules"], json!(["M1", "M2"]));
        assert_eq!(
            all["edges"],
            json!([{"from": "addition", "to": "fractions"}])
        );
        assert_eq!(all["nodes"][0]["id"], "addition");
        assert_eq!(all["nodes"][0]["name"], "The addition topic");
        assert_eq!(all["nodes"][0]["status"], "untouched");
        assert!(all["now"].is_string());

        let (status, _, body) =
            call(&app, Method::GET, "/api/graph?scope=c1", Some(user), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let one = parse(&body);
        assert_eq!(one["counts"]["nodes"], 2);
        // `fractions` is out of scope, so its prerequisite edge is out too.
        assert_eq!(one["counts"]["edges"], 0);
        assert_eq!(one["modules"], json!(["M1"]));

        let (status, _, body) = call(
            &app,
            Method::GET,
            "/api/graph?scope=no-such-course",
            Some(user),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(parse(&body)["error"]["code"], "unknown_course");
    })
    .await;
}

/// `GET /api/modules` lists the enrolled course's modules in curriculum order.
#[tokio::test]
async fn modules_lists_the_enrolled_course_modules() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "modules@example.com").await;
        let app = app(&db);

        // With no enrollment the whole curriculum is in scope.
        let (status, _, body) = call(&app, Method::GET, "/api/modules", Some(user), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let every = parse(&body);
        assert_eq!(every["course"]["id"], Value::Null);
        assert_eq!(every["modules"], json!(["M1", "M2"]));

        seed_event(
            &db,
            user,
            1,
            &Event::Enrolled(cadus_core::event::Enrolled {
                ts: Timestamp::from_micros(BASE_US),
                session: None,
                v: SchemaVersion,
                course: cadus_core::event::Slug::new("c2").unwrap(),
                reason: None,
                return_to: None,
            }),
        )
        .await;

        let (status, _, body) = call(&app, Method::GET, "/api/modules", Some(user), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let scoped = parse(&body);
        assert_eq!(scoped["course"]["id"], "c2");
        assert_eq!(scoped["course"]["name"], "Proofs");
        assert_eq!(scoped["modules"], json!(["M2"]));
    })
    .await;
}
