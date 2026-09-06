//! The held-out unit drafts import as pending rows, with zero model cost, and
//! once only (C6) — the same generic importer arithmetic-core already uses.
#![allow(clippy::unwrap_used)]
mod common;
use std::path::{Path, PathBuf};

use cadus_store::test_support::TestDb;

/// Unit id to the number of knowledge points its manifest names (so drafts = 2x).
const UNITS: &[(&str, i64)] = &[
    ("fractions-decimals", 20),
    ("integers-negatives", 14),
    ("rational-trig", 1),
];

fn script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/authoring/import_local_drafts.py")
}

fn manifest(unit: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/content-foundations")
        .join(unit)
        .join("manifest.json")
}

async fn import(manifest: &Path, dsn: &str) -> std::process::Output {
    tokio::process::Command::new("python3")
        .arg(script())
        .arg("--manifest")
        .arg(manifest)
        .arg("--worker")
        .arg(env!("CARGO_BIN_EXE_cadus-worker"))
        .env("DATABASE_URL", dsn)
        .output()
        .await
        .unwrap()
}

#[tokio::test]
async fn every_unit_imports_pending_only_at_zero_cost_and_a_second_pass_skips() {
    TestDb::with(|db| async move {
        let dsn = common::superuser_dsn(&db.name);
        let total_drafts: i64 = UNITS.iter().map(|(_, kps)| kps * 2).sum();
        for pass in 0..2 {
            for (unit, kps) in UNITS {
                let output = import(&manifest(unit), &dsn).await;
                let stdout = String::from_utf8_lossy(&output.stdout);
                assert!(
                    output.status.success(),
                    "{unit}: {stdout}{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert!(
                    stdout.contains("model cost reported 0 micro-USD"),
                    "{unit}: {stdout}"
                );
                let drafts = kps * 2;
                let summary = if pass == 0 {
                    format!("local drafts: stored {drafts} skipped 0 declined 0 calls {drafts}")
                } else {
                    format!("local drafts: stored 0 skipped {drafts} declined 0 calls 0")
                };
                assert!(stdout.contains(&summary), "{unit}: {stdout}");
            }
        }
        let counts: (i64, i64) = sqlx::query_as(
            "SELECT count(*), count(*) FILTER (WHERE status = 'pending') FROM content_store",
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(counts, (total_drafts, total_drafts));
        let approved: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM content_store WHERE status = 'approved'",
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(approved.0, 0, "no held-out draft is ever approved by this importer");
        let ledger: (i64, bool, bool) = sqlx::query_as(
            "SELECT count(*), coalesce(sum(cost_usd), 0) = 0, bool_and(model_id = 'operator-draft-v1') \
             FROM model_call_log",
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(ledger, (total_drafts, true, true));
    })
    .await;
}
