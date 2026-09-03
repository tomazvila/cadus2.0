//! The in-progress placement diagnostic: one document per tenant, written,
//! read, and dropped inside the tenant transaction, and refused with the
//! privilege it lacks.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use cadus_store::begin_tenant;
use cadus_store::state::{clear_diag_state, load_diag_state, save_diag_state};
use cadus_store::test_support::TestDb;
use common::fault::revoke;
use common::store_sqlstate;
use serde_json::json;

/// The document round-trips, a second save replaces it, another tenant reads
/// none, and the clear removes it.
#[tokio::test]
async fn the_diagnostic_document_round_trips_inside_the_tenant() {
    TestDb::with(|db| async move {
        let alice = db.seed_user("alice@example.test").await;
        let bob = db.seed_user("bob@example.test").await;

        let mut tx = begin_tenant(&db.app, alice).await.unwrap();
        assert_eq!(load_diag_state(&mut tx, alice).await.unwrap(), None);
        save_diag_state(&mut tx, alice, &json!({"step": 1}))
            .await
            .unwrap();
        save_diag_state(&mut tx, alice, &json!({"step": 2}))
            .await
            .unwrap();
        assert_eq!(
            load_diag_state(&mut tx, alice).await.unwrap(),
            Some(json!({"step": 2}))
        );
        tx.commit().await.unwrap();

        let mut tx = begin_tenant(&db.app, bob).await.unwrap();
        assert_eq!(load_diag_state(&mut tx, alice).await.unwrap(), None);
        tx.rollback().await.unwrap();

        let mut tx = begin_tenant(&db.app, alice).await.unwrap();
        clear_diag_state(&mut tx, alice).await.unwrap();
        assert_eq!(load_diag_state(&mut tx, alice).await.unwrap(), None);
        tx.commit().await.unwrap();
    })
    .await;
}

/// Every statement on `diag_states` reports the privilege it lacks.
///
/// A refused statement aborts its transaction, so each one gets its own.
#[tokio::test]
async fn every_diagnostic_statement_reports_a_refused_privilege() {
    TestDb::with(|db| async move {
        let alice = db.seed_user("alice@example.test").await;
        revoke(&db, "SELECT, INSERT, DELETE", "diag_states").await;

        let mut tx = begin_tenant(&db.app, alice).await.unwrap();
        let err = load_diag_state(&mut tx, alice).await.unwrap_err();
        assert_eq!(store_sqlstate(&err), "42501", "{err:?}");
        tx.rollback().await.unwrap();

        let mut tx = begin_tenant(&db.app, alice).await.unwrap();
        let err = save_diag_state(&mut tx, alice, &json!({}))
            .await
            .unwrap_err();
        assert_eq!(store_sqlstate(&err), "42501", "{err:?}");
        tx.rollback().await.unwrap();

        let mut tx = begin_tenant(&db.app, alice).await.unwrap();
        let err = clear_diag_state(&mut tx, alice).await.unwrap_err();
        assert_eq!(store_sqlstate(&err), "42501", "{err:?}");
        tx.rollback().await.unwrap();
    })
    .await;
}
