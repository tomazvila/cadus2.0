//! M5 U11 acceptance: the four new `/metrics` series (T6, spec section 7).
//!
//! Spec `docs/reference/web-service-1.0-spec.md` section 7, last paragraph, and
//! row U11 of section 11. The series are:
//!
//! - `cadus_deterministic_grade_total{result}`, counted in this process;
//! - `cadus_diagnosis_jobs_total{result}`, one label counted here and the rest
//!   read from `diagnosis_jobs`;
//! - `cadus_model_call_tokens_total{purpose,kind}` and
//!   `cadus_model_call_latency_seconds{purpose}`, read from `model_call_log`.
//!
//! The model calls run in `cadus-worker`, a different process, so a counter in
//! this one reports zero for ever. The scrape reads those series through
//! the two SECURITY DEFINER aggregates of `migrations/0009_metrics_readers.sql`
//! on the `cadus_app` connection of the request tier.
//!
//! Every expected line below is a LITERAL of this file. Nothing is rendered by
//! the code under test and compared with itself.
//!
//! `crates/web/tests/grade_route.rs` holds the counter half of the acceptance:
//! the five `result` labels, counted over HTTP by the grade route.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::scrape;

use axum::Router;
use axum::body::Body;
use axum::http::Request;
use cadus_core::event::WorkQuality;
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::grade::Grade;
use cadus_web::metrics::{LedgerTotals, PurposeTotals, grade_result, render_ledger};
use cadus_web::{AppState, create_app};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use sqlx::types::Uuid;
use tower::ServiceExt;

/// The application under test, on the `cadus_app` pool of `db`.
fn app_of(db: &TestDb) -> Router {
    create_app(AppState::new(Db::new(
        db.app.clone(),
        DEFAULT_CLIENT_TIMEOUT_MS,
    )))
}

/// An application whose pool points at an address with no server.
fn offline_app() -> Router {
    let pool = PgPoolOptions::new()
        .connect_lazy("postgresql://nobody@127.0.0.1:1/nodb")
        .expect("a lazy pool needs no server");
    create_app(AppState::new(Db::new(pool, DEFAULT_CLIENT_TIMEOUT_MS)))
}

/// Assert that `text` holds `line`, and print the whole scrape when it does not.
fn holds(text: &str, line: &str) {
    assert!(
        text.contains(&format!("{line}\n")),
        "the scrape carries no line {line:?}:\n{text}"
    );
}

/// One ledger row, written the way the worker writes it (as the migration
/// runner here, which is what the test cluster gives).
async fn seed_call(
    pool: &PgPool,
    purpose: &str,
    user: Uuid,
    tokens: (i32, i32, i32, i32),
    latency_ms: i32,
) {
    sqlx::query!(
        "INSERT INTO model_call_log
             (purpose, model_id, user_id, session_id, input_tokens_cached,
              input_tokens_uncached, output_tokens, reasoning_tokens, latency_ms)
         VALUES ($1, 'deepseek/deepseek-v4-pro', $2, 'session-1', $3, $4, $5, $6, $7)",
        purpose,
        user,
        tokens.0,
        tokens.1,
        tokens.2,
        tokens.3,
        latency_ms,
    )
    .execute(pool)
    .await
    .unwrap();
}

/// One queue row in `status`.
async fn seed_job(pool: &PgPool, user: Uuid, attempt: &str, status: &str) {
    sqlx::query!(
        "INSERT INTO diagnosis_jobs (user_id, attempt_id, payload, status)
         VALUES ($1, $2, '{}'::jsonb, $3)",
        user,
        attempt,
        status,
    )
    .execute(pool)
    .await
    .unwrap();
}

// --------------------------------------------------------------------------- //
// The grade label of one verdict
// --------------------------------------------------------------------------- //

/// The four labels a graded submission can carry, from the three server tags.
///
/// `undecidable` is not one of them: it belongs to the answer KIND, and the
/// route counts it where it refuses the kind (`grade_route.rs`).
#[test]
fn the_grade_label_of_a_verdict_is_the_one_the_tags_name() {
    let correct = Grade {
        correct: true,
        work_quality: WorkQuality::NearlyPerfect,
        error_tags: Vec::new(),
    };
    assert_eq!(grade_result(&correct), "correct");

    let notation = Grade {
        correct: true,
        work_quality: WorkQuality::NearlyPerfect,
        error_tags: vec!["notation".to_string()],
    };
    assert_eq!(grade_result(&notation), "notation");

    let blank = Grade {
        correct: false,
        work_quality: WorkQuality::Poor,
        error_tags: vec!["blank-answer".to_string()],
    };
    assert_eq!(grade_result(&blank), "blank");

    let miss = Grade {
        correct: false,
        work_quality: WorkQuality::NearlyPassable,
        error_tags: Vec::new(),
    };
    assert_eq!(grade_result(&miss), "incorrect");

    // A miss the clock tagged is still a miss.
    let slow = Grade {
        correct: false,
        work_quality: WorkQuality::NearlyPassable,
        error_tags: vec!["timing-unreliable".to_string()],
    };
    assert_eq!(grade_result(&slow), "incorrect");
}

