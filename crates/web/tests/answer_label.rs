//! A `label` item through the answer route: the option list is closed, so a
//! miss is the other option and never reaches the background equivalence
//! check (owner, 2026-10-10: a wrong yes or no has no check cycle).

#![allow(clippy::unwrap_used, clippy::panic)]

mod common;

use cadus_core::answer::AnswerContract;
use cadus_store::test_support::TestDb;
use common::{LESSON, PROBLEM_ID, answer_task_ok, lesson_app, lesson_learner, lesson_problem};
use serde_json::{Value, json};
use sqlx::types::Uuid;

/// "Are 4(x + 3) and 4x + 3 equivalent?", stored with the answer `no`.
fn yes_or_no() -> cadus_web::state::ServedProblem {
    let mut live = lesson_problem(5.0, "kp1", Vec::new());
    live.text = "Are $4(x + 3)$ and $4x + 3$ equivalent?".to_owned();
    live.expected.answer = "no".to_owned();
    live.expected.answer_contract = Some(
        serde_json::from_str::<AnswerContract>(
            r#"{"kind":"label","options":[["yes","they are equivalent"],["no","they are not equivalent"]]}"#,
        )
        .unwrap(),
    );
    live.solution_sketch = Some("Distributing gives $4x + 12$, not $4x + 3$.".to_owned());
    live
}

async fn answer(db: &TestDb, email: &str, given: &str) -> (Uuid, Value) {
    let user = lesson_learner(db, email, yes_or_no()).await;
    let app = lesson_app(db);
    let reply = answer_task_ok(
        &app,
        user,
        LESSON,
        json!({"problem_id": PROBLEM_ID, "answer": given}),
    )
    .await;
    (user, reply)
}

async fn equivalence_jobs(db: &TestDb, user: Uuid) -> i64 {
    let mut tx = cadus_store::begin_tenant(&db.app, user).await.unwrap();
    let count =
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM equivalence_jobs WHERE user_id = $1")
            .bind(user)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    tx.commit().await.unwrap();
    count
}

#[tokio::test]
async fn the_other_option_is_a_final_miss_with_no_background_check() {
    TestDb::with(|db| async move {
        let (user, reply) = answer(&db, "label-miss@example.test", "yes").await;
        assert_eq!(reply["outcome"], json!("incorrect"), "{reply}");
        assert!(
            reply["equivalence"].is_null() || reply["equivalence"]["status"] != json!("pending"),
            "a closed option list never waits for the model: {reply}"
        );
        assert_eq!(equivalence_jobs(&db, user).await, 0);
        assert_eq!(
            reply["diagnosis"]["status"],
            json!("not_offered"),
            "no model job works out a wrong yes or no: {reply}"
        );
    })
    .await;
}

#[tokio::test]
async fn the_right_option_grades_correct() {
    TestDb::with(|db| async move {
        let (user, reply) = answer(&db, "label-hit@example.test", "they are not equivalent").await;
        assert_eq!(reply["outcome"], json!("correct"), "{reply}");
        assert_eq!(equivalence_jobs(&db, user).await, 0);
    })
    .await;
}
