//! Benchmark B: the Postgres segment of the L2 budget — the grade transaction.
//!
//! Requirements: L2 (grade a verifiable answer, p95 < 300 ms), D-O2 (the grade
//! transaction), C2 (the log is append-only), C3 (row-level security on the
//! path), D-S6 (the hot state row), A4 (the diagnosis job), T1 (no model call).
//!
//! `docs/reference/l1-budget.md` gives this file 150 ms of the 300 ms of L2.
//! `crates/store/tests/bench_serve_roundtrip.rs` is the SERVE half of benchmark
//! B; this file is the GRADE half. The transaction is the one spec section 4.3
//! writes, statement for statement:
//!
//! ```text
//! BEGIN;                                       -- with the tenant bound (C3)
//!   SELECT pg_advisory_xact_lock(...)          -- section 4.2, one tenant lock
//!   SELECT ... FROM events ORDER BY seq        -- the log
//!   SELECT ... FROM learner_models             -- the D4 cache, then the fold
//!   SELECT doc FROM web_states                 -- the D-S6 document
//!   INSERT INTO events ... ON CONFLICT DO NOTHING   -- step 6, the one append
//!   UPDATE learner_models ...                  -- step 7, the fold write
//!   INSERT INTO diagnosis_jobs ...             -- step 9, the A4 job
//!   SELECT ... FROM serving_pool FOR UPDATE SKIP LOCKED  -- step 10, the pop
//!   UPDATE serving_pool SET claimed_at = now() -- the claim
//!   UPDATE web_states SET doc = $2             -- step 10, the state write
//! COMMIT;
//! ```
//!
//! # The measured code is the production code
//!
//! Every statement above runs through the `cadus_store` function the
//! `cadus_web::grade::answer` handler calls: `lock_web_state`, `load_events`,
//! `project_current`, `load_web_state`, `append_event`, `project_and_save`,
//! `diagnosis::enqueue`, `pool::pop_with_ring_tx`, and `save_web_state`. A
//! benchmark with its own SQL measures its own SQL, which M4 review 1 finding 11
//! recorded on the serve half.
//!
//! # The fixture stays stationary
//!
//! One sample appends one event, writes one job row, and claims one pool row.
//! Left alone, sample 500 would fold a log 500 events longer than sample 1 and
//! the percentiles would describe a run, not a transaction. After each timed
//! sample the harness therefore restores the fixture with the admin pool,
//! OUTSIDE the measured window: it deletes the appended event and the job row,
//! releases the pool row, and rewrites the `learner_models` row to the snapshot
//! it took before the FIRST grade of the run. That snapshot names the head of
//! the restored log, so every sample folds its appended attempt on the
//! incremental branch, which is the branch the request path takes.
//!
//! # Gate policy
//!
//! The gate fails at p95 above 150 ms. It never fails on p50 and never fails on
//! one slow sample: a shared runner produces those, and a flaky gate blocks good
//! work (spec section 10.2).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use std::time::Instant;

use cadus_core::config::Config;
use cadus_core::curriculum::Curriculum;
use cadus_core::event::{Event, SchemaVersion, SessionStart, Timestamp};
use cadus_core::pool::{Avoid, Ring, TaskMemory};
use cadus_core::projector::ProjectionInput;
use cadus_store::diagnosis::{JobPayload, PAYLOAD_VERSION, enqueue};
use cadus_store::pool::{NewInstance, insert_batch, pop_with_ring_tx};
use cadus_store::state::{
    append_event, load_events, load_web_state, lock_web_state, project_and_save, project_current,
    save_web_state,
};
use cadus_store::test_support::TestDb;
use cadus_store::{StoreError, begin_tenant};
use common::bench::{
    Percentiles, Snapshot, artifact_json, bench_instance, curriculum, delete_job, dsn_set,
    full_windows, profile, report, restore, seed_web_state, snapshot, timed, timed_rounds,
    write_artifact,
};
use common::events::{BASE_US, attempt_row};
use serde_json::{Value as Json, json};
use sqlx::PgPool;
use uuid::Uuid;

/// The session of every event of the fixture.
const SESSION: &str = "s_2026-01-01a";

/// The topic of every attempt of the fixture.
const TOPIC: &str = "adding-integers";

/// The serving key of the pool rows.
const KP_ID: &str = "adding-integers/kp1";

/// The task id of every attempt of the fixture.
const TASK_ID: &str = "s_2026-01-01a-review-adding-integers";

