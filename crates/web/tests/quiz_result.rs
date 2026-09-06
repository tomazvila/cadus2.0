//! End-to-end blind quiz close, explicit reveal, and score-independent practice.
#![allow(clippy::unwrap_used)]
mod common;
use axum::http::{Method, StatusCode};
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::{AppState, create_app};
use common::{
    PROBLEM_ID, PROBLEM_TWO, QUIZ, answer_task, call, events_of_type, first_question, parse,
    put_quiz_live, quiz_content, quiz_learner, second_question, serve_task, stored_state,
};
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn quiz_reveal_is_gated_and_fresh_practice_never_reprices_originals() {
    TestDb::with(|db| async move {
        let mut content = quiz_content();
        content = common::open_content_with(
            common::one_unit_curriculum(vec![
                common::topic(
                    "addition",
                    vec![common::kp("kp1", vec![common::exemplar("8 + 5.5", "13.5")])],
                ),
                common::topic(
                    "subtraction",
                    vec![common::kp(
                        "kp1",
                        vec![
                            common::exemplar(common::TEXT_TWO, "37.5"),
                            common::exemplar("What is 50 - 3?", "47"),
                        ],
                    )],
                ),
            ]),
            content.cfg.clone(),
        );
        let app = create_app(
            AppState::new(Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS))
                .with_content(Arc::new(content)),
        );
        let user = quiz_learner(&db, "quiz-result@example.com", first_question()).await;
        let path = format!("/api/task/{QUIZ}/quiz-result");
        let (status, early) = call(
            &app,
            Method::POST,
            &path,
            Some(user),
            Some(json!({"practice":true})),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{early}");
        assert!(!early.contains("solution_sketch"));
        let (_, first) = answer_task(
            &app,
            user,
            QUIZ,
            json!({"problem_id":PROBLEM_ID,"answer":"13.5"}),
        )
        .await;
        assert_eq!(first["quiz_complete"], false);
        assert!(events_of_type(&db, user, "quiz_result").await.is_empty());
        put_quiz_live(&db, user, second_question()).await;
        let (_, last) = answer_task(
            &app,
            user,
            QUIZ,
            json!({"problem_id":PROBLEM_TWO,"answer":"999"}),
        )
        .await;
        assert_eq!(last.as_object().unwrap().len(), 3);
        assert_eq!(last["quiz_complete"], true);
        let original = events_of_type(&db, user, "quiz_result").await;
        assert_eq!(original.len(), 1);
        assert_eq!(original[0]["score"], 0.5);
        let (status, raw) = call(&app, Method::POST, &path, Some(user), Some(json!({}))).await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        assert_eq!(parse(&raw)["answers"].as_array().unwrap().len(), 2);
        assert_eq!(parse(&raw)["practice_available"], true);
        assert!(stored_state(&db, user).await.tasks[QUIZ].done);
        let (status, raw) = start_practice(&app, user, &path).await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        assert_eq!(parse(&raw)["practice_pending"], true);
        let (status, fresh) = serve_task(&app, user, QUIZ).await;
        assert_eq!(status, StatusCode::OK, "{fresh}");
        assert_eq!(fresh["feedback_practice"], true);
        assert_ne!(fresh["text"], common::TEXT_TWO);
        let live = stored_state(&db, user).await.served[QUIZ].clone();
        let (status, done) = answer_task(
            &app,
            user,
            QUIZ,
            json!({"problem_id":live.problem_id,"answer":live.expected.answer}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{done}");
        assert_eq!(done["correct"], true);
        assert_eq!(events_of_type(&db, user, "quiz_result").await, original);
        let scratch = stored_state(&db, user).await;
        assert_eq!(scratch.tasks[QUIZ].answered, 2);
        assert!(scratch.tasks[QUIZ].done);
        let attempts = events_of_type(&db, user, "attempt").await;
        assert_eq!(attempts[2]["feedback_practice"], true);
        assert_eq!(attempts[2]["independent_after_feedback"], true);
        let (status, raw) = start_practice(&app, user, &path).await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        assert_eq!(parse(&raw)["practice_pending"], false);
    })
    .await;
}

#[tokio::test]
async fn unknown_quiz_evidence_has_a_pending_verdict_and_zero_xp() {
    TestDb::with(|db| async move {
        let app = common::quiz_app(&db);
        let mut question = first_question();
        question.answer_kind = Some("proof".to_owned());
        let user = quiz_learner(&db, "quiz-unknown@example.com", question).await;
        let (status, first) = answer_task(
            &app,
            user,
            QUIZ,
            json!({"problem_id":PROBLEM_ID,"answer":"a proof"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{first}");
        put_quiz_live(&db, user, second_question()).await;
        let (status, last) = answer_task(
            &app,
            user,
            QUIZ,
            json!({"problem_id":PROBLEM_TWO,"answer":"37.5"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{last}");
        let log = events_of_type(&db, user, "quiz_result").await;
        assert_eq!(log.len(), 1);
        assert_eq!(log[0]["inconclusive"], true);
        assert_eq!(log[0]["xp"], 0.0);
        assert_eq!(log[0]["per_topic"].as_array().unwrap().len(), 1);
        let path = format!("/api/task/{QUIZ}/quiz-result");
        let (status, raw) = call(&app, Method::POST, &path, Some(user), Some(json!({}))).await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        assert_eq!(parse(&raw)["inconclusive"], true);
        assert_eq!(parse(&raw)["answers"][0]["outcome"], "ungraded");
        assert_eq!(parse(&raw)["practice_available"], false);
    })
    .await;
}

async fn start_practice(
    app: &axum::Router,
    user: sqlx::types::Uuid,
    path: &str,
) -> (StatusCode, String) {
    call(
        app,
        Method::POST,
        path,
        Some(user),
        Some(json!({"practice":true})),
    )
    .await
}
