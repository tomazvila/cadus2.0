//! The deaf Postgres of `test_support`: it finishes the handshake, answers the
//! pool ping, and then answers no query, so a bounded query ends at the bound.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::time::Duration;

use cadus_store::test_support::DeafPostgres;
use cadus_store::{Db, DbConfig, StoreError, bounded};
use sqlx::{Connection, PgConnection};

/// A connection that sends no query opens and closes cleanly; the pool opens,
/// its ping is answered, the first query reaches the server and never
/// returns, and the bound ends it.
#[tokio::test]
async fn a_deaf_server_takes_the_query_and_the_bound_ends_it() {
    let server = DeafPostgres::start();
    assert!(server.port() > 0);
    assert!(!server.query_seen());

    let quiet = PgConnection::connect(&server.dsn()).await.unwrap();
    quiet.close().await.unwrap();
    assert!(!server.query_seen());

    let mut cfg = DbConfig::new(server.dsn());
    cfg.statement_timeout_ms = 0;
    cfg.client_timeout_ms = 300;
    let db = Db::connect(&cfg).await.unwrap();
    let err = bounded(&db, sqlx::query("SELECT 1").execute(db.pool()))
        .await
        .unwrap_err();
    assert!(
        matches!(err, StoreError::Timeout { after_ms: 300 }),
        "{err}"
    );
    assert!(server.query_seen());
    // The connection is inside a query the server never answers, so a close
    // never returns. The bound ends the wait, and the process end closes the
    // socket.
    let _ = tokio::time::timeout(Duration::from_millis(200), db.pool().close()).await;
}
