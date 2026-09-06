//! The session guard under the faults of its touch and of its account read,
//! and the one store call of the auth tier that runs past its client bound.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::future::Future;
use std::pin::Pin;
use std::time::{Duration, Instant};

use cadus_store::Db;
use cadus_web::auth::password::Argon2Profile;
use cadus_web::{AppState, create_app};
use common::*;
use sqlx::postgres::PgPoolOptions;

/// Seed `email` with a session whose last touch is two hours old, and give the
/// raw token back.
async fn stale_session(db: &TestDb, email: &str) -> &'static str {
    let user = db.seed_user(email).await;
    seed_session(
        db,
        user,
        SESSION_TOKEN_ONE.1,
        shift(-7_200),
        shift(-7_200),
        shift(3_600),
    )
    .await;
    SESSION_TOKEN_ONE.0
}

/// `GET /api/export` with `token`, and the status it answers.
async fn export_status(app: &Router, token: &str) -> u16 {
    send(app, get_bearer("/api/export", token))
        .await
        .status
        .as_u16()
}

/// A stale `last_seen_at` is touched by the first request, and a touch whose
/// bind, write, or commit fails refuses the request as `401`.
#[tokio::test]
async fn a_stale_last_seen_is_touched_and_a_touch_that_fails_is_401() {
    TestDb::with(|db| async move {
        let token = stale_session(&db, "touch@example.com").await;
        let before = last_seen_at(&db, SESSION_TOKEN_ONE.1).await;
        assert_eq!(export_status(&app_of(&db), token).await, 200);
        assert!(last_seen_at(&db, SESSION_TOKEN_ONE.1).await > before);
    })
    .await;
    TestDb::with(|db| async move {
        let token = stale_session(&db, "touch-write@example.com").await;
        fail_writes(&db, "auth_sessions", "NEW.last_seen_at <> NEW.created_at").await;
        assert_eq!(export_status(&app_of(&db), token).await, 401);
    })
    .await;
    TestDb::with(|db| async move {
        let token = stale_session(&db, "touch-commit@example.com").await;
        fail_commit_after(&db, "auth_sessions", "UPDATE", "true").await;
        assert_eq!(export_status(&app_of(&db), token).await, 401);
    })
    .await;
    TestDb::with(|db| async move {
        let token = stale_session(&db, "touch-bind@example.com").await;
        fail_tenant_bind(&db).await;
        assert_eq!(export_status(&app_of(&db), token).await, 401);
    })
    .await;
}

/// One fault a test applies to the database before its request.
type BoxFault<'a> = Pin<Box<dyn Future<Output = ()> + Send + 'a>>;

/// Seed a learner, apply `fault` to the database, and read `401` back from a
/// guarded read of that learner.
async fn guarded_read_is_401_after(email: &'static str, fault: fn(&TestDb) -> BoxFault<'_>) {
    TestDb::with(move |db| async move {
        let user = seed_learner(&db, email).await;
        fault(&db).await;
        let (status, _) = call(&app_of(&db), Method::GET, "/api/export", Some(user), None).await;
        assert_eq!(status.as_u16(), 401);
    })
    .await;
}

/// A live session whose account read fails, or finds no row, is `401`.
#[tokio::test]
async fn a_session_whose_account_read_fails_or_finds_nothing_is_401() {
    guarded_read_is_401_after("read-fault@example.com", |db| {
        Box::pin(drop_function(db, "auth_user_by_id(uuid)"))
    })
    .await;
    guarded_read_is_401_after("read-hidden@example.com", |db| {
        Box::pin(hide_user_by_id(db))
    })
    .await;
}

/// A session lookup that runs past the client bound is `500` inside the bound,
/// never a wait for the pool.
///
/// The pool holds one connection and the test keeps it, so the lookup waits
/// for a connection that never comes; the 300 ms bound of the handle ends the
/// wait long before the 5 s acquire timeout of the pool.
#[tokio::test]
async fn a_session_lookup_past_the_client_bound_is_500() {
    TestDb::with(|db| async move {
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(5))
            .connect(&dsn_for(&db.name, Some("cadus_app")))
            .await
            .unwrap();
        let held = pool.acquire().await.unwrap();
        let app =
            create_app(AppState::new(Db::new(pool.clone(), 300)).with_argon2(Argon2Profile::TEST));
        let started = Instant::now();
        let answer = send(&app, get_bearer("/api/auth/me", "any-token")).await;
        assert_eq!(answer.status.as_u16(), 500, "{}", answer.body);
        assert!(started.elapsed() < Duration::from_secs(3));
        drop(held);
        pool.close().await;
    })
    .await;
}
