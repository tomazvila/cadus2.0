//! The fixtures of `tests/session_routes.rs` and its parts.

use std::collections::BTreeMap;

use axum::http::header;
use cadus_core::config::Config;
use cadus_core::curriculum::{
    AnswerKind, Catalog, Course, Curriculum, PrereqEdge, RawCurriculum, RawUnit, Slug, Topic, Unit,
};
use cadus_core::event::{Event, SchemaVersion, SessionStart, Timestamp, TopicStatus};
use cadus_core::learner::{LearnerModel, TopicState};
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::state::{Content, WebState};
use cadus_web::{AppState, create_app};
use sqlx::types::chrono::{DateTime, Utc};

pub use super::prelude::*;
use super::*;

/// One topic of the fixture curriculum, with no knowledge point and one key
/// prerequisite when `prereq` names it.
pub fn topic(id: &str, prereq: Option<&str>) -> Topic {
    let mut record = super::topic(id, Vec::new());
    record.prerequisites = prereq
        .map(|pid| {
            vec![PrereqEdge {
                id: Slug::new(pid).unwrap(),
                weight: 0.8,
                key: true,
            }]
        })
        .unwrap_or_default();
    record
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
            unit(
                "c1",
                "M1",
                vec![topic("addition", None), topic("subtraction", None)],
                0,
            ),
            unit("c2", "M2", vec![topic("fractions", Some("addition"))], 2),
        ],
    })
    .unwrap()
}

/// The router of a test, with the fixture curriculum loaded.
pub fn app(db: &TestDb) -> Router {
    app_with_content(db, graph())
}

/// The router of a test with NO curriculum loaded.
pub fn app_without_content(db: &TestDb) -> Router {
    create_app(AppState::new(Db::new(
        db.app.clone(),
        DEFAULT_CLIENT_TIMEOUT_MS,
    )))
}

/// One request against the router. `tenant` is the bound learner, or `None` for
/// an unauthenticated call. The answer carries the `Content-Type` too.
pub async fn call(
    app: &Router,
    method: Method,
    uri: &str,
    tenant: Option<Uuid>,
    body: Option<Value>,
) -> (StatusCode, String, String) {
    let response = respond(app, method, uri, tenant, body).await;
    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    (status, content_type, body_text(response).await)
}

/// Insert one event as the superuser, at line `seq`.
pub async fn seed_event(db: &TestDb, user: Uuid, seq: i64, event: &Event) {
    seed_typed_event(db, user, seq, event).await;
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

/// The `enrolled` event of course `c1`, one microsecond into the session.
pub fn enrolled_c1() -> Event {
    Event::Enrolled(cadus_core::event::Enrolled {
        ts: Timestamp::from_micros(BASE_US + 1),
        session: Some(SESSION.to_string()),
        v: SchemaVersion,
        course: cadus_core::event::Slug::new("c1").unwrap(),
        reason: None,
        return_to: None,
    })
}

/// `GET /api/session/plan` as `user`, and the `200` body.
pub async fn plan_of(app: &Router, user: Uuid) -> Value {
    let (status, _, body) = call(app, Method::GET, "/api/session/plan", Some(user), None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    parse(&body)
}

/// What one learner's three rows hold: the event count, the D-S6 document and
/// its `updated_at`, and the `built_at` and the cursor of the model.
#[derive(Debug, Clone, PartialEq)]
pub struct StorageSnapshot {
    pub events: i64,
    pub doc: Value,
    pub touched: DateTime<Utc>,
    pub built: DateTime<Utc>,
    pub cursor: i64,
}

/// Read the [`StorageSnapshot`] of `user`.
pub async fn storage_snapshot(db: &TestDb, user: Uuid) -> StorageSnapshot {
    // The text of the statement is byte for byte the one the query cache
    // holds, so the offline check finds it.
    let row = sqlx::query!(
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
    StorageSnapshot {
        events: row.events,
        doc: row.doc,
        touched: row.touched,
        built: row.built,
        cursor: row.cursor,
    }
}
