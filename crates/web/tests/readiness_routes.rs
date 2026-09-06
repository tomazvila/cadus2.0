//! The readiness rule of D-F5 on the HTTP surface: the plan, the blocked list,
//! and the serve route that refuses a lesson it cannot teach.
//!
//! Audit finding (j). Every other route test turns the rule off, because a test
//! database approves no document. This file turns it ON and is the test of the
//! rule.
//!
//! Every expected value is a literal: a literal status code, a literal error
//! code, a literal blocker name.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::Arc;

use axum::Router;
use axum::http::StatusCode;
use cadus_core::curriculum::Curriculum;
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::state::{Content, TaskProgress, WebState};
use cadus_web::{AppState, create_app};
use common::{
    KEY, LESSON, SESSION, drill_app, exemplar, gated_app, kp, one_unit_curriculum, parse, plan_body,
    put_state, seed_content, seed_learner, seed_open_session, serve_ok, serve_raw, topic,
};
use serde_json::{Value, json};
use sqlx::types::Uuid;

/// The serving key of the second knowledge point of `addition`.
const KEY_TWO: &str = "addition/kp2";

/// A curriculum whose `addition` topic authors FOUR decidable exemplars per
/// knowledge point: three stay in practice and the fourth is held out, so the
/// practice and assessment conditions hold and the teach page is the only one
/// the store decides.
fn ready_curriculum() -> Curriculum {
    one_unit_curriculum(vec![topic(
        "addition",
        vec![kp("kp1", four_exemplars(1)), kp("kp2", four_exemplars(10))],
    )])
}

/// Four exemplars `Compute n + n.` from `first`.
fn four_exemplars(first: i64) -> Vec<cadus_core::curriculum::Exemplar> {
    (first..first + 4)
        .map(|n| exemplar(&format!("Compute {n} + {n}."), &(n * 2).to_string()))
        .collect()
}

/// The router of `ready_curriculum` with the readiness rule ON.
fn ready_app(db: &TestDb) -> Router {
    create_app(
        AppState::new(Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS))
            .with_content(Arc::new(Content::new(ready_curriculum()))),
    )
}

/// Approve a teach page for `key`.
async fn seed_teach_page(db: &TestDb, key: &str, digest: &str) {
    seed_content(
        db,
        key,
        "teach",
        digest,
        json!({
            "concept": "Addition combines two counts.",
            "worked_example": {"problem": "Compute 2 + 3.", "steps": ["2 + 3 = 5."]}
        }),
    )
    .await;
}

/// Stand the lesson at `kp` in the D-S6 row, the way a passed knowledge point
/// leaves it mid-task.
async fn stand_lesson_at(db: &TestDb, user: Uuid, kp_id: &str) {
    let mut scratch = WebState {
        session: Some(SESSION.to_string()),
        ..WebState::default()
    };
    scratch.tasks.insert(
        LESSON.to_string(),
        TaskProgress {
            task_id: LESSON.to_string(),
            task_type: "lesson".to_string(),
            current_kp: Some(kp_id.to_string()),
            ..TaskProgress::default()
        },
    );
    put_state(db, user, &scratch).await;
}

/// The blocker names of one entry of the `blocked` list.
fn blockers(entry: &Value) -> Vec<&str> {
    entry["blockers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|name| name.as_str().unwrap())
        .collect()
}

/// With an empty `content_store` the plan serves no lesson, and the `blocked`
/// list names the topic and the three conditions the content does not meet.
/// Audit findings (h) and (j) together.
#[tokio::test]
async fn an_empty_content_store_blocks_every_lesson_and_the_plan_says_why() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "readiness-plan@example.com").await;
        let app = gated_app(&db);
        seed_open_session(&db, user).await;

        let plan = plan_body(&app, user).await;
        assert_eq!(plan["tasks"].as_array().unwrap().len(), 0, "{plan}");
        let blocked = plan["blocked"].as_array().unwrap();
        assert_eq!(blocked.len(), 3, "{plan}");
        assert_eq!(blocked[0]["topic"], "addition");
        assert_eq!(blocked[0]["task_type"], "lesson");
        assert_eq!(blocked[0]["kp"], "kp1");
        assert_eq!(
            blockers(&blocked[0]),
            ["teachable", "practicable", "assessable"]
        );
    })
    .await;
}

/// An approved teach page alone does not open the lesson: three practice items
/// and one held-out item are owed too, and that fixture authors two exemplars.
#[tokio::test]
async fn a_teach_page_alone_leaves_the_practice_and_assessment_blockers() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "readiness-teach@example.com").await;
        let app = gated_app(&db);
        seed_open_session(&db, user).await;
        seed_teach_page(&db, KEY, "digest-teach").await;

        let plan = plan_body(&app, user).await;
        let first = plan["blocked"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["topic"] == "addition")
            .unwrap()
            .clone();
        assert_eq!(blockers(&first), ["practicable", "assessable"]);
        assert_eq!(plan["tasks"].as_array().unwrap().len(), 0, "{plan}");
    })
    .await;
}

/// A knowledge point that teaches, practices and assesses is planned and
/// served, and the SECOND knowledge point of the same lesson — the one with no
/// approved teach page — takes `409 no_instruction` instead of practice.
///
/// It is the server half of audit finding (j): the plan gate reads the
/// knowledge point the lesson STARTS at, and the serve reads the one it stands
/// at, so the serve carries its own check.
#[tokio::test]
async fn a_ready_lesson_serves_and_the_untaught_next_point_is_409_no_instruction() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "readiness-ready@example.com").await;
        let app = ready_app(&db);
        seed_open_session(&db, user).await;
        seed_teach_page(&db, KEY, "digest-teach-1").await;

        let plan = plan_body(&app, user).await;
        assert_eq!(plan["blocked"].as_array().unwrap().len(), 0, "{plan}");
        assert_eq!(plan["tasks"][0]["task_id"], LESSON, "{plan}");
        let served = serve_ok(&app, user, LESSON).await;
        assert_eq!(served["kp"], "kp1", "{served}");

        // The lesson now stands at `kp2`, which no approved page teaches.
        stand_lesson_at(&db, user, "kp2").await;
        let (status, body) = serve_raw(&app, user, LESSON).await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(parse(&body)["error"]["code"], "no_instruction", "{body}");

        // Approve the second page and the same request serves.
        seed_teach_page(&db, KEY_TWO, "digest-teach-2").await;
        stand_lesson_at(&db, user, "kp2").await;
        let served = serve_ok(&app, user, LESSON).await;
        assert_eq!(served["kp"], "kp2", "{served}");
    })
    .await;
}

/// With the rule OFF the plan lists the lesson and the serve hands a problem
/// over, which is the behavior of every earlier unit.
#[tokio::test]
async fn the_rule_off_serves_the_lesson_as_before() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "readiness-off@example.com").await;
        let app = drill_app(&db);
        seed_open_session(&db, user).await;

        let plan = plan_body(&app, user).await;
        assert_eq!(plan["blocked"].as_array().unwrap().len(), 0, "{plan}");
        let served = serve_ok(&app, user, LESSON).await;
        assert_eq!(served["kp"], "kp1", "{served}");
    })
    .await;
}
