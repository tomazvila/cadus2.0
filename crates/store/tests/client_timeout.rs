//! R2, open finding "no TCP keepalive in sqlx 0.9": the client-side query
//! bound.
//!
//! `statement_timeout` is a server-side bound. The server cancels the statement
//! and reports SQLSTATE 57014, so that bound needs a live server. sqlx 0.9 sets
//! no TCP keepalive, so a server that accepts the socket and then answers
//! nothing leaves the caller in a read that the kernel never ends.
//! `cadus_store::bounded` adds the client-side bound.
//!
//! The test points a lazy pool at `DeafPostgres::start_silent`, which accepts
//! the connection and writes nothing. The connect therefore never finishes, so
//! `acquire` never returns and only the client-side bound ends the wait.
//!
//! `DB_CLIENT_TIMEOUT_MS=300` gives the 300 ms bound of this test. The unit
//! test `the_client_timeout_reads_the_same_three_rules` in `src/lib.rs` pins
//! that step, so this test sets the field and touches no process environment.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::{Duration, Instant};

use cadus_store::test_support::DeafPostgres;
use cadus_store::{Db, DbConfig, StoreError, bounded, connect_options};
use sqlx::postgres::PgPoolOptions;

#[tokio::test]
async fn bounded_stops_an_acquire_against_a_deaf_server() {
    let server = DeafPostgres::start_silent();
    let cfg = DbConfig {
        database_url: server.dsn(),
        statement_timeout_ms: 0,
        client_timeout_ms: 300,
    };

    // The pool is lazy, so the connect starts inside the `acquire` below and
    // the bound of `bounded` is the only bound that stops it. The acquire
    // timeout of 5 s is the backstop of the test itself: it is longer than the
    // 2 s that the assertion allows, so a pass proves the 300 ms bound.
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(5))
        .connect_lazy_with(connect_options(&cfg).unwrap());
    let db = Db::new(pool, cfg.client_timeout_ms);

    let start = Instant::now();
    let outcome = bounded(&db, db.pool().acquire()).await;
    let elapsed = start.elapsed();

    let err = outcome.expect_err("the deaf server answers no connect");
    let StoreError::Timeout { after_ms } = &err else {
        panic!("expected StoreError::Timeout, got {err}");
    };
    assert_eq!(*after_ms, 300);
    assert_eq!(err.to_string(), "the query did not answer within 300 ms");
    assert!(
        elapsed < Duration::from_secs(2),
        "the acquire took {elapsed:?}, so the client-side bound did not apply"
    );

    db.pool().close().await;
}

/// A bound of 0 turns the client-side bound off, so the future runs to its own
/// end. Port 1 accepts no connection, so the acquire ends with the pool error.
#[tokio::test]
async fn a_bound_of_zero_runs_the_future_without_a_bound() {
    let cfg = DbConfig {
        database_url: "postgresql://cadus_app@127.0.0.1:1/cadus".to_string(),
        statement_timeout_ms: 0,
        client_timeout_ms: 0,
    };
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_millis(500))
        .connect_lazy_with(connect_options(&cfg).unwrap());
    let db = Db::new(pool, cfg.client_timeout_ms);

    assert_eq!(db.client_timeout_ms(), 0);
    assert_eq!(db.client_timeout(), None);

    let err = bounded(&db, db.pool().acquire())
        .await
        .expect_err("port 1 is closed");
    assert!(
        matches!(err, StoreError::Db(_)),
        "a closed port must give StoreError::Db, not {err}"
    );

    db.pool().close().await;
}
