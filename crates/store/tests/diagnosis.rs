//! The A4 diagnosis queue: the idempotent enqueue inside the grade
//! transaction, the tenant-scoped read, and the error of a refused insert.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_store::begin_tenant;
use cadus_store::diagnosis::{JOB_PENDING, enqueue, job};
use cadus_store::test_support::TestDb;
use common::fault::fail_on;
use common::store_sqlstate;
use serde_json::json;
use uuid::Uuid;

/// The enqueue writes one row per `(user, attempt)` and answers the standing
/// id on a repeat; the read is scoped to the bound tenant.
#[tokio::test]
async fn the_enqueue_is_idempotent_and_the_read_is_tenant_scoped() {
    TestDb::with(|db| async move {
        let alice = db.seed_user("alice@example.test").await;
        let bob = db.seed_user("bob@example.test").await;
        let payload = json!({"v": 1, "task_id": "t1"});

        let mut tx = begin_tenant(&db.app, alice).await.unwrap();
        let first = enqueue(&mut tx, alice, "attempt-1", &payload)
            .await
            .unwrap();
        let again = enqueue(&mut tx, alice, "attempt-1", &payload)
            .await
            .unwrap();
        assert_eq!(first, again);
        let row = job(&mut *tx, first).await.unwrap().expect("the job row");
        assert_eq!(row.id, first);
        assert_eq!(row.attempt_id, "attempt-1");
        assert_eq!(row.status, JOB_PENDING);
        assert_eq!(row.result, None);
        assert_eq!(job(&mut *tx, Uuid::new_v4()).await.unwrap(), None);
        tx.commit().await.unwrap();
        // The superuser pool reads the row outside a tenant transaction.
        assert_eq!(
            job(&db.admin, first).await.unwrap().map(|row| row.id),
            Some(first)
        );

        let mut tx = begin_tenant(&db.app, bob).await.unwrap();
        assert_eq!(job(&mut *tx, first).await.unwrap(), None);
        tx.rollback().await.unwrap();
    })
    .await;
}

/// A refused insert and a closed pool are the errors of the two statements.
#[tokio::test]
async fn a_refused_insert_and_a_closed_pool_are_database_errors() {
    TestDb::with(|db| async move {
        let alice = db.seed_user("alice@example.test").await;
        fail_on(&db, "INSERT", "diagnosis_jobs").await;
        let mut tx = begin_tenant(&db.app, alice).await.unwrap();
        let err = enqueue(&mut tx, alice, "attempt-1", &json!({}))
            .await
            .unwrap_err();
        assert_eq!(store_sqlstate(&err), "P0001");
        tx.rollback().await.unwrap();

        let pool = common::fault::closed_pool(&db).await;
        assert!(job(&pool, alice).await.is_err());
    })
    .await;
}
