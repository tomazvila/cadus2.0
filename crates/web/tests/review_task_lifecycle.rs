//! A served review remains addressable for the lifetime of its session.
#![allow(clippy::unwrap_used)]
mod common;

use axum::Router;
use axum::http::StatusCode;
use cadus_core::config::Config;
use cadus_core::curriculum::review_context_digest;
use cadus_core::event::{Timestamp, TopicStatus};
use cadus_core::learner::{LearnerModel, TopicState};
use cadus_core::selector::{SeededSampler, SessionContext, compose_session};
use cadus_store::test_support::TestDb;
use cadus_web::state::WebState;
use common::*;
use serde_json::{Value, json};
use sqlx::types::Uuid;

const CONFIRM: &str = "s_2026-01-01a-review-addition-confirm-kp2";

async fn learner(db: &TestDb, email: &str, confirmation: bool) -> Uuid {
    let user = seed_learner(db, email).await;
    seed_open_session(db, user).await;
    seed_due_review(db, user).await;
    put_state(db, user, &WebState::for_session(SESSION)).await;
    if confirmation {
        seed_event(
            db,
            user,
            2,
            BASE_US + 1,
            SESSION,
            json!({
                "type": "remediation_triggered", "v": 1,
                "ts": "2026-01-01T00:00:00.000001Z", "session": SESSION,
                "kind": "review_confirmation:kp2", "source_topic": "addition",
                "targets": ["addition"]
            }),
        )
        .await;
    }
    let curriculum = addition_curriculum(Vec::new());
    let digest = review_context_digest(&curriculum).unwrap();
    for kp in ["kp1", "kp2"] {
        for index in 0..8 {
            seed_pool_row(
                db,
                user,
                &format!("addition/{kp}"),
                &format!("Give {kp} value {index}."),
                &index.to_string(),
                &format!("lifecycle-{kp}-{index}"),
                (&digest, cadus_core::review_engine::DIGEST),
            )
            .await;
        }
    }
    user
}

async fn answer_live(app: &Router, db: &TestDb, user: Uuid, task: &str, correct: bool) -> Value {
    let live = stored_state(db, user).await.served[task].clone();
    let reply = answer_task(
        app,
        user,
        task,
        json!({
            "problem_id": live.problem_id,
            "answer": if correct { live.expected.answer } else { "99999".to_owned() }
        }),
    )
    .await;
    assert_eq!(reply.0, StatusCode::OK, "{}", reply.1);
    reply.1
}

async fn assert_selector_omits(db: &TestDb, user: Uuid, task: &str) {
    let raw: Value = sqlx::query_scalar("SELECT model FROM learner_models WHERE user_id = $1")
        .bind(user)
        .fetch_one(&db.admin)
        .await
        .unwrap();
    let model: LearnerModel = serde_json::from_value(raw).unwrap();
    let curriculum = addition_curriculum(Vec::new());
    let cfg = Config::default();
    let context = SessionContext::default()
        .with_session_id(SESSION)
        .with_pending_remediation(&model.pending_remediation);
    let mut sampler = SeededSampler::new(17);
    let plan = compose_session(
        &model.topics,
        &curriculum,
        &cfg,
        sqlx::types::chrono::Utc::now().timestamp_micros(),
        &mut sampler,
        &context,
    );
    assert!(
        plan.tasks.iter().all(|item| item.task_id != task),
        "{plan:?}"
    );
}

async fn make_review_not_due(db: &TestDb, user: Uuid) {
    let head = log_head(db, user).await;
    assert_eq!(fold_cursor(db, user).await, head);
    let fresh = TopicState {
        status: TopicStatus::Learning,
        rep_num: 1.0,
        memory_base: 1.0,
        t0: Some(Timestamp::from_micros(
            sqlx::types::chrono::Utc::now().timestamp_micros(),
        )),
        interval_days: 36_500.0,
        ability: 0.6,
        ..TopicState::default()
    };
    let changed = sqlx::query(
        "UPDATE learner_models SET model = jsonb_set(model, '{topics,addition}', $2) WHERE user_id = $1 AND through_seq = $3"
    ).bind(user).bind(json!(fresh)).bind(head).execute(&db.admin).await.unwrap();
    assert_eq!(changed.rows_affected(), 1);
    assert_selector_omits(db, user, REVIEW).await;
}

