//! Proof tests for the worker skeleton, in process (R4, L1).
//!
//! The tests here drive the tick loop with a `Db` handle of their own and a
//! shutdown future they control. `run_binary.rs` starts the real binary.
//!
//! Every expected value below is a literal: a literal tick count, a literal log
//! line.
//!
//! Every test that needs a database uses `TestDb::with`, so a failed assertion
//! drops the throwaway database instead of leaving it on the shared cluster.
//! The tests that need none use `cadus_store::test_support::DeafPostgres`,
//! which speaks the Postgres wire protocol itself and stops answering at the
//! exact moment the test wants.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::time::{Duration, Instant};

use cadus_store::test_support::{DeafPostgres, TestDb};
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db, DbConfig, connect_options};
use cadus_worker::{RefillJob, WorkerConfig, run_with};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

use common::{
    ADDING, Capture, FakeModel, SQUARES, arena, diagnosis_reply, enqueue, handle, payload, row_of,
    seed_drained_pair, seed_squares_pair, with_grants,
};

/// Wrap a pool in the `Db` that `run` takes, with the documented default
/// client-side bound of 10000 ms.
///
/// `DEFAULT_CLIENT_TIMEOUT_MS` is the value that an absent `DB_CLIENT_TIMEOUT_MS`
/// gives, so these tests run the loop exactly as the deployment does.
fn db_with(pool: PgPool) -> Db {
    Db::new(pool, DEFAULT_CLIENT_TIMEOUT_MS)
}

/// A tick period of this many milliseconds.
fn every(millis: u64) -> WorkerConfig {
    WorkerConfig {
        tick: Duration::from_millis(millis),
    }
}

/// Run the loop with these jobs for `millis` milliseconds, with a 50 ms tick.
async fn run_for(
    db: &Db,
    refill: Option<&RefillJob<'_>>,
    diagnosis: Option<&mut cadus_worker::DiagnosisJob>,
    millis: u64,
) -> u64 {
    run_with(
        db,
        &every(50),
        refill,
        diagnosis,
        tokio::time::sleep(Duration::from_millis(millis)),
    )
    .await
    .expect("the tick loop must not fail")
}

/// (1) A 50 ms tick and a 400 ms run give 3 ticks or more.
///
/// The budget holds 8 periods but the test demands 3. Each tick makes one real
/// Postgres round trip, and the test bounds neither the database latency nor the
/// scheduler, so a tight count is a wall-clock flake on a loaded machine
/// (finding #43). The upper bound of 12 keeps the test honest: a loop that
/// ignores the period, or a tick counter that counts without a tick, still
/// fails.
#[tokio::test]
async fn run_counts_ticks_until_shutdown() {
    TestDb::with(|db| async move {
        let ticks = run_for(&db_with(db.admin.clone()), None, None, 400).await;

        assert!(
            ticks >= 3,
            "the loop must reach at least 3 ticks in 400 ms, it reached {ticks}"
        );
        assert!(
            ticks <= 12,
            "the loop must stop at 12 ticks or fewer in 400 ms, it reached {ticks}"
        );
    })
    .await;
}

/// (2) R4: the heartbeat touches the database.
///
/// A closed pool answers every query with an error, so `run` must return `Err`.
/// A heartbeat that does no database work returns `Ok` here and the tick counter
/// alone keeps the old tests green (finding #19).
#[tokio::test]
async fn run_fails_when_the_pool_is_closed() {
    TestDb::with(|db| async move {
        let pool = db.admin.clone();
        pool.close().await;

        let result = cadus_worker::run(
            &db_with(pool),
            &every(20),
            tokio::time::sleep(Duration::from_secs(5)),
        )
        .await;

        assert!(
            result.is_err(),
            "a closed pool must make the tick loop fail, it gave {result:?}"
        );
    })
    .await;
}

