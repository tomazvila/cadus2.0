//! Amendment K point 6: the request-tier half of background proof grading,
//! end to end over the fixture curriculum.
//!
//! The web tier never calls a model (L6). These tests pin the enqueue (an
//! ungraded written proof writes one `proof_grading_jobs` row in the grade
//! transaction and the reply names it), the routing (a written proof skips
//! the equivalence check; a decided miss never enqueues a proof job), and the
//! poll route (pending, then the worker's checks, feedback and the reference
//! solution; another tenant reads 404).

#![allow(clippy::unwrap_used, clippy::panic)]

mod common;

use axum::http::{Method, StatusCode};
use cadus_store::test_support::TestDb;
use common::{LESSON, PROBLEM_ID, answer_task_ok, lesson_app, lesson_learner, lesson_problem};
use serde_json::{Value, json};
use sqlx::types::Uuid;

/// The learner's proof.
const PROOF: &str = "Let a = 2k+1 and b = 2m+1. Then a + b = 2(k+m+1), which is even.";

/// The reference solution the item carries.
const REFERENCE: &str = "Write a = 2k+1, b = 2m+1; then a+b = 2(k+m+1) is even.";

/// A written-proof item: kind `proof`, the contract `none`, the placeholder key.
fn proof_item() -> cadus_web::state::ServedProblem {
    let mut live = lesson_problem(5.0, "kp1", Vec::new());
    live.answer_kind = Some("proof".to_owned());
    live.expected.answer = "See the solution.".to_owned();
    live.expected.answer_contract = Some(cadus_core::answer::AnswerContract::None);
    live.text = "Write the full proof: the sum of two odd integers is even.".to_owned();
    live.solution_sketch = Some(REFERENCE.to_owned());
    live
}

/// A GET as `tenant`, parsed.
async fn get(app: &axum::Router, uri: &str, tenant: Uuid) -> (StatusCode, Value) {
    let (status, text) = common::call(app, Method::GET, uri, Some(tenant), None).await;
    (status, serde_json::from_str(&text).unwrap_or(Value::Null))
}

/// The count of rows of `table` the learner owns, read as admin.
async fn count(db: &TestDb, table: &str, user: Uuid) -> i64 {
    let sql = match table {
        "proof_grading_jobs" => "SELECT count(*) FROM proof_grading_jobs WHERE user_id = $1",
        "equivalence_jobs" => "SELECT count(*) FROM equivalence_jobs WHERE user_id = $1",
        other => panic!("no count statement for {other}"),
    };
    sqlx::query_scalar::<_, i64>(sql)
        .bind(user)
        .fetch_one(&db.admin)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_written_proof_enqueues_one_job_and_the_poll_follows_it() {
    TestDb::with(|db| async move {
        let user = lesson_learner(&db, "proof-enqueue@example.test", proof_item()).await;
        let other = common::seed_learner(&db, "proof-other@example.test").await;
        let app = lesson_app(&db);
        let reply = answer_task_ok(
            &app,
            user,
            LESSON,
            json!({"problem_id": PROBLEM_ID, "answer": PROOF}),
        )
        .await;

        // The instant reply: ungraded, the pending job, the next task.
        assert_eq!(reply["outcome"], json!("ungraded"), "{reply}");
        assert_eq!(reply["proof_grading"]["status"], json!("pending"));
        assert_eq!(reply["equivalence"], json!(null), "a proof has no key to be equivalent to");
        assert!(reply.get("solution").is_none(), "no reveal before the grading lands");
        let id = Uuid::parse_str(reply["proof_grading"]["id"].as_str().unwrap()).unwrap();
        assert_eq!(count(&db, "proof_grading_jobs", user).await, 1);
        assert_eq!(count(&db, "equivalence_jobs", user).await, 0);

        // The payload carries the prompt inputs and drops the placeholder key.
        let payload = sqlx::query_scalar::<_, Value>(
            "SELECT payload FROM proof_grading_jobs WHERE id = $1",
        )
        .bind(id)
        .fetch_one(&db.admin)
        .await
        .unwrap();
        let payload: cadus_store::proof_grading::JobPayload =
            serde_json::from_value(payload).unwrap();
        assert_eq!(payload.given_answer, PROOF);
        assert_eq!(payload.reference.as_deref(), Some(REFERENCE));
        assert_eq!(payload.expected, None);
        assert!(payload.rubric.is_empty());

        // The poll: pending for the owner, 404 for another tenant.
        let uri = format!("/api/proof-grading/{id}");
        let (status, body) = get(&app, &uri, user).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["status"], json!("pending"));
        assert!(body.get("checks").is_none());
        let (status, _) = get(&app, &uri, other).await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        // The worker settles a needs-revision grading; the poll shows it.
        let result = json!({
            "v": 1, "verdict": "needs_revision", "model": "deepseek/deepseek-v4-pro",
            "feedback": "The conclusion is not stated.",
            "checks": [
                {"id": "G5", "text": "The conclusion is reached.", "minor": false, "met": false, "evidence": "not found"},
                {"id": "S1", "text": "Writes both odd integers as 2k+1.", "minor": false, "met": true, "evidence": "Let a = 2k+1"}
            ]
        });
        sqlx::query("UPDATE proof_grading_jobs SET status = 'done', result = $2 WHERE id = $1")
            .bind(id)
            .bind(&result)
            .execute(&db.admin)
            .await
            .unwrap();
        let (status, body) = get(&app, &uri, user).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], json!("needs_revision"));
        assert_eq!(body["feedback"], json!("The conclusion is not stated."));
        assert_eq!(body["checks"][1]["evidence"], json!("Let a = 2k+1"));
        assert_eq!(body["solution"], json!(REFERENCE));
    })
    .await;
}

/// A decided miss on a numeric item never enqueues a proof job.
#[tokio::test]
async fn a_numeric_miss_enqueues_no_proof_job() {
    TestDb::with(|db| async move {
        let live = lesson_problem(5.0, "kp1", Vec::new());
        let user = lesson_learner(&db, "proof-numeric@example.test", live).await;
        let app = lesson_app(&db);
        let reply = answer_task_ok(
            &app,
            user,
            LESSON,
            json!({"problem_id": PROBLEM_ID, "answer": "99"}),
        )
        .await;
        assert_eq!(reply["outcome"], json!("incorrect"), "{reply}");
        assert_eq!(reply["proof_grading"], json!(null));
        assert_eq!(count(&db, "proof_grading_jobs", user).await, 0);
    })
    .await;
}
