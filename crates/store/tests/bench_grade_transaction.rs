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

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use cadus_core::config::Config;
use cadus_core::curriculum::{Curriculum, load_curriculum};
use cadus_core::event::{
    AnswerKind, Attempt, AttemptProblem, Event, SchemaVersion, Secs, SessionStart, Slug, TaskType,
    Timestamp, WorkQuality,
};
use cadus_core::learner::problem_text_hash;
use cadus_core::pool::{
    Avoid, POOL_ROW_VERSION, PoolAnswer, PoolProblem, Ring, Source, TaskMemory,
};
use cadus_core::projector::ProjectionInput;
use cadus_store::diagnosis::{JobPayload, PAYLOAD_VERSION, enqueue};
use cadus_store::pool::{NewInstance, insert_batch, pop_with_ring_tx};
use cadus_store::state::{
    append_event, load_events, load_web_state, lock_web_state, project_and_save, project_current,
    save_web_state,
};
use cadus_store::test_support::TestDb;
use cadus_store::{StoreError, begin_tenant};
use serde_json::{Value as Json, json};
use sqlx::PgPool;
use uuid::Uuid;

/// The Unix microsecond instant of 2026-01-01T00:00:00Z.
const BASE_US: i64 = 1_767_225_600_000_000;

/// The session every seeded event belongs to.
const SESSION: &str = "s_2026-01-01a";

/// The topic of every seeded attempt.
const TOPIC: &str = "adding-integers";

/// The serving key of the pool rows.
const KP_ID: &str = "adding-integers/kp1";

/// The task the attempts belong to.
const TASK_ID: &str = "s_2026-01-01a-review-adding-integers";

/// The count of attempt events the fixture log carries before the run.
///
/// A learner some weeks into a course carries a log of this order. The fold of
/// every sample reads all of them, so the number is part of what the p95 below
/// describes.
const SEEDED_ATTEMPTS: usize = 200;

/// The unclaimed rows the fixture puts in the pool.
const POOL_DEPTH: usize = 64;

/// The seed the refill batch records in every `problem` document.
const BATCH_SEED: u64 = 20_260_829;

/// The untimed transactions that warm the connection and the plan cache.
const WARMUPS: usize = 20;

/// The timed transactions.
const SAMPLES: usize = 200;

/// The p95 budget of one grade transaction, in nanoseconds: 150 ms of the 300 ms
/// of L2 (`docs/reference/l1-budget.md`).
const P95_BUDGET_NS: u128 = 150_000_000;

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
    let path = dir.join("benchmark-b-grade.json");
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

/// The committed curriculum tree. The fold of every sample reads it, so the
/// benchmark folds against the arena the deployment folds against.
fn curriculum() -> Curriculum {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../curriculum");
    let (graph, _) = load_curriculum(&root)
        .unwrap_or_else(|err| panic!("the curriculum at {} did not load: {err}", root.display()));
    graph
}

