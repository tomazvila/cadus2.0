//! `GET /api/reports` (the learner list) and `GET /api/admin/reports` (the owner list).
//!
//! The rows come from direct inserts through the admin pool, so each test controls
//! the status, the result, and the owner of every report.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod common;
use common::admin::*;
use common::{SESSION_TOKEN_ONE, SESSION_TOKEN_TWO, get_bearer, send};

const MINE: &str = "/api/reports";
const ADMIN: &str = "/api/admin/reports";

/// Insert one report of `user`. `n` makes the request id and the hash unique.
async fn insert(db: &TestDb, user: Uuid, n: u32, status: &str, result: Option<Value>) -> Uuid {
    let input = json!({
        "note": format!("note {n}"),
        "report_identity": {"topic": "band", "task_id": "t-1"},
        "attempt": {"kp": "kp1"},
        "source": {"problem": {"text": format!("Question {n}?")}},
    });
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO problem_reports(user_id,request_id,task_id,problem_id,attempt_id,source_hash,
                                     input,status,stage,result,created_at)
         VALUES($1,gen_random_uuid(),'t-1',$2,$3,$4,$5,$6,'adjudicator',$7,
                now() + make_interval(secs => $8)) RETURNING id",
    )
    .bind(user)
    .bind(format!("p{n}"))
    .bind(format!("a{n}"))
    .bind(format!("{n:064x}"))
    .bind(input)
    .bind(status)
    .bind(result)
    .bind(f64::from(n))
    .fetch_one(&db.admin)
    .await
    .unwrap();
    id
}

fn done(resolution: &str) -> Value {
    json!({"resolution": resolution, "message": "m", "explanation": "Because five.",
           "corrected_answer": "5", "grade_corrected": resolution == "regraded",
           "qwen_verdict": "correct", "verification": "model_judged",
           "content_published": false})
}

#[tokio::test]
async fn the_learner_list_needs_a_session() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let answer = send(&app, common::get(MINE)).await;
        assert_eq!(answer.status.as_u16(), 401, "{}", answer.body);
    })
    .await;
}

#[tokio::test]
async fn the_learner_list_holds_only_own_reports_newest_first() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let me = seed_admin(&db).await;
        let other = seed_account(&db, "other@example.test", SESSION_TOKEN_TWO.1, false).await;
        insert(&db, me, 1, "completed", Some(done("regraded"))).await;
        insert(&db, me, 2, "queued", None).await;
        insert(&db, other, 3, "completed", Some(done("no_issue_found"))).await;
        let answer = send(&app, get_bearer(MINE, SESSION_TOKEN_ONE.0)).await;
        assert_eq!(answer.status.as_u16(), 200, "{}", answer.body);
        let rows = answer.body["reports"].as_array().unwrap().clone();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["question"], "Question 2?");
        assert_eq!(rows[0]["status"], "queued");
        assert!(rows[0]["outcome"].is_null());
        assert_eq!(rows[1]["question"], "Question 1?");
        assert_eq!(rows[1]["outcome"], "regraded");
        assert_eq!(rows[1]["grade_corrected"], true);
        assert_eq!(rows[1]["explanation"], "Because five.");
        assert_eq!(rows[1]["corrected_answer"], "5");
        assert_eq!(rows[1]["note"], "note 1");
        assert_eq!(rows[1]["topic"], "band");
        // The other learner sees the one report of that learner and nothing of mine.
        let theirs = send(&app, get_bearer(MINE, SESSION_TOKEN_TWO.0)).await;
        let rows = theirs.body["reports"].as_array().unwrap().clone();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["outcome"], "no_issue_found");
        assert!(!theirs.body.to_string().contains("Question 1?"));
    })
    .await;
}

