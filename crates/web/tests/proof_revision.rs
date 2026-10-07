//! D-PR1: the proof revision loop, end to end over a fixture curriculum.
//!
//! The `addition` lesson of these tests has two knowledge points:
//!
//! - `kp1` mixes a decided exemplar with a written-proof exemplar;
//! - `kp2` (the last point) is one written proof alone.
//!
//! The worker's own `land` settles each grading, so the `regraded`
//! correction and the refold are the ones production writes. Every expected
//! value is a literal of the test that reads it.

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
    EXPECTED_ANSWER, LESSON, PROBLEM_ID, PROBLEM_TEXT, answer_task, answer_task_ok, call,
    events_of_type, exemplar, kp, lesson_learner, lesson_problem, one_unit_curriculum, parse,
    put_state, seed_learner, seed_open_session, serve_raw, state_with_content, stored_state, topic,
};
use serde_json::{Value, json};
use sqlx::types::Uuid;

/// The written proof of `kp1`.
const PROOF1: &str = "Write the full proof: the sum of two odd integers is even.";

/// The reference solution of `kp1`'s proof.
const REF1: &str = "Write a = 2k+1 and b = 2m+1; then a + b = 2(k+m+1) is even.";

/// The written proof of `kp2`.
const PROOF2: &str = "Prove that the square of an even integer is even.";

/// The reference solution of `kp2`'s proof. No draft below quotes it.
const REF2: &str = "Let n = 2k. Then n^2 = 4k^2 = 2(2k^2), which is even.";

/// Three drafts of the learner.
const DRAFT1: &str = "n is even so n squared is even.";
const DRAFT2: &str = "Let n = 2k. Then n squared is 4k squared.";
const DRAFT3: &str = "Let n = 2k, so n^2 = 4k^2.";

/// One written-proof exemplar.
fn proof_exemplar(problem: &str, reference: &str) -> Exemplar {
    Exemplar {
        answer_contract: Some(AnswerContract::None),
        problem: problem.to_owned(),
        answer: "See the solution.".to_owned(),
        solution_sketch: Some(reference.to_owned()),
    }
}

/// The fixture curriculum of the module header.
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

/// The router: the fixture curriculum, the admin path, the rollover as named.
fn app(db: &TestDb, rollover: bool) -> Router {
    cadus_web::create_app(
        state_with_content(db, curriculum())
            .with_admin(Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS))
            .with_day_rollover(rollover),
    )
}

/// The live `kp2` proof of the lesson.
fn proof_problem() -> cadus_web::state::ServedProblem {
    let mut live = lesson_problem(5.0, "kp2", Vec::new());
    live.text = PROOF2.to_owned();
    live.expected.answer = "See the solution.".to_owned();
    live.expected.answer_contract = Some(AnswerContract::None);
    live.solution_sketch = Some(REF2.to_owned());
    live
}

/// One request as `user`, parsed, with the raw text beside it.
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

/// Submit one draft of the live lesson proof and read the job id.
async fn submit(app: &Router, user: Uuid, draft: &str) -> (Uuid, Value) {
    let reply = answer_task_ok(
        app,
        user,
        LESSON,
        json!({"problem_id": PROBLEM_ID, "answer": draft}),
    )
    .await;
    let id = Uuid::parse_str(reply["proof_grading"]["id"].as_str().unwrap()).unwrap();
    (id, reply)
}

/// Settle job `id` with a pass or a needs-revision grading, through the
/// worker's own landing (the `regraded` correction, the refold, the settle).
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