#[tokio::test]
async fn served_due_review_accepts_correct_answers_after_selector_removal() {
    TestDb::with(|db| async move {
        let app = lesson_app(&db);
        let user = learner(&db, "review-live@example.com", false).await;
        serve_ok(&app, user, REVIEW).await;
        make_review_not_due(&db, user).await;
        let stale = answer_task(
            &app,
            user,
            REVIEW,
            json!({"problem_id": "p-stale", "answer": "7"}),
        )
        .await;
        assert_refused(&stale, StatusCode::NOT_FOUND, "unknown_problem");
        for index in 0..4 {
            let answer = answer_live(&app, &db, user, REVIEW, true).await;
            assert_eq!(answer["correct"], true);
            if index == 3 {
                assert_eq!(answer["task_status"], "task_passed");
            }
        }
        let scratch = stored_state(&db, user).await;
        assert_eq!(scratch.tasks[REVIEW].answered, 4);
        assert!(scratch.tasks[REVIEW].done);
        let closes = events_of_type(&db, user, "review_result").await;
        assert_eq!(closes.len(), 1);
        assert_eq!(closes[0]["passed"], true);
        let before = log_head(&db, user).await;
        let closed = answer_task(
            &app,
            user,
            REVIEW,
            json!({"problem_id": "p-stale", "answer": "7"}),
        )
        .await;
        assert_refused(&closed, StatusCode::CONFLICT, "task_complete");
        assert_refused(
            &serve_task(&app, user, REVIEW).await,
            StatusCode::CONFLICT,
            "task_complete",
        );
        assert_eq!(log_head(&db, user).await, before);
    })
    .await;
}

#[tokio::test]
async fn wrong_confirmation_survives_replan_and_reload_until_fresh_practice_closes_it() {
    for legacy in [false, true] {
        TestDb::with(move |db| async move {
            let app = lesson_app(&db);
            let user = learner(&db, "review-confirm@example.com", true).await;
            serve_ok(&app, user, CONFIRM).await;
            let original = stored_state(&db, user).await.served[CONFIRM].clone();
            assert_eq!(original.kp.as_deref(), Some("kp2"));
            let wrong = answer_live(&app, &db, user, CONFIRM, false).await;
            assert_eq!(wrong["feedback_practice"], true);
            assert_selector_omits(&db, user, CONFIRM).await;
            assert!(events_of_type(&db, user, "review_result").await.is_empty());
            let fresh = stored_state(&db, user).await.served[CONFIRM].clone();
            assert_ne!(fresh.problem_id, original.problem_id);
            assert_ne!(fresh.text, original.text);
            assert_eq!(fresh.kp, original.kp);
            if legacy {
                let changed = sqlx::query(
                    "UPDATE web_states SET doc = doc - 'review_tasks' WHERE user_id = $1",
                )
                .bind(user)
                .execute(&db.admin)
                .await
                .unwrap();
                assert_eq!(changed.rows_affected(), 1);
                let raw: Value =
                    sqlx::query_scalar("SELECT doc FROM web_states WHERE user_id = $1")
                        .bind(user)
                        .fetch_one(&db.admin)
                        .await
                        .unwrap();
                assert!(raw.get("review_tasks").is_none());
            }
            serve_ok(&app, user, CONFIRM).await;
            assert_eq!(
                stored_state(&db, user).await.served[CONFIRM].problem_id,
                fresh.problem_id
            );
            let old = answer_task(
                &app,
                user,
                CONFIRM,
                json!({"problem_id": original.problem_id, "answer": original.expected.answer}),
            )
            .await;
            assert_refused(&old, StatusCode::NOT_FOUND, "unknown_problem");
            let done = answer_live(&app, &db, user, CONFIRM, true).await;
            assert_eq!(done["correct"], true);
            assert_eq!(done["task_status"], "task_failed");
            let state = stored_state(&db, user).await;
            assert_eq!(state.tasks[CONFIRM].answered, 1);
            assert_eq!(state.tasks[CONFIRM].total, 1);
            assert!(state.tasks[CONFIRM].done);
            let attempts = events_of_type(&db, user, "attempt").await;
            assert_eq!(attempts.len(), 2);
            assert_eq!(attempts[1]["feedback_practice"], true);
            let closes = events_of_type(&db, user, "review_result").await;
            assert_eq!(closes.len(), 1);
            assert_eq!(closes[0]["passed"], false);
            assert_refused(
                &serve_task(&app, user, CONFIRM).await,
                StatusCode::CONFLICT,
                "task_complete",
            );
        })
        .await;
    }
}

