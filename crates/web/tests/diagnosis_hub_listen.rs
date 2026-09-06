//! The process `LISTEN` loop of the diagnosis hub (M5 U9, D7): what it
//! publishes, what it drops, and the three ways it ends.
//!
//! Every test here runs the loop in process on a pool of its own, so the
//! process pool of the other tests never closes under them.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::Arc;
use std::time::Duration;

use cadus_store::diagnosis::{Notice, notify_payload};
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::diagnosis::DiagnosisHub;
use common::dsn_for;
use sqlx::Executor;
use sqlx::postgres::PgPoolOptions;
use sqlx::types::Uuid;

/// Send `payload` on the diagnosis channel with the admin pool.
async fn notify(db: &TestDb, payload: &str) {
    sqlx::query("SELECT pg_notify('diagnosis_done', $1)")
        .bind(payload)
        .execute(&db.admin)
        .await
        .unwrap();
}

/// The listener publishes every notice that reads, drops a payload that does
/// not, and ends with an error when its pool closes under it.
#[tokio::test]
async fn the_listener_publishes_a_notice_and_ends_when_its_pool_closes() {
    TestDb::with(|db| async move {
        let pool = db.pool_as("cadus_app", 2).await;
        let hub = Arc::new(DiagnosisHub::new());
        let listening = tokio::spawn({
            let hub = Arc::clone(&hub);
            let handle = Db::new(pool.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
            async move { hub.listen(&handle).await }
        });
        let mut receiver = hub.subscribe();
        let (job_id, user_id) = (Uuid::new_v4(), Uuid::new_v4());

        // The LISTEN is established asynchronously, so the pair is repeated
        // until the notice lands. A payload that does not read goes first each
        // time, so the loop dropped one before it published the good one.
        let received = loop {
            notify(&db, "not a notice").await;
            notify(&db, &notify_payload(job_id, user_id)).await;
            match tokio::time::timeout(Duration::from_millis(200), receiver.recv()).await {
                Ok(Ok(notice)) => break notice,
                _ => continue,
            }
        };
        assert_eq!(received, Notice { job_id, user_id });

        pool.close().await;
        let ended = tokio::time::timeout(Duration::from_secs(5), listening)
            .await
            .expect("the listener must end when its pool closes")
            .expect("the listener task must not panic");
        assert!(ended.is_err(), "a closed pool ends the loop with an error");
    })
    .await;
}

/// A pool that is already closed gives no connection, so the loop never starts.
#[tokio::test]
async fn the_listener_cannot_start_on_a_closed_pool() {
    TestDb::with(|db| async move {
        let pool = db.pool_as("cadus_app", 1).await;
        pool.close().await;
        let outcome = DiagnosisHub::new()
            .listen(&Db::new(pool, DEFAULT_CLIENT_TIMEOUT_MS))
            .await;
        assert!(outcome.is_err(), "a closed pool must refuse the listener");
    })
    .await;
}

/// A connection whose transaction is already aborted refuses the `LISTEN`
/// statement, so the loop never starts.
#[tokio::test]
async fn the_listener_cannot_start_on_a_connection_in_an_aborted_transaction() {
    TestDb::with(|db| async move {
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .test_before_acquire(false)
            .after_connect(|conn, _meta| {
                Box::pin(async move {
                    conn.execute("BEGIN").await?;
                    let _ = conn.execute("SELECT 1/0").await;
                    Ok(())
                })
            })
            .connect(&dsn_for(&db.name, Some("cadus_app")))
            .await
            .unwrap();
        let outcome = DiagnosisHub::new()
            .listen(&Db::new(pool.clone(), DEFAULT_CLIENT_TIMEOUT_MS))
            .await;
        assert!(
            outcome.is_err(),
            "an aborted transaction must refuse the LISTEN statement"
        );
        pool.close().await;
    })
    .await;
}