/// (3) The stop signal wins while a heartbeat query is in flight.
///
/// The pool points at a closed port and keeps the sqlx default acquire timeout
/// of 30 s, so the heartbeat does not answer inside this test. The shutdown
/// future completes after 200 ms. A loop that waits for the query inside a
/// select branch body needs the whole 30 s (finding #10).
#[tokio::test]
async fn shutdown_wins_over_a_heartbeat_that_does_not_answer() {
    let pool = PgPoolOptions::new()
        .acquire_timeout(Duration::from_secs(30))
        .connect_lazy("postgresql://nobody@127.0.0.1:1/nodb")
        .expect("a lazy pool needs no server");

    let start = Instant::now();
    let outcome = tokio::time::timeout(
        Duration::from_secs(5),
        cadus_worker::run(
            &db_with(pool),
            &every(10),
            tokio::time::sleep(Duration::from_millis(200)),
        ),
    )
    .await;
    let elapsed = start.elapsed();

    let ticks = outcome
        .expect("run must return within 5 s")
        .expect("the tick loop must not fail");
    assert_eq!(ticks, 0, "no heartbeat completed, so the count must be 0");
    assert!(
        elapsed < Duration::from_secs(2),
        "run must return within 2 s of the stop signal, it took {elapsed:?}"
    );
}

/// (7) L1: a heartbeat that does not answer logs a warning and the loop goes on.
///
/// `DeafPostgres::start_silent` accepts the connection and writes nothing, so
/// the sqlx connect never finishes. Only the client-side bound of
/// `cadus_store::bounded` ends the wait. A stalled database must not kill the
/// worker: the loop logs `heartbeat timed out after 300 ms` at warn level and
/// takes the next tick, and it still returns when the shutdown future resolves.
///
/// The pool is lazy, so the connect starts inside the heartbeat. The acquire
/// timeout of 5 s is the backstop of the test itself: it is longer than the 1 s
/// run, so a warning proves the 300 ms bound and not the acquire timeout.
///
/// `client_timeout_ms: 300` is the value that `DB_CLIENT_TIMEOUT_MS=300` gives.
/// The unit test `the_client_timeout_reads_the_same_three_rules` in
/// `crates/store/src/lib.rs` pins that step, so this test sets the field and
/// touches no process environment: a `set_var` reaches every other test in this
/// binary.
///
/// `tracing::subscriber::set_default` binds the subscriber to THIS thread only,
/// so the other tests of this binary keep their own log.
#[tokio::test]
async fn run_logs_a_heartbeat_timeout_and_keeps_ticking() {
    let deaf = DeafPostgres::start_silent();
    let cfg = DbConfig {
        database_url: deaf.dsn(),
        statement_timeout_ms: 0,
        client_timeout_ms: 300,
    };
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(5))
        .connect_lazy_with(connect_options(&cfg).expect("the deaf DSN parses"));
    let db = Db::new(pool.clone(), cfg.client_timeout_ms);

    let capture = Capture::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(capture.clone())
        .with_ansi(false)
        .with_max_level(tracing::Level::WARN)
        .finish();
    let recorder = tracing::subscriber::set_default(subscriber);

    let start = Instant::now();
    let outcome = tokio::time::timeout(
        Duration::from_secs(5),
        cadus_worker::run(&db, &every(50), tokio::time::sleep(Duration::from_secs(1))),
    )
    .await;
    let elapsed = start.elapsed();
    drop(recorder);

    let log = capture.text();
    let ticks = outcome
        .expect("run must return within 5 s")
        .expect("a heartbeat timeout must not fail the loop");

    assert_eq!(ticks, 0, "no heartbeat answered, so the count must be 0");
    assert!(
        log.contains("heartbeat timed out after 300 ms"),
        "the log must hold the literal `heartbeat timed out after 300 ms`; log:\n{log}"
    );
    assert!(
        elapsed < Duration::from_secs(3),
        "run must return soon after the 1 s shutdown, it took {elapsed:?}"
    );

    pool.close().await;
}

/// The stop signal wins while a refill pass is in flight.
///
/// The test holds an exclusive lock on `serving_pool`, so the retire statement
/// of the first refill pass blocks. The heartbeat answered, so the loop counts
/// one tick, and the shutdown future then wins over the blocked pass: the loop
/// returns inside the budget instead of waiting for the lock.
#[tokio::test]
async fn shutdown_wins_over_a_refill_pass_that_does_not_answer() {
    TestDb::with(|db| async move {
        let mut lock = db.admin.begin().await.unwrap();
        sqlx::query("LOCK TABLE serving_pool IN ACCESS EXCLUSIVE MODE")
            .execute(&mut *lock)
            .await
            .unwrap();
        let curriculum = arena();
        let job = RefillJob::new(&curriculum);

        let start = Instant::now();
        let ticks = tokio::time::timeout(
            Duration::from_secs(5),
            run_for(&handle(&db), Some(&job), None, 300),
        )
        .await
        .expect("run must return within 5 s");
        let elapsed = start.elapsed();

        assert_eq!(
            ticks, 1,
            "the heartbeat of the first tick answered, then the pass blocked"
        );
        assert!(
            elapsed < Duration::from_secs(2),
            "run must return soon after the stop signal, it took {elapsed:?}"
        );
        lock.rollback().await.unwrap();
    })
    .await;
}