#[tokio::test]
async fn the_learner_list_maps_unresolved_and_failed_and_stops_at_fifty() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let me = seed_admin(&db).await;
        insert(&db, me, 1, "unresolved", Some(done("needs_review"))).await;
        insert(&db, me, 2, "failed", None).await;
        for n in 3..60 {
            insert(&db, me, n, "queued", None).await;
        }
        let answer = send(&app, get_bearer(MINE, SESSION_TOKEN_ONE.0)).await;
        let rows = answer.body["reports"].as_array().unwrap().clone();
        assert_eq!(rows.len(), 50);
        let all = send(&app, get_bearer(MINE, SESSION_TOKEN_ONE.0)).await.body;
        assert!(all["reports"][0]["private"].is_null());
        assert!(all["reports"][0].get("input").is_none());
        // The two oldest rows fall outside the limit; check their words on a short list.
        sqlx::query("DELETE FROM problem_reports WHERE problem_id NOT IN ('p1','p2')")
            .execute(&db.admin)
            .await
            .unwrap();
        let rows = send(&app, get_bearer(MINE, SESSION_TOKEN_ONE.0)).await.body;
        assert_eq!(rows["reports"][0]["outcome"], "failed");
        assert_eq!(rows["reports"][1]["outcome"], "needs_review");
    })
    .await;
}

#[tokio::test]
async fn the_admin_list_refuses_without_a_session_and_for_a_learner() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let none = send(&app, common::get(ADMIN)).await;
        assert_eq!(none.status.as_u16(), 401, "{}", none.body);
        seed_account(&db, "learner@example.test", SESSION_TOKEN_TWO.1, false).await;
        let learner = send(&app, get_bearer(ADMIN, SESSION_TOKEN_TWO.0)).await;
        assert_eq!(learner.status.as_u16(), 403, "{}", learner.body);
        assert_eq!(learner.code(), "forbidden");
    })
    .await;
}

#[tokio::test]
async fn the_admin_list_shows_unresolved_reports_of_every_learner() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let owner = seed_admin(&db).await;
        let other = seed_account(&db, "other@example.test", SESSION_TOKEN_TWO.1, false).await;
        let mut stuck = done("needs_review");
        stuck["message"] = json!("The check did not settle this.");
        let stuck_id = insert(&db, other, 1, "unresolved", Some(stuck)).await;
        insert(&db, owner, 2, "completed", Some(done("no_issue_found"))).await;
        sqlx::query(
            "INSERT INTO problem_report_steps(report_id,stage,evidence) VALUES($1,'verification',$2)",
        )
        .bind(stuck_id)
        .bind(json!({"message": "Lean could not prove it."}))
        .execute(&db.admin)
        .await
        .unwrap();
        let answer = admin_get(&app, ADMIN).await;
        assert_eq!(answer.status.as_u16(), 200, "{}", answer.body);
        let rows = answer.body["reports"].as_array().unwrap().clone();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["user_id"], other.to_string());
        assert_eq!(rows[0]["question"], "Question 1?");
        assert_eq!(rows[0]["note"], "note 1");
        assert_eq!(rows[0]["topic"], "band");
        assert_eq!(rows[0]["kp"], "kp1");
        assert_eq!(rows[0]["result_message"], "The check did not settle this.");
        assert_eq!(rows[0]["last_step"]["stage"], "verification");
        assert_eq!(rows[0]["last_step"]["message"], "Lean could not prove it.");
        let all = admin_get(&app, &format!("{ADMIN}?status=all")).await.body;
        assert_eq!(all["reports"].as_array().unwrap().len(), 2);
        let bad = admin_get(&app, &format!("{ADMIN}?status=done")).await;
        assert_eq!(bad.status.as_u16(), 422, "{}", bad.body);
    })
    .await;
}

#[tokio::test]
async fn the_admin_list_answers_503_without_an_admin_connection() {
    TestDb::with(|db| async move {
        let app = app_without_admin(&db);
        seed_admin(&db).await;
        let answer = admin_get(&app, ADMIN).await;
        assert_eq!(answer.status.as_u16(), 503, "{}", answer.body);
        assert_eq!(answer.code(), "admin_path_unavailable");
    })
    .await;
}
