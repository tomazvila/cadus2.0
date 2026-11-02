//! D-RP2: a learner says a question is unclear or wrong. The flag sets the question
//! aside, starts no model review, reaches the owner, and the owner keeps or retires it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_core::learner::problem_text_hash;
use common::admin::{admin_get, admin_post, app as admin_app, seed_account, seed_admin};
use common::*;

const ORIGINAL: &str = "Compute 8 - 5.";

fn original_problem() -> cadus_web::state::ServedProblem {
    let mut live = lesson_problem(5.0, "kp1", Vec::new());
    live.text = ORIGINAL.to_owned();
    live.expected.answer = "4".to_owned();
    live
}

async fn handoff(db: &TestDb, user: Uuid) {
    seed_event(
        db,
        user,
        2,
        BASE_US,
        SESSION,
        json!({
            "type":"ordinary_problem_served","v":2,"ts":"2026-01-01T00:00:00Z",
            "session":SESSION,"task_id":LESSON,"problem_id":PROBLEM_ID,
            "kp_id":"addition/kp1","item_digest":"a".repeat(64),
            "item_source":"exemplar","exposure":"first"
        }),
    )
    .await;
}

async fn learner(db: &TestDb, email: &str) -> (Uuid, String) {
    let user = lesson_learner(db, email, original_problem()).await;
    handoff(db, user).await;
    let id = format!("{LESSON}-submitted");
    let mut verdict = a_miss();
    verdict.given_answer = "3";
    let event = attempt_payload(LESSON, &id, "kp1", (ORIGINAL, "4"), &verdict);
    seed_attempt_row(db, user, 3, &id, &event).await;
    (user, id)
}

fn body(attempt: &str) -> Value {
    json!({"request_id": Uuid::new_v4().to_string(), "problem_id": PROBLEM_ID,
           "attempt_id": attempt, "concern": "question_unclear", "note": "What does it ask?"})
}

async fn post(app: &Router, user: Uuid, body: Value) -> (StatusCode, Value) {
    let (status, text) = call(
        app,
        Method::POST,
        &format!("/api/task/{LESSON}/report"),
        Some(user),
        Some(body),
    )
    .await;
    (status, parse(&text))
}

#[tokio::test]
async fn an_unclear_question_waits_for_the_owner_and_starts_no_review() {
    TestDb::with(|db| async move {
        let app = lesson_app(&db);
        let (user, attempt) = learner(&db, "flag-owner@example.com").await;
        let other = seed_learner(&db, "flag-other@example.com").await;
        let request = body(&attempt);
        let (status, first) = post(&app, user, request.clone()).await;
        assert_eq!(status, StatusCode::OK, "{first}");
        assert_eq!(first["status"], "waiting_owner");
        assert_eq!(first["set_aside"], true);
        assert_eq!(first["retryable"], false);
        // The same request answers the same flag.
        let (_, again) = post(&app, user, request).await;
        assert_eq!(again["report_id"], first["report_id"]);
        let flags: i64 = sqlx::query_scalar("SELECT count(*) FROM question_flags")
            .fetch_one(&db.admin)
            .await
            .unwrap();
        let jobs: i64 = sqlx::query_scalar("SELECT count(*) FROM problem_reports")
            .fetch_one(&db.admin)
            .await
            .unwrap();
        assert_eq!((flags, jobs), (1, 0), "a flag must start no model review");
        let id = first["report_id"].as_str().unwrap();
        let path = format!("/api/reports/{id}");
        let (status, text) = call(&app, Method::GET, &path, Some(user), None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(parse(&text)["status"], "waiting_owner");
        assert_eq!(
            call(&app, Method::GET, &path, Some(other), None).await.0,
            StatusCode::NOT_FOUND
        );
        let (_, list) = call(&app, Method::GET, "/api/reports", Some(user), None).await;
        let rows = parse(&list)["reports"].as_array().unwrap().clone();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["kind"], "question_unclear");
        assert_eq!(rows[0]["status"], "waiting_owner");
        let (_, theirs) = call(&app, Method::GET, "/api/reports", Some(other), None).await;
        assert!(parse(&theirs)["reports"].as_array().unwrap().is_empty());
    })
    .await;
}