#[tokio::test]
async fn unknown_and_previous_session_review_ids_are_refused_without_attempts() {
    TestDb::with(|db| async move {
        let app = lesson_app(&db);
        let user = learner(&db, "review-session@example.com", false).await;
        serve_ok(&app, user, REVIEW).await;
        let live = stored_state(&db, user).await.served[REVIEW].clone();
        for task in [
            "s_2026-01-01a-review-nowhere",
            "s_2026-01-01a-review-addition-confirm-kp9",
        ] {
            let reply = answer_task(
                &app,
                user,
                task,
                json!({"problem_id": live.problem_id, "answer": live.expected.answer}),
            )
            .await;
            assert_refused(&reply, StatusCode::NOT_FOUND, "unknown_task");
            assert_refused(
                &serve_task(&app, user, task).await,
                StatusCode::NOT_FOUND,
                "unknown_task",
            );
        }
        let next = log_head(&db, user).await + 1;
        let stamp = sqlx::types::chrono::Utc::now().timestamp_micros() + 1;
        seed_event(
            &db,
            user,
            next,
            stamp,
            SESSION_2,
            json!({
                "type": "session_start", "v": 1, "session": SESSION_2,
                "ts": Timestamp::from_micros(stamp)
            }),
        )
        .await;
        let before = log_head(&db, user).await;
        let reply = answer_task(
            &app,
            user,
            REVIEW,
            json!({"problem_id": live.problem_id, "answer": live.expected.answer}),
        )
        .await;
        assert_refused(&reply, StatusCode::NOT_FOUND, "unknown_task");
        assert_refused(
            &serve_task(&app, user, REVIEW).await,
            StatusCode::NOT_FOUND,
            "unknown_task",
        );
        assert_eq!(log_head(&db, user).await, before);
        assert!(events_of_type(&db, user, "attempt").await.is_empty());
    })
    .await;
}

#[tokio::test]
async fn legacy_live_review_recovers_only_from_its_current_session_serve_event() {
    TestDb::with(|db| async move {
        let app = lesson_app(&db);
        let user = learner(&db, "review-legacy@example.com", false).await;
        serve_ok(&app, user, REVIEW).await;
        make_review_not_due(&db, user).await;
        let events = events_of_type(&db, user, "task_served").await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["task_id"], REVIEW);
        assert_eq!(events[0]["session"], SESSION);
        let changed =
            sqlx::query("UPDATE web_states SET doc = doc - 'review_tasks' WHERE user_id = $1")
                .bind(user)
                .execute(&db.admin)
                .await
                .unwrap();
        assert_eq!(changed.rows_affected(), 1);
        let reply = answer_live(&app, &db, user, REVIEW, true).await;
        assert_eq!(reply["correct"], true);
        assert_eq!(stored_state(&db, user).await.tasks[REVIEW].answered, 1);
        assert!(events_of_type(&db, user, "review_result").await.is_empty());
        assert_eq!(events_of_type(&db, user, "task_served").await.len(), 1);
    })
    .await;
}
