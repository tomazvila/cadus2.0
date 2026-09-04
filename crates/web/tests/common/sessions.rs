//! The fixtures of `tests/session_routes.rs` and its parts.

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

use super::*;

/// The Unix microsecond instant of 2026-01-01T00:00:00Z.
pub const BASE_US: i64 = 1_767_225_600_000_000;

/// The session id every seeded log opens.
pub const SESSION: &str = "s_2026-01-01a";

/// One topic of the fixture curriculum.
pub fn topic(id: &str, prereq: Option<&str>) -> Topic {
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
pub fn graph() -> Curriculum {
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
pub fn app(db: &TestDb) -> Router {
    create_app(
        AppState::new(Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS))
            .with_content(Arc::new(Content::new(graph()))),
    )
}

/// The router of a test with NO curriculum loaded.
pub fn app_without_content(db: &TestDb) -> Router {
    create_app(AppState::new(Db::new(
        db.app.clone(),
        DEFAULT_CLIENT_TIMEOUT_MS,
    )))
}

/// One request against the router. `tenant` is the bound learner, or `None` for
/// an unauthenticated call.
pub async fn call(
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
        super::present_session(request.headers_mut(), user);
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
pub fn parse(body: &str) -> Value {
    serde_json::from_str(body).unwrap_or_else(|err| panic!("body is not JSON: {err}\n{body}"))
}

/// Insert one event as the superuser, at line `seq`.
pub async fn seed_event(db: &TestDb, user: Uuid, seq: i64, event: &Event) {
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
pub async fn seed_open_session(db: &TestDb, user: Uuid) {
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
pub async fn seed_due_review(db: &TestDb, user: Uuid, through_seq: i64) {
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