/// One graded attempt on [`TOPIC`].
fn attempt_event(attempt_id: &str, index: usize) -> Event {
    Event::Attempt(Attempt {
        ts: Timestamp::from_micros(BASE_US + (index as i64) * 60_000_000),
        session: Some(SESSION.to_string()),
        v: SchemaVersion,
        attempt_id: attempt_id.to_string(),
        task_id: TASK_ID.to_string(),
        topic: Slug::new(TOPIC).unwrap(),
        kp: None,
        task_type: TaskType::Review,
        problem: AttemptProblem {
            text: format!("Compute ${index} + 5$."),
            expected: (index + 5).to_string(),
        },
        given_answer: (index + 5).to_string(),
        work: None,
        answer_kind: Some(AnswerKind::Numeric),
        correct: true,
        secs: Secs::new(12).unwrap(),
        error_tags: Vec::new(),
        work_quality: WorkQuality::NearlyPerfect,
        grader_note: Some("deterministic".to_string()),
        assisted: false,
    })
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

/// Pool row `index`, in the shape the D-O4 refill inserts (D-S5).
fn new_instance(index: usize) -> NewInstance {
    let text = format!("Compute ${} + 11$.", index + 1);
    let mut bindings = BTreeMap::new();
    bindings.insert("a".to_string(), (index + 1).to_string());
    NewInstance {
        source: Source::Template,
        content_digest: None,
        instance_hash: problem_text_hash(&text),
        problem: PoolProblem {
            v: POOL_ROW_VERSION,
            text,
            bindings,
            seed: BATCH_SEED,
        },
        expected_answer: PoolAnswer {
            v: POOL_ROW_VERSION,
            answer: (index + 12).to_string(),
        },
    }
}

/// The state of the fixture the run restores between samples.
struct Snapshot {
    /// The `learner_models` document before the first timed sample.
    model: Json,
    /// The `through_seq` of that row.
    through_seq: i64,
    /// The `projector_version` of that row.
    projector_version: i32,
    /// The `config_hash` of that row.
    config_hash: String,
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

    let rows: Vec<NewInstance> = (0..POOL_DEPTH).map(new_instance).collect();
    let inserted = insert_batch(&db.admin, user, KP_ID, &rows).await.unwrap();
    assert_eq!(
        inserted, POOL_DEPTH as u64,
        "insert_batch skipped a digest, so the pool is not the depth it claims"
    );

    let mut ring = Ring::new();
    for filler in 0..Ring::capacity() {
        ring.push(&problem_text_hash(&format!("An older problem {filler}.")));
    }
    let mut task = TaskMemory::new();
    for filler in 0..TaskMemory::capacity() {
        task.push(&problem_text_hash(&format!("A task problem {filler}.")));
    }

    sqlx::query!(
        "INSERT INTO web_states (user_id, doc) VALUES ($1, $2)",
        user,
        json!({"served": {}, "ring": ring, "task_memory": task})
    )
    .execute(&db.admin)
    .await
    .unwrap();

    (user, ring, task)
}

/// Read the `learner_models` row of `user`.
async fn snapshot(pool: &PgPool, user: Uuid) -> Snapshot {
    let row = sqlx::query!(
        r#"SELECT model AS "model!", through_seq AS "through_seq!",
                  projector_version AS "projector_version!", config_hash AS "config_hash!"
             FROM learner_models WHERE user_id = $1"#,
        user
    )
    .fetch_one(pool)
    .await
    .unwrap();
    Snapshot {
        model: row.model,
        through_seq: row.through_seq,
        projector_version: row.projector_version,
        config_hash: row.config_hash,
    }
}

/// Put the fixture back the way the run found it, outside the measured window.
async fn restore(pool: &PgPool, user: Uuid, attempt_id: &str, claimed: Uuid, snap: &Snapshot) {
    sqlx::query!(
        "DELETE FROM events WHERE user_id = $1 AND attempt_id = $2",
        user,
        attempt_id
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query!(
        "DELETE FROM diagnosis_jobs WHERE user_id = $1 AND attempt_id = $2",
        user,
        attempt_id
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query!(
        "UPDATE serving_pool SET claimed_at = NULL WHERE id = $1",
        claimed
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query!(
        "UPDATE learner_models
            SET model = $2, through_seq = $3, projector_version = $4, config_hash = $5
          WHERE user_id = $1",
        user,
        snap.model,
        snap.through_seq,
        snap.projector_version,
        snap.config_hash
    )
    .execute(pool)
    .await
    .unwrap();
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
        restore(&db.admin, user, &id, graded.claimed, &snap).await;
    }
    snap
}

// ---------------------------------------------------------------------------
// The benchmark
// ---------------------------------------------------------------------------

/// Benchmark B: 200 grade transactions hold the 150 ms segment of L2.
#[tokio::test]
async fn benchmark_b_grade_transaction_holds_the_l2_segment() {
    if std::env::var_os(BENCH_VAR).is_none() {
        println!("SKIPPED benchmark B (grade): {BENCH_VAR} is not set");
        return;
    }
    if std::env::var_os(TEST_DSN_VAR).is_none() {
        println!("SKIPPED benchmark B (grade): {TEST_DSN_VAR} is not set");
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
        let mut samples: Vec<u128> = Vec::with_capacity(SAMPLES);
        let mut graded_rows: Vec<Graded> = Vec::with_capacity(SAMPLES);
        for index in 0..SAMPLES {
            let id = format!("sample-{index}");
            let start = Instant::now();
            let graded = grade_once(&app, user, &input, &id, index, &avoid)
                .await
                .unwrap_or_else(|err| panic!("sample {index} did not grade: {err}"));
            samples.push(start.elapsed().as_nanos());
            restore(&db.admin, user, &id, graded.claimed, &snap).await;
            graded_rows.push(graded);
        }

        let times = Percentiles::of(&samples);
        println!(
            "benchmark B grade ({}): p50 {} ns, p95 {} ns, p99 {} ns, max {} ns over {} samples, \
             {} events per fold",
            profile(),
            times.p50,
            times.p95,
            times.p99,
            times.max,
            samples.len(),
            graded_rows[0].events,
        );
        write_artifact(&format!(
            "{{\n  \"benchmark\": \"B-grade\",\n  \"profile\": {:?},\n  \"seeded_attempts\": {},\n  \
             \"log_events\": {},\n  \"pool_depth\": {},\n  \"warmups\": {},\n  \"samples\": {},\n  \
             \"grade_ns\": {{\"p50_ns\": {}, \"p95_ns\": {}, \"p99_ns\": {}, \"max_ns\": {}}},\n  \
             \"p95_budget_ns\": {}\n}}\n",
            profile(),
            SEEDED_ATTEMPTS,
            graded_rows[0].events,
            POOL_DEPTH,
            WARMUPS,
            SAMPLES,
            times.p50,
            times.p95,
            times.p99,
            times.max,
            P95_BUDGET_NS,
        ));

        assert_eq!(samples.len(), SAMPLES, "every sample is measured");
        // The restore puts the log back, so every sample folds the same events
        // and appends at the same `seq`. A drifting `seq` means the fixture is
        // growing and the percentiles describe a run, not a transaction.
        for (index, graded) in graded_rows.iter().enumerate() {
            assert_eq!(
                graded.events,
                SEEDED_ATTEMPTS + 1,
                "sample {index} folded a log of another length"
            );
            assert_eq!(
                graded.seq,
                SEEDED_ATTEMPTS as i64 + 2,
                "sample {index} appended at another seq"
            );
            assert!(
                !graded.replayed,
                "sample {index} took the full-replay branch, which no grade of a fresh attempt \
                 takes (spec section 4.3)"
            );
            assert!(
                !graded.resume_replayed,
                "sample {index} replayed the whole log BEFORE the append, which no grade of a \
                 warm fixture takes (spec section 4.3)"
            );
            assert_eq!(
                graded.resumed_through,
                SEEDED_ATTEMPTS as i64 + 1,
                "sample {index} resumed from a cursor that is not the head of the log"
            );
            assert_eq!(
                graded.folded_through,
                SEEDED_ATTEMPTS as i64 + 2,
                "sample {index} did not fold the appended attempt"
            );
        }
        assert!(
            times.p95 < P95_BUDGET_NS,
            "the p95 grade transaction took {} ns, and the budget is {P95_BUDGET_NS} ns",
            times.p95
        );
    })
    .await;
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
    if std::env::var_os(TEST_DSN_VAR).is_none() {
        println!("SKIPPED the harness check: {TEST_DSN_VAR} is not set");
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
        restore(&db.admin, user, "check-0", graded.claimed, &snap).await;

        assert_eq!(
            graded.events,
            SEEDED_ATTEMPTS + 1,
            "the grade folded a log of another length"
        );
        assert_eq!(
            graded.seq,
            SEEDED_ATTEMPTS as i64 + 2,
            "the grade appended at another seq"
        );
        assert!(
            !graded.resume_replayed,
            "the fold before the append replayed the whole log, so the restored cursor names a \
             line the log does not hold"
        );
        assert_eq!(
            graded.resumed_through,
            SEEDED_ATTEMPTS as i64 + 1,
            "the fold before the append reached another seq than the head of the log"
        );
        assert_eq!(
            graded.folded_through,
            SEEDED_ATTEMPTS as i64 + 2,
            "the fold after the append did not reach the appended attempt"
        );
        assert!(
            !graded.replayed,
            "the fold after the append replayed the whole log"
        );
    })
    .await;
}
