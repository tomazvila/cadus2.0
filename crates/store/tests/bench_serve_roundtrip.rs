//! Benchmark B: the Postgres segment of the L1 budget (spec section 10.2).
//!
//! Requirements: L1 (serve a problem, p95 < 150 ms), D-O1 (the serve
//! transaction), D-S5 (the serving pool), D-S6 (the hot state row), D5 (the
//! anti-repeat ring), C3 (row-level security on the path), T1 (no model call).
//!
//! `docs/reference/l1-budget.md` gives this file 100 ms of the 150 ms of L1.
//! The transaction is the one spec section 7.2 writes:
//!
//! ```text
//! BEGIN;                                   -- with the tenant bound (C3)
//!   SELECT model FROM learner_models        -- D-S3, one read
//!   -- cadus_store::pool::pop_with_ring_tx runs the next two statements:
//!   SELECT ... FROM serving_pool
//!    WHERE claimed_at IS NULL
//!    ORDER BY created_at, id LIMIT 8
//!      FOR UPDATE SKIP LOCKED               -- D-O1 pop, at most POP_LIMIT
//!   -- the D5 ring rejects the digests it holds; the first survivor wins
//!   UPDATE serving_pool SET claimed_at = now()   -- the claim
//!   UPDATE web_states  SET doc = $2               -- served problem plus ring
//! COMMIT;
//! ```
//!
//! # The measured code is the production code
//!
//! The benchmark pops with [`cadus_store::pool::pop_with_ring_tx`], the function
//! the M5 serve path calls, and it seeds with
//! [`cadus_store::pool::insert_batch`], the function the D-O4 worker calls. An
//! earlier version of this file carried its own SELECT and its own fixture
//! documents. Those documents did not decode through the production reader, and
//! the inline SELECT diverged from the pop in its ORDER BY and in its claim
//! (M4 review 1, finding 11).
//!
//! # The fixture
//!
//! One seeded user, one knowledge point, and [`POOL_DEPTH`] unclaimed rows
//! written by [`BATCHES`] calls of `insert_batch`. One call is one statement, so
//! every row of one batch carries ONE `created_at`: the pop reads the batches in
//! age order and breaks the tie inside a batch by `id`. That tie sort over
//! [`BATCH_ROWS`] equal timestamps is the sort every production pop performs,
//! and the old fixture, with a distinct `created_at` per row, never measured it.
//!
//! The fixture builds the ring from the pool and not from a guess: it reads the
//! first [`RING_OVERLAP`] digests in the pop's own `created_at, id` order and
//! puts those into the D5 ring. The pop therefore skips three rows on every
//! sample and serves the fourth, whatever order the random row ids take.
//!
//! Every sample starts from the same in-memory ring and restores the claim it
//! made, so the 500 samples measure one transaction shape and not a pool that
//! drains as the run goes on.
//!
//! # Gate policy
//!
//! The gate fails at p95 above 100 ms. It never fails on p50 or on one slow
//! sample: a shared runner produces those, and a flaky gate blocks good work
//! (spec section 10.2). The numbers go to a JSON artifact on every run.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use std::time::Instant;

use cadus_core::learner::problem_text_hash;
use cadus_core::pool::{Avoid, Ring, TaskMemory};
use cadus_store::pool::{NewInstance, POP_LIMIT, insert_batch, pop_with_ring_tx};
use cadus_store::test_support::TestDb;
use cadus_store::{StoreError, begin_tenant};
use common::bench::{
    Percentiles, artifact_json, budget, dsn_set, instance_row, release, report, rounds, timed,
    timed_rounds, write_artifact,
};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

/// The serving key of the fixture: `"<topic_id>/<kp_id>"`.
const KP_ID: &str = "perfect-squares";

/// The unclaimed rows the fixture puts in the pool.
const POOL_DEPTH: usize = 200;

/// The batches the fixture inserts. Every row of one `insert_batch` call takes
/// the transaction instant as its `created_at`, so the pool carries this many
/// distinct timestamps and the pop sorts a real tie inside each one (M4 review
/// 1, finding 11).
const BATCHES: usize = 2;

/// The rows of one batch.
const BATCH_ROWS: usize = POOL_DEPTH / BATCHES;

/// The seed the refill batch records in every `problem` document.
const BATCH_SEED: u64 = 20_260_827;

/// The untimed transactions that warm the connection and the plan cache.
const WARMUPS: usize = 50;

/// The timed transactions.
const SAMPLES: usize = 500;

/// The popped rows the ring blocks. The pop reads eight candidates and the
/// anti-repeat rule walks past this many before it serves one (D5).
const RING_OVERLAP: usize = 3;

/// The p95 budget of one serve transaction, in nanoseconds: the 100 ms Postgres
/// segment of L1 (`docs/reference/l1-budget.md` section 2).
const P95_BUDGET_NS: u128 = 100_000_000;

/// The statement of pool row `index`.
fn statement(index: usize) -> String {
    format!("Compute ${}^{{2}}$.", index + 1)
}

