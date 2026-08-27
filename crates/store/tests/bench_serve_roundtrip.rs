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
//!   SELECT ... FROM serving_pool
//!    WHERE claimed_at IS NULL
//!    ORDER BY created_at LIMIT 8
//!      FOR UPDATE SKIP LOCKED               -- D-O1 pop, at most POP_CANDIDATES
//!   -- the D5 ring rejects the digests it holds; the first survivor wins
//!   UPDATE serving_pool SET claimed_at = now()   -- the claim
//!   UPDATE web_states  SET doc = $2               -- served problem plus ring
//! COMMIT;
//! ```
//!
//! # The fixture
//!
//! One seeded user, one knowledge point, [`POOL_DEPTH`] unclaimed rows with
//! distinct `created_at` values, and a full [`RING_OVERLAP`]-deep overlap
//! between the D5 ring and the oldest pool rows. The pop therefore skips three
//! rows on every sample and serves the fourth, so the measured transaction
//! carries the skip loop and not a lucky first hit.
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

use std::path::{Path, PathBuf};
use std::time::Instant;

use cadus_core::learner::problem_text_hash;
use cadus_core::pool::{Avoid, POP_CANDIDATES, Pick, Ring, TaskMemory, pick};
use cadus_store::begin_tenant;
use cadus_store::test_support::TestDb;
use sqlx::PgPool;
use sqlx::types::JsonValue;
use uuid::Uuid;

/// The knowledge point of the fixture.
const KP_ID: &str = "perfect-squares";

/// The unclaimed rows the fixture puts in the pool (spec section 10.2).
const POOL_DEPTH: usize = 200;

/// The untimed transactions that warm the connection and the plan cache.
const WARMUPS: usize = 50;

/// The timed transactions (spec section 10.2).
const SAMPLES: usize = 500;

/// The count of ring digests that also sit in the pool.
///
/// The three rows are the three oldest, so `ORDER BY created_at` puts them
/// first and the anti-repeat rule skips exactly three rows per serve.
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

