//! D-PR1: several written items in one knowledge point, end to end.
//!
//! The `addition` lesson of these tests has one point, `kp1`, in two shapes:
//!
//! - all written: a `written` sentence item, then a written proof;
//! - mixed: a decided item first, then the same two written items.
//!
//! A chain is keyed by `(topic, kp, problem_hash)`; the point closes when
//! every written item's chain closed. Every expected value is a literal of
//! the test that reads it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use cadus_core::answer::AnswerContract;
use cadus_core::curriculum::{Curriculum, Exemplar};
use cadus_core::learner::problem_text_hash;
use cadus_store::proof_grading::{Check, Claimed, Grading};
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use common::{
    EXPECTED_ANSWER, LESSON, PROBLEM_ID, PROBLEM_TEXT, answer_task_ok, call, events_of_type,
    exemplar, kp, lesson_learner, lesson_problem, one_unit_curriculum, parse, serve_raw,
    state_with_content, topic,
};
use serde_json::{Value, json};
use sqlx::types::Uuid;

/// The `written` item of `kp1`.
const SENTENCE: &str = "Write the contrapositive of: if n is even, n squared is even.";

/// Its reference sentence.
const SENTENCE_REF: &str = "If n squared is odd, then n is odd.";

/// The written proof of `kp1`.
const PROOF: &str = "Prove that the product of two even integers is even.";

/// The reference solution of the proof.
const PROOF_REF: &str = "Let a = 2k and b = 2m. Then ab = 2(2km).";

/// The learner's sentence.
const GIVEN: &str = "If n squared is not even, then n is not even.";

/// The `written` exemplar.
fn written_exemplar() -> Exemplar {
    Exemplar {
        answer_contract: Some(AnswerContract::Written),
        problem: SENTENCE.to_owned(),
        answer: SENTENCE_REF.to_owned(),
        solution_sketch: Some("Swap the two sides and negate both.".to_owned()),
        visual: None,
    }
}

/// The written-proof exemplar.
fn proof_exemplar() -> Exemplar {
    Exemplar {
        answer_contract: Some(AnswerContract::None),
        problem: PROOF.to_owned(),
        answer: "See the solution.".to_owned(),
        solution_sketch: Some(PROOF_REF.to_owned()),
        visual: None,
    }
}

/// The curriculum: `kp1` holds `exemplars`.
fn curriculum(exemplars: Vec<Exemplar>) -> Curriculum {
    one_unit_curriculum(vec![topic("addition", vec![kp("kp1", exemplars)])])
}

/// The router over the all-written curriculum.
fn app(db: &TestDb, exemplars: Vec<Exemplar>) -> Router {
    cadus_web::create_app(
        state_with_content(db, curriculum(exemplars))
            .with_admin(Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS)),
    )
}

/// The live `written` item of the lesson.
fn written_problem() -> cadus_web::state::ServedProblem {
    let mut live = lesson_problem(5.0, "kp1", Vec::new());
    live.text = SENTENCE.to_owned();
    live.expected.answer = SENTENCE_REF.to_owned();
    live.expected.answer_contract = Some(AnswerContract::Written);
    live.solution_sketch = Some("Swap the two sides and negate both.".to_owned());
    live
}

/// Settle job `id` as a pass, through the worker's own landing.
async fn land_pass(db: &TestDb, user: Uuid, id: Uuid, exemplars: Vec<Exemplar>) {
    let (attempt_id, payload): (String, Value) =
        sqlx::query_as("SELECT attempt_id, payload FROM proof_grading_jobs WHERE id = $1")
            .bind(id)
            .fetch_one(&db.admin)
            .await
            .unwrap();
    sqlx::query("UPDATE proof_grading_jobs SET status = 'running', attempts = 1 WHERE id = $1")
        .bind(id)
        .execute(&db.admin)
        .await
        .unwrap();
    let claimed = Claimed {
        id,
        user_id: user,
        attempt_id,
        payload,
        attempts: 1,
    };
    let grading = Grading {
        v: 1,
        verdict: "pass".to_owned(),
        checks: vec![Check {
            id: "W1".to_owned(),
            text: "The statement has the same meaning as the reference.".to_owned(),
            minor: false,
            met: true,
            evidence: "not found".to_owned(),
            quote_verified: true,
        }],
        feedback: "Same meaning.".to_owned(),
        model: "test-model".to_owned(),
    };
    cadus_worker::proof_grading::land(
        &Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS),
        &curriculum(exemplars),
        &claimed,
        &grading,
    )
    .await
    .unwrap();
}

/// Submit one answer of the live item and read the job id and the reply.
async fn submit(app: &Router, user: Uuid, problem_id: &str, answer: &str) -> (Uuid, Value) {
    let reply = answer_task_ok(
        app,
        user,
        LESSON,
        json!({"problem_id": problem_id, "answer": answer}),
    )
    .await;
    let id = Uuid::parse_str(reply["proof_grading"]["id"].as_str().unwrap()).unwrap();
    (id, reply)
}

