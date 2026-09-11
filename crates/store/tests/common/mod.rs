//! Shared fixtures of the store tests: the serving key, the seeded rows, the
//! error readers, and the fault injections of `fault`.
//!
//! Every test binary compiles this module, and no binary uses every helper, so
//! the dead-code lint is off for the module.

#![allow(dead_code, unused_imports, unused_macros)]

pub mod auth;
pub mod bench;
pub mod content;
pub mod events;
pub mod fault;
pub mod long_log;
pub mod migrate;
pub mod rls;
pub mod state;

use cadus_core::pool::{Avoid, PoolAnswer, PoolProblem, Ring, Source, TaskMemory};
use cadus_store::pool::{Claimed, NewInstance, Pop, pop_with_ring};
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db, StoreError};
use sqlx::types::chrono::{DateTime, Utc};
use sqlx::{Connection, PgPool};
use uuid::Uuid;

/// The serving key of the tests: `"<topic_id>/<kp_id>"`.
pub const KP: &str = "perfect-squares/kp1";

/// The Unix instant of 2026-01-01T00:00:00Z.
pub const BASE_INSTANT: i64 = 1_767_225_600;

/// The instant `index` seconds after [`BASE_INSTANT`]. Seeded rows are one
/// second apart, so an `ORDER BY` on their instant is the seeding order.
pub fn at(index: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(BASE_INSTANT + index, 0).expect("the instant is inside the range")
}

/// The SQLSTATE of a database error, or `"none"` when the error carries none.
pub fn sqlstate(err: &sqlx::Error) -> String {
    match err.as_database_error().and_then(|db| db.code()) {
        Some(code) => code.to_string(),
        None => "none".to_string(),
    }
}

/// The SQLSTATE behind a `StoreError::Db`, or `"none"` for any other error.
pub fn store_sqlstate(err: &StoreError) -> String {
    match err {
        StoreError::Db(inner) => sqlstate(inner),
        _ => "none".to_string(),
    }
}

/// The message of the database error behind a `StoreError::Db`, or the
/// Display of any other error.
pub fn db_message(err: &StoreError) -> String {
    match err {
        StoreError::Db(inner) => match inner.as_database_error() {
            Some(db_err) => db_err.message().to_string(),
            None => inner.to_string(),
        },
        other => other.to_string(),
    }
}

/// A `Db` on the app pool of `db`, with the shipped client-side bound.
pub fn app_db(db: &TestDb) -> Db {
    Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS)
}

/// Wrap a pool in a `Db` with no client-side bound. The tests measure the
/// statement, not the timeout, and `client_timeout.rs` pins the bound.
pub fn handle(pool: &PgPool) -> Db {
    Db::new(pool.clone(), 0)
}

