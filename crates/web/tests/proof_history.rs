//! Durable proof history remains actionable beyond the recent-list window.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use cadus_store::test_support::TestDb;
use common::{
    call, exemplar, kp, lesson_learner, lesson_problem, one_unit_curriculum, parse,
    state_with_content, topic,
};
use serde_json::{Value, json};
use sqlx::types::Uuid;
use sqlx::types::chrono::{DateTime, Utc};

fn app(db: &TestDb) -> Router {
    cadus_web::create_app(state_with_content(
        db,
        one_unit_curriculum(vec![topic(
            "addition",
            vec![kp("kp1", vec![exemplar("What is 2 + 2?", "4")])],
        )]),
    ))
}

#[expect(
    clippy::too_many_arguments,
    reason = "the fixture names one chain draft and its verdict"
)]
async fn insert_job(
    db: &TestDb,
    user: Uuid,
    attempt: &str,
    context: &str,
    created_at: DateTime<Utc>,
    revision_of: Option<Uuid>,
    revision: i32,
    verdict: &str,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO proof_grading_jobs \
         (user_id, attempt_id, payload, context, status, result, created_at, revision_of, revision) \
         VALUES ($1, $2, $3, $4, 'done', $5, $6, $7, $8) RETURNING id",
    )
    .bind(user)
    .bind(attempt)
    .bind(json!({
        "v": 1, "task_id": "lesson-addition", "topic": "addition", "kp": "kp1",
        "item_digest": "0123456789ab", "problem": "Write a proof of 2+2=4.",
        "reference": "2+2=4.", "expected": null, "rubric": [], "given_answer": "Draft."
    }))
    .bind(context)
    .bind(json!({
        "v": 1, "verdict": verdict, "checks": [], "feedback": "Review the proof.",
        "model": "history-test"
    }))
    .bind(created_at)
    .bind(revision_of)
    .bind(revision)
    .fetch_one(&db.admin)
    .await
    .unwrap()
}

async fn add_recent_noise(db: &TestDb, user: Uuid) {
    sqlx::query(
        "INSERT INTO proof_grading_jobs (user_id, attempt_id, payload, context, status, result, created_at) \
         SELECT $1, 'history-noise-' || n::text, $2, 'legacy', 'done', $3, \
                now() + n * interval '1 second' \
           FROM generate_series(1, 501) AS n",
    )
    .bind(user)
    .bind(json!({"v": 1, "task_id": "noise", "topic": "addition", "given_answer": "x"}))
    .bind(json!({"v": 1, "verdict": "pass", "checks": [], "feedback": "ok", "model": "noise"}))
    .execute(&db.admin)
    .await
    .unwrap();
}

async fn request(
    app: &Router,
    method: Method,
    path: &str,
    user: Uuid,
    body: Option<Value>,
) -> (StatusCode, Value, String) {
    let (status, raw) = call(app, method, path, Some(user), body).await;
    (status, parse(&raw), raw)
}