/// One column of one job row, read as admin.
async fn job_row(db: &TestDb, id: Uuid) -> (Option<Uuid>, i32, bool, bool, String) {
    sqlx::query_as(
        "SELECT revision_of, revision, rewrite, closed_at IS NOT NULL, context \
         FROM proof_grading_jobs WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&db.admin)
    .await
    .unwrap()
}

/// The owed proofs of `user`, as `(topic, kp)`.
async fn owed(db: &TestDb, user: Uuid) -> Vec<(String, String)> {
    sqlx::query_as("SELECT topic, kp FROM proof_owed WHERE user_id = $1 ORDER BY topic, kp")
        .bind(user)
        .fetch_all(&db.admin)
        .await
        .unwrap()
}

/// The XP of the two-point lesson closed at `tier`.
fn lesson_xp(tier: WorkQuality) -> f64 {
    let xp = cadus_core::xp::task_xp(TaskType::Lesson, tier, &Config::default(), 2, 0, false);
    (xp * 100.0).round() / 100.0
}

/// Pass the decided items of `kp1`: two correct answers, the second of which
/// holds the point for its proof. Gives the proof's problem id.
async fn pass_kp1_decided(app: &Router, user: Uuid) -> String {
    let reply = answer_task_ok(
        app,
        user,
        LESSON,
        json!({"problem_id": PROBLEM_ID, "answer": EXPECTED_ANSWER}),
    )
    .await;
    let next = reply["next"]["problem_id"].as_str().unwrap().to_owned();
    let reply = answer_task_ok(
        app,
        user,
        LESSON,
        json!({"problem_id": next, "answer": EXPECTED_ANSWER}),
    )
    .await;
    assert_eq!(reply["next"]["text"], PROOF1, "{reply}");
    reply["next"]["problem_id"].as_str().unwrap().to_owned()
}

/// The cached learner model of `user`.
async fn model_of(db: &TestDb, user: Uuid) -> Value {
    sqlx::query_scalar("SELECT model FROM learner_models WHERE user_id = $1")
        .bind(user)
        .fetch_one(&db.admin)
        .await
        .unwrap()
}

/// Needs-revision does not close the point; the resubmission links to the
/// draft it revises and is graded again; the pass closes the lesson with XP
/// and reaches the fold; the solution stays out of every payload until then.
#[tokio::test]
async fn a_lesson_proof_closes_on_a_pass_and_not_on_the_submission() {
    TestDb::with(|db| async move {
        let app = app(&db, false);
        let user = lesson_learner(&db, "pr-pass@example.test", proof_problem()).await;

        // A blank proof records nothing.
        let (status, body) = answer_task(
            &app,
            user,
            LESSON,
            json!({"problem_id": PROBLEM_ID, "answer": "  "}),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(body["error"]["code"], "proof_blank");

        // The first draft waits for its verdict: no close, no next problem.
        let (first, reply) = submit(&app, user, DRAFT1).await;
        assert_eq!(reply["task_status"], "proof_pending", "{reply}");
        assert_eq!(reply["proof"]["phase"], "grading");
        assert_eq!(reply["next"], Value::Null);
        assert!(reply.get("next_unavailable").is_none(), "{reply}");
        assert!(reply.get("xp").is_none());
        assert!(!reply.to_string().contains(REF2));
        assert!(events_of_type(&db, user, "lesson_result").await.is_empty());
        assert_eq!(job_row(&db, first).await.4, "lesson");

        // A second draft while the first is graded is refused.
        let (status, body) = answer_task(
            &app,
            user,
            LESSON,
            json!({"problem_id": PROBLEM_ID, "answer": DRAFT2}),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(body["error"]["code"], "proof_grading_pending");

        // Needs revision: a decided miss in the log, and the point stays open.
        land(&db, user, first, false).await;
        let corrections = events_of_type(&db, user, "regraded").await;
        assert_eq!(corrections.len(), 1);
        assert_eq!(corrections[0]["attempts"][0]["outcome"], "incorrect");
        assert!(events_of_type(&db, user, "lesson_result").await.is_empty());
        assert!(!stored_state(&db, user).await.tasks[LESSON].done);

        // The re-serve: the same problem, the feedback, the first unmet check
        // with its quote, the last draft — and no solution anywhere.
        let (status, raw) = serve_raw(&app, user, LESSON).await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        assert!(!raw.contains(REF2), "{raw}");
        let served = parse(&raw);
        assert_eq!(served["problem_id"], PROBLEM_ID);
        assert_eq!(served["proof"]["phase"], "revise");
        assert_eq!(served["proof"]["draft"], DRAFT1);
        assert_eq!(served["proof"]["revisions_left"], 2);
        assert_eq!(
            served["proof"]["feedback"],
            "Justify why n squared is even."
        );
        assert_eq!(served["proof"]["first_unmet"]["id"], "G3");
        assert_eq!(
            served["proof"]["first_unmet"]["evidence"],
            "so n squared is even"
        );
        let (status, poll, raw) = request(
            &app,
            Method::GET,
            &format!("/api/proof-grading/{first}"),
            user,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        assert_eq!(poll["status"], "needs_revision");
        assert_eq!(poll["chain"]["phase"], "revise");
        assert!(!raw.contains(REF2), "{raw}");

        // The resubmission is a new graded draft on the chain, linked to the
        // draft it revises; it appends no attempt event (one problem, one
        // scheduling signal).
        let (second, reply) = submit(&app, user, DRAFT2).await;
        assert_eq!(reply["task_status"], "proof_pending");
        assert_eq!(reply["proof"]["revision"], 1);
        assert_eq!(reply["attempt_id"], "s_2026-01-01a-lesson-addition-1-r1");
        assert_eq!(events_of_type(&db, user, "attempt").await.len(), 1);
        let (revision_of, revision, rewrite, closed, _) = job_row(&db, second).await;
        assert_eq!(
            (revision_of, revision, rewrite, closed),
            (Some(first), 1, false, false)
        );

        // Continue before the pass is refused.
        let (status, body, _) = request(
            &app,
            Method::POST,
            &format!("/api/task/{LESSON}/proof/continue"),
            user,
            Some(json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(body["error"]["code"], "proof_not_passed");

        // The pass: no second correction (the first draft was the signal),
        // and the poll now shows the solution.
        land(&db, user, second, true).await;
        assert_eq!(events_of_type(&db, user, "regraded").await.len(), 1);
        let (_, poll, _) = request(
            &app,
            Method::GET,
            &format!("/api/proof-grading/{second}"),
            user,
            None,
        )
        .await;
        assert_eq!(poll["status"], "pass");
        assert_eq!(poll["chain"]["phase"], "passed");
        assert_eq!(poll["solution"], REF2);

        // Continue closes the last point: the lesson passes with its XP, and
        // the fold reads the close (FIRe moves the topic to learning).
        let (status, closed, raw) = request(
            &app,
            Method::POST,
            &format!("/api/task/{LESSON}/proof/continue"),
            user,
            Some(json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        assert_eq!(closed["task_status"], "task_passed");
        // A pass reached through revision closes below a first-try pass.
        assert_eq!(
            closed["xp"],
            json!(lesson_xp(WorkQuality::Passable)),
            "{closed}"
        );
        let results = events_of_type(&db, user, "lesson_result").await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["passed"], true);
        assert_eq!(results[0]["quality_tier"], "passable");
        assert_ne!(results[0]["assisted"], true, "{}", results[0]);
        assert_eq!(events_of_type(&db, user, "regraded").await.len(), 1);
        assert_eq!(
            model_of(&db, user).await["topics"]["addition"]["status"],
            "learning"
        );
        assert!(stored_state(&db, user).await.tasks[LESSON].done);
        assert!(job_row(&db, second).await.3, "the chain closed");
    })
    .await;
}

/// The cap: after two revisions the solution is shown ONCE, then one
/// unaided rewrite closes the point as assisted, with less XP.
#[tokio::test]
async fn the_cap_shows_the_solution_once_and_the_rewrite_closes_assisted() {
    TestDb::with(|db| async move {
        let app = app(&db, false);
        let user = lesson_learner(&db, "pr-cap@example.test", proof_problem()).await;
        let mut head = Uuid::nil();
        for draft in [DRAFT1, DRAFT2, DRAFT3] {
            let (id, _) = submit(&app, user, draft).await;
            land(&db, user, id, false).await;
            head = id;
        }
        assert_eq!(job_row(&db, head).await.1, 2);

        // At the cap: no draft until the solution was read, and no solution
        // in the serve or the poll.
        let (status, raw) = serve_raw(&app, user, LESSON).await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        assert!(!raw.contains(REF2), "{raw}");
        assert_eq!(parse(&raw)["proof"]["phase"], "reveal");
        assert_eq!(parse(&raw)["proof"]["revisions_left"], 0);
        let (status, body) = answer_task(
            &app,
            user,
            LESSON,
            json!({"problem_id": PROBLEM_ID, "answer": DRAFT3}),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(body["error"]["code"], "proof_solution_unseen");
        let uri = format!("/api/proof-grading/{head}");
        let (_, _, raw) = request(&app, Method::GET, &uri, user, None).await;
        assert!(!raw.contains(REF2), "{raw}");

        // The one reveal.
        let seen = format!("/api/proof-grading/{head}/seen");
        let (status, body, raw) = request(&app, Method::POST, &seen, user, Some(json!({}))).await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        assert_eq!(body["chain"]["solution"], REF2);
        assert_eq!(body["chain"]["phase"], "rewrite");
        let (_, _, raw) = request(&app, Method::POST, &seen, user, Some(json!({}))).await;
        assert!(!raw.contains(REF2), "the solution shows once: {raw}");
        let (_, raw) = serve_raw(&app, user, LESSON).await;
        assert!(!raw.contains(REF2), "{raw}");
        assert_eq!(parse(&raw)["proof"]["phase"], "rewrite");

        // The unaided rewrite closes the last point as assisted.
        let (rewrite, reply) = submit(&app, user, "Let n = 2k; then n^2 = 2(2k^2).").await;
        assert_eq!(reply["task_status"], "task_passed", "{reply}");
        assert_eq!(reply["proof"]["phase"], "closed");
        let (revision_of, _, is_rewrite, closed, _) = job_row(&db, rewrite).await;
        assert_eq!((revision_of, is_rewrite, closed), (Some(head), true, true));
        assert_eq!(reply["xp"], json!(lesson_xp(WorkQuality::Passable)));
        let results = events_of_type(&db, user, "lesson_result").await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["assisted"], true);
        assert_eq!(results[0]["quality_tier"], "passable");
        // One problem, one signal: the first draft's attempt and its one
        // correction (a miss); the revisions and the rewrite stay on the chain.
        assert_eq!(events_of_type(&db, user, "attempt").await.len(), 1);
        let corrections = events_of_type(&db, user, "regraded").await;
        assert_eq!(corrections.len(), 1);
        assert_eq!(corrections[0]["attempts"][0]["outcome"], "incorrect");
        assert_eq!(
            corrections[0]["attempts"][0]["attempt_id"],
            "s_2026-01-01a-lesson-addition-1"
        );

        // The closed chain shows its solution.
        let (_, poll, _) = request(&app, Method::GET, &uri, user, None).await;
        assert_eq!(poll["solution"], REF2);
    })
    .await;
}

/// A mixed point: its decided items pass the standing rule, THEN its proof
/// is served; the proof's pass advances the lesson to the next point.
#[tokio::test]
async fn a_mixed_point_serves_its_proof_after_the_decided_items_pass() {
    TestDb::with(|db| async move {
        let app = app(&db, false);
        let user = lesson_learner(
            &db,
            "pr-mixed@example.test",
            lesson_problem(5.0, "kp1", Vec::new()),
        )
        .await;
        // Two correct decided answers pass `2consec`.
        let reply = answer_task_ok(
            &app,
            user,
            LESSON,
            json!({"problem_id": PROBLEM_ID, "answer": EXPECTED_ANSWER}),
        )
        .await;
        assert_eq!(reply["task_status"], "continue", "{reply}");
        let next = reply["next"]["problem_id"].as_str().unwrap().to_owned();
        assert_eq!(reply["next"]["text"], PROBLEM_TEXT);
        let reply = answer_task_ok(
            &app,
            user,
            LESSON,
            json!({"problem_id": next, "answer": EXPECTED_ANSWER}),
        )
        .await;
        // The point does not advance: its proof is served next, without its
        // solution.
        assert_eq!(reply["task_status"], "continue", "{reply}");
        assert_eq!(reply["next"]["text"], PROOF1);
        assert_eq!(reply["next"]["kp"], "kp1");
        assert_eq!(reply["next"]["proof"]["phase"], "draft");
        assert!(!reply.to_string().contains(REF1), "{reply}");
        let state = stored_state(&db, user).await;
        assert_eq!(
            owed(&db, user).await,
            [("addition".to_owned(), "kp1".to_owned())]
        );
        assert_eq!(state.tasks[LESSON].current_kp.as_deref(), Some("kp1"));

        // The proof's pass closes kp1 and advances the lesson to kp2, whose
        // proof (its one item) is served at once.
        let proof_id = reply["next"]["problem_id"].as_str().unwrap().to_owned();
        let reply = answer_task_ok(
            &app,
            user,
            LESSON,
            json!({"problem_id": proof_id, "answer": "Let a = 2k+1 and b = 2m+1."}),
        )
        .await;
        assert_eq!(reply["task_status"], "proof_pending");
        assert!(
            owed(&db, user).await.is_empty(),
            "the first draft opened the chain"
        );
        let id = Uuid::parse_str(reply["proof_grading"]["id"].as_str().unwrap()).unwrap();
        land(&db, user, id, true).await;
        let (status, closed, raw) = request(
            &app,
            Method::POST,
            &format!("/api/task/{LESSON}/proof/continue"),
            user,
            Some(json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        assert_eq!(closed["task_status"], "kp_advance");
        assert_eq!(closed["next"]["kp"], "kp2");
        assert_eq!(closed["next"]["text"], PROOF2);
        assert!(!raw.contains(REF2), "{raw}");
        let state = stored_state(&db, user).await;
        assert_eq!(state.tasks[LESSON].current_kp.as_deref(), Some("kp2"));
        assert!(events_of_type(&db, user, "lesson_result").await.is_empty());
    })
    .await;
}

/// The plan names the open revision (not the last attempt) and carries its
/// lesson FIRST into the next day's session; `seen_at` is recorded on the
/// job row; no restore panel field remains.
#[tokio::test]
async fn an_open_revision_carries_into_the_next_days_plan_first() {
    TestDb::with(|db| async move {
        let app = app(&db, true);
        let user = seed_learner(&db, "pr-carry@example.test").await;
        seed_open_session(&db, user).await;
        // Yesterday's draft of the kp2 proof, sent back for revision.
        let head: Uuid = sqlx::query_scalar(
            "INSERT INTO proof_grading_jobs (user_id, attempt_id, payload, context, status, result) \
             VALUES ($1, 's_2026-01-01a-lesson-addition-1', $2, 'lesson', 'done', $3) RETURNING id",
        )
        .bind(user)
        .bind(json!({"v": 1, "task_id": LESSON, "topic": "addition", "kp": "kp2",
                     "item_digest": "abc123def456", "problem": PROOF2, "reference": REF2,
                     "problem_hash": cadus_core::learner::problem_text_hash(PROOF2),
                     "given_answer": DRAFT1}))
        .bind(json!({"v": 1, "verdict": "needs_revision", "model": "m",
                     "feedback": "Justify the last step.", "checks": []}))
        .fetch_one(&db.admin)
        .await
        .unwrap();

        // The first start of the day rolls the stale session over.
        let (status, started, raw) =
            request(&app, Method::POST, "/api/session/start", user, None).await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        let today = started["session"].as_str().unwrap().to_owned();
        assert_ne!(today, "s_2026-01-01a");

        let (status, plan, raw) =
            request(&app, Method::GET, "/api/session/plan", user, None).await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        let first = &plan["tasks"][0];
        let task = format!("{today}-lesson-addition");
        assert_eq!(first["task_id"], task.as_str(), "{plan}");
        assert_eq!(first["start_at_kp"], "kp2");
        assert_eq!(first["proof_revision"]["job_id"], head.to_string());
        assert_eq!(first["proof_revision"]["phase"], "revise");
        assert_eq!(plan["open_revisions"].as_array().unwrap().len(), 1);
        assert_eq!(plan["open_revisions"][0]["seen"], false);
        assert!(plan.get("proof_grading").is_none(), "{plan}");

        // The verdict seen: the job row records it, and the plan reads it.
        let (status, _, raw) = request(
            &app,
            Method::POST,
            &format!("/api/proof-grading/{head}/seen"),
            user,
            Some(json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        let seen: bool = sqlx::query_scalar(
            "SELECT seen_at IS NOT NULL FROM proof_grading_jobs WHERE id = $1",
        )
        .bind(head)
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert!(seen);
        let (_, plan, _) = request(&app, Method::GET, "/api/session/plan", user, None).await;
        assert_eq!(plan["open_revisions"][0]["seen"], true);

        // The carried lesson serves the chain's own problem, in revision.
        let (status, raw) = serve_raw(&app, user, &task).await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        assert!(!raw.contains(REF2), "{raw}");
        let served = parse(&raw);
        assert_eq!(served["text"], PROOF2);
        assert_eq!(served["proof"]["phase"], "revise");
        assert_eq!(served["proof"]["draft"], DRAFT1);
    })
    .await;
}

/// "This grade is wrong": the attempt joins the admin recovery list, and a
/// human pass there closes the point on Continue.
#[tokio::test]
async fn a_disputed_grade_reaches_the_admin_regrade() {
    TestDb::with(|db| async move {
        let app = app(&db, false);
        let user = lesson_learner(&db, "pr-dispute@example.test", proof_problem()).await;
        sqlx::query("UPDATE users SET is_admin = true WHERE id = $1")
            .bind(user)
            .execute(&db.admin)
            .await
            .unwrap();
        let (id, reply) = submit(&app, user, DRAFT2).await;
        let attempt = reply["attempt_id"].as_str().unwrap().to_owned();
        let dispute = format!("/api/proof-grading/{id}/dispute");
        // Nothing to dispute while it is graded.
        let (status, body, _) = request(&app, Method::POST, &dispute, user, Some(json!({}))).await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        land(&db, user, id, false).await;
        let (status, body, raw) = request(
            &app,
            Method::POST,
            &dispute,
            user,
            Some(json!({"note": "Step 2 is justified by the definition."})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        assert_eq!(body["disputed"], true);

        let (status, list, raw) =
            request(&app, Method::GET, "/api/admin/ungraded", user, None).await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        assert_eq!(list["disputed"][0]["attempt_id"], attempt.as_str());
        assert_eq!(list["disputed"][0]["verdict"], "needs_revision");
        assert_eq!(
            list["disputed"][0]["note"],
            "Step 2 is justified by the definition."
        );

        let (status, _, raw) = request(
            &app,
            Method::POST,
            &format!("/api/admin/ungraded/{attempt}/regrade"),
            user,
            Some(json!({"outcome": "correct"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        let (_, poll, _) = request(
            &app,
            Method::GET,
            &format!("/api/proof-grading/{id}"),
            user,
            None,
        )
        .await;
        assert_eq!(poll["status"], "pass");
        let (_, list, _) = request(&app, Method::GET, "/api/admin/ungraded", user, None).await;
        assert_eq!(list["disputed"].as_array().unwrap().len(), 0);
        let (status, closed, raw) = request(
            &app,
            Method::POST,
            &format!("/api/task/{LESSON}/proof/continue"),
            user,
            Some(json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        assert_eq!(closed["task_status"], "task_passed");
    })
    .await;
}

/// A review proof runs no blocking loop: it is revised from the proofs list,
/// regraded, and shows its solution only once it passes.
#[tokio::test]
async fn a_review_proof_is_revised_from_the_proofs_list() {
    TestDb::with(|db| async move {
        let app = app(&db, false);
        let user = seed_learner(&db, "pr-review@example.test").await;
        seed_open_session(&db, user).await;
        put_state(&db, user, &cadus_web::state::WebState::for_session("s_2026-01-01a")).await;
        let root: Uuid = sqlx::query_scalar(
            "INSERT INTO proof_grading_jobs (user_id, attempt_id, payload, context, status, result) \
             VALUES ($1, 's_2026-01-01a-review-addition-1', $2, 'review', 'done', $3) RETURNING id",
        )
        .bind(user)
        .bind(json!({"v": 1, "task_id": "s_2026-01-01a-review-addition", "topic": "addition",
                     "kp": "kp2", "item_digest": "abc123def456", "problem": PROOF2,
                     "reference": REF2, "given_answer": DRAFT1}))
        .bind(json!({"v": 1, "verdict": "needs_revision", "model": "m",
                     "feedback": "Justify the last step.", "checks": []}))
        .fetch_one(&db.admin)
        .await
        .unwrap();

        let (status, list, raw) =
            request(&app, Method::GET, "/api/proofs?topic=addition", user, None).await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        assert_eq!(list["chains"][0]["phase"], "revise");
        assert_eq!(list["chains"][0]["topic_name"], "The addition topic");
        assert!(!raw.contains(REF2), "{raw}");

        let (status, revised, raw) = request(
            &app,
            Method::POST,
            &format!("/api/proofs/{root}/revise"),
            user,
            Some(json!({"answer": DRAFT2})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        let next = Uuid::parse_str(revised["proof_grading"]["id"].as_str().unwrap()).unwrap();
        assert_eq!(job_row(&db, next).await.0, Some(root));
        assert_eq!(revised["chain"]["phase"], "grading");
        // The head moved on: the old head takes no second draft.
        let (status, _, _) = request(
            &app,
            Method::POST,
            &format!("/api/proofs/{root}/revise"),
            user,
            Some(json!({"answer": DRAFT3})),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        land(&db, user, next, true).await;
        // No attempt stands for a draft of the list, so no correction lands.
        assert!(events_of_type(&db, user, "regraded").await.is_empty());
        let (_, list, _) = request(&app, Method::GET, "/api/proofs", user, None).await;
        assert_eq!(list["chains"][0]["phase"], "passed");
        assert_eq!(list["chains"][0]["solution"], REF2);
        assert_eq!(list["chains"][0]["versions"].as_array().unwrap().len(), 2);
    })
    .await;
}

/// A free explanation (no proof asked) keeps the note 84 (b) self-check
/// completion: the answer completes the point at once.
#[tokio::test]
async fn a_free_explanation_keeps_the_self_check_completion() {
    TestDb::with(|db| async move {
        let graph = one_unit_curriculum(vec![topic(
            "addition",
            vec![kp(
                "kp1",
                vec![Exemplar {
                    answer_contract: Some(AnswerContract::None),
                    problem: "Explain why 0 is even.".to_owned(),
                    answer: "See the solution.".to_owned(),
                    solution_sketch: Some("0 = 2 * 0.".to_owned()),
                }],
            )],
        )]);
        let app = cadus_web::create_app(state_with_content(&db, graph));
        let mut live = lesson_problem(5.0, "kp1", Vec::new());
        live.text = "Explain why 0 is even.".to_owned();
        live.expected.answer = "See the solution.".to_owned();
        live.expected.answer_contract = Some(AnswerContract::None);
        let user = lesson_learner(&db, "pr-explain@example.test", live).await;
        let reply = answer_task_ok(
            &app,
            user,
            LESSON,
            json!({"problem_id": PROBLEM_ID, "answer": "Because 0 = 2 times 0."}),
        )
        .await;
        assert_eq!(reply["task_status"], "task_passed", "{reply}");
        assert!(reply.get("proof").is_none());
        let id = Uuid::parse_str(reply["proof_grading"]["id"].as_str().unwrap()).unwrap();
        assert_eq!(job_row(&db, id).await.4, "selfcheck");
    })
    .await;
}

/// A first-draft pass is a normal correct: one correction (correct) on the
/// first draft's attempt, and the point closes at the pass tier.
#[tokio::test]
async fn a_first_draft_pass_closes_at_the_pass_tier() {
    TestDb::with(|db| async move {
        let app = app(&db, false);
        let user = lesson_learner(&db, "pr-first@example.test", proof_problem()).await;
        let (id, _) = submit(&app, user, DRAFT3).await;
        land(&db, user, id, true).await;
        let (status, closed, raw) = request(
            &app,
            Method::POST,
            &format!("/api/task/{LESSON}/proof/continue"),
            user,
            Some(json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        assert_eq!(closed["task_status"], "task_passed");
        assert_eq!(closed["xp"], json!(lesson_xp(WorkQuality::NearlyPerfect)));
        assert_eq!(events_of_type(&db, user, "attempt").await.len(), 1);
        let corrections = events_of_type(&db, user, "regraded").await;
        assert_eq!(corrections.len(), 1);
        assert_eq!(corrections[0]["attempts"][0]["outcome"], "correct");
        let results = events_of_type(&db, user, "lesson_result").await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["quality_tier"], "nearly_perfect");
        assert_ne!(results[0]["assisted"], true);
    })
    .await;
}

/// The proof a point owes outlives the D-S6 row: after a session end the
/// next session's plan carries the lesson FIRST at the point, and its serve
/// is the proof — no decided item is answered again.
#[tokio::test]
async fn an_owed_proof_survives_a_session_end() {
    TestDb::with(|db| async move {
        let app = app(&db, false);
        let user = lesson_learner(
            &db,
            "pr-owed-end@example.test",
            lesson_problem(5.0, "kp1", Vec::new()),
        )
        .await;
        pass_kp1_decided(&app, user).await;
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
        assert_eq!(plan["tasks"][0]["task_id"], task.as_str(), "{plan}");
        assert_eq!(plan["tasks"][0]["start_at_kp"], "kp1");
        assert_eq!(plan["tasks"][0]["proof_revision"]["phase"], "draft");
        let (status, raw) = serve_raw(&app, user, &task).await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        assert_eq!(parse(&raw)["text"], PROOF1);
        assert_eq!(parse(&raw)["proof"]["phase"], "draft");
        assert!(!raw.contains(REF1), "{raw}");
        assert_eq!(events_of_type(&db, user, "attempt").await.len(), 2);
    })
    .await;
}

/// The same across the day rollover: yesterday's owed proof comes first today.
#[tokio::test]
async fn an_owed_proof_survives_the_day_rollover() {
    TestDb::with(|db| async move {
        let app = app(&db, true);
        let user = seed_learner(&db, "pr-owed-roll@example.test").await;
        seed_open_session(&db, user).await;
        sqlx::query("INSERT INTO proof_owed (user_id, topic, kp) VALUES ($1, 'addition', 'kp1')")
            .bind(user)
            .execute(&db.admin)
            .await
            .unwrap();
        let (status, started, raw) =
            request(&app, Method::POST, "/api/session/start", user, None).await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        let today = started["session"].as_str().unwrap().to_owned();
        assert_ne!(today, "s_2026-01-01a");
        let task = format!("{today}-lesson-addition");
        let (_, plan, _) = request(&app, Method::GET, "/api/session/plan", user, None).await;
        assert_eq!(plan["tasks"][0]["task_id"], task.as_str(), "{plan}");
        assert_eq!(plan["open_revisions"][0]["phase"], "draft");
        let (status, raw) = serve_raw(&app, user, &task).await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        assert_eq!(parse(&raw)["text"], PROOF1);
    })
    .await;
}

/// A grading the daily cap refused never strands the point: the learner may
/// dispute it (the admin path sees it), resubmit for free, or leave it — the
/// plan keeps carrying it; a human pass then closes the point.
#[tokio::test]
async fn a_capped_grading_can_be_disputed_and_never_strands_the_point() {
    TestDb::with(|db| async move {
        let app = app(&db, false);
        let user = lesson_learner(&db, "pr-capped@example.test", proof_problem()).await;
        sqlx::query("UPDATE users SET is_admin = true WHERE id = $1")
            .bind(user)
            .execute(&db.admin)
            .await
            .unwrap();
        let (id, reply) = submit(&app, user, DRAFT1).await;
        let attempt = reply["attempt_id"].as_str().unwrap().to_owned();
        sqlx::query(
            "UPDATE proof_grading_jobs SET status = 'capped', finished_at = now() WHERE id = $1",
        )
        .bind(id)
        .execute(&db.admin)
        .await
        .unwrap();
        let (_, raw) = serve_raw(&app, user, LESSON).await;
        assert_eq!(parse(&raw)["proof"]["phase"], "unavailable");
        let (_, plan, _) = request(&app, Method::GET, "/api/session/plan", user, None).await;
        assert_eq!(plan["tasks"][0]["proof_revision"]["phase"], "unavailable");

        let (status, body, raw) = request(
            &app,
            Method::POST,
            &format!("/api/proof-grading/{id}/dispute"),
            user,
            Some(json!({"note": "The daily limit stopped the check."})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        assert_eq!(body["status"], "capped");
        let (_, list, _) = request(&app, Method::GET, "/api/admin/ungraded", user, None).await;
        assert_eq!(list["disputed"][0]["attempt_id"], attempt.as_str());
        let (status, _, raw) = request(
            &app,
            Method::POST,
            &format!("/api/admin/ungraded/{attempt}/regrade"),
            user,
            Some(json!({"outcome": "correct"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        let (status, closed, raw) = request(
            &app,
            Method::POST,
            &format!("/api/task/{LESSON}/proof/continue"),
            user,
            Some(json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        assert_eq!(closed["task_status"], "task_passed");
    })
    .await;
}

/// A capped grading takes a free resubmission (no revision used).
#[tokio::test]
async fn a_capped_grading_takes_a_free_resubmission() {
    TestDb::with(|db| async move {
        let app = app(&db, false);
        let user = lesson_learner(&db, "pr-retry@example.test", proof_problem()).await;
        let (id, _) = submit(&app, user, DRAFT1).await;
        sqlx::query("UPDATE proof_grading_jobs SET status = 'capped' WHERE id = $1")
            .bind(id)
            .execute(&db.admin)
            .await
            .unwrap();
        let (retry, reply) = submit(&app, user, DRAFT1).await;
        assert_eq!(reply["task_status"], "proof_pending");
        let (revision_of, revision, _, _, _) = job_row(&db, retry).await;
        assert_eq!((revision_of, revision), (Some(id), 0));
        // The retry's verdict is the chain's first: it folds onto the root.
        land(&db, user, retry, false).await;
        let corrections = events_of_type(&db, user, "regraded").await;
        assert_eq!(corrections.len(), 1);
        assert_eq!(
            corrections[0]["attempts"][0]["attempt_id"],
            "s_2026-01-01a-lesson-addition-1"
        );
    })
    .await;
}
