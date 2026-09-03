//! The error paths of the serving pool: a refused statement, a dead backend,
//! a commit that fails, a claim that writes no row, a source value this build
//! does not know, and the two document writers of a drawn instance.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use std::collections::BTreeMap;

use cadus_core::pool::{Avoid, Ring, Source, TaskMemory};
use cadus_store::pool::{
    NewInstance, approved_template, insert_batch, insert_batch_for_user, operator_flags,
    pop_with_ring, pop_with_ring_tx, reclaim_exemplar_tx, refill_targets, retire_unapproved,
    unclaimed_depth,
};
use cadus_store::test_support::TestDb;
use cadus_store::{StoreError, begin_tenant};
use common::fault::{
    closed_pool, dead_pool, drop_checks, fail_commit_on, fail_on, revoke, skip_updates_on,
};
use common::{KP, new_instance, seed_pool_row, seed_pool_rows, store_sqlstate};
use uuid::Uuid;

/// The message of a `StoreError::PoolRow`, or the Display of any other error.
fn pool_row_message(err: &StoreError) -> String {
    match err {
        StoreError::PoolRow(message) => message.clone(),
        other => other.to_string(),
    }
}

/// The two row documents of a drawn instance carry its text, its bindings,
/// its answer, and its digest.
#[test]
fn the_row_of_an_instance_carries_its_documents() {
    let ast = cadus_core::answer::parse::parse("4").unwrap();
    let instance = cadus_core::template::Instance {
        bindings: BTreeMap::new(),
        text: "Compute $2^2$.".to_string(),
        answer: "4".to_string(),
        canon: cadus_core::answer::canon(&ast).unwrap(),
        instance_hash: "hash-4".to_string(),
    };
    let row = NewInstance::from_instance(&instance, Source::Template, Some("d1".to_string()), 9);
    assert_eq!(row.source, Source::Template);
    assert_eq!(row.content_digest.as_deref(), Some("d1"));
    assert_eq!(row.problem.text, "Compute $2^2$.");
    assert_eq!(row.problem.seed, 9);
    assert_eq!(row.expected_answer.answer, "4");
    assert_eq!(row.instance_hash, "hash-4");
}

/// The batch insert reports a refused statement, and the transaction wrapper
/// reports a closed pool, a refused insert, and a failed commit.
#[tokio::test]
async fn the_batch_insert_reports_every_failed_step() {
    TestDb::with(|db| async move {
        let user = db.seed_user("insert@example.test").await;
        let rows = vec![new_instance(1)];

        let dead = dead_pool(&db).await;
        let mut tx = dead.begin().await.err();
        assert!(tx.take().is_some(), "a dead backend refuses BEGIN");
        let closed = closed_pool(&db).await;
        let err = insert_batch_for_user(&closed, user, KP, &rows)
            .await
            .unwrap_err();
        assert_eq!(store_sqlstate(&err), "none");

        revoke(&db, "INSERT", "serving_pool").await;
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        let err = insert_batch(&mut *tx, user, KP, &rows).await.unwrap_err();
        assert_eq!(store_sqlstate(&err), "42501");
        tx.rollback().await.unwrap();
        let err = insert_batch_for_user(&db.app, user, KP, &rows)
            .await
            .unwrap_err();
        assert_eq!(store_sqlstate(&err), "42501");
    })
    .await;
}

/// A commit that fails after the insert is the error of the commit.
#[tokio::test]
async fn a_failed_commit_after_the_insert_is_the_error_of_the_commit() {
    TestDb::with(|db| async move {
        let user = db.seed_user("commit@example.test").await;
        fail_commit_on(&db, "serving_pool").await;
        let mut row = new_instance(1);
        row.content_digest = None;
        let err = insert_batch_for_user(&db.app, user, KP, &[row])
            .await
            .unwrap_err();
        assert_eq!(store_sqlstate(&err), "P0001");
        assert_eq!(unclaimed_depth(&db.admin, user, KP).await.unwrap(), 0);
    })
    .await;
}

