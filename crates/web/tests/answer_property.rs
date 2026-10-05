//! A `property` item through the answer route: the stored answer is one
//! example, the predicate decides the learner's own object, and a decided
//! property verdict never reaches the background equivalence check.

#![allow(clippy::unwrap_used, clippy::panic)]

mod common;

use cadus_core::answer::AnswerContract;
use cadus_store::test_support::TestDb;
use common::{LESSON, PROBLEM_ID, answer_task_ok, lesson_app, lesson_learner, lesson_problem};
use serde_json::{Value, json};
use sqlx::types::Uuid;

/// "Give a number with exactly three factors", stored with the example 9.
fn three_factors() -> cadus_web::state::ServedProblem {
    let mut live = lesson_problem(5.0, "kp1", Vec::new());
    live.text = "Give a whole number that has exactly three factors.".to_owned();
    live.answer_kind = Some("expression".to_owned());
    live.expected.answer = "9".to_owned();
    live.expected.answer_contract = Some(
        serde_json::from_str::<AnswerContract>(
            r#"{"kind":"property","check":"divisor_count","args":{"n":3}}"#,
        )
        .unwrap(),
    );
    live.solution_sketch = Some("One example is 9: its factors are 1, 3 and 9.".to_owned());
    live
}

async fn answer(db: &TestDb, email: &str, given: &str) -> (Uuid, Value) {
    let user = lesson_learner(db, email, three_factors()).await;
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
async fn a_different_valid_example_grades_correct() {
    TestDb::with(|db| async move {
        for (email, given) in [
            ("property-25@example.test", "25"),
            ("property-49@example.test", "49"),
            ("property-4@example.test", "4"),
        ] {
            let (user, reply) = answer(&db, email, given).await;
            assert_eq!(reply["outcome"], json!("correct"), "{given}: {reply}");
            assert_eq!(reply["correct"], json!(true));
            assert_eq!(reply["equivalence"], json!(null));
            assert_eq!(equivalence_jobs(&db, user).await, 0);
        }
    })
    .await;
}

#[tokio::test]
async fn a_property_miss_is_final_and_skips_the_equivalence_check() {
    TestDb::with(|db| async move {
        let (user, reply) = answer(&db, "property-miss@example.test", "8").await;
        assert_eq!(reply["outcome"], json!("incorrect"), "{reply}");
        assert_eq!(reply["correct"], json!(false));
        assert_eq!(reply["equivalence"], json!(null));
        assert_eq!(equivalence_jobs(&db, user).await, 0);
        // The worked solution, with its one example, follows the attempt.
        assert_eq!(
            reply["solution"],
            json!("One example is 9: its factors are 1, 3 and 9.")
        );
    })
    .await;
}

#[tokio::test]
async fn an_unreadable_property_answer_stays_ungraded_without_a_model_check() {
    TestDb::with(|db| async move {
        let (user, reply) = answer(&db, "property-garbage@example.test", "9 or 25 maybe").await;
        assert_eq!(reply["outcome"], json!("ungraded"), "{reply}");
        assert_eq!(reply["equivalence"], json!(null));
        assert_eq!(equivalence_jobs(&db, user).await, 0);
        // An ungraded attempt reveals nothing (Hard Rule 1).
        assert_eq!(reply.get("solution"), None);
    })
    .await;
}

/// The diagnosis job of a property miss names the property, so the model
/// never compares the learner's 8 with the example 9.
#[tokio::test]
async fn a_property_miss_sends_the_property_to_the_diagnosis_job() {
    use common::diagnosis::{app, jobs_of, seed_distractors};
    TestDb::with(|db| async move {
        let user = lesson_learner(&db, "property-diagnosis@example.test", three_factors()).await;
        seed_distractors(
            &db,
            "property-digest",
            &json!({"v": 1, "distractors": [
                {"answer": "3", "error_tag": "arithmetic-slip", "note": "3 has two factors."}
            ]}),
        )
        .await;
        let reply = answer_task_ok(
            &app(&db),
            user,
            LESSON,
            json!({"problem_id": PROBLEM_ID, "answer": "8"}),
        )
        .await;
        assert_eq!(reply["outcome"], json!("incorrect"), "{reply}");
        assert_eq!(reply["diagnosis"]["status"], json!("pending"), "{reply}");
        let jobs = jobs_of(&db, user).await;
        assert_eq!(jobs.len(), 1);
        assert_eq!(
            jobs[0].2["answer_property"],
            json!("a positive integer with exactly 3 positive divisors")
        );
        assert_eq!(jobs[0].2["expected"], json!("9"));
        assert_eq!(equivalence_jobs(&db, user).await, 0);
    })
    .await;
}
