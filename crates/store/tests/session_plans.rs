//! The frozen plan of the day (D-DP1): `save_session_plan` and
//! `load_session_plan` round-trip under the tenant setting, and a second tenant
//! reads nothing (`migrations/0006_grants_rls.sql`, `tenant_isolation`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_store::begin_tenant;
use cadus_store::state::{load_session_plan, save_session_plan};
use cadus_store::test_support::TestDb;
use common::app_db as app;
use serde_json::json;

#[tokio::test]
async fn the_plan_round_trips_and_a_second_tenant_reads_none() {
    TestDb::with(|db| async move {
        let alice = db.seed_user("plan-alice@example.com").await;
        let bob = db.seed_user("plan-bob@example.com").await;
        let handle = app(&db);

        let mut tx = begin_tenant(handle.pool(), alice).await.unwrap();
        assert_eq!(load_session_plan(&mut tx, alice).await.unwrap(), None);
        let first = json!({"day": "2026-10-26", "set": [], "read": null});
        save_session_plan(&mut tx, alice, "s_2026-10-26a", &first)
            .await
            .unwrap();
        let second = json!({"day": "2026-10-27", "set": [], "read": null});
        save_session_plan(&mut tx, alice, "s_2026-10-27a", &second)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let mut tx = begin_tenant(handle.pool(), alice).await.unwrap();
        let read = load_session_plan(&mut tx, alice).await.unwrap();
        tx.rollback().await.unwrap();
        assert_eq!(read, Some(("s_2026-10-27a".to_string(), second)));

        let mut tx = begin_tenant(handle.pool(), bob).await.unwrap();
        let leaked = load_session_plan(&mut tx, alice).await.unwrap();
        tx.rollback().await.unwrap();
        assert_eq!(leaked, None, "the policy let another tenant read the row");
    })
    .await;
}
