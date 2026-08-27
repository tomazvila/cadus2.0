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

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use cadus_core::learner::problem_text_hash;
use cadus_core::pool::{
    Avoid, POOL_ROW_VERSION, PoolAnswer, PoolProblem, Ring, Source, TaskMemory,
};
use cadus_store::pool::{NewInstance, POP_LIMIT, insert_batch, pop_with_ring_tx};
use cadus_store::test_support::TestDb;
use cadus_store::{StoreError, begin_tenant};
use sqlx::PgPool;
use uuid::Uuid;

/// The knowledge point of the fixture.
const KP_ID: &str = "perfect-squares";

/// The unclaimed rows the fixture puts in the pool (spec section 10.2).
const POOL_DEPTH: usize = 200;

/// The count of `insert_batch` calls that write the pool.
///
/// The D-O4 worker refills in batches, and one batch is one statement. Two
/// batches give the pop both halves of its `ORDER BY created_at, id`: the age
/// order between the batches, and the tie sort inside one batch.
const BATCHES: usize = 2;

/// The rows of one batch.
const BATCH_ROWS: usize = POOL_DEPTH / BATCHES;

/// The seed the refill batch records in every `problem` document.
const BATCH_SEED: u64 = 20_260_827;

/// The untimed transactions that warm the connection and the plan cache.
const WARMUPS: usize = 50;

/// The timed transactions (spec section 10.2).
const SAMPLES: usize = 500;

/// The count of ring digests that also sit in the pool.
///
/// The three rows are the three the pop reads first, so the anti-repeat rule
/// skips exactly three rows per serve.
const RING_OVERLAP: usize = 3;

/// The p95 budget of one serve transaction, in nanoseconds: 100 ms of the
/// 150 ms of L1 (`docs/reference/l1-budget.md`).
const P95_BUDGET_NS: u128 = 100_000_000;

/// The environment variable that turns the benchmarks on.
const BENCH_VAR: &str = "CADUS_BENCH";

/// The environment variable that moves the artifact directory.
const ARTIFACT_DIR_VAR: &str = "CADUS_BENCH_DIR";

/// The environment variable that names the throwaway cluster.
const TEST_DSN_VAR: &str = "CADUS_TEST_DATABASE_URL";

// ---------------------------------------------------------------------------
// Percentiles and the artifact
// ---------------------------------------------------------------------------

/// The `percent` percentile of a sorted sample, by the nearest-rank rule.
///
/// The rank is `ceil(percent * n / 100)`, counted from one. The arithmetic is
/// integer arithmetic, so no float enters a reported number (D6).
fn percentile(sorted: &[u128], percent: u128) -> u128 {
    assert!(!sorted.is_empty(), "a percentile needs a sample");
    let count = sorted.len() as u128;
    let rank = (percent * count).div_ceil(100).max(1);
    let index = usize::try_from(rank - 1).unwrap_or(0);
    sorted[index.min(sorted.len() - 1)]
}

/// The p50, p95, p99, and maximum of a sample of nanosecond durations.
struct Percentiles {
    p50: u128,
    p95: u128,
    p99: u128,
    max: u128,
}

impl Percentiles {
    /// Read the percentiles of one sample. The function sorts its own copy.
    fn of(samples: &[u128]) -> Self {
        let mut sorted = samples.to_vec();
        sorted.sort_unstable();
        Self {
            p50: percentile(&sorted, 50),
            p95: percentile(&sorted, 95),
            p99: percentile(&sorted, 99),
            max: *sorted.last().unwrap(),
        }
    }
}

/// Write the benchmark artifact and print its path.
fn write_artifact(body: &str) {
    let dir = match std::env::var_os(ARTIFACT_DIR_VAR) {
        Some(value) => PathBuf::from(value),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/bench"),
    };
    std::fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    let path = dir.join("benchmark-b.json");
    std::fs::write(&path, body).unwrap_or_else(|err| panic!("write {}: {err}", path.display()));
    println!("artifact: {}", path.display());
}

/// The name of the build profile, for the artifact.
fn profile() -> &'static str {
    if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    }
}

// ---------------------------------------------------------------------------
// The fixture
// ---------------------------------------------------------------------------

/// The statement of pool row `index`.
fn statement(index: usize) -> String {
    format!("Compute ${}^{{2}}$.", index + 1)
}