/// The `problem` document of pool row `index` (D-S5, spec section 3.2).
///
/// The row carries the seed and the drawn bindings, so a reviewer reproduces
/// the instance from the row alone.
fn problem_document(index: usize) -> JsonValue {
    let text = statement(index);
    serde_json::json!({
        "text": text,
        "seed": 20_260_827_u64,
        "bindings": {"a": index + 1},
        "kind": "numeric",
    })
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

    // The ring holds 20 digests. The three oldest pool rows are among them, so
    // the pop skips three rows and serves the fourth.
    let mut ring = Ring::new();
    for index in 0..RING_OVERLAP {
        ring.push(&instance_hash(index));
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

    for index in 0..POOL_DEPTH {
        // The oldest row is index 0. `ORDER BY created_at` then reads the rows
        // in index order, so the fixture's ring overlap is deterministic.
        let age_secs = i32::try_from(POOL_DEPTH - index).unwrap();
        sqlx::query!(
            "INSERT INTO serving_pool
                 (user_id, kp_id, source, problem, expected_answer, instance_hash, created_at)
             VALUES ($1, $2, 'template', $3, $4, $5, now() - ($6::int * interval '1 second'))",
            user,
            KP_ID,
            problem_document(index),
            JsonValue::String(((index + 1) * (index + 1)).to_string()),
            instance_hash(index),
            age_secs
        )
        .execute(&db.admin)
        .await
        .unwrap();
    }

    (user, ring, task)
}

// ---------------------------------------------------------------------------
// The measured transaction
// ---------------------------------------------------------------------------

/// The outcome of one serve transaction.
struct Served {
    /// The pool row the transaction claimed.
    id: Uuid,
    /// The anti-repeat decision the pop made.
    pick: Pick,
}

/// Run the D-O1 serve transaction once and return what it served.
///
/// The caller times this function. Everything inside it is on the request path
/// of `POST /api/task/{task_id}/serve`: the tenant binding, the learner-model
/// read, the pool pop, the claim, the state write, and the commit.
async fn serve_once(pool: &PgPool, user: Uuid, ring: &Ring, task: &TaskMemory) -> Served {
    let mut tx = begin_tenant(pool, user).await.unwrap();

    let model = sqlx::query!(
        "SELECT model, through_seq FROM learner_models WHERE user_id = $1",
        user
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        model.through_seq, 41,
        "the learner model row is the fixture"
    );

    let candidates = sqlx::query!(
        "SELECT id, problem, expected_answer, instance_hash, source, content_digest
           FROM serving_pool
          WHERE user_id = $1 AND kp_id = $2 AND claimed_at IS NULL
          ORDER BY created_at
          LIMIT $3
            FOR UPDATE SKIP LOCKED",
        user,
        KP_ID,
        POP_CANDIDATES as i64
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();

    let hashes: Vec<String> = candidates
        .iter()
        .map(|row| row.instance_hash.clone())
        .collect();
    let avoid = Avoid::new(ring, task);
    let chosen = pick(&hashes, &avoid).expect("the pool is not empty");
    let row = &candidates[chosen.index];

    sqlx::query!(
        "UPDATE serving_pool SET claimed_at = now() WHERE id = $1",
        row.id
    )
    .execute(&mut *tx)
    .await
    .unwrap();

    // The served digest enters BOTH windows, and the caller writes them back in
    // the same transaction as the claim (D5, D-S6).
    let mut next_ring = ring.clone();
    let mut next_task = task.clone();
    next_ring.push(&row.instance_hash);
    next_task.push(&row.instance_hash);
    let doc = serde_json::json!({
        "served": {"pool_id": row.id, "text": row.problem, "expected": row.expected_answer},
        "ring": next_ring,
        "task_memory": next_task,
    });
    sqlx::query!(
        "UPDATE web_states SET doc = $2, updated_at = now() WHERE user_id = $1",
        user,
        doc
    )
    .execute(&mut *tx)
    .await
    .unwrap();

    tx.commit().await.unwrap();
    Served {
        id: row.id,
        pick: chosen,
    }
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

        for _ in 0..WARMUPS {
            let served = serve_once(&app, user, &ring, &task).await;
            release(&db.admin, served.id).await;
        }

        let mut samples: Vec<u128> = Vec::with_capacity(SAMPLES);
        let mut picks: Vec<Pick> = Vec::with_capacity(SAMPLES);
        for _ in 0..SAMPLES {
            let start = Instant::now();
            let served = serve_once(&app, user, &ring, &task).await;
            samples.push(start.elapsed().as_nanos());
            picks.push(served.pick);
            release(&db.admin, served.id).await;
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
             \"ring_overlap\": {},\n  \"pop_candidates\": {},\n  \"warmups\": {},\n  \
             \"samples\": {},\n  \"serve_ns\": {{\"p50_ns\": {}, \"p95_ns\": {}, \
             \"p99_ns\": {}, \"max_ns\": {}}},\n  \"p95_budget_ns\": {}\n}}\n",
            profile(),
            POOL_DEPTH,
            RING_OVERLAP,
            POP_CANDIDATES,
            WARMUPS,
            SAMPLES,
            times.p50,
            times.p95,
            times.p99,
            times.max,
            P95_BUDGET_NS,
        ));

        assert_eq!(samples.len(), SAMPLES, "every sample is measured");
        // The ring blocks the three oldest rows on every sample, so the pop
        // skips three and serves the fourth. A pick that skips none means the
        // fixture stopped exercising the anti-repeat rule.
        for (index, chosen) in picks.iter().enumerate() {
            assert_eq!(
                (chosen.index, chosen.skipped, chosen.exhausted),
                (3, 3, false),
                "sample {index} did not skip the three blocked rows"
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
