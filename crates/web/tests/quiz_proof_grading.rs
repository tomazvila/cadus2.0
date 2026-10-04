//! Amendment K point 6 inside a quiz: a written proof answered in a quiz is
//! graded in the background, but only once the quiz is revealed. While the
//! quiz is open nothing is enqueued and nothing about the grading is shown.

#![allow(clippy::unwrap_used)]

mod common;

use axum::http::{Method, StatusCode};
use cadus_store::test_support::TestDb;
use common::{
    PROBLEM_ID, PROBLEM_TWO, QUIZ, answer_task, call, first_question, parse, put_quiz_live,
    quiz_learner, second_question,
};
use serde_json::{Value, json};
use sqlx::types::Uuid;

/// The proof-grading rows of one learner, as (attempt id, payload).
async fn jobs(db: &TestDb, user: Uuid) -> Vec<(String, Value)> {
    sqlx::query_as::<_, (String, Value)>(
        "SELECT attempt_id, payload FROM proof_grading_jobs WHERE user_id = $1",
    )
    .bind(user)
    .fetch_all(&db.admin)
    .await
    .unwrap()
}

#[tokio::test]
async fn a_quiz_proof_is_enqueued_at_the_reveal_and_never_before() {
    TestDb::with(|db| async move {
        let app = common::quiz_app(&db);
        let mut question = first_question();
        question.answer_kind = Some("proof".to_owned());
        question.expected.answer = "See the solution.".to_owned();
        question.text = "Write the full proof: 8 + 5.5 = 13.5.".to_owned();
        let user = quiz_learner(&db, "quiz-proof@example.com", question).await;

        let (status, first) = answer_task(
            &app,
            user,
            QUIZ,
            json!({"problem_id": PROBLEM_ID, "answer": "8 + 5.5 = 13.5 since 8 + 5 = 13."}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{first}");
        assert!(first.get("proof_grading").is_none(), "{first}");
        assert!(
            jobs(&db, user).await.is_empty(),
            "no grading while the quiz is open"
        );

        put_quiz_live(&db, user, second_question()).await;
        let (status, last) = answer_task(
            &app,
            user,
            QUIZ,
            json!({"problem_id": PROBLEM_TWO, "answer": "37.5"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{last}");
        assert_eq!(last["quiz_complete"], true);
        assert!(!last.to_string().contains("proof"), "{last}");
        assert!(
            jobs(&db, user).await.is_empty(),
            "the close alone enqueues nothing"
        );

        // The reveal enqueues the proof and names its job.
        let path = format!("/api/task/{QUIZ}/quiz-result");
        let (status, raw) = call(&app, Method::POST, &path, Some(user), Some(json!({}))).await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        let body = parse(&raw);
        let proof = &body["answers"][0];
        assert_eq!(proof["outcome"], "ungraded");
        assert_eq!(proof["proof_grading"]["status"], "pending", "{body}");
        assert!(
            proof.get("proof_payload").is_none(),
            "the payload stays in the buffer"
        );
        assert!(body["answers"][1].get("proof_grading").is_none());
        let rows = jobs(&db, user).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, proof["attempt_id"].as_str().unwrap());
        assert_eq!(
            rows[0].1["given_answer"],
            "8 + 5.5 = 13.5 since 8 + 5 = 13."
        );
        assert_eq!(rows[0].1["expected"], Value::Null);

        // A second reveal names the same job and writes no second row.
        let (status, again) = call(&app, Method::POST, &path, Some(user), Some(json!({}))).await;
        assert_eq!(status, StatusCode::OK, "{again}");
        assert_eq!(
            parse(&again)["answers"][0]["proof_grading"],
            proof["proof_grading"]
        );
        assert_eq!(jobs(&db, user).await.len(), 1);
    })
    .await;
}