#[tokio::test]
async fn an_unknown_concern_and_an_unsupported_kind_are_refused() {
    TestDb::with(|db| async move {
        let app = lesson_app(&db);
        let (user, attempt) = learner(&db, "flag-bad@example.com").await;
        let mut bad = body(&attempt);
        bad["concern"] = json!("other");
        assert_eq!(
            post(&app, user, bad).await.0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let mut integrated = body(&attempt);
        integrated["report_kind"] = json!("diagnostic");
        assert_eq!(
            post(&app, user, integrated).await.0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    })
    .await;
}

#[tokio::test]
async fn the_serve_skips_a_retired_question_and_a_set_aside_question() {
    TestDb::with(|db| async move {
        let app = drill_app(&db);
        // Retired for everyone: the other authored question always serves.
        sqlx::query("INSERT INTO retired_items(problem_hash) VALUES($1)")
            .bind(problem_text_hash(PROBLEM_TEXT))
            .execute(&db.admin)
            .await
            .unwrap();
        for n in 0..4 {
            let user = seed_learner(&db, &format!("retired{n}@example.com")).await;
            seed_open_session(&db, user).await;
            let served = serve_ok(&app, user, LESSON).await;
            assert_eq!(served["text"], EXEMPLAR_TEXT_2);
        }
        // Set aside for one learner only: the flag of another learner changes nothing.
        sqlx::query("DELETE FROM retired_items")
            .execute(&db.admin)
            .await
            .unwrap();
        for n in 0..4 {
            let user = seed_learner(&db, &format!("aside{n}@example.com")).await;
            seed_open_session(&db, user).await;
            sqlx::query(
                "INSERT INTO question_flags(user_id,request_id,task_id,problem_id,problem_hash,question)
                 VALUES($1,gen_random_uuid(),'t','p',$2,'q')",
            )
            .bind(user)
            .bind(problem_text_hash(PROBLEM_TEXT))
            .execute(&db.admin)
            .await
            .unwrap();
            let served = serve_ok(&app, user, LESSON).await;
            assert_eq!(served["text"], EXEMPLAR_TEXT_2);
        }
    })
    .await;
}

#[tokio::test]
async fn the_owner_keeps_or_retires_a_flagged_question() {
    TestDb::with(|db| async move {
        let app = admin_app(&db);
        let owner = seed_admin(&db).await;
        let learner = seed_account(&db, "flagger@example.test", SESSION_TOKEN_TWO.1, false).await;
        let mut ids = Vec::new();
        for (n, hash) in ["h-keep", "h-retire"].iter().enumerate() {
            let id: Uuid = sqlx::query_scalar(
                "INSERT INTO question_flags(user_id,request_id,task_id,problem_id,problem_hash,topic,kp,question,note)
                 VALUES($1,gen_random_uuid(),'t-1',$2,$3,'band','kp1','Question?','unclear') RETURNING id",
            )
            .bind(learner)
            .bind(format!("p{n}"))
            .bind(hash)
            .fetch_one(&db.admin)
            .await
            .unwrap();
            ids.push(id);
        }
        let list = admin_get(&app, "/api/admin/reports").await;
        assert_eq!(list.status.as_u16(), 200, "{}", list.body);
        let rows = list.body["reports"].as_array().unwrap().clone();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["kind"], "question_unclear");
        assert_eq!(rows[0]["status"], "waiting_owner");
        assert_eq!(rows[0]["kp"], "kp1");
        // A learner cannot decide.
        let denied = common::send(
            &app,
            common::post_bearer(
                &format!("/api/admin/reports/{}/keep", ids[0]),
                SESSION_TOKEN_TWO.0,
                &json!({}),
            ),
        )
        .await;
        assert_eq!(denied.status.as_u16(), 403);
        let kept = admin_post(&app, &format!("/api/admin/reports/{}/keep", ids[0]), &json!({})).await;
        assert_eq!(kept.status.as_u16(), 200, "{}", kept.body);
        let retired =
            admin_post(&app, &format!("/api/admin/reports/{}/retire", ids[1]), &json!({})).await;
        assert_eq!(retired.status.as_u16(), 200, "{}", retired.body);
        let again = admin_post(&app, &format!("/api/admin/reports/{}/keep", ids[0]), &json!({})).await;
        assert_eq!(again.status.as_u16(), 404);
        let waiting = admin_get(&app, "/api/admin/reports").await;
        assert!(waiting.body["reports"].as_array().unwrap().is_empty());
        let all = admin_get(&app, "/api/admin/reports?status=all").await;
        assert_eq!(all.body["reports"].as_array().unwrap().len(), 2);
        let hashes: Vec<String> = sqlx::query_scalar("SELECT problem_hash FROM retired_items")
            .fetch_all(&db.admin)
            .await
            .unwrap();
        assert_eq!(hashes, vec!["h-retire".to_owned()]);
        let _ = owner;
    })
    .await;
}
