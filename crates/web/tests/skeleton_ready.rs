//! Part of `tests/skeleton.rs`: the header of that file gives the
//! requirements and the rules.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::skeleton::*;

// ---------------------------------------------------------------------------
// /api/ready (D-M5-6)
// ---------------------------------------------------------------------------

/// (14) `/api/ready` reports the age of the oldest unclaimed diagnosis job, and
/// a stale worker is a warning and never a `503`.
///
/// Ruling D-M5-6 takes worker liveness from the `diagnosis_jobs` claim age. Spec
/// section 10, row "Ready": a heartbeat past the threshold is a warning, never a
/// `503`. The learner can still study while the worker is down, because the
/// whole grade verdict is local CPU work (A3) and only the diagnosis prose waits
/// (A4).
///
/// The job row goes in through the ADMIN pool. `diagnosis_jobs` carries a FORCEd
/// tenant policy, and the web pool is unbound, so this test also proves that the
/// SECURITY DEFINER function of migration 0007 is what crosses it: without that
/// function the handler reads zero rows and reports `null` here.
#[tokio::test]
async fn ready_reports_a_stale_worker_as_a_warning_and_not_a_503() {
    TestDb::with(|db| async move {
        let user = db.seed_user("claim-age@example.test").await;
        sqlx::query(
            "INSERT INTO diagnosis_jobs (user_id, attempt_id, payload, created_at) \
             VALUES ($1, 't-1-1', '{}'::jsonb, now() - interval '120 seconds')",
        )
        .bind(user)
        .execute(&db.admin)
        .await
        .expect("seed one pending diagnosis job");

        let (status, body) = ready_of(&db).await;

        assert_eq!(
            status.as_u16(),
            200,
            "a stale worker is never a 503: {body}"
        );

        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed["ok"], serde_json::json!(true));
        assert_eq!(parsed["db"], serde_json::json!("ok"));
        assert_eq!(parsed["worker"]["stale"], serde_json::json!(true));
        assert_eq!(
            parsed["warnings"],
            serde_json::json!(["worker_claim_stale"])
        );

        let age = parsed["worker"]["claim_age_secs"].as_f64().unwrap();
        assert!(
            age >= 120.0,
            "the job was seeded 120 s in the past, the probe read {age}"
        );
    })
    .await;
}

/// (15) A claimed job is no backlog, so the age goes back to `null`.
///
/// The reading counts the jobs that WAIT for a claim. A worker that claims the
/// row is a worker that lives, so a running job must not read as a stale one.
#[tokio::test]
async fn a_claimed_job_leaves_no_backlog() {
    TestDb::with(|db| async move {
        let user = db.seed_user("claimed@example.test").await;
        sqlx::query(
            "INSERT INTO diagnosis_jobs (user_id, attempt_id, payload, created_at, claimed_at, \
             status) VALUES ($1, 't-1-1', '{}'::jsonb, now() - interval '600 seconds', now(), \
             'running')",
        )
        .bind(user)
        .execute(&db.admin)
        .await
        .expect("seed one running diagnosis job");

        let (status, body) = ready_of(&db).await;

        assert_eq!(status.as_u16(), 200);
        assert_eq!(
            body,
            r#"{"db":"ok","ok":true,"worker":{"claim_age_secs":null,"stale":false}}"#
        );
    })
    .await;
}

/// (16) A young backlog is no warning.
///
/// The threshold is 60 seconds (the 1.0 value). A job that waits a moment is the
/// normal state of a healthy queue.
#[tokio::test]
async fn a_young_backlog_reports_no_warning() {
    TestDb::with(|db| async move {
        let user = db.seed_user("young@example.test").await;
        sqlx::query(
            "INSERT INTO diagnosis_jobs (user_id, attempt_id, payload, created_at) \
             VALUES ($1, 't-1-1', '{}'::jsonb, now() - interval '5 seconds')",
        )
        .bind(user)
        .execute(&db.admin)
        .await
        .expect("seed one fresh diagnosis job");

        let app = app_on(db.app.clone());
        let (status, _headers, body) = send(&app, get("/api/ready")).await;

        assert_eq!(status.as_u16(), 200);
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed["worker"]["stale"], serde_json::json!(false));
        assert_eq!(parsed["warnings"], serde_json::Value::Null);
        let age = parsed["worker"]["claim_age_secs"].as_f64().unwrap();
        assert!((5.0..60.0).contains(&age), "the probe read {age}");
    })
    .await;
}

/// (17) One tenant's backlog is visible to a probe that is bound to nobody, and
/// the answer still names no tenant.
///
/// The readiness probe crosses the tenant policy on purpose (migration 0007).
/// The whole justification is that it reads ONE aggregate number, so the body
/// must never carry an id, an attempt, or a payload.
#[tokio::test]
async fn the_readiness_body_names_no_tenant() {
    TestDb::with(|db| async move {
        let user = db.seed_user("tenant@example.test").await;
        sqlx::query(
            "INSERT INTO diagnosis_jobs (user_id, attempt_id, payload, created_at) \
             VALUES ($1, 't-secret-task-9', '{\"answer\":\"the learner wrote this\"}'::jsonb, \
             now() - interval '90 seconds')",
        )
        .bind(user)
        .execute(&db.admin)
        .await
        .expect("seed one pending diagnosis job");

        let app = app_on(db.app.clone());
        let (_status, _headers, body) = send(&app, get("/api/ready")).await;

        assert!(
            !body.contains(&user.to_string()),
            "the body names a user: {body}"
        );
        assert!(
            !body.contains("t-secret-task-9"),
            "the body names an attempt: {body}"
        );
        assert!(
            !body.contains("the learner wrote this"),
            "the body carries a payload: {body}"
        );
    })
    .await;
}
