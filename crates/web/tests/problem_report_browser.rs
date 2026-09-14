//! Manual signed-in browser fixture on a disposable database.
//! Run with CADUS_TEST_DATABASE_URL and --ignored --nocapture.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod common;
use common::*;
use cadus_core::event::{Timestamp, TopicStatus};
use cadus_core::learner::{LearnerModel, TopicState};
use cadus_web::auth::password::{Argon2Profile, hash_password};
use std::collections::BTreeMap;
use std::time::Duration;

const EMAIL: &str = "report-browser@example.test";
const PASSWORD: &str = "Cadus-browser-report-2026";
const QUESTION: &str = "Compute 8 - 5.";

#[tokio::test]
#[ignore = "manual browser fixture; waits for the stop file"]
async fn signed_in_problem_report_browser() {
    TestDb::with(|db| async move {
        let graph = one_unit_curriculum(vec![topic("addition", vec![kp("kp1", vec![
            exemplar_with_solution(QUESTION, "4", "The stored answer is four."),
            exemplar_with_solution("Compute 9 - 5.", "4", "Subtract five from nine."),
        ])])]);
        let user = seed_learner(&db, EMAIL).await;
        mark_verified(&db, EMAIL).await;
        let password_hash = hash_password(Argon2Profile::PROD, PASSWORD).unwrap();
        sqlx::query("UPDATE users SET password_hash=$1 WHERE id=$2")
            .bind(password_hash).bind(user).execute(&db.admin).await.unwrap();
        seed_open_session(&db, user).await;
        seed_typed_event(&db, user, 2, &sessions::enrolled_c1()).await;
        seed_event(&db, user, 3, BASE_US + 2, SESSION, json!({
            "type":"diagnostic_placed","v":2,"ts":"2026-01-01T00:00:00Z",
            "session":SESSION,"balances":{},"conditional":[],"refresh":false
        })).await;
        let mut topics = BTreeMap::new();
        topics.insert("addition".to_owned(), TopicState {
            status: TopicStatus::Learning, rep_num: 1.0, memory_base: 1.0,
            t0: Some(Timestamp::from_micros(BASE_US - 400 * 86_400_000_000)),
            interval_days: 1.0, ability: 0.6, ..TopicState::default()
        });
        seed_cached_model(&db, user, &LearnerModel { topics, ..LearnerModel::default() }, 3).await;
        let mut live = lesson_problem(5.0, "kp1", Vec::new());
        live.task_id = REVIEW.to_owned();
        live.text = QUESTION.to_owned();
        live.expected.answer = "4".to_owned();
        live.solution_sketch = Some("The stored answer is four.".to_owned());
        let mut scratch = lesson_state(live, 0, false);
        let progress = scratch.tasks.get_mut(REVIEW).unwrap();
        progress.task_type = "review".to_owned();
        progress.total = 4;
        put_state(&db, user, &scratch).await;
        seed_event(&db, user, 4, BASE_US + 3, SESSION, json!({
            "type":"ordinary_problem_served","v":2,"ts":"2026-01-01T00:00:00Z",
            "session":SESSION,"task_id":REVIEW,"problem_id":PROBLEM_ID,
            "kp_id":"addition/kp1","item_digest":"a".repeat(64),
            "item_source":"exemplar","exposure":"first"
        })).await;
        let app = app_with_content(&db, graph);
        let status = status_body(&app, user).await;
        assert_eq!(status["placed"], true, "{status}");
        let plan = plan_body(&app, user).await;
        assert!(plan["tasks"].as_array().unwrap().iter().any(|task| task["task_id"] == REVIEW), "{plan}");
        let port = std::env::var("CADUS_REPORT_BROWSER_PORT").unwrap_or_else(|_| "4380".to_owned());
        let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}")).await.unwrap();
        let manifest = json!({"database_url":db.superuser_dsn(),"database":db.name,
            "user_id":user,"email":EMAIL,"password":PASSWORD,"task_id":REVIEW,
            "problem_id":PROBLEM_ID,"api_port":port});
        std::fs::write("/tmp/cadus-report-browser.json", manifest.to_string()).unwrap();
        println!("Browser fixture ready on 127.0.0.1:{port}; manifest=/tmp/cadus-report-browser.json");
        axum::serve(listener, app).with_graceful_shutdown(async {
            for _ in 0..3600 {
                if std::path::Path::new("/tmp/cadus-report-browser.stop").exists() { return; }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }).await.unwrap();
    }).await;
}