/// The stop signal wins while a diagnosis pass waits for the model.
///
/// The fake endpoint answers after 2 s, and the shutdown future completes after
/// 300 ms. The loop returns at the stop signal; the row stays claimed, and the
/// sweep of a later process reclaims it.
#[tokio::test]
async fn shutdown_wins_over_a_diagnosis_pass_that_waits_for_the_model() {
    TestDb::with(|db| async move {
        let user = db.seed_user("slow@example.test").await;
        let id = enqueue(&db.admin, user, "task-1", &payload(None)).await;
        let server = FakeModel::start_with_delay(
            vec![diagnosis_reply(
                "{\"error_tags\":[],\"prose\":\"Try again.\"}",
            )],
            Duration::from_secs(2),
        )
        .await;
        let mut job = server.diagnosis_job(0);

        let start = Instant::now();
        let ticks = tokio::time::timeout(
            Duration::from_secs(5),
            run_for(&handle(&db), None, Some(&mut job), 300),
        )
        .await
        .expect("run must return within 5 s");
        let elapsed = start.elapsed();

        assert_eq!(ticks, 1);
        assert!(
            elapsed < Duration::from_secs(2),
            "run must return at the stop signal and not at the reply, it took {elapsed:?}"
        );
        assert_eq!(row_of(&db.admin, id).await.0, "running");
    })
    .await;
}

/// A refill pass and a diagnosis pass that both answer run on the same tick:
/// the pool takes its rows, the row settles, and the loop counts the tick.
#[tokio::test]
async fn both_passes_answer_on_one_tick() {
    TestDb::with(|db| async move {
        let user = seed_squares_pair(&db).await;
        let id = enqueue(&db.admin, user, "task-1", &payload(None)).await;
        let server = FakeModel::start(vec![diagnosis_reply(
            "{\"error_tags\":[],\"prose\":\"Try again.\"}",
        )])
        .await;
        let mut job = server.diagnosis_job(0);
        let curriculum = arena();
        let refill = RefillJob::new(&curriculum);

        let ticks = run_for(&handle(&db), Some(&refill), Some(&mut job), 300).await;

        assert!(
            ticks >= 1,
            "the loop must tick at least once in 300 ms, it reached {ticks}"
        );
        assert_eq!(row_of(&db.admin, id).await.0, "done");
        assert_eq!(server.call_count(), 1);
        assert!(
            cadus_store::pool::unclaimed_depth(&db.admin, user, SQUARES)
                .await
                .unwrap()
                > 0,
            "the refill pass wrote rows"
        );
    })
    .await;
}

/// A refill pass or a diagnosis pass that fails is news, not a fatal error: the
/// loop logs the failure and takes the next tick.
///
/// The role below runs the heartbeat and nothing else: it holds no privilege on
/// `serving_pool` and none on `diagnosis_jobs`, so the retire statement of the
/// refill pass and the sweep of the diagnosis pass both fail. The loop still
/// counts its ticks and returns at the stop signal.
#[tokio::test]
async fn a_pass_that_fails_does_not_stop_the_loop() {
    TestDb::with(|db| async move {
        let user = seed_squares_pair(&db).await;
        seed_drained_pair(&db.admin, user, ADDING).await;
        enqueue(&db.admin, user, "task-1", &payload(None)).await;

        with_grants(
            &db,
            &["GRANT USAGE ON SCHEMA public TO {role}"],
            move |db, handle| async move {
                let server = FakeModel::start(Vec::new()).await;
                let mut job = server.diagnosis_job(0);
                let curriculum = arena();
                let refill = RefillJob::new(&curriculum);

                let ticks = run_for(&handle, Some(&refill), Some(&mut job), 400).await;

                assert!(
                    ticks >= 2,
                    "the loop must keep ticking past a failed pass, it reached {ticks}"
                );
                assert!(server.calls().is_empty(), "a failed sweep claims no row");
                assert_eq!(
                    cadus_store::pool::unclaimed_depth(&db.admin, user, SQUARES)
                        .await
                        .unwrap(),
                    0,
                    "a failed refill pass wrote no row"
                );
            },
        )
        .await;
    })
    .await;
}