/// Pool row `index`, in the shape the D-O4 refill inserts (D-S5).
///
/// The `problem` document carries the batch seed and the drawn binding, so a
/// reviewer reproduces the instance from the row alone.
fn new_instance(index: usize) -> NewInstance {
    instance_row(
        statement(index),
        ((index + 1) * (index + 1)).to_string(),
        (index + 1).to_string(),
        BATCH_SEED,
    )
}

/// Seed one user, one learner model, one state row, and the pool.
///
/// The seeds run on the admin pool, which bypasses row-level security, exactly
/// as the worker does with `cadus_admin` (D-O4).
async fn seed(db: &TestDb) -> (Uuid, Ring, TaskMemory) {
    let user = db.seed_user("bench-serve@example.test").await;

    let model = serde_json::json!({
        "topics": {
            "perfect-squares": {"status": "learning", "repNum": 3.0, "memoryBase": 0.7}
        },
        "xp": {"total": 120},
    });
    sqlx::query!(
        "INSERT INTO learner_models (user_id, model, through_seq, projector_version, config_hash)
         VALUES ($1, $2, 41, 3, 'bench')",
        user,
        model
    )
    .execute(&db.admin)
    .await
    .unwrap();

    // The production insert, one call per batch. Every row of one call takes one
    // `created_at`, so the pool carries BATCHES timestamps and BATCH_ROWS rows
    // per timestamp.
    let rows: Vec<NewInstance> = (0..POOL_DEPTH).map(new_instance).collect();
    for batch in rows.chunks(BATCH_ROWS) {
        let inserted = insert_batch(&db.admin, user, KP_ID, batch).await.unwrap();
        assert_eq!(
            inserted,
            batch.len() as u64,
            "insert_batch skipped a digest, so the fixture is not the depth it claims"
        );
    }

    let timestamps = sqlx::query_scalar!(
        r#"SELECT count(DISTINCT created_at) AS "count!"
             FROM serving_pool WHERE user_id = $1 AND kp_id = $2"#,
        user,
        KP_ID
    )
    .fetch_one(&db.admin)
    .await
    .unwrap();
    assert_eq!(
        timestamps, BATCHES as i64,
        "the pool carries one created_at per batch, so the pop sorts a real tie"
    );

    // Read the first digests in the pop's own order and put them into the ring.
    // The ring then blocks the rows the pop reads first, whatever order the
    // random row ids take inside the oldest batch.
    let head = sqlx::query_scalar!(
        r#"SELECT instance_hash AS "instance_hash!"
             FROM serving_pool
            WHERE user_id = $1 AND kp_id = $2 AND claimed_at IS NULL
            ORDER BY created_at, id
            LIMIT $3"#,
        user,
        KP_ID,
        POP_LIMIT
    )
    .fetch_all(&db.admin)
    .await
    .unwrap();
    assert_eq!(
        head.len(),
        POP_LIMIT as usize,
        "the pool is shallower than one pop"
    );

    // The ring holds 20 digests. The RING_OVERLAP rows the pop reads first are
    // among them, so the pop skips those and serves the next one.
    let mut ring = Ring::new();
    for digest in head.iter().take(RING_OVERLAP) {
        ring.push(digest);
    }
    for filler in 0..(Ring::capacity() - RING_OVERLAP) {
        ring.push(&problem_text_hash(&format!("An older problem {filler}.")));
    }
    assert_eq!(ring.len(), Ring::capacity(), "the ring starts full");

    let mut task = TaskMemory::new();
    for filler in 0..TaskMemory::capacity() {
        task.push(&problem_text_hash(&format!("A task problem {filler}.")));
    }

    let doc = serde_json::json!({
        "served": {},
        "ring": ring,
        "task_memory": task,
    });
    sqlx::query!(
        "INSERT INTO web_states (user_id, doc) VALUES ($1, $2)",
        user,
        doc
    )
    .execute(&db.admin)
    .await
    .unwrap();

    (user, ring, task)
}

// ---------------------------------------------------------------------------
// The measured transaction
// ---------------------------------------------------------------------------

/// The outcome of one serve transaction.
struct Served {
    /// The pool row the transaction claimed.
    id: Uuid,
    /// The index of the claimed row among the popped candidates.
    index: usize,
    /// The count of candidates the anti-repeat rule walked past.
    skipped: usize,
    /// Whether every popped row sat inside the anti-repeat windows.
    exhausted: bool,
    /// The count of rows the pop read.
    candidates: usize,
    /// The statement of the served row, as the production reader decoded it.
    text: String,
    /// The expected answer of the served row, as the production reader decoded
    /// it.
    answer: String,
}

