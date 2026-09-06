//! The operator-draft helper reaches six pending rows through the real CLI (C6).
#![allow(clippy::unwrap_used)]
mod common;
use cadus_store::test_support::TestDb;

#[tokio::test]
async fn the_zero_cost_import_is_pending_only_and_a_second_pass_skips() {
    TestDb::with(|db| async move {
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/authoring/import_pilot_drafts.py");
        for _ in 0..2 {
            let output = tokio::process::Command::new("python3")
                .arg(&script)
                .arg("--worker")
                .arg(env!("CARGO_BIN_EXE_cadus-worker"))
                .env("DATABASE_URL", common::superuser_dsn(&db.name))
                .output()
                .await
                .unwrap();
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
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/authoring/import_pilot_drafts.py");
        let output = tokio::process::Command::new("python3")
            .arg(&script)
            .arg("--worker")
            .arg(env!("CARGO_BIN_EXE_cadus-worker"))
            .arg("--include-templates")
            .env("DATABASE_URL", common::superuser_dsn(&db.name))
            .output()
            .await
            .unwrap();
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