/// The two row documents of the seeded pool row at `index`: the problem
/// `Compute $<index>^2$.` and the answer `2`.
pub fn pool_documents(index: usize) -> (String, &'static str) {
    (
        format!(r#"{{"v":1,"text":"Compute ${index}^2$.","seed":0}}"#),
        r#"{"v":1,"answer":"2"}"#,
    )
}

/// The digest of the pool row at `index`: `pool-instance-0007`.
pub fn pool_digest(index: usize) -> String {
    format!("pool-instance-{index:04}")
}

/// Seed one pool row of `source` for `(user_id, kp_id)` with the instant and
/// the digest of `index`, and return its id.
pub async fn seed_pool_row(
    admin: &PgPool,
    user_id: Uuid,
    kp_id: &str,
    index: usize,
    source: Source,
) -> Uuid {
    let (problem, expected) = pool_documents(index);
    sqlx::query_scalar!(
        r#"
        INSERT INTO serving_pool
            (user_id, kp_id, source, problem, expected_answer, instance_hash, created_at)
        VALUES ($1, $2, $3, $4::text::jsonb, $5::text::jsonb, $6, $7)
        RETURNING id
        "#,
        user_id,
        kp_id,
        source.as_str(),
        problem,
        expected,
        pool_digest(index),
        at(index as i64),
    )
    .fetch_one(admin)
    .await
    .expect("the seeded pool row inserts")
}

/// Seed `count` template rows, oldest first.
pub async fn seed_pool_rows(admin: &PgPool, user_id: Uuid, kp_id: &str, count: usize) -> Vec<Uuid> {
    let mut ids = Vec::with_capacity(count);
    for index in 0..count {
        ids.push(seed_pool_row(admin, user_id, kp_id, index, Source::Template).await);
    }
    ids
}

/// One instance on its way into the pool, with the digest of `index`.
pub fn new_instance(index: usize) -> NewInstance {
    NewInstance {
        source: Source::Template,
        content_digest: Some("deadbeefdeadbeef".to_string()),
        generation_context: None,
        problem: PoolProblem {
            v: 1,
            text: format!("Compute ${index}^2$."),
            bindings: [("a".to_string(), index.to_string())].into_iter().collect(),
            seed: 7,
        },
        expected_answer: PoolAnswer {
            answer_contract: None,
            v: 1,
            answer: (index * index).to_string(),
        },
        instance_hash: pool_digest(index),
    }
}

/// Seed one authored document of `kind` and `status` for [`KP`].
pub async fn seed_doc(
    admin: &PgPool,
    digest: &str,
    kind: &str,
    status: &str,
    body: serde_json::Value,
    approved: Option<DateTime<Utc>>,
) {
    sqlx::query!(
        r#"
        INSERT INTO content_store (digest, kp_id, kind, body, status, approved_at)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
        digest,
        KP,
        kind,
        body,
        status,
        approved,
    )
    .execute(admin)
    .await
    .expect("the seeded document inserts");
}

/// Insert one template `content_store` row of `status` with `statement` as
/// the text of its body. An approved row is stamped now.
pub async fn seed_template(
    admin: &PgPool,
    digest: &str,
    kp_id: &str,
    status: &str,
    statement: &str,
) {
    let body = format!(r#"{{"v":1,"statement":"{statement}"}}"#);
    sqlx::query!(
        r#"
        INSERT INTO content_store (digest, kp_id, kind, body, status, approved_at)
        VALUES ($1, $2, 'template', $3::text::jsonb, $4,
                CASE WHEN $4 = 'approved' THEN now() ELSE NULL END)
        "#,
        digest,
        kp_id,
        body,
        status,
    )
    .execute(admin)
    .await
    .expect("the content row inserts");
}

/// One pop of `(user_id, kp_id)` with `ring` and an empty task memory, in a
/// tenant transaction of its own.
pub async fn pop_with(pool: &PgPool, user_id: Uuid, kp_id: &str, ring: &Ring) -> Pop {
    let task = TaskMemory::new();
    let avoid = Avoid::new(ring, &task);
    pop_with_ring(pool, user_id, kp_id, &avoid)
        .await
        .expect("the pop runs")
}

/// One pop of `(user_id, kp_id)` with both anti-repeat windows empty.
pub async fn pop_fresh(pool: &PgPool, user_id: Uuid, kp_id: &str) -> Pop {
    pop_with(pool, user_id, kp_id, &Ring::new()).await
}

/// The row one fresh pop of `(user_id, kp_id)` claims.
pub async fn claim_fresh(pool: &PgPool, user_id: Uuid, kp_id: &str) -> Claimed {
    pop_fresh(pool, user_id, kp_id)
        .await
        .claimed
        .expect("the pool holds a servable row")
}

/// The count of rows of `query` whose one bind is `name`, read through a
/// maintenance connection of the test cluster.
pub async fn cluster_count(query: &'static str, name: &str) -> i64 {
    let dsn = std::env::var("CADUS_TEST_DATABASE_URL").expect("the test DSN is set");
    let mut conn = sqlx::PgConnection::connect(&dsn)
        .await
        .expect("the maintenance connection opens");
    let count: i64 = sqlx::query_scalar(query)
        .bind(name)
        .fetch_one(&mut conn)
        .await
        .expect("the count reads");
    conn.close()
        .await
        .expect("the maintenance connection closes");
    count
}

/// The SQLSTATE of one statement that runs inside a tenant transaction of its
/// own; the transaction is rolled back after it.
///
/// A refused statement aborts its transaction, so a test that checks several
/// refusals gives each one its own transaction.
macro_rules! sqlstate_in_tx {
    ($db:expr, $user:expr, |$tx:ident| $call:expr) => {{
        let mut $tx = cadus_store::begin_tenant(&$db.app, $user)
            .await
            .expect("the tenant transaction starts");
        let err = $call.await.expect_err("the statement is refused");
        $tx.rollback().await.expect("the transaction rolls back");
        $crate::common::store_sqlstate(&err)
    }};
}
pub(crate) use sqlstate_in_tx;