/// The attempts the fixture log carries before the first sample.
///
/// One session of 200 graded attempts is one long session of a learner, and
/// the fold of every sample reads them all.
const SEEDED_ATTEMPTS: usize = 200;

/// The unclaimed rows the fixture puts in the pool.
const POOL_DEPTH: usize = 64;

/// The seed the refill batch records in every `problem` document.
const BATCH_SEED: u64 = 20_260_829;

/// The untimed transactions that warm the connection and the plan cache.
const WARMUPS: usize = 20;

/// The timed transactions.
const SAMPLES: usize = 200;

/// The p95 budget of one grade transaction, in nanoseconds: the 150 ms
/// Postgres segment of L2 (`docs/reference/l1-budget.md` section 3).
const P95_BUDGET_NS: u128 = 150_000_000;

/// One graded attempt on [`TOPIC`].
fn attempt_event(attempt_id: &str, index: usize) -> Event {
    attempt_row(
        Timestamp::from_micros(BASE_US + (index as i64) * 60_000_000),
        SESSION,
        TASK_ID,
        TOPIC,
        attempt_id,
        format!("Compute ${index} + 5$."),
        (index + 5).to_string(),
    )
}

/// The A4 job document one grade enqueues (spec section 6.3).
fn job_payload(index: usize) -> Json {
    serde_json::to_value(JobPayload {
        v: PAYLOAD_VERSION,
        session: Some(SESSION.to_string()),
        task_id: TASK_ID.to_string(),
        topic: TOPIC.to_string(),
        kp: Some(KP_ID.to_string()),
        problem: format!("Compute ${index} + 5$."),
        expected: (index + 5).to_string(),
        answer_kind: "numeric".to_string(),
        given_answer: (index + 4).to_string(),
        work: None,
    })
    .unwrap()
}

/// Seed one user, one session-start event, [`SEEDED_ATTEMPTS`] attempts, the
/// D-S6 row, and the pool.
///
/// The seeds run on the admin pool, which bypasses row-level security, exactly
/// as the worker does with `cadus_admin` (D-O4).
async fn seed(db: &TestDb, graph: &Curriculum, cfg: &Config) -> (Uuid, Ring, TaskMemory) {
    let user = db.seed_user("bench-grade@example.test").await;

    let mut tx = begin_tenant(&db.admin, user).await.unwrap();
    append_event(
        &mut tx,
        user,
        &Event::SessionStart(SessionStart {
            ts: Timestamp::from_micros(BASE_US),
            session: Some(SESSION.to_string()),
            v: SchemaVersion,
        }),
        None,
    )
    .await
    .unwrap();
    for index in 0..SEEDED_ATTEMPTS {
        let id = format!("{TASK_ID}-{index}");
        append_event(&mut tx, user, &attempt_event(&id, index), Some(&id))
            .await
            .unwrap();
    }
    let input = ProjectionInput::new(graph, cfg, Timestamp::from_micros(BASE_US));
    project_and_save(&mut tx, user, &input, None).await.unwrap();
    tx.commit().await.unwrap();

    let rows: Vec<NewInstance> = (0..POOL_DEPTH)
        .map(|index| bench_instance(index, BATCH_SEED))
        .collect();
    let inserted = insert_batch(&db.admin, user, KP_ID, &rows).await.unwrap();
    assert_eq!(
        inserted, POOL_DEPTH as u64,
        "insert_batch skipped a digest, so the pool is not the depth it claims"
    );

    let (ring, task) = full_windows();
    seed_web_state(&db.admin, user, &ring, &task).await;
    (user, ring, task)
}

/// Put the fixture back the way the run found it, outside the measured window.
async fn restore_grade(
    pool: &PgPool,
    user: Uuid,
    attempt_id: &str,
    claimed: Uuid,
    snap: &Snapshot,
) {
    delete_job(pool, user, attempt_id).await;
    restore(pool, user, attempt_id, claimed, snap).await;
}

// ---------------------------------------------------------------------------
// The measured transaction
// ---------------------------------------------------------------------------

/// What one grade transaction did.
struct Graded {
    /// The `seq` the appended attempt took.
    seq: i64,
    /// The pool row the transaction claimed for the next problem.
    claimed: Uuid,
    /// Whether the step-7 fold took the full-replay branch.
    replayed: bool,
    /// Whether the step-5 fold took the full-replay branch.
    resume_replayed: bool,
    /// The `seq` the step-5 fold reached, before the append.
    resumed_through: i64,
    /// The `seq` the step-7 fold reached, after the append.
    folded_through: i64,
    /// The count of events the fold read.
    events: usize,
}

