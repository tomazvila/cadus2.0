//! Proof completion quality survives later knowledge points and session boundaries.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use cadus_core::answer::AnswerContract;
use cadus_core::config::Config;
use cadus_core::curriculum::{Curriculum, Exemplar};
use cadus_core::event::{TaskType, WorkQuality};
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

const PROOF1: &str = "Write the full proof: the sum of two odd integers is even.";
const REF1: &str = "Write a = 2k+1 and b = 2m+1; then a + b = 2(k+m+1) is even.";
const PROOF2: &str = "Prove that the square of an even integer is even.";
const REF2: &str = "Let n = 2k. Then n^2 = 4k^2 = 2(2k^2), which is even.";
const DRAFT1: &str = "n is even so n squared is even.";
const DRAFT2: &str = "Let n = 2k. Then n squared is 4k squared.";
const DRAFT3: &str = "Let n = 2k, so n^2 = 4k^2.";

fn proof_exemplar(problem: &str, reference: &str) -> Exemplar {
    Exemplar {
        answer_contract: Some(AnswerContract::None),
        problem: problem.to_owned(),
        answer: "See the solution.".to_owned(),
        solution_sketch: Some(reference.to_owned()),
    }
}

fn curriculum() -> Curriculum {
    one_unit_curriculum(vec![topic(
        "addition",
        vec![
            kp(
                "kp1",
                vec![
                    exemplar(PROBLEM_TEXT, EXPECTED_ANSWER),
                    proof_exemplar(PROOF1, REF1),
                ],
            ),
            kp("kp2", vec![proof_exemplar(PROOF2, REF2)]),
        ],
    )])
}

fn app(db: &TestDb) -> Router {
    app_with(db, curriculum())
}

fn app_with(db: &TestDb, graph: Curriculum) -> Router {
    cadus_web::create_app(
        state_with_content(db, graph)
            .with_admin(Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS)),
    )
}

fn decided_last_curriculum() -> Curriculum {
    one_unit_curriculum(vec![topic(
        "addition",
        vec![
            kp(
                "kp1",
                vec![
                    exemplar(PROBLEM_TEXT, EXPECTED_ANSWER),
                    proof_exemplar(PROOF1, REF1),
                ],
            ),
            kp("kp2", vec![exemplar(PROBLEM_TEXT, EXPECTED_ANSWER)]),
        ],
    )])
}

async fn request(
    app: &Router,
    method: Method,
    uri: &str,
    user: Uuid,
    body: Option<Value>,
) -> (StatusCode, Value, String) {
    let (status, raw) = call(app, method, uri, Some(user), body).await;
    (
        status,
        serde_json::from_str(&raw).unwrap_or(Value::Null),
        raw,
    )
}

async fn submit(
    app: &Router,
    user: Uuid,
    task: &str,
    problem_id: &str,
    draft: &str,
) -> (Uuid, Value) {
    let reply = answer_task_ok(
        app,
        user,
        task,
        json!({"problem_id": problem_id, "answer": draft}),
    )
    .await;
    let id = Uuid::parse_str(reply["proof_grading"]["id"].as_str().unwrap()).unwrap();
    (id, reply)
}

async fn land(db: &TestDb, user: Uuid, id: Uuid, pass: bool) {
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
        verdict: if pass { "pass" } else { "needs_revision" }.to_owned(),
        checks: vec![
            Check {
                id: "G1".to_owned(),
                text: "The claim and its hypotheses are stated.".to_owned(),
                minor: true,
                met: pass,
                evidence: if pass { "Let n = 2k" } else { "not found" }.to_owned(),
                quote_verified: pass,
            },
            Check {
                id: "G3".to_owned(),
                text: "Each step is justified.".to_owned(),
                minor: false,
                met: pass,
                evidence: if pass {
                    "n^2 = 4k^2"
                } else {
                    "so n squared is even"
                }
                .to_owned(),
                quote_verified: true,
            },
        ],
        feedback: "Justify why n squared is even.".to_owned(),
        model: "test-model".to_owned(),
    };
    cadus_worker::proof_grading::land(
        &Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS),
        &curriculum(),
        &claimed,
        &grading,
    )
    .await
    .unwrap();
}

fn lesson_xp(tier: WorkQuality) -> f64 {
    let xp = cadus_core::xp::task_xp(TaskType::Lesson, tier, &Config::default(), 2, 0, false);
    (xp * 100.0).round() / 100.0
}