/// The pop reports a refused read, a refused claim, and a refused retire of
/// an undecodable row; the transaction wrapper reports a closed pool and a
/// failed commit.
#[tokio::test]
async fn the_pop_reports_every_failed_statement() {
    TestDb::with(|db| async move {
        let user = db.seed_user("pop@example.test").await;
        seed_pool_rows(&db.admin, user, KP, 2).await;
        let ring = Ring::new();
        let task = TaskMemory::new();
        let avoid = Avoid::new(&ring, &task);

        let closed = closed_pool(&db).await;
        let err = pop_with_ring(&closed, user, KP, &avoid).await.unwrap_err();
        assert_eq!(store_sqlstate(&err), "none");

        revoke(&db, "UPDATE", "serving_pool").await;
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        let err = pop_with_ring_tx(&mut tx, user, KP, &avoid)
            .await
            .unwrap_err();
        assert_eq!(store_sqlstate(&err), "42501", "the claim is refused");
        tx.rollback().await.unwrap();

        sqlx::query("UPDATE serving_pool SET problem = '{}'::jsonb")
            .execute(&db.admin)
            .await
            .unwrap();
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        let err = pop_with_ring_tx(&mut tx, user, KP, &avoid)
            .await
            .unwrap_err();
        assert_eq!(store_sqlstate(&err), "42501", "the retire is refused");
        tx.rollback().await.unwrap();

        revoke(&db, "SELECT", "serving_pool").await;
        let err = pop_with_ring(&db.app, user, KP, &avoid).await.unwrap_err();
        assert_eq!(store_sqlstate(&err), "42501", "the read is refused");
    })
    .await;
}

/// A commit that fails after the claim is the error of the commit, and a
/// claim that writes no row is the typed pool error.
#[tokio::test]
async fn a_claim_that_writes_no_row_and_a_failed_commit_are_reported() {
    TestDb::with(|db| async move {
        let user = db.seed_user("claim@example.test").await;
        seed_pool_rows(&db.admin, user, KP, 2).await;
        let ring = Ring::new();
        let task = TaskMemory::new();
        let avoid = Avoid::new(&ring, &task);

        fail_commit_on(&db, "serving_pool").await;
        let err = pop_with_ring(&db.app, user, KP, &avoid).await.unwrap_err();
        assert_eq!(store_sqlstate(&err), "P0001");

        skip_updates_on(&db, "serving_pool").await;
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        let err = pop_with_ring_tx(&mut tx, user, KP, &avoid)
            .await
            .unwrap_err();
        assert!(
            pool_row_message(&err).contains("was claimed by another transaction"),
            "{err}"
        );
        tx.rollback().await.unwrap();
    })
    .await;
}

/// The exemplar rotation reports a refused read and a refused re-stamp.
#[tokio::test]
async fn the_rotation_reports_a_refused_read_and_a_refused_restamp() {
    TestDb::with(|db| async move {
        let user = db.seed_user("rotate@example.test").await;
        let id = seed_pool_row(&db.admin, user, KP, 0, Source::Exemplar).await;
        sqlx::query("UPDATE serving_pool SET claimed_at = now() WHERE id = $1")
            .bind(id)
            .execute(&db.admin)
            .await
            .unwrap();
        let ring = Ring::new();
        let task = TaskMemory::new();
        let avoid = Avoid::new(&ring, &task);

        fail_on(&db, "UPDATE", "serving_pool").await;
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        let err = reclaim_exemplar_tx(&mut tx, user, KP, &avoid)
            .await
            .unwrap_err();
        assert_eq!(store_sqlstate(&err), "P0001", "the re-stamp is refused");
        tx.rollback().await.unwrap();

        revoke(&db, "SELECT", "serving_pool").await;
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        let err = reclaim_exemplar_tx(&mut tx, user, KP, &avoid)
            .await
            .unwrap_err();
        assert_eq!(store_sqlstate(&err), "42501", "the read is refused");
        tx.rollback().await.unwrap();
    })
    .await;
}

/// Every refill read reports a closed pool.
#[tokio::test]
async fn every_refill_read_reports_a_closed_pool() {
    TestDb::with(|db| async move {
        let pool = closed_pool(&db).await;
        assert!(retire_unapproved(&pool).await.is_err());
        assert!(unclaimed_depth(&pool, Uuid::nil(), KP).await.is_err());
        assert!(refill_targets(&pool, 8, 8).await.is_err());
        assert!(approved_template(&pool, KP).await.is_err());
        assert!(operator_flags(&pool).await.is_err());
    })
    .await;
}

/// A claimed row whose source a later migration wrote stops the operator
/// flags with the typed error that names the knowledge point and the value.
#[tokio::test]
async fn a_source_this_build_does_not_know_stops_the_operator_flags() {
    TestDb::with(|db| async move {
        let user = db.seed_user("oracle@example.test").await;
        let id = seed_pool_row(&db.admin, user, KP, 0, Source::Template).await;
        drop_checks(&db, "serving_pool").await;
        sqlx::query("UPDATE serving_pool SET source = 'oracle', claimed_at = now() WHERE id = $1")
            .bind(id)
            .execute(&db.admin)
            .await
            .unwrap();
        let err = operator_flags(&db.admin).await.unwrap_err();
        assert_eq!(
            pool_row_message(&err),
            "knowledge point perfect-squares/kp1 last served source \"oracle\", which this build \
             does not know"
        );
    })
    .await;
}