/// Run the D-O2 grade transaction once.
///
/// The caller times this function. Everything inside it is on the request path
/// of `POST /api/task/{task_id}/answer`.
///
/// # Errors
///
/// Returns the error of any step. A sample that does not grade is a broken
/// fixture, and the caller stops the run.
async fn grade_once(
    pool: &PgPool,
    user: Uuid,
    input: &ProjectionInput<'_>,
    attempt_id: &str,
    index: usize,
    avoid: &Avoid<'_>,
) -> Result<Graded, StoreError> {
    let mut tx = begin_tenant(pool, user).await?;
    lock_web_state(&mut tx, user).await?;
    let events = load_events(&mut tx, user).await?;
    let resumed = project_current(&mut tx, user, input).await?;
    let doc = load_web_state(&mut tx, user)
        .await?
        .unwrap_or_else(|| json!({}));

    let event = attempt_event(attempt_id, index);
    let seq = append_event(&mut tx, user, &event, Some(attempt_id))
        .await?
        .expect("the attempt is new, so the append returns a seq");
    let projection = project_and_save(&mut tx, user, input, None).await?;
    enqueue(&mut tx, user, attempt_id, &job_payload(index)).await?;
    let claimed = pop_with_ring_tx(&mut tx, user, KP_ID, avoid)
        .await?
        .claimed
        .expect("the pool holds an unclaimed row");
    save_web_state(&mut tx, user, &doc).await?;
    tx.commit().await?;

    Ok(Graded {
        seq,
        claimed: claimed.row.id,
        replayed: projection.replayed,
        resume_replayed: resumed.replayed,
        resumed_through: resumed.through_seq,
        folded_through: projection.through_seq,
        events: events.len(),
    })
}

/// Run `rounds` untimed grade transactions and return the snapshot every later
/// restore writes back.
///
/// The snapshot is taken BEFORE the first grade of the run, and every restore
/// of the run writes back that one snapshot. A snapshot taken AFTER a grade
/// carries a `through_seq` one line ahead of the log the same restore leaves
/// behind, and a cursor the log does not hold sends the step-5 fold down the
/// full-replay branch and the step-7 fold down the "nothing new" branch: the
/// appended attempt is never folded and the row goes back unchanged (M5 review
/// 2, finding V10).
async fn warm_up(
    db: &TestDb,
    app: &PgPool,
    user: Uuid,
    input: &ProjectionInput<'_>,
    avoid: &Avoid<'_>,
    rounds: usize,
) -> Snapshot {
    let snap = snapshot(&db.admin, user).await;
    for index in 0..rounds {
        let id = format!("warmup-{index}");
        let graded = grade_once(app, user, input, &id, index, avoid)
            .await
            .unwrap_or_else(|err| panic!("warm-up {index} did not grade: {err}"));
        restore_grade(&db.admin, user, &id, graded.claimed, &snap).await;
    }
    snap
}

// ---------------------------------------------------------------------------
// The benchmark
// ---------------------------------------------------------------------------