/// `POST /api/task/{LESSON}/proof/continue` as `user`.
async fn proceed(app: &Router, user: Uuid) -> Value {
    let (status, raw) = call(
        app,
        Method::POST,
        &format!("/api/task/{LESSON}/proof/continue"),
        Some(user),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{raw}");
    parse(&raw)
}

/// The owed items of `user`, as `(kp, problem_hash)`.
async fn owed(db: &TestDb, user: Uuid) -> Vec<(String, String)> {
    sqlx::query_as(
        "SELECT kp, problem_hash FROM proof_owed WHERE user_id = $1 ORDER BY problem_hash",
    )
    .bind(user)
    .fetch_all(&db.admin)
    .await
    .unwrap()
}

/// A point of two written items closes only when both chains closed: the
/// `written` item goes to the model grader with its own payload, and the
/// proof is served next.
#[tokio::test]
async fn a_point_with_two_written_items_closes_after_both_chains() {
    TestDb::with(|db| async move {
        let items = vec![written_exemplar(), proof_exemplar()];
        let app = app(&db, items.clone());
        let user = lesson_learner(&db, "pw-two@example.test", written_problem()).await;

        // The served `written` item says so; the reference never rides.
        let (status, raw) = serve_raw(&app, user, LESSON).await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        let served = parse(&raw);
        assert_eq!(served["written"], true, "{raw}");
        assert_eq!(served["proof"]["phase"], "draft");
        assert!(!raw.contains(SENTENCE_REF), "{raw}");

        // The answer is graded by the model, in `written` mode, and the
        // payload names the item.
        let (first, reply) = submit(&app, user, PROBLEM_ID, GIVEN).await;
        assert_eq!(reply["task_status"], "proof_pending", "{reply}");
        assert!(!reply.to_string().contains(SENTENCE_REF), "{reply}");
        let payload: Value =
            sqlx::query_scalar("SELECT payload FROM proof_grading_jobs WHERE id = $1")
                .bind(first)
                .fetch_one(&db.admin)
                .await
                .unwrap();
        assert_eq!(payload["mode"], "written");
        assert_eq!(payload["problem_hash"], problem_text_hash(SENTENCE));
        assert_eq!(payload["reference"], SENTENCE_REF);
        assert_eq!(payload["given_answer"], GIVEN);

        // The pass of the sentence closes ITS chain only: the lesson stays
        // open and serves the proof.
        land_pass(&db, user, first, items.clone()).await;
        let closed = proceed(&app, user).await;
        assert_eq!(closed["task_status"], "continue", "{closed}");
        assert!(events_of_type(&db, user, "lesson_result").await.is_empty());
        assert_eq!(closed["next"]["text"], PROOF, "{closed}");
        assert_eq!(closed["next"]["proof"]["phase"], "draft");
        assert!(closed["next"].get("written").is_none(), "{closed}");
        assert!(!closed.to_string().contains(PROOF_REF), "{closed}");

        // The proof is a second chain of the same point; its pass closes it.
        let proof_id = closed["next"]["problem_id"].as_str().unwrap().to_owned();
        let (second, reply) =
            submit(&app, user, &proof_id, "Let a = 2k, b = 2m; ab = 2(2km).").await;
        assert_eq!(reply["task_status"], "proof_pending", "{reply}");
        let hash: String = sqlx::query_scalar(
            "SELECT payload->>'problem_hash' FROM proof_grading_jobs WHERE id = $1",
        )
        .bind(second)
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(hash, problem_text_hash(PROOF));
        land_pass(&db, user, second, items).await;
        let closed = proceed(&app, user).await;
        assert_eq!(closed["task_status"], "task_passed", "{closed}");
        assert_eq!(events_of_type(&db, user, "lesson_result").await.len(), 1);
    })
    .await;
}

/// The decided items of a mixed point pass first; the point then owes every
/// written item, and a first draft settles only its own item.
#[tokio::test]
async fn a_mixed_point_owes_each_written_item_once() {
    TestDb::with(|db| async move {
        let items = vec![
            exemplar(PROBLEM_TEXT, EXPECTED_ANSWER),
            written_exemplar(),
            proof_exemplar(),
        ];
        let app = app(&db, items);
        let user = lesson_learner(
            &db,
            "pw-mixed@example.test",
            lesson_problem(5.0, "kp1", Vec::new()),
        )
        .await;

        // Two decided passes hold the point and serve the first written item.
        let reply = answer_task_ok(
            &app,
            user,
            LESSON,
            json!({"problem_id": PROBLEM_ID, "answer": EXPECTED_ANSWER}),
        )
        .await;
        let next = reply["next"]["problem_id"].as_str().unwrap().to_owned();
        let reply = answer_task_ok(
            &app,
            user,
            LESSON,
            json!({"problem_id": next, "answer": EXPECTED_ANSWER}),
        )
        .await;
        assert_eq!(reply["next"]["text"], SENTENCE, "{reply}");
        assert_eq!(reply["next"]["written"], true, "{reply}");
        let mut expected = vec![
            ("kp1".to_owned(), problem_text_hash(SENTENCE)),
            ("kp1".to_owned(), problem_text_hash(PROOF)),
        ];
        expected.sort_by(|a, b| a.1.cmp(&b.1));
        assert_eq!(owed(&db, user).await, expected);

        // The first draft of the sentence settles its row and keeps the
        // proof's.
        let live = reply["next"]["problem_id"].as_str().unwrap().to_owned();
        submit(&app, user, &live, GIVEN).await;
        assert_eq!(
            owed(&db, user).await,
            [("kp1".to_owned(), problem_text_hash(PROOF))]
        );
    })
    .await;
}