async fn close_proof(app: &Router, user: Uuid, task: &str) -> (Value, String) {
    let (status, body, raw) = request(
        app,
        Method::POST,
        &format!("/api/task/{task}/proof/continue"),
        user,
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{raw}");
    (body, raw)
}

/// A capped rewrite on kp1 remains assisted/passable when kp2 closes in a new
/// session. Revisions remain one proof attempt, with no scheduling events.
#[tokio::test]
async fn capped_earlier_proof_tier_survives_rollover_to_later_point() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = lesson_learner(
            &db,
            "proof-score-rollover@example.test",
            lesson_problem(5.0, "kp1", Vec::new()),
        )
        .await;
        let first = answer_task_ok(
            &app,
            user,
            LESSON,
            json!({"problem_id": PROBLEM_ID, "answer": EXPECTED_ANSWER}),
        )
        .await;
        let decided_id = first["next"]["problem_id"].as_str().unwrap();
        let second = answer_task_ok(
            &app,
            user,
            LESSON,
            json!({"problem_id": decided_id, "answer": EXPECTED_ANSWER}),
        )
        .await;
        assert_eq!(second["next"]["text"], PROOF1);
        let proof1_id = second["next"]["problem_id"].as_str().unwrap().to_owned();

        let mut head = Uuid::nil();
        for draft in [DRAFT1, DRAFT2, DRAFT3] {
            let (id, _) = submit(&app, user, LESSON, &proof1_id, draft).await;
            land(&db, user, id, false).await;
            head = id;
        }
        let (status, raw) = serve_raw(&app, user, LESSON).await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        assert_eq!(parse(&raw)["proof"]["phase"], "reveal");
        let (status, reveal, raw) = request(
            &app,
            Method::POST,
            &format!("/api/proof-grading/{head}/seen"),
            user,
            Some(json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        assert_eq!(reveal["chain"]["solution"], REF1);
        let (rewrite, reply) =
            submit(&app, user, LESSON, &proof1_id, "Let a = 2k+1 and b = 2m+1.").await;
        assert_eq!(reply["task_status"], "kp_advance", "{reply}");
        assert_eq!(reply["next"]["kp"], "kp2", "{reply}");
        assert_eq!(events_of_type(&db, user, "attempt").await.len(), 3);
        let closed: bool = sqlx::query_scalar(
            "SELECT closed_at IS NOT NULL FROM proof_grading_jobs WHERE id = $1",
        )
        .bind(rewrite)
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert!(closed);

        let (status, _, raw) = request(
            &app,
            Method::POST,
            "/api/session/end",
            user,
            Some(json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        let (status, started, raw) =
            request(&app, Method::POST, "/api/session/start", user, None).await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        let task = format!("{}-lesson-addition", started["session"].as_str().unwrap());
        let (_, plan, _) = request(&app, Method::GET, "/api/session/plan", user, None).await;
        assert_eq!(plan["tasks"][0]["start_at_kp"], "kp1", "{plan}");
        let (status, raw) = serve_raw(&app, user, &task).await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        let decided = parse(&raw);
        assert_eq!(decided["text"], PROBLEM_TEXT, "{raw}");
        let _first = answer_task_ok(
            &app,
            user,
            &task,
            json!({"problem_id": decided["problem_id"], "answer": EXPECTED_ANSWER}),
        )
        .await;
        let (status, raw) = serve_raw(&app, user, &task).await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        let decided = parse(&raw);
        let second = answer_task_ok(
            &app,
            user,
            &task,
            json!({"problem_id": decided["problem_id"], "answer": EXPECTED_ANSWER}),
        )
        .await;
        assert_eq!(second["next"]["text"], PROOF1, "{second}");
        let (id, pending) = submit(
            &app,
            user,
            &task,
            second["next"]["problem_id"].as_str().unwrap(),
            DRAFT3,
        )
        .await;
        assert_eq!(pending["task_status"], "proof_pending");
        land(&db, user, id, true).await;
        let (moved, raw) = close_proof(&app, user, &task).await;
        assert_eq!(moved["task_status"], "kp_advance", "{raw}");

        let (status, raw) = serve_raw(&app, user, &task).await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        let served = parse(&raw);
        assert_eq!(served["text"], PROOF2, "{raw}");
        let (id, pending) = submit(
            &app,
            user,
            &task,
            served["problem_id"].as_str().unwrap(),
            DRAFT3,
        )
        .await;
        assert_eq!(pending["task_status"], "proof_pending");
        land(&db, user, id, true).await;
        let (closed, raw) = close_proof(&app, user, &task).await;
        assert_eq!(closed["task_status"], "task_passed", "{raw}");
        assert_eq!(
            closed["xp"],
            json!(lesson_xp(WorkQuality::Passable)),
            "{raw}"
        );
        let results = events_of_type(&db, user, "lesson_result").await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["quality_tier"], "passable");
        assert_eq!(results[0]["assisted"], true);
    })
    .await;
}

/// A revised pass remains passable when a normal decided answer closes the
/// final point, even after its root falls outside the newest 500 jobs.
#[tokio::test]
async fn revised_earlier_proof_scores_a_decided_final_point_past_list_limit() {
    TestDb::with(|db| async move {
        let app = app_with(&db, decided_last_curriculum());
        let user = lesson_learner(
            &db,
            "proof-score-revised-list@example.test",
            lesson_problem(5.0, "kp1", Vec::new()),
        )
        .await;
        let first = answer_task_ok(
            &app,
            user,
            LESSON,
            json!({"problem_id": PROBLEM_ID, "answer": EXPECTED_ANSWER}),
        )
        .await;
        let second = answer_task_ok(
            &app,
            user,
            LESSON,
            json!({"problem_id": first["next"]["problem_id"], "answer": EXPECTED_ANSWER}),
        )
        .await;
        assert_eq!(second["next"]["text"], PROOF1);
        let proof_id = second["next"]["problem_id"].as_str().unwrap().to_owned();

        let (root, _) = submit(&app, user, LESSON, &proof_id, DRAFT1).await;
        land(&db, user, root, false).await;
        sqlx::query(
            "INSERT INTO proof_grading_jobs (user_id, attempt_id, payload, context, status, finished_at, closed_at) \
             SELECT $1, 'noise-' || n::text, jsonb_build_object('v', 1, 'task_id', 'noise', 'topic', 'other', 'given_answer', 'x'), \
                    'review', 'failed', now(), now() FROM generate_series(1, 501) AS n",
        )
        .bind(user)
        .execute(&db.admin)
        .await
        .unwrap();
        let (revised, pending) = submit(&app, user, LESSON, &proof_id, DRAFT2).await;
        assert_eq!(pending["proof"]["revision"], 1);
        land(&db, user, revised, true).await;
        let (status, moved, raw) = request(
            &app,
            Method::POST,
            &format!("/api/task/{LESSON}/proof/continue"),
            user,
            Some(json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        assert_eq!(moved["task_status"], "kp_advance", "{raw}");

        let (status, raw) = serve_raw(&app, user, LESSON).await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        let served = parse(&raw);
        assert_eq!(served["text"], PROBLEM_TEXT, "{raw}");
        let first = answer_task_ok(
            &app,
            user,
            LESSON,
            json!({"problem_id": served["problem_id"], "answer": EXPECTED_ANSWER}),
        )
        .await;
        let (status, raw) = serve_raw(&app, user, LESSON).await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        let served = parse(&raw);
        let final_reply = answer_task_ok(
            &app,
            user,
            LESSON,
            json!({"problem_id": served["problem_id"], "answer": EXPECTED_ANSWER}),
        )
        .await;
        assert_eq!(final_reply["task_status"], "task_passed", "{final_reply}");
        assert_eq!(final_reply["xp"], json!(lesson_xp(WorkQuality::Passable)));
        let results = events_of_type(&db, user, "lesson_result").await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["quality_tier"], "passable");
        assert_eq!(results[0]["assisted"], false);
        assert_eq!(events_of_type(&db, user, "attempt").await.len(), 5);
        assert_eq!(events_of_type(&db, user, "regraded").await.len(), 1);

        // Once the lesson result is appended, the chain no longer belongs to
        // a later run of this topic.
        let mut tx = cadus_store::begin_tenant(&db.app, user).await.unwrap();
        let heads = cadus_store::proof_grading::closed_lesson_heads_after_latest_result(
            &mut *tx,
            "addition",
        )
        .await
        .unwrap();
        assert!(heads.is_empty(), "completed lesson chains must reset");
        tx.rollback().await.unwrap();
        let _ = first;
    })
    .await;
}