/// Run the D-O1 serve transaction once and return what it served.
///
/// The caller times this function. Everything inside it is on the request path
/// of `POST /api/task/{task_id}/serve`: the tenant binding, the learner-model
/// read, the production pool pop with its claim, the state write, and the
/// commit.
///
/// # Errors
///
/// Returns the error of the pop or of a statement. A sample that does not serve
/// is a broken fixture, and the caller stops the run.
async fn serve_once(
    pool: &PgPool,
    user: Uuid,
    ring: &Ring,
    task: &TaskMemory,
) -> Result<Served, StoreError> {
    let mut tx = begin_tenant(pool, user).await?;

    let model = sqlx::query!(
        "SELECT model, through_seq FROM learner_models WHERE user_id = $1",
        user
    )
    .fetch_one(&mut *tx)
    .await?;
    assert_eq!(
        model.through_seq, 41,
        "the learner model row is the fixture"
    );

    let avoid = Avoid::new(ring, task);
    let claimed = pop_with_ring_tx(&mut tx, user, KP_ID, &avoid)
        .await?
        .claimed
        .expect("the pool holds an unclaimed row");

    // The served digest enters BOTH windows, and the caller writes them back in
    // the same transaction as the claim (D5, D-S6).
    let mut next_ring = ring.clone();
    let mut next_task = task.clone();
    next_ring.push(&claimed.row.instance_hash);
    next_task.push(&claimed.row.instance_hash);
    let doc = serde_json::json!({
        "served": {
            "pool_id": claimed.row.id,
            "text": claimed.row.problem.text,
            "expected": claimed.row.expected_answer.answer,
        },
        "ring": next_ring,
        "task_memory": next_task,
    });
    sqlx::query!(
        "UPDATE web_states SET doc = $2, updated_at = now() WHERE user_id = $1",
        user,
        doc
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(Served {
        id: claimed.row.id,
        index: claimed.pick.index,
        skipped: claimed.pick.skipped,
        exhausted: claimed.pick.exhausted,
        candidates: claimed.candidates,
        text: claimed.row.problem.text,
        answer: claimed.row.expected_answer.answer,
    })
}

// ---------------------------------------------------------------------------
// The benchmark
// ---------------------------------------------------------------------------

/// Benchmark B: 500 serve transactions hold the 100 ms segment of L1.
///
/// A plain `cargo test` drives one warm-up and three samples and pins every
/// count; `CADUS_BENCH` arms the clock and the full sample count.
#[tokio::test]
async fn benchmark_b_serve_round_trip_holds_the_l1_segment() {
    if !dsn_set("benchmark B") {
        return;
    }
    TestDb::with(|db| async move {
        let (user, ring, task) = seed(&db).await;
        // One connection, no concurrency: the benchmark measures the shape of
        // the transaction and never the contention of a pool (spec 10.2).
        let app = db.pool_as("cadus_app", 1).await;

        for index in 0..rounds(WARMUPS, 1) {
            let served = serve_once(&app, user, &ring, &task)
                .await
                .unwrap_or_else(|err| panic!("warm-up {index} did not serve: {err}"));
            release(&db.admin, served.id).await;
        }

        let count = rounds(SAMPLES, 3);
        let (samples, served_rows): (Vec<u128>, Vec<Served>) = timed_rounds(count, |index| {
            let (db, app, ring, task) = (&db, &app, &ring, &task);
            async move {
                let start = Instant::now();
                let served = serve_once(app, user, ring, task)
                    .await
                    .unwrap_or_else(|err| panic!("sample {index} did not serve: {err}"));
                let nanos = start.elapsed().as_nanos();
                release(&db.admin, served.id).await;
                (nanos, served)
            }
        })
        .await;

        let times = Percentiles::of(&samples);
        let limit = budget(P95_BUDGET_NS);
        report("benchmark B serve", &times, samples.len(), "");
        if timed() {
            write_artifact(
                "benchmark-b.json",
                &artifact_json(
                    "B",
                    "serve_ns",
                    &times,
                    P95_BUDGET_NS,
                    &[
                        ("pool_depth", json!(POOL_DEPTH)),
                        ("batches", json!(BATCHES)),
                        ("ring_overlap", json!(RING_OVERLAP)),
                        ("pop_candidates", json!(POP_LIMIT)),
                        ("warmups", json!(WARMUPS)),
                        ("samples", json!(SAMPLES)),
                    ],
                ),
            );
        }

        assert_eq!(samples.len(), count, "every sample is measured");
        // The ring blocks the three rows the pop reads first, so the pop skips
        // three and serves the fourth. A pick that skips none means the fixture
        // stopped exercising the anti-repeat rule. The two decoded fields prove
        // the production reader read the production writer's documents, which
        // the old inline SELECT never did (M4 review 1, finding 11).
        for (index, served) in served_rows.iter().enumerate() {
            assert_eq!(
                (served.index, served.skipped, served.exhausted),
                (RING_OVERLAP, RING_OVERLAP, false),
                "sample {index} did not skip the three blocked rows"
            );
            assert_eq!(
                served.candidates, POP_LIMIT as usize,
                "sample {index} popped a different count of candidates"
            );
            assert!(
                served.text.starts_with("Compute $") && served.text.ends_with("^{2}$."),
                "sample {index} decoded the statement as {:?}",
                served.text
            );
            assert!(
                served.answer.chars().all(|c| c.is_ascii_digit()),
                "sample {index} decoded the expected answer as {:?}",
                served.answer
            );
        }
        assert!(
            !timed() || times.p95 < limit,
            "the p95 serve transaction took {} ns, and the budget is {limit} ns",
            times.p95
        );
    })
    .await;
}
