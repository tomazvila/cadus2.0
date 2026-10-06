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

use cadus_core::answer::{AnswerContract, Quantity};
use cadus_store::test_support::TestDb;
use cadus_web::equivalence::{cache_item_digest, cache_key, item_digest};
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

fn unit_case() -> cadus_web::state::ServedProblem {
    let mut live = lesson_problem(5.0, "kp1", Vec::new());
    live.expected.answer = "8".to_owned();
    live.expected.answer_contract = Some(AnswerContract::Unit {
        quantity: Quantity::Length,
        unit: "m".to_owned(),
        allow_omitted: true,
        form: None,
    });
    live.answer_kind = Some("numeric".to_owned());
    live.text = "A length is 8 m. Give its value in metres.".to_owned();
    live
}

#[tokio::test]
async fn a_unit_policy_does_not_reuse_an_old_accepted_answer_for_the_same_text_key() {
    TestDb::with(|db| async move {
        let served = unit_case();
        let user =
            lesson_learner(&db, "unit-cache-invalidation@example.test", served.clone()).await;
        let answer = "8 kg";
        let old_verdict = cadus_store::equivalence::Verdict {
            equivalent: true,
            reason: "old cache interpretation".to_owned(),
            model: "old-model".to_owned(),
        };
        // Simulate a positive cache record from before the Unit policy existed.
        cadus_store::equivalence::cache_put(
            &db.admin,
            &item_digest(&served),
            &cache_key(answer),
            &old_verdict,
        )
        .await
        .unwrap();

        let app = lesson_app(&db);
        let reply = answer_task_ok(
            &app,
            user,
            LESSON,
            json!({"problem_id": PROBLEM_ID, "answer": answer}),
        )
        .await;
        assert_eq!(reply["correct"], json!(false), "{reply}");
        assert_eq!(reply["equivalence"]["status"], json!("pending"), "{reply}");
        assert_ne!(cache_item_digest(&served), item_digest(&served));
    })
    .await;
}

#[tokio::test]
async fn a_unit_cache_uses_its_versioned_identity_and_non_unit_identity_stays_compatible() {
    TestDb::with(|db| async move {
        let unit = unit_case();
        let user = lesson_learner(&db, "unit-cache-hit@example.test", unit.clone()).await;
        let answer = "8x";
        let verdict = cadus_store::equivalence::Verdict {
            equivalent: true,
            reason: "cached unit-model result".to_owned(),
            model: "unit-model".to_owned(),
        };
        let unit_digest = cache_item_digest(&unit);
        let mut different_key = unit.clone();
        different_key.expected.answer = "9".to_owned();
        assert_ne!(unit_digest, cache_item_digest(&different_key));
        let mut different_policy = unit.clone();
        different_policy.expected.answer_contract = Some(AnswerContract::Unit {
            quantity: Quantity::Length,
            unit: "cm".to_owned(),
            allow_omitted: true,
            form: None,
        });
        assert_ne!(unit_digest, cache_item_digest(&different_policy));
        cadus_store::equivalence::cache_put(&db.admin, &unit_digest, &cache_key(answer), &verdict)
            .await
            .unwrap();

        let app = lesson_app(&db);
        let reply = answer_task_ok(
            &app,
            user,
            LESSON,
            json!({"problem_id": PROBLEM_ID, "answer": answer}),
        )
        .await;
        assert_eq!(reply["correct"], json!(true), "{reply}");
        assert_eq!(reply["equivalence"]["status"], json!("accepted"), "{reply}");

        let non_unit = owner_case();
        assert_eq!(cache_item_digest(&non_unit), item_digest(&non_unit));
    })
    .await;
}

#[tokio::test]
async fn unit_notation_is_deterministic_and_unsupported_input_stays_pending() {
    TestDb::with(|db| async move {
        let served = unit_case();
        let app = lesson_app(&db);
        let normal_user =
            lesson_learner(&db, "unit-normal-notation@example.test", served.clone()).await;
        let normal = answer_task_ok(
            &app,
            normal_user,
            LESSON,
            json!({"problem_id": PROBLEM_ID, "answer": "8m"}),
        )
        .await;
        assert_eq!(normal["correct"], json!(true), "{normal}");
        assert_eq!(normal["equivalence"], json!(null), "{normal}");

        let uncertain_user =
            lesson_learner(&db, "unit-uncertain-answer@example.test", served).await;
        let uncertain = answer_task_ok(
            &app,
            uncertain_user,
            LESSON,
            json!({"problem_id": PROBLEM_ID, "answer": "8x"}),
        )
        .await;
        assert_eq!(uncertain["outcome"], json!("ungraded"), "{uncertain}");
        assert!(uncertain.get("correct").is_none(), "{uncertain}");
        assert_eq!(
            uncertain["equivalence"]["status"],
            json!("pending"),
            "{uncertain}"
        );

        let job_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM equivalence_jobs WHERE user_id = $1")
                .bind(normal_user)
                .fetch_one(&db.admin)
                .await
                .unwrap();
        assert_eq!(job_count, 0);
    })
    .await;
}