#[tokio::test]
async fn old_open_chains_remain_actionable_and_recent_revised_heads_keep_their_root() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = lesson_learner(
            &db,
            "proof-history@example.test",
            lesson_problem(5.0, "kp1", Vec::new()),
        )
        .await;
        let other = lesson_learner(
            &db,
            "proof-history-other@example.test",
            lesson_problem(5.0, "kp1", Vec::new()),
        )
        .await;
        let old: DateTime<Utc> = sqlx::query_scalar("SELECT now() - interval '3 days'")
            .fetch_one(&db.admin)
            .await
            .unwrap();
        let seen_id = insert_job(
            &db,
            user,
            "history-seen",
            "review",
            old,
            None,
            0,
            "needs_revision",
        )
        .await;
        let dispute_id = insert_job(
            &db,
            user,
            "history-dispute",
            "review",
            old,
            None,
            0,
            "needs_revision",
        )
        .await;
        let revise_id = insert_job(
            &db,
            user,
            "history-revise",
            "review",
            old,
            None,
            0,
            "needs_revision",
        )
        .await;
        let split_root = insert_job(
            &db,
            user,
            "history-split-root",
            "review",
            old,
            None,
            0,
            "needs_revision",
        )
        .await;
        add_recent_noise(&db, user).await;
        let newer: DateTime<Utc> = sqlx::query_scalar("SELECT now() + interval '1 hour'")
            .fetch_one(&db.admin)
            .await
            .unwrap();
        let split_head = insert_job(
            &db,
            user,
            "history-split-head",
            "review",
            newer,
            Some(split_root),
            1,
            "pass",
        )
        .await;

        let (status, polled, raw) = request(
            &app,
            Method::GET,
            &format!("/api/proof-grading/{split_head}"),
            user,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        assert_eq!(
            polled["chain"]["root_id"],
            split_root.to_string(),
            "{polled}"
        );
        assert_eq!(
            polled["chain"]["head_id"],
            split_head.to_string(),
            "{polled}"
        );

        let (status, seen, raw) = request(
            &app,
            Method::POST,
            &format!("/api/proof-grading/{seen_id}/seen"),
            user,
            Some(json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        assert_eq!(seen["chain"]["root_id"], seen_id.to_string(), "{seen}");
        let seen_at: bool =
            sqlx::query_scalar("SELECT seen_at IS NOT NULL FROM proof_grading_jobs WHERE id = $1")
                .bind(seen_id)
                .fetch_one(&db.admin)
                .await
                .unwrap();
        assert!(seen_at);

        let (status, disputed, raw) = request(
            &app,
            Method::POST,
            &format!("/api/proof-grading/{dispute_id}/dispute"),
            user,
            Some(json!({"note": "Please check this."})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        assert_eq!(
            disputed["chain"]["root_id"],
            dispute_id.to_string(),
            "{disputed}"
        );

        let (status, revised, raw) = request(
            &app,
            Method::POST,
            &format!("/api/proofs/{revise_id}/revise"),
            user,
            Some(json!({"answer": "A revised proof."})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        assert!(revised["proof_grading"]["id"].is_string(), "{revised}");

        let (status, listed, raw) = request(&app, Method::GET, "/api/proofs", user, None).await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        for root in [seen_id, dispute_id, revise_id, split_root] {
            assert!(
                listed["chains"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|chain| chain["root_id"] == root.to_string()),
                "missing old root {root}: {listed}"
            );
        }

        let (status, _, raw) = request(
            &app,
            Method::POST,
            &format!("/api/proof-grading/{seen_id}/seen"),
            other,
            Some(json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{raw}");
        let (status, _, raw) = request(
            &app,
            Method::GET,
            &format!("/api/proof-grading/{split_head}"),
            other,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{raw}");
    })
    .await;
}

/// Concurrent tabs may reveal a capped solution only once.
#[tokio::test]
async fn concurrent_seen_requests_reveal_the_reference_once() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = lesson_learner(
            &db,
            "proof-reveal-race@example.test",
            lesson_problem(5.0, "kp1", Vec::new()),
        )
        .await;
        let id = insert_job(
            &db,
            user,
            "reveal-race",
            "review",
            Utc::now(),
            None,
            2,
            "needs_revision",
        )
        .await;
        sqlx::query("UPDATE proof_grading_jobs SET seen_at = now() WHERE id = $1")
            .bind(id)
            .execute(&db.admin)
            .await
            .unwrap();
        // Hold the reveal row until both requests have read the same phase.
        let mut blocker = db.admin.begin().await.unwrap();
        sqlx::query("SELECT id FROM proof_grading_jobs WHERE id = $1 FOR UPDATE")
            .bind(id)
            .fetch_one(&mut *blocker)
            .await
            .unwrap();
        let mut requests = Vec::new();
        for _ in 0..2 {
            let app = app.clone();
            let path = format!("/api/proof-grading/{id}/seen");
            requests.push(tokio::spawn(async move {
                request(&app, Method::POST, &path, user, Some(json!({}))).await
            }));
        }
        let mut both_waiting = false;
        for _ in 0..200 {
            let waiting: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM pg_stat_activity WHERE datname = current_database() \
                 AND wait_event_type = 'Lock' \
                 AND query LIKE 'UPDATE proof_grading_jobs SET revealed_at%'",
            )
            .fetch_one(&db.admin)
            .await
            .unwrap();
            if waiting == 2 {
                both_waiting = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        blocker.commit().await.unwrap();
        let mut revealed = 0;
        for pending in requests {
            let (status, body, raw) = pending.await.unwrap();
            assert_eq!(status, StatusCode::OK, "{raw}");
            if body["chain"]["solution"].is_string() {
                revealed += 1;
            }
        }
        assert!(
            both_waiting,
            "both requests must reach the contested reveal write"
        );
        assert_eq!(
            revealed, 1,
            "only the request that stamps revealed_at may return the solution"
        );
    })
    .await;
}