/// Benchmark B: 200 grade transactions hold the 150 ms segment of L2.
#[tokio::test]
async fn benchmark_b_grade_transaction_holds_the_l2_segment() {
    if !timed() {
        println!("SKIPPED benchmark B (grade): CADUS_BENCH is not set");
        return;
    }
    if !dsn_set("benchmark B (grade)") {
        return;
    }
    TestDb::with(|db| async move {
        let graph = curriculum();
        let cfg = Config::default();
        let (user, ring, task) = seed(&db, &graph, &cfg).await;
        let input = ProjectionInput::new(&graph, &cfg, Timestamp::from_micros(BASE_US));
        let avoid = Avoid::new(&ring, &task);
        // One connection, no concurrency: the benchmark measures the shape of
        // the transaction and never the contention of a pool (spec 10.2).
        let app = db.pool_as("cadus_app", 1).await;

        let snap = warm_up(&db, &app, user, &input, &avoid, WARMUPS).await;
        assert_eq!(
            snap.through_seq,
            SEEDED_ATTEMPTS as i64 + 1,
            "the restored cursor must name the head of the restored log"
        );
        let (samples, graded_rows) = timed_rounds(SAMPLES, |index| {
            let id = format!("sample-{index}");
            let (db, app, input, avoid, snap) = (&db, &app, &input, &avoid, &snap);
            async move {
                let start = Instant::now();
                let graded = grade_once(app, user, input, &id, index, avoid)
                    .await
                    .unwrap_or_else(|err| panic!("sample {index} did not grade: {err}"));
                let nanos = start.elapsed().as_nanos();
                restore_grade(&db.admin, user, &id, graded.claimed, snap).await;
                (nanos, graded)
            }
        })
        .await;

        let times = Percentiles::of(&samples);
        report(
            "benchmark B grade",
            &times,
            samples.len(),
            &format!(", {} events per fold", graded_rows[0].events),
        );
        write_artifact(
            "benchmark-b-grade.json",
            &artifact_json(
                "B-grade",
                "grade_ns",
                &times,
                P95_BUDGET_NS,
                &[
                    ("seeded_attempts", json!(SEEDED_ATTEMPTS)),
                    ("log_events", json!(graded_rows[0].events)),
                    ("pool_depth", json!(POOL_DEPTH)),
                    ("warmups", json!(WARMUPS)),
                    ("samples", json!(SAMPLES)),
                ],
            ),
        );

        assert_eq!(samples.len(), SAMPLES, "every sample is measured");
        // The restore puts the log back, so every sample folds the same events
        // and appends at the same `seq`. A drifting `seq` means the fixture is
        // growing and the percentiles describe a run, not a transaction.
        for (index, graded) in graded_rows.iter().enumerate() {
            check_graded(graded, &format!("sample {index}"));
        }
        assert!(
            times.p95 < P95_BUDGET_NS,
            "the p95 grade transaction took {} ns, and the budget is {P95_BUDGET_NS} ns",
            times.p95
        );
    })
    .await;
}

/// The six literals every graded sample of a warm fixture holds: the log
/// length, the appended `seq`, no replay before or after the append, and the
/// two cursors.
fn check_graded(graded: &Graded, who: &str) {
    assert_eq!(
        graded.events,
        SEEDED_ATTEMPTS + 1,
        "{who} folded a log of another length"
    );
    assert_eq!(
        graded.seq,
        SEEDED_ATTEMPTS as i64 + 2,
        "{who} appended at another seq"
    );
    assert!(
        !graded.replayed,
        "{who} took the full-replay branch, which no grade of a fresh attempt takes (spec \
         section 4.3)"
    );
    assert!(
        !graded.resume_replayed,
        "{who} replayed the whole log BEFORE the append, which no grade of a warm fixture \
         takes (spec section 4.3)"
    );
    assert_eq!(
        graded.resumed_through,
        SEEDED_ATTEMPTS as i64 + 1,
        "{who} resumed from a cursor that is not the head of the log"
    );
    assert_eq!(
        graded.folded_through,
        SEEDED_ATTEMPTS as i64 + 2,
        "{who} did not fold the appended attempt"
    );
}

/// The harness invariant: the restored fixture folds the appended attempt.
///
/// The benchmark restores the fixture between samples, so the restored state
/// must be the state a grade meets in a deployment: a `learner_models` cursor
/// that names the HEAD of the log. A cursor one line ahead of the log sends the
/// step-5 fold down the full-replay branch and the step-7 fold down the "nothing
/// new" branch. The appended attempt is then never folded and the row goes back
/// unchanged, so the timed samples measure a transaction no request runs (M5
/// review 2, finding V10).
///
/// This test needs no `CADUS_BENCH`: it takes no measurement, and the shape of
/// the harness is a property of every run.
#[tokio::test]
async fn the_restored_fixture_folds_the_appended_attempt() {
    if !dsn_set("the harness check") {
        return;
    }
    TestDb::with(|db| async move {
        let graph = curriculum();
        let cfg = Config::default();
        let (user, ring, task) = seed(&db, &graph, &cfg).await;
        let input = ProjectionInput::new(&graph, &cfg, Timestamp::from_micros(BASE_US));
        let avoid = Avoid::new(&ring, &task);
        let app = db.pool_as("cadus_app", 1).await;

        let snap = warm_up(&db, &app, user, &input, &avoid, 2).await;
        assert_eq!(
            snap.through_seq,
            SEEDED_ATTEMPTS as i64 + 1,
            "the snapshot the run restores must name the head of the restored log"
        );

        let graded = grade_once(&app, user, &input, "check-0", 0, &avoid)
            .await
            .unwrap_or_else(|err| panic!("the checked grade did not run: {err}"));
        restore_grade(&db.admin, user, "check-0", graded.claimed, &snap).await;
        check_graded(&graded, "the checked grade");
        println!(
            "harness check ({}): the restored fixture folds the appended attempt",
            profile()
        );
    })
    .await;
}