// --------------------------------------------------------------------------- //
// The exposition text of the ledger-backed series
// --------------------------------------------------------------------------- //

/// The literal lines of one filled ledger.
///
/// `authoring` renders at zero even though nothing authored yet: a dashboard
/// that silently misses a series reads as a service that never spent there.
#[test]
fn the_ledger_series_render_the_literal_exposition_lines() {
    let totals = LedgerTotals {
        model_calls: vec![PurposeTotals {
            purpose: "diagnosis".to_string(),
            cached: 200,
            uncached: 300,
            output: 20,
            reasoning: 40,
            latency_ms: 2_500,
            calls: 2,
        }],
        jobs: vec![
            ("done".to_string(), 3),
            ("failed".to_string(), 1),
            ("capped".to_string(), 2),
            ("pending".to_string(), 4),
        ],
    };

    let text = render_ledger(&totals, 7);

    holds(
        &text,
        "cadus_diagnosis_jobs_total{result=\"ready_preauthored\"} 7",
    );
    holds(&text, "cadus_diagnosis_jobs_total{result=\"enqueued\"} 10");
    holds(&text, "cadus_diagnosis_jobs_total{result=\"done\"} 3");
    holds(&text, "cadus_diagnosis_jobs_total{result=\"failed\"} 1");
    holds(&text, "cadus_diagnosis_jobs_total{result=\"capped\"} 2");

    holds(
        &text,
        "cadus_model_call_tokens_total{purpose=\"diagnosis\",kind=\"cached\"} 200",
    );
    holds(
        &text,
        "cadus_model_call_tokens_total{purpose=\"diagnosis\",kind=\"uncached\"} 300",
    );
    holds(
        &text,
        "cadus_model_call_tokens_total{purpose=\"diagnosis\",kind=\"output\"} 20",
    );
    holds(
        &text,
        "cadus_model_call_tokens_total{purpose=\"diagnosis\",kind=\"reasoning\"} 40",
    );
    holds(
        &text,
        "cadus_model_call_tokens_total{purpose=\"authoring\",kind=\"cached\"} 0",
    );

    holds(
        &text,
        "cadus_model_call_latency_seconds_sum{purpose=\"diagnosis\"} 2.5",
    );
    holds(
        &text,
        "cadus_model_call_latency_seconds_count{purpose=\"diagnosis\"} 2",
    );
    holds(
        &text,
        "cadus_model_call_latency_seconds_sum{purpose=\"authoring\"} 0",
    );
    holds(
        &text,
        "cadus_model_call_latency_seconds_count{purpose=\"authoring\"} 0",
    );
}

/// An empty ledger renders every series at zero, and no series is missing.
#[test]
fn an_empty_ledger_renders_every_series_at_zero() {
    let text = render_ledger(&LedgerTotals::default(), 0);

    holds(&text, "cadus_diagnosis_jobs_total{result=\"enqueued\"} 0");
    holds(&text, "cadus_diagnosis_jobs_total{result=\"done\"} 0");
    holds(
        &text,
        "cadus_model_call_tokens_total{purpose=\"diagnosis\",kind=\"output\"} 0",
    );
    holds(
        &text,
        "cadus_model_call_latency_seconds_count{purpose=\"diagnosis\"} 0",
    );
}

// --------------------------------------------------------------------------- //
// The scrape end to end
// --------------------------------------------------------------------------- //

/// `GET /metrics` reports the worker's two tables, read as `cadus_app`.
///
/// The rows below are the ones the worker writes. `cadus_app` holds no privilege
/// on `model_call_log` and reads zero rows of `diagnosis_jobs` unbound, so this
/// test fails the moment the scrape stops going through the aggregates of
/// migration 0009.
#[tokio::test]
async fn the_scrape_reports_the_ledger_of_the_worker() {
    TestDb::with(|db| async move {
        let user = db.seed_user("scrape@example.test").await;
        seed_call(&db.admin, "diagnosis", user, (200, 300, 20, 40), 1_500).await;
        seed_call(&db.admin, "diagnosis", user, (0, 100, 10, 0), 1_000).await;
        seed_job(&db.admin, user, "task-1-1", "done").await;
        seed_job(&db.admin, user, "task-2-1", "failed").await;
        seed_job(&db.admin, user, "task-3-1", "pending").await;

        let text = scrape(&app_of(&db)).await;

        holds(&text, "cadus_diagnosis_jobs_total{result=\"enqueued\"} 3");
        holds(&text, "cadus_diagnosis_jobs_total{result=\"done\"} 1");
        holds(&text, "cadus_diagnosis_jobs_total{result=\"failed\"} 1");
        holds(&text, "cadus_diagnosis_jobs_total{result=\"capped\"} 0");
        holds(
            &text,
            "cadus_diagnosis_jobs_total{result=\"ready_preauthored\"} 0",
        );
        holds(
            &text,
            "cadus_model_call_tokens_total{purpose=\"diagnosis\",kind=\"cached\"} 200",
        );
        holds(
            &text,
            "cadus_model_call_tokens_total{purpose=\"diagnosis\",kind=\"uncached\"} 400",
        );
        holds(
            &text,
            "cadus_model_call_tokens_total{purpose=\"diagnosis\",kind=\"output\"} 30",
        );
        holds(
            &text,
            "cadus_model_call_tokens_total{purpose=\"diagnosis\",kind=\"reasoning\"} 40",
        );
        holds(
            &text,
            "cadus_model_call_latency_seconds_sum{purpose=\"diagnosis\"} 2.5",
        );
        holds(
            &text,
            "cadus_model_call_latency_seconds_count{purpose=\"diagnosis\"} 2",
        );
    })
    .await;
}

