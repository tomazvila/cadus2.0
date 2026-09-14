//! Explicitly opted-in live Qwen/verifier acceptance on a disposable database.
//! No production account, event log, or application database is used.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::future::pending;
use std::time::Duration;

use cadus_core::event::Event;
use cadus_store::reports;
use cadus_store::state::{append_event, lock_web_state};
use cadus_store::test_support::TestDb;
use cadus_store::{Db, begin_tenant};
use cadus_worker::reports::{ReportConfig, run};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::types::Uuid;

const SESSION: &str = "s_2026-01-01a";
const TASK: &str = "s_2026-01-01a-lesson-addition";
const ATTEMPT: &str = "s_2026-01-01a-lesson-addition-live-report";
const PROBLEM: &str = "live-report-two-plus-two";

async fn worker_db(db: &TestDb) -> Db {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .after_connect(|connection, _| {
            Box::pin(async move {
                sqlx::query("SET ROLE cadus_admin")
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect_with(db.admin.connect_options().as_ref().clone())
        .await
        .unwrap();
    Db::new(pool, 3000)
}

fn event(value: Value) -> Event {
    Event::from_json(&value.to_string()).unwrap()
}

async fn seed(db: &TestDb) -> (Uuid, Uuid, Vec<Value>) {
    let user = db.seed_user("live-report-isolated@example.com").await;
    let attempt = event(json!({
        "type":"attempt","v":2,"ts":"2026-01-01T00:00:00Z","session":SESSION,
        "attempt_id":ATTEMPT,"task_id":TASK,"topic":"addition","kp":"kp1",
        "task_type":"lesson","problem":{"text":"Compute 2 + 2.","expected":"5"},
        "given_answer":"4","answer_kind":"numeric","correct":false,
        "outcome":"incorrect","secs":12,"work_quality":"nearly_passable","assisted":false
    }));
    let originals = [
        event(json!({
            "type":"session_start","v":2,"ts":"2026-01-01T00:00:00Z","session":SESSION
        })),
        event(json!({
            "type":"ordinary_problem_served","v":2,"ts":"2026-01-01T00:00:00Z",
            "session":SESSION,"task_id":TASK,"problem_id":PROBLEM,"kp_id":"addition/kp1",
            "item_digest":"a".repeat(64),"item_source":"exemplar","exposure":"first"
        })),
        attempt,
    ];
    let mut tx = begin_tenant(&db.app, user).await.unwrap();
    lock_web_state(&mut tx, user).await.unwrap();
    for (index, item) in originals.iter().enumerate() {
        let idempotency = if index == 2 { Some(ATTEMPT) } else { None };
        assert_eq!(
            append_event(&mut tx, user, item, idempotency)
                .await
                .unwrap(),
            Some(i64::try_from(index + 1).unwrap())
        );
    }
    let recorded = serde_json::to_value(&originals[2]).unwrap();
    let source = json!({
        "problem":recorded["problem"],
        "curriculum_digest":"b".repeat(64),
        "engine_digest":cadus_core::review_engine::DIGEST
    });
    let source_hash = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&reports::source_identity(&source)).unwrap())
    );
    let input = json!({
        "attempt":recorded,"event_seq":3,"source":source,
        "note":"I answered four, but the expected answer says five. Please check 2 + 2.",
        "problem_id":PROBLEM,"correction_version":0,"previous_correction":null,
        "task_policy":{"config":cadus_core::config::Config::default(),"knowledge_points":["kp1"],"expected_time_secs":30}
    });
    assert!(reports::capacity(&mut tx, user, ATTEMPT).await.unwrap());
    let report = reports::enqueue(
        &mut tx,
        user,
        Uuid::new_v4(),
        TASK,
        PROBLEM,
        ATTEMPT,
        &source_hash,
        &input,
    )
    .await
    .unwrap();
    let report_id = report["id"].as_str().unwrap().parse().unwrap();
    tx.commit().await.unwrap();
    let immutable = sqlx::query_scalar::<_, Value>(
        "SELECT payload FROM events WHERE user_id=$1 AND seq<=3 ORDER BY seq",
    )
    .bind(user)
    .fetch_all(&db.admin)
    .await
    .unwrap();
    (user, report_id, immutable)
}

async fn terminal(db: &TestDb, report: Uuid) -> Result<Value, String> {
    let mut previous = String::new();
    loop {
        let row: Value = sqlx::query_scalar(
            "SELECT jsonb_build_object('status',status,'stage',stage,'attempt',attempt,'result',result) \
             FROM problem_reports WHERE id=$1",
        ).bind(report).fetch_one(&db.admin).await
            .map_err(|_| "live report status query failed".to_owned())?;
        let status = row["status"].as_str().unwrap_or("missing");
        let stage = row["stage"].as_str().unwrap_or("missing");
        let current = format!("{status}/{stage}/{}", row["attempt"]);
        if previous != current {
            eprintln!("live report: {current}");
            previous = current;
        }
        if ["completed", "unresolved", "failed"].contains(&status) {
            // This contains public outcome text for the synthetic arithmetic
            // fixture; private model responses and endpoint credentials stay out.
            let message = row["result"]["message"].as_str().unwrap_or("");
            eprintln!(
                "live report result: {}",
                message.chars().take(600).collect::<String>()
            );
            return Ok(row);
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

#[tokio::test]
#[ignore = "requires CADUS_QWEN_LIVE_TEST=1, disposable test DB, actual Qwen and verifier"]
async fn live_qwen_report_corrects_two_plus_two_with_independent_proof() {
    assert_eq!(
        std::env::var("CADUS_QWEN_LIVE_TEST").ok().as_deref(),
        Some("1"),
        "explicit CADUS_QWEN_LIVE_TEST=1 opt-in is required"
    );
    assert!(
        std::env::var("CADUS_TEST_DATABASE_URL").is_ok(),
        "CADUS_TEST_DATABASE_URL must name the isolated test cluster"
    );
    let config = ReportConfig::from_env().unwrap();
    TestDb::with(|db| async move {
        let (user, report_id, immutable) = seed(&db).await;
        let worker = worker_db(&db).await;
        // No detached task or OS process: leaving this scope drops the graph,
        // its heartbeat, and any in-flight HTTP request on every exit path.
        let outcome = {
            let running = run(&worker, &config, pending::<()>());
            tokio::pin!(running);
            tokio::select! {
                result = &mut running => Err(match result {
                    Ok(()) => "worker stopped before a terminal report".to_owned(),
                    Err(error) => format!("worker stopped: {error}"),
                }),
                result = tokio::time::timeout(Duration::from_secs(15 * 60), terminal(&db, report_id)) => {
                    result.unwrap_or_else(|_| Err("live report exceeded 15 minutes".to_owned()))
                }
            }
        };
        worker.pool().close().await;
        let final_row = outcome.unwrap();
        assert_eq!(final_row["status"], "completed", "{final_row}");
        let result = &final_row["result"];
        assert_eq!(result["resolution"], "confirmed_issue", "{final_row}");
        assert_eq!(result["qwen_verdict"], "correct", "{final_row}");
        assert_eq!(result["verification"], "proved", "{final_row}");
        assert_eq!(result["grade_corrected"], true, "{final_row}");
        assert_eq!(result["content_published"], true, "{final_row}");
        let after: Vec<Value> = sqlx::query_scalar(
            "SELECT payload FROM events WHERE user_id=$1 AND seq<=3 ORDER BY seq",
        ).bind(user).fetch_all(&db.admin).await.unwrap();
        assert_eq!(after, immutable, "the original native events must remain immutable");
        let regraded: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM events WHERE user_id=$1 AND type='regraded'",
        ).bind(user).fetch_one(&db.admin).await.unwrap();
        assert_eq!(regraded, 1, "publication must append exactly one native correction");
        let correction: Value = sqlx::query_scalar(
            "SELECT body FROM problem_corrections WHERE report_id=$1",
        ).bind(report_id).fetch_one(&db.admin).await.unwrap();
        assert_eq!(correction["verification"]["supported"], true);
        assert_eq!(correction["verification"]["status"], "completed");
        assert_eq!(correction["verification"]["learner"]["status"], "proved");
        assert_eq!(correction["verification"]["candidate"]["status"], "proved");
        assert!(correction["accepted_answers"].as_array().unwrap().iter().any(|value| value == "4"));
    }).await;
}
