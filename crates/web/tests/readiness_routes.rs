//! The readiness rule of D-F5 on the HTTP surface: the plan, the blocked list,
//! and the serve route that degrades to practice-only for a knowledge point it
//! cannot teach.
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
    KEY, LESSON, SESSION, answer_task_ok, drill_app, exemplar, gated_app, kp, one_unit_curriculum,
    plan_body, put_state, seed_learner, seed_open_session, serve_ok, stored_state, teach_task,
    topic,
};
use serde_json::{Value, json};
use sqlx::types::Uuid;

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
async fn seed_teach_page(db: &TestDb, curriculum: &Curriculum, key: &str, digest: &str) {
    common::seed_content_for(
        db,
        curriculum,
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

/// The answer of a `Compute n + n.` statement, read from its text.
///
/// The `ready_curriculum` exemplars all add a number to itself, so the test can
/// close the lesson without reading the `expected` the payload never carries.
fn answer_of(text: &str) -> String {
    let inner = text.trim_start_matches("Compute ").trim_end_matches('.');
    let (left, _right) = inner.split_once(" + ").expect("an addition statement");
    let n: i64 = left.trim().parse().expect("the addend is a number");
    (n * 2).to_string()
}

/// With an empty `content_store` the plan serves no lesson, and the `blocked`
/// list names the topic and the missing teach page. Audit findings (h) and (j)
/// together; since note 103 b the short pool of a point with decidable
/// exemplars is no lesson blocker, so the teach page is the one named.
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
        assert_eq!(blockers(&blocked[0]), ["teachable"]);
    })
    .await;
}

/// An approved teach page opens the lesson of a point with two decidable
/// exemplars: the short practice pool and the missing held-out item are pool
/// rules that never stop the lesson (note 103 b).
#[tokio::test]
async fn a_teach_page_opens_the_lesson_of_a_short_pool() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "readiness-teach@example.com").await;
        let app = gated_app(&db);
        seed_open_session(&db, user).await;
        seed_teach_page(&db, &common::drill_curriculum(), KEY, "digest-teach").await;

        let plan = plan_body(&app, user).await;
        assert!(
            !plan["blocked"]
                .as_array()
                .unwrap()
                .iter()
                .any(|entry| entry["topic"] == "addition"),
            "{plan}"
        );
        assert!(
            plan["tasks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|task| task["topic"]["id"] == "addition" && task["task_type"] == "lesson"),
            "{plan}"
        );
    })
    .await;
}

/// A knowledge point that teaches, practices and assesses is planned and
/// served. When the lesson stands at its SECOND knowledge point — the one with
/// no approved teach page — serve falls back to practice-only instead of
/// `409 no_instruction`, and the task can still reach `done` (ISSUE-5).
///
/// The plan gate reads the knowledge point the lesson STARTS at, so a lesson
/// may begin at a taught point and later cross into an untaught one. The serve
/// no longer dead-ends there.
#[tokio::test]
async fn a_ready_lesson_serves_and_the_untaught_next_point_is_practice_only() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "readiness-ready@example.com").await;
        let app = ready_app(&db);
        seed_open_session(&db, user).await;
        seed_teach_page(&db, &ready_curriculum(), KEY, "digest-teach-1").await;

        let plan = plan_body(&app, user).await;
        assert_eq!(plan["blocked"].as_array().unwrap().len(), 0, "{plan}");
        assert_eq!(plan["tasks"][0]["task_id"], LESSON, "{plan}");
        let served = serve_ok(&app, user, LESSON).await;
        assert_eq!(served["kp"], "kp1", "{served}");

        // The lesson now stands at `kp2`, which no approved page teaches. The
        // teach route still reports the missing page...
        stand_lesson_at(&db, user, "kp2").await;
        let (status, body) = teach_task(&app, user, LESSON).await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(body["error"]["code"], "no_instruction", "{body}");

        // ...but serve degrades to a practice-only problem of `kp2`.
        let served = serve_ok(&app, user, LESSON).await;
        assert_eq!(served["kp"], "kp2", "{served}");

        // ...and the learner can finish the point and the task.
        let mut next = served;
        let mut passed = false;
        for _ in 0..4 {
            let reply = answer_task_ok(
                &app,
                user,
                LESSON,
                json!({
                    "problem_id": next["problem_id"],
                    "answer": answer_of(next["text"].as_str().unwrap()),
                }),
            )
            .await;
            if reply["task_status"] == "task_passed" {
                passed = true;
                break;
            }
            next = reply["next"].clone();
            assert_eq!(next["kp"], "kp2", "{reply}");
        }
        assert!(passed, "the lesson never reached task_passed");
        assert!(stored_state(&db, user).await.tasks[LESSON].done);
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
