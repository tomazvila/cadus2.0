//! Amendment K (steer note 114, the owner's design): the equivalence flow of
//! the answer route, end to end over the fixture curriculum.
//!
//! The web tier never calls a model (L6), so the FAKE MODEL of these tests is
//! the worker's verdict, already settled into `equivalence_cache` (a cache
//! hit) or absent (a fresh miss whose background check stays pending). The
//! four cases of the owner's design:
//!
//! 1. the owner's case (`not a solution, 5` against `verdict = contradicts;
//!    D = 5`), cached EQUIVALENT → the reply grades CORRECT;
//! 2. a cache hit writes no background job (the model is never asked twice);
//! 3. a miss (the timeout path) answers `status: "pending"` and serves the
//!    next task; one job row stands for the worker to fold later;
//! 4. a cached NOT keeps the deterministic wrong and carries the reason.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_store::test_support::TestDb;
use cadus_web::equivalence::{cache_key, item_digest};
use common::{LESSON, PROBLEM_ID, answer_task_ok, lesson_app, lesson_learner, lesson_problem};
use serde_json::json;
use sqlx::types::Uuid;

/// The answer of the owner's case.
const OWNER_ANSWER: &str = "not a solution, 5";

/// The item the fixture serves: the owner's case. The exact-numeric key
/// refuses the natural-language answer, the way the owner's item did.
fn owner_case() -> cadus_web::state::ServedProblem {
    let mut live = lesson_problem(5.0, "kp1", Vec::new());
    live.expected.answer = "verdict = contradicts; D = 5".to_owned();
    live.answer_kind = Some("expression".to_owned());
    live.text = "State the verdict and the number of dissenting judges.".to_owned();
    live
}

/// The cache key of the owner's answer.

#[tokio::test]
async fn the_owners_case_grades_correct_through_the_cached_verdict() {
    TestDb::with(|db| async move {
        let served = owner_case();
        let user = lesson_learner(&db, "owner-case@example.test", served.clone()).await;
        // The fake model's verdict: the worker already settled EQUIVALENT for
        // this exact answer on this item.
        let verdict = cadus_store::equivalence::Verdict {
            equivalent: true,
            reason: "the same verdict and the same number, in other words".to_owned(),
            model: "qwen-general-8bit".to_owned(),
        };
        cadus_store::equivalence::cache_put(
            &db.admin,
            &item_digest(&served),
            &cache_key(OWNER_ANSWER),
            &verdict,
        )
        .await
        .unwrap();

        let app = lesson_app(&db);
        let reply = answer_task_ok(
            &app,
            user,
            LESSON,
            json!({"problem_id": PROBLEM_ID, "answer": OWNER_ANSWER}),
        )
        .await;
        // EQUIVALENT counts as correct (note 114, point 4).
        assert_eq!(reply["outcome"], json!("correct"), "{reply}");
        assert_eq!(reply["correct"], json!(true));
        assert_eq!(reply["equivalence"]["status"], json!("accepted"));
        // No background job: the cache answered at once.
        let mut tx = cadus_store::begin_tenant(&db.app, user).await.unwrap();
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM equivalence_jobs WHERE user_id = $1",
        )
        .bind(user)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();
        assert_eq!(count, 0);
    })
    .await;
}

#[tokio::test]
async fn a_miss_enqueues_and_serves_the_next_task() {
    TestDb::with(|db| async move {
        let served = owner_case();
        let user = lesson_learner(&db, "pending-case@example.test", served.clone()).await;
        let app = lesson_app(&db);
        let reply = answer_task_ok(
            &app,
            user,
            LESSON,
            json!({"problem_id": PROBLEM_ID, "answer": OWNER_ANSWER}),
        )
        .await;
        // The timeout path: the deterministic verdict (the checker cannot
        // parse the owner's natural-language answer) stands and the
        // background check is pending; the next task is served.
        assert_eq!(reply["outcome"], json!("ungraded"), "{reply}");
        assert_eq!(reply["equivalence"]["status"], json!("pending"));
        let id = Uuid::parse_str(reply["equivalence"]["id"].as_str().unwrap_or_default()).unwrap();
        assert!(!reply["next"].is_null(), "the next task is served");
        // One job row stands for the worker, with the model prompt in it.
        let mut tx = cadus_store::begin_tenant(&db.app, user).await.unwrap();
        let row = cadus_store::equivalence::job(&mut *tx, id)
            .await
            .unwrap()
            .expect("the job row");
        assert_eq!(row.status, "pending");
        let payload: Option<serde_json::Value> =
            sqlx::query_scalar("SELECT payload FROM equivalence_jobs WHERE id = $1")
                .bind(id)
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        let payload: cadus_store::equivalence::JobPayload =
            serde_json::from_value(payload.unwrap()).unwrap();
        assert_eq!(payload.given_answer, OWNER_ANSWER);
        assert_eq!(payload.expected, "verdict = contradicts; D = 5");
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn a_cached_not_keeps_the_deterministic_wrong_and_shows_the_reason() {
    TestDb::with(|db| async move {
        let served = owner_case();
        let user = lesson_learner(&db, "not-case@example.test", served.clone()).await;
        let verdict = cadus_store::equivalence::Verdict {
            equivalent: false,
            reason: "the verdict part disagrees with the key".to_owned(),
            model: "qwen-general-8bit".to_owned(),
        };
        cadus_store::equivalence::cache_put(
            &db.admin,
            &item_digest(&served),
            &cache_key(OWNER_ANSWER),
            &verdict,
        )
        .await
        .unwrap();

        let app = lesson_app(&db);
        let reply = answer_task_ok(
            &app,
            user,
            LESSON,
            json!({"problem_id": PROBLEM_ID, "answer": OWNER_ANSWER}),
        )
        .await;
        // The deterministic verdict stands (the checker cannot parse the
        // answer); the model's one-line reason rides beside the solution.
        assert_eq!(reply["outcome"], json!("ungraded"), "{reply}");
        assert_eq!(reply["equivalence"]["status"], json!("refused"));
        assert_eq!(
            reply["equivalence_reason"],
            json!("the verdict part disagrees with the key")
        );
    })
    .await;
}

/// The key's own spelling grades correct deterministically and never reaches
/// the equivalence path.
#[tokio::test]
async fn a_deterministic_correct_answer_skips_the_equivalence_path() {
    TestDb::with(|db| async move {
        let served = owner_case();
        let user = lesson_learner(&db, "correct-case@example.test", served.clone()).await;
        let app = lesson_app(&db);
        let reply = answer_task_ok(
            &app,
            user,
            LESSON,
            json!({"problem_id": PROBLEM_ID, "answer": "verdict = contradicts; D = 5"}),
        )
        .await;
        assert_eq!(reply["outcome"], json!("correct"), "{reply}");
        assert_eq!(reply["equivalence"], json!(null));
    })
    .await;
}
