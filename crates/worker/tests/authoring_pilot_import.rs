//! The operator-draft helper reaches six pending rows through the real CLI (C6).
#![allow(clippy::unwrap_used)]
mod common;
use cadus_store::test_support::TestDb;

#[tokio::test]
async fn the_zero_cost_import_is_pending_only_and_a_second_pass_skips() {
    TestDb::with(|db| async move {
        let dsn = common::superuser_dsn(&db.name);
        for _ in 0..2 {
            let output = common::authoring::import_pilot_drafts(&dsn, false).await;
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(String::from_utf8_lossy(&output.stdout).contains("reported: 0 micro-USD"));
        }
        let counts: (i64, i64) = sqlx::query_as(
            "SELECT count(*), count(*) FILTER (WHERE status = 'pending') FROM content_store",
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(counts, (6, 6));
        let ledger: (i64, bool) =
            sqlx::query_as("SELECT count(*), coalesce(sum(cost_usd), 0) = 0 FROM model_call_log")
                .fetch_one(&db.admin)
                .await
                .unwrap();
        assert_eq!(ledger.0, 6);
        assert!(ledger.1);
    })
    .await;
}

#[tokio::test]
async fn the_full_zero_cost_pilot_stores_three_templates_and_six_instruction_drafts() {
    TestDb::with(|db| async move {
        let output =
            common::authoring::import_pilot_drafts(&common::superuser_dsn(&db.name), true).await;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let counts: (i64, i64) = sqlx::query_as(
            "SELECT count(*), count(*) FILTER (WHERE status = 'pending') FROM content_store",
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(counts, (9, 9));
        let ledger: (i64, bool) =
            sqlx::query_as("SELECT count(*), coalesce(sum(cost_usd), 0) = 0 FROM model_call_log")
                .fetch_one(&db.admin)
                .await
                .unwrap();
        assert_eq!(ledger, (9, true));
    })
    .await;
}
