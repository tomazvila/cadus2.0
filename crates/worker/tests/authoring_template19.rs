//! The exact production-declined cohort crosses the real pending-only worker.
#![allow(clippy::unwrap_used)]

mod common;

use std::collections::BTreeSet;
use std::path::Path;

use cadus_core::curriculum::{Curriculum, load_curriculum};
use cadus_store::test_support::TestDb;
use cadus_worker::authoring::cli::select;
use cadus_worker::authoring::job::{AuthoringJob, Outcome, Report, author_one};
use cadus_worker::authoring::prompt::Kind;
use common::{FakeModel, content_rows, handle, reply};
use serde_json::{Value, json};

const KEYS: [&str; 19] = [
    "basic-absolute-value-inequalities/kp3",
    "comparing-integers/kp2",
    "converting-to-vertex-form/kp3",
    "law-of-sines-cosines/kp1",
    "logarithm-basics/kp1",
    "logarithm-basics/kp2",
    "parabola-vertex-form/kp3",
    "percentages/kp3",
    "pythagorean-converse/kp3",
    "quadratic-applications/kp1",
    "quadratic-applications/kp2",
    "quadratic-graphs-vertex/kp2",
    "radical-equations-basic/kp1",
    "radical-equations-basic/kp2",
    "radical-equations-basic/kp3",
    "ratio-tables-equivalent-ratios/kp3",
    "trig-applications/kp3",
    "understanding-ratios/kp3",
    "unit-rates/kp2",
];

fn drafts() -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/content-foundations/template19-production-gate/drafts.json");
    let rows: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let keys: BTreeSet<_> = rows
        .iter()
        .map(|row| row["kp_id"].as_str().unwrap())
        .collect();
    assert_eq!(rows.len(), 19);
    assert_eq!(keys, BTreeSet::from(KEYS));
    assert!(rows.iter().all(|row| row["kind"] == "template"));
    rows
}

fn free_reply(arguments: &Value) -> (u16, String) {
    reply(
        Kind::Template.tool_name(),
        &arguments.to_string(),
        Some(json!({"prompt_tokens":0,"completion_tokens":0,"cost":0})),
    )
}

async fn import_row(
    db: &TestDb,
    curriculum: &Curriculum,
    job: &AuthoringJob,
    row: &Value,
) -> Report {
    let key = row["kp_id"].as_str().unwrap();
    let spec = select(curriculum, &[key.to_owned()]).unwrap().remove(0);
    author_one(&handle(db), job, Kind::Template, &spec)
        .await
        .unwrap()
}

#[tokio::test]
async fn all_nineteen_import_at_zero_cost_as_pending_and_missing_only_skips_them() {
    TestDb::with(|db| async move {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../curriculum");
        let (curriculum, findings) = load_curriculum(&root).unwrap();
        assert!(findings.is_empty());
        let rows = drafts();
        let fake = FakeModel::start(
            rows.iter()
                .map(|row| free_reply(&row["arguments"]))
                .collect(),
        )
        .await;
        let job = fake.job_with_attempts(1).with_missing_only(true);
        for row in &rows {
            let key = row["kp_id"].as_str().unwrap();
            let result = import_row(&db, &curriculum, &job, row).await;
            assert_eq!(result.outcome, Outcome::Stored, "{key}: {result:?}");
            assert_eq!(result.attempts, 1, "{key}");
            assert!(result.decline.is_none(), "{key}");
            assert!(result.cost_usd.as_deref().is_some_and(|cost| {
                cost.contains('0') && cost.chars().all(|ch| ch == '0' || ch == '.')
            }));
            let stored = content_rows(&db.admin, key).await;
            assert_eq!(stored.len(), 1, "{key}");
            assert_eq!(stored[0].status, "pending", "{key}");
            assert_eq!(
                stored[0].body["answer_contract"], row["arguments"]["answer_contract"],
                "{key}: typed policy changed while crossing the worker"
            );
            let again = author_one(&handle(&db), &job, Kind::Template, &spec)
                .await
                .unwrap();
            assert_eq!(again.outcome, Outcome::Skipped, "{key}");
            assert_eq!(again.attempts, 0, "{key}");
        }
        assert_eq!(fake.call_count(), 19);
        let counts: (i64, i64) = sqlx::query_as(
            "SELECT count(*), count(*) FILTER (WHERE status = 'pending') FROM content_store",
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(counts, (19, 19));
        let ledger: (i64, bool) = sqlx::query_as(
            "SELECT count(*), bool_and(cost_usd IS NOT NULL AND cost_usd = 0) FROM model_call_log",
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(ledger, (19, true));
    })
    .await;
}

#[tokio::test]
async fn no_recipe_can_import_without_its_required_typed_contract() {
    TestDb::with(|db| async move {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../curriculum");
        let (curriculum, _) = load_curriculum(&root).unwrap();
        let mut rows = drafts();
        for row in &mut rows {
            row["arguments"]["answer_contract"] = json!({"kind":"none"});
        }
        let fake = FakeModel::start(
            rows.iter()
                .map(|row| free_reply(&row["arguments"]))
                .collect(),
        )
        .await;
        let job = fake.job_with_attempts(1);
        for row in &rows {
            let key = row["kp_id"].as_str().unwrap();
            let result = import_row(&db, &curriculum, &job, row).await;
            assert_eq!(result.outcome, Outcome::Declined, "{key}: {result:?}");
            assert_eq!(result.attempts, 1, "{key}");
            assert!(content_rows(&db.admin, key).await.is_empty(), "{key}");
        }
        assert_eq!(fake.call_count(), 19);
        let counts: (i64,) = sqlx::query_as("SELECT count(*) FROM content_store")
            .fetch_one(&db.admin)
            .await
            .unwrap();
        assert_eq!(counts.0, 0);
    })
    .await;
}
