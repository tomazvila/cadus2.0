//! The generic local-draft importer stores the arithmetic-core drafts as
//! pending rows through the real CLI, with zero model cost, and once only (C6).
#![allow(clippy::unwrap_used)]
mod common;
use std::path::{Path, PathBuf};

use cadus_store::test_support::TestDb;

/// The number of drafts the arithmetic-core manifest names.
const DRAFTS: i64 = 162;

fn manifest() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/content-foundations/arithmetic-core/manifest.json")
}

#[tokio::test]
#[ignore]
async fn the_import_is_pending_only_costs_nothing_and_a_second_pass_skips() {
    TestDb::with(|db| async move {
        let dsn = common::superuser_dsn(&db.name);
        for pass in 0..2 {
            let output = common::authoring::import_local_drafts(&manifest(), &dsn).await;
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(output.status.success(), "{stdout}{}", String::from_utf8_lossy(&output.stderr));
            assert!(stdout.contains("model cost reported 0 micro-USD"), "{stdout}");
            let summary = if pass == 0 {
                "local drafts: stored 162 skipped 0 declined 0 calls 162"
            } else {
                "local drafts: stored 0 skipped 162 declined 0 calls 0"
            };
            assert!(stdout.contains(summary), "{stdout}");
        }
        let counts: (i64, i64) = sqlx::query_as(
            "SELECT count(*), count(*) FILTER (WHERE status = 'pending') FROM content_store",
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(counts, (DRAFTS, DRAFTS));
        let ledger: (i64, bool, bool) = sqlx::query_as(
            "SELECT count(*), coalesce(sum(cost_usd), 0) = 0, bool_and(model_id = 'operator-draft-v1') \
             FROM model_call_log",
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(ledger, (DRAFTS, true, true));
    })
    .await;
}

#[tokio::test]
async fn an_invalid_manifest_is_refused_before_any_process_starts() {
    TestDb::with(|db| async move {
        let scratch = Path::new(&std::env::var("HOME").unwrap()).join(".cache/cadus2_tmp");
        std::fs::create_dir_all(&scratch).unwrap();
        let path = scratch.join(format!("{}-duplicate.json", db.name));
        let row = serde_json::json!({
            "kp_id": "single-digit-addition/kp1",
            "kind": "hint_ladder",
            "arguments": {"hints": ["Which number do you start from?"]}
        });
        std::fs::write(&path, serde_json::json!([row, row]).to_string()).unwrap();
        let output =
            common::authoring::import_local_drafts(&path, &common::superuser_dsn(&db.name)).await;
        std::fs::remove_file(&path).unwrap();
        assert_eq!(output.status.code(), Some(2));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("duplicate draft key"), "{stderr}");
        let rows: (i64,) = sqlx::query_as("SELECT count(*) FROM content_store")
            .fetch_one(&db.admin)
            .await
            .unwrap();
        assert_eq!(rows.0, 0);
    })
    .await;
}