/// The digest of pool row `index`. This is the M3 `problem_text_hash`.
fn instance_hash(index: usize) -> String {
    problem_text_hash(&statement(index))
}

/// Pool row `index`, in the shape the D-O4 refill inserts (D-S5).
///
/// The row is a [`NewInstance`], so `insert_batch` writes the two documents with
/// the production writers and `pop_with_ring_tx` reads them back with the
/// production readers. The `problem` document carries the batch seed and the
/// drawn binding, so a reviewer reproduces the instance from the row alone.
fn new_instance(index: usize) -> NewInstance {
    let mut bindings = BTreeMap::new();
    bindings.insert("a".to_string(), (index + 1).to_string());
    NewInstance {
        source: Source::Template,
        content_digest: None,
        problem: PoolProblem {
            v: POOL_ROW_VERSION,
            text: statement(index),
            bindings,
            seed: BATCH_SEED,
        },
        expected_answer: PoolAnswer {
            v: POOL_ROW_VERSION,
            answer: ((index + 1) * (index + 1)).to_string(),
        },
        instance_hash: instance_hash(index),
    }
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

/// Put the claimed row back, so the next sample reads the same pool.
async fn release(pool: &PgPool, id: Uuid) {
    sqlx::query!(
        "UPDATE serving_pool SET claimed_at = NULL WHERE id = $1",
        id
    )
    .execute(pool)
    .await
    .unwrap();
}

// ---------------------------------------------------------------------------
// The benchmark
// ---------------------------------------------------------------------------

/// Benchmark B: 500 serve transactions hold the 100 ms segment of L1.
#[tokio::test]
async fn benchmark_b_serve_round_trip_holds_the_l1_segment() {
    if std::env::var_os(BENCH_VAR).is_none() {
        println!("SKIPPED benchmark B: {BENCH_VAR} is not set");
        return;
    }
    if std::env::var_os(TEST_DSN_VAR).is_none() {
        println!("SKIPPED benchmark B: {TEST_DSN_VAR} is not set");
        return;
    }
    TestDb::with(|db| async move {
        let (user, ring, task) = seed(&db).await;
        // One connection, no concurrency: the benchmark measures the shape of
        // the transaction and never the contention of a pool (spec 10.2).
        let app = db.pool_as("cadus_app", 1).await;

        for index in 0..WARMUPS {
            let served = serve_once(&app, user, &ring, &task)
                .await
                .unwrap_or_else(|err| panic!("warm-up {index} did not serve: {err}"));
            release(&db.admin, served.id).await;
        }

        let mut samples: Vec<u128> = Vec::with_capacity(SAMPLES);
        let mut served_rows: Vec<Served> = Vec::with_capacity(SAMPLES);
        for index in 0..SAMPLES {
            let start = Instant::now();
            let served = serve_once(&app, user, &ring, &task)
                .await
                .unwrap_or_else(|err| panic!("sample {index} did not serve: {err}"));
            samples.push(start.elapsed().as_nanos());
            release(&db.admin, served.id).await;
            served_rows.push(served);
        }

        let times = Percentiles::of(&samples);
        println!(
            "benchmark B serve ({}): p50 {} ns, p95 {} ns, p99 {} ns, max {} ns over {} samples",
            profile(),
            times.p50,
            times.p95,
            times.p99,
            times.max,
            samples.len()
        );
        write_artifact(&format!(
            "{{\n  \"benchmark\": \"B\",\n  \"profile\": {:?},\n  \"pool_depth\": {},\n  \
             \"batches\": {},\n  \"ring_overlap\": {},\n  \"pop_candidates\": {},\n  \
             \"warmups\": {},\n  \"samples\": {},\n  \"serve_ns\": {{\"p50_ns\": {}, \
             \"p95_ns\": {}, \"p99_ns\": {}, \"max_ns\": {}}},\n  \"p95_budget_ns\": {}\n}}\n",
            profile(),
            POOL_DEPTH,
            BATCHES,
            RING_OVERLAP,
            POP_LIMIT,
            WARMUPS,
            SAMPLES,
            times.p50,
            times.p95,
            times.p99,
            times.max,
            P95_BUDGET_NS,
        ));

        assert_eq!(samples.len(), SAMPLES, "every sample is measured");
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
            times.p95 < P95_BUDGET_NS,
            "the p95 serve transaction took {} ns, and the budget is {P95_BUDGET_NS} ns",
            times.p95
        );
    })
    .await;
}