/// Every `result` label of the grade counter ships, at zero, on a fresh process.
#[tokio::test]
async fn a_fresh_process_ships_every_grade_label_at_zero() {
    TestDb::with(|db| async move {
        let text = scrape(&app_of(&db)).await;

        for result in ["correct", "notation", "blank", "incorrect", "undecidable"] {
            holds(
                &text,
                &format!("cadus_deterministic_grade_total{{result=\"{result}\"}} 0"),
            );
        }
        assert!(
            text.contains("# TYPE cadus_deterministic_grade_total counter\n"),
            "the counter carries no TYPE line:\n{text}"
        );
    })
    .await;
}

/// A datastore that answers nothing drops the ledger series and keeps the rest.
///
/// An operator who cannot see the token counters must still see the request
/// counters, so the endpoint answers 200 with the series it does hold. Zeros
/// are worse than a gap: they read as a counter reset that never happened.
#[tokio::test]
async fn a_failed_ledger_read_never_fails_the_scrape() {
    let app = offline_app();
    let request = Request::builder()
        .method("GET")
        .uri("/api/health")
        .body(Body::empty())
        .unwrap();
    app.clone().oneshot(request).await.unwrap();

    let text = scrape(&app).await;

    holds(
        &text,
        "cadus_http_requests_total{method=\"GET\",route=\"/api/health\",status=\"200\"} 1",
    );
    holds(
        &text,
        "cadus_deterministic_grade_total{result=\"correct\"} 0",
    );
    assert!(
        !text.contains("cadus_model_call_tokens_total"),
        "a read that failed must render no token series:\n{text}"
    );
}

/// Scrape `db` and read back a text that carries the request series and no
/// ledger series.
async fn scrape_without_ledger(db: &TestDb) -> String {
    let text = scrape(&app_of(db)).await;
    holds(
        &text,
        "cadus_deterministic_grade_total{result=\"correct\"} 0",
    );
    assert!(
        !text.contains("cadus_model_call_tokens_total"),
        "a ledger that did not read must render no token series:\n{text}"
    );
    text
}

/// A model-call aggregate that does not read drops the ledger series and keeps
/// the rest of the scrape.
#[tokio::test]
async fn a_model_call_read_that_fails_drops_the_ledger_series() {
    TestDb::with(|db| async move {
        common::drop_function(&db, "model_call_totals()").await;
        scrape_without_ledger(&db).await;
    })
    .await;
}

/// A job aggregate that does not read drops the ledger series too: the model
/// calls read, the jobs did not, and a half ledger renders as no ledger.
#[tokio::test]
async fn a_job_read_that_fails_drops_the_ledger_series() {
    TestDb::with(|db| async move {
        common::drop_function(&db, "diagnosis_job_totals()").await;
        scrape_without_ledger(&db).await;
    })
    .await;
}

/// A ledger read that runs past `LEDGER_READ_BOUND` drops the ledger series
/// and the scrape still answers inside the bound of the scrape itself.
#[tokio::test]
async fn a_ledger_read_past_its_bound_drops_the_ledger_series() {
    TestDb::with(|db| async move {
        sqlx::query(
            "CREATE OR REPLACE FUNCTION model_call_totals() \
             RETURNS TABLE (purpose text, input_cached bigint, input_uncached bigint, \
                            output_tokens bigint, reasoning_tokens bigint, latency_ms bigint, \
                            calls bigint) \
             LANGUAGE sql SECURITY DEFINER STABLE AS $$ \
             SELECT 'diagnosis'::text, 0::bigint, 0::bigint, 0::bigint, 0::bigint, 0::bigint, \
                    0::bigint FROM pg_sleep(3) $$",
        )
        .execute(&db.admin)
        .await
        .unwrap();
        let started = std::time::Instant::now();
        scrape_without_ledger(&db).await;
        assert!(
            started.elapsed() < std::time::Duration::from_secs(3),
            "the scrape must give the ledger up at its 2 s bound"
        );
    })
    .await;
}
