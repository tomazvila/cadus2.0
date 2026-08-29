//! Benchmark B at LIFETIME log length: the serve read and the grade transaction
//! over a log of [`SEEDED_EVENTS`] events (M5 review 1, findings F15 and F18).
//!
//! Requirements: L1 (serve a problem, p95 < 150 ms), L2 (grade a verifiable
//! answer, p95 < 300 ms), L4, L5, C2, C3, D4 (`through_seq` is the fold cursor),
//! D-O1, D-O2, D-S2 (the log grows forever), D-S6.
//!
//! `crates/store/tests/bench_serve_roundtrip.rs` and
//! `crates/store/tests/bench_grade_transaction.rs` seed 0 and 201 events. D-S2
//! says the log grows forever, so those two numbers describe a new account and
//! not a learner. This file seeds the log of a learner two hundred sessions in
//! and holds the SAME two segments of `docs/reference/l1-budget.md`: 100 ms for
//! the serve transaction and 150 ms for the grade transaction.
//!
//! # The two findings
//!
//! - **F18** (`benchmark_long_log_serve_holds_the_l1_segment`): the serve, teach
//!   and hint routes open with `cadus_web::serve::open`, which read the whole
//!   log and then folded it. Both terms grow with the lifetime event count, so
//!   the three routes passed their 150 ms budget on the log alone.
//! - **F15** (`benchmark_long_log_grade_holds_the_l2_segment`): the grade
//!   transaction read the whole log twice, once in `open` and once inside
//!   `project_current`, and then folded it twice.
//!
//! # The fixture is a lifetime, not a session
//!
//! [`SESSIONS`] sessions of [`EVENTS_PER_SESSION`] events each. Every session
//! but the last one is closed. The last one is OPEN and carries
//! [`OPEN_SESSION_EVENTS`] events, so the log of the CURRENT session is short
//! while the log of the account is long. That is the shape D-S2 produces, and it
//! is the shape a per-request whole-log read fails on.
//!
//! # What runs when
//!
//! The file carries two gates, and they run under different rules.
//!
//! - **The counting gate runs ALWAYS** (it needs the test DSN and nothing else).
//!   It asserts the LITERAL row counts and the LITERAL fold cursor of every
//!   sample: the open read decodes [`OPEN_SESSION_EVENTS`] rows and never
//!   [`SEEDED_EVENTS`], and the grade fold advances by exactly one line. Those
//!   numbers are deterministic, so a contended runner cannot move them, and a
//!   whole-log read that comes back fails them at once.
//! - **The timing gate runs under `CADUS_BENCH`**, the switch every other
//!   benchmark of `docs/reference/l1-budget.md` takes. Spec section 10.5: a
//!   benchmark beside a test suite measures the scheduler, so a timing
//!   assertion never runs inside `cargo test --workspace`. It fails at p95 above
//!   the segment, never on p50 and never on one slow sample. A debug build holds
//!   a budget ten times wider, the rule `crates/core/tests/answer_check.rs`
//!   already carries for L2.
//!
//! Without `CADUS_BENCH` the run still takes [`samples`] samples and still
//! prints the percentiles; it takes three of them instead of a hundred, so the
//! counting gate stays cheap.

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
    AnswerKind, Attempt, AttemptProblem, Event, SchemaVersion, Secs, SessionEnd, SessionStart,
    Slug, TaskType, Timestamp, WorkQuality,
};
use cadus_core::learner::problem_text_hash;
use cadus_core::pool::{
    Avoid, POOL_ROW_VERSION, PoolAnswer, PoolProblem, Ring, Source, TaskMemory,
};
use cadus_core::projector::ProjectionInput;
use cadus_store::pool::{NewInstance, insert_batch, pop_with_ring_tx};
use cadus_store::state::{
    append_event, load_events_after, load_web_state, lock_web_state, project_and_save,
    project_current, save_web_state,
};
use cadus_store::test_support::TestDb;
use cadus_store::{StoreError, begin_tenant};
use serde_json::{Value as Json, json};
use sqlx::PgPool;
use sqlx::types::chrono::{DateTime, Utc};
use uuid::Uuid;

/// The Unix microsecond instant of 2026-01-01T00:00:00Z.
const BASE_US: i64 = 1_767_225_600_000_000;

/// One day, in microseconds. Session `i` runs on day `i`.
const DAY_US: i64 = 86_400_000_000;

/// The topic of every seeded attempt.
const TOPIC: &str = "adding-integers";

/// The serving key of the pool rows.
const KP_ID: &str = "adding-integers/kp1";

/// The sessions the fixture log carries.
const SESSIONS: usize = 200;

/// The events one CLOSED session carries: one start, 98 attempts, one end.
const EVENTS_PER_SESSION: usize = 100;

/// The events the one OPEN session carries: one start and 99 attempts.
const OPEN_SESSION_EVENTS: usize = 100;

/// The events the whole fixture log carries.
///
/// 199 closed sessions of [`EVENTS_PER_SESSION`], plus the open session.
const SEEDED_EVENTS: usize = (SESSIONS - 1) * EVENTS_PER_SESSION + OPEN_SESSION_EVENTS;

/// The unclaimed rows the fixture puts in the pool.
const POOL_DEPTH: usize = 64;

/// The seed the refill batch records in every `problem` document.
const BATCH_SEED: u64 = 20_260_829;

/// The untimed transactions that warm the connection and the plan cache, under
/// `CADUS_BENCH`.
const BENCH_WARMUPS: usize = 10;

/// The timed transactions of each half, under `CADUS_BENCH`.
const BENCH_SAMPLES: usize = 100;

/// The samples a plain `cargo test` takes. It gates the COUNTS and not the
/// clock, and three samples pin every count this file asserts.
const COUNTING_SAMPLES: usize = 3;

/// The p95 budget of one serve transaction, in nanoseconds: the 100 ms Postgres
/// segment of L1 (`docs/reference/l1-budget.md` section 2).
const SERVE_P95_BUDGET_NS: u128 = 100_000_000;

/// The p95 budget of one grade transaction, in nanoseconds: the 150 ms Postgres
/// segment of L2 (`docs/reference/l1-budget.md` section 3).
const GRADE_P95_BUDGET_NS: u128 = 150_000_000;

/// How much wider the budget is in a debug build.
const DEBUG_BUDGET_FACTOR: u128 = 10;

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

/// Whether the timing gate is armed.
fn timed() -> bool {
    std::env::var_os(BENCH_VAR).is_some()
}

/// The warm-up count of this run.
fn warmups() -> usize {
    if timed() { BENCH_WARMUPS } else { 1 }
}

/// The sample count of this run.
fn samples() -> usize {
    if timed() {
        BENCH_SAMPLES
    } else {
        COUNTING_SAMPLES
    }
}

/// The budget of this build: the release number, ten times wider in debug.
const fn budget(release_ns: u128) -> u128 {
    if cfg!(debug_assertions) {
        release_ns * DEBUG_BUDGET_FACTOR
    } else {
        release_ns
    }
}

/// Write one benchmark artifact and print its path.
fn write_artifact(name: &str, body: &str) {
    let dir = match std::env::var_os(ARTIFACT_DIR_VAR) {
        Some(value) => PathBuf::from(value),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/bench"),
    };
    std::fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    let path = dir.join(name);
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

/// The committed curriculum tree. The fold of every sample reads it.
fn curriculum() -> Curriculum {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../curriculum");
    let (graph, _) = load_curriculum(&root)
        .unwrap_or_else(|err| panic!("the curriculum at {} did not load: {err}", root.display()));
    graph
}

/// The session id of day `day`: `s_<date>a`.
fn session_id(day: usize) -> String {
    let stamp = DateTime::<Utc>::from_timestamp_micros(BASE_US + (day as i64) * DAY_US)
        .expect("the fixture instant is representable");
    format!("s_{}a", stamp.date_naive())
}

/// The task id of the one review task of day `day`.
fn task_id(day: usize) -> String {
    format!("{}-review-{TOPIC}", session_id(day))
}

/// One graded attempt of day `day`, at minute `minute` of that day.
fn attempt_event(day: usize, minute: usize, attempt_id: &str) -> Event {
    Event::Attempt(Attempt {
        ts: Timestamp::from_micros(BASE_US + (day as i64) * DAY_US + (minute as i64) * 60_000_000),
        session: Some(session_id(day)),
        v: SchemaVersion,
        attempt_id: attempt_id.to_string(),
        task_id: task_id(day),
        topic: Slug::new(TOPIC).unwrap(),
        kp: None,
        task_type: TaskType::Review,
        problem: AttemptProblem {
            text: format!("Compute ${minute} + 5$."),
            expected: (minute + 5).to_string(),
        },
        given_answer: (minute + 5).to_string(),
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

/// The whole fixture log, in `seq` order, as `(attempt_id, event)` pairs.
fn fixture_log() -> Vec<(Option<String>, Event)> {
    let mut out: Vec<(Option<String>, Event)> = Vec::with_capacity(SEEDED_EVENTS);
    for day in 0..SESSIONS {
        let session = session_id(day);
        out.push((
            None,
            Event::SessionStart(SessionStart {
                ts: Timestamp::from_micros(BASE_US + (day as i64) * DAY_US),
                session: Some(session.clone()),
                v: SchemaVersion,
            }),
        ));
        let last = day + 1 == SESSIONS;
        let attempts = if last {
            OPEN_SESSION_EVENTS - 1
        } else {
            EVENTS_PER_SESSION - 2
        };
        for index in 0..attempts {
            let id = format!("{}-{index}", task_id(day));
            out.push((Some(id.clone()), attempt_event(day, index + 1, &id)));
        }
        if !last {
            out.push((
                None,
                Event::SessionEnd(SessionEnd {
                    ts: Timestamp::from_micros(
                        BASE_US + (day as i64) * DAY_US + (EVENTS_PER_SESSION as i64) * 60_000_000,
                    ),
                    session: Some(session),
                    v: SchemaVersion,
                    xp_earned: 0.0,
                    minutes: 60.0,
                }),
            ));
        }
    }
    out
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

/// The state of the fixture the run restores between grade samples.
///
/// It is taken BEFORE the first grade of the run, so every sample folds the same
/// log from the same cursor. A snapshot taken after a grade would carry a
/// `through_seq` one line ahead of the restored log, and every later sample
/// would then take the "nothing new" branch and measure nothing.
struct Snapshot {
    model: Json,
    view: Option<Json>,
    through_seq: i64,
    projector_version: i32,
    config_hash: String,
}

/// Insert the whole fixture log in ONE statement, on the admin pool.
///
/// The seed is not measured, so it takes the bulk path: 20,000 round trips of
/// `append_event` would make the fixture the slowest part of the run.
async fn seed_log(pool: &PgPool, user: Uuid) {
    let log = fixture_log();
    assert_eq!(
        log.len(),
        SEEDED_EVENTS,
        "the fixture builder and SEEDED_EVENTS disagree"
    );
    let mut seqs: Vec<i64> = Vec::with_capacity(log.len());
    let mut stamps: Vec<DateTime<Utc>> = Vec::with_capacity(log.len());
    let mut types: Vec<String> = Vec::with_capacity(log.len());
    let mut sessions: Vec<Option<String>> = Vec::with_capacity(log.len());
    let mut attempt_ids: Vec<Option<String>> = Vec::with_capacity(log.len());
    let mut payloads: Vec<Json> = Vec::with_capacity(log.len());
    for (index, (attempt_id, event)) in log.iter().enumerate() {
        seqs.push(index as i64 + 1);
        stamps.push(
            DateTime::<Utc>::from_timestamp_micros(event.ts().micros())
                .expect("the fixture instant is representable"),
        );
        types.push(event.type_name().to_string());
        sessions.push(event.session().map(str::to_string));
        attempt_ids.push(attempt_id.clone());
        payloads.push(serde_json::to_value(event).unwrap());
    }
    sqlx::query!(
        r#"
        INSERT INTO events (user_id, seq, ts, type, session_id, v, attempt_id, payload)
        SELECT $1, line.seq, line.ts, line.type, line.session_id, 1, line.attempt_id, line.payload
        FROM UNNEST($2::bigint[], $3::timestamptz[], $4::text[], $5::text[], $6::text[],
                    $7::jsonb[])
             AS line(seq, ts, type, session_id, attempt_id, payload)
        "#,
        user,
        &seqs[..],
        &stamps[..],
        &types[..],
        &sessions[..] as &[Option<String>],
        &attempt_ids[..] as &[Option<String>],
        &payloads[..]
    )
    .execute(pool)
    .await
    .unwrap();
}

/// Seed one user, the whole log, the folded model, the D-S6 row, and the pool.
async fn seed(db: &TestDb, graph: &Curriculum, cfg: &Config) -> (Uuid, Ring, TaskMemory) {
    let user = db.seed_user("bench-long-log@example.test").await;
    seed_log(&db.admin, user).await;

    let mut tx = begin_tenant(&db.admin, user).await.unwrap();
    let input = ProjectionInput::new(graph, cfg, Timestamp::from_micros(BASE_US));
    project_and_save(&mut tx, user, &input, None).await.unwrap();
    tx.commit().await.unwrap();

    let rows: Vec<NewInstance> = (0..POOL_DEPTH).map(new_instance).collect();
    let inserted = insert_batch(&db.admin, user, KP_ID, &rows).await.unwrap();
    assert_eq!(inserted, POOL_DEPTH as u64, "insert_batch skipped a digest");

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
                  projector_version AS "projector_version!", config_hash AS "config_hash!",
                  session_view
             FROM learner_models WHERE user_id = $1"#,
        user
    )
    .fetch_one(pool)
    .await
    .unwrap();
    Snapshot {
        model: row.model,
        view: row.session_view,
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
    release(pool, claimed).await;
    sqlx::query!(
        "UPDATE learner_models
            SET model = $2, through_seq = $3, projector_version = $4, config_hash = $5,
                session_view = $6
          WHERE user_id = $1",
        user,
        snap.model,
        snap.through_seq,
        snap.projector_version,
        snap.config_hash,
        snap.view
    )
    .execute(pool)
    .await
    .unwrap();
}

/// Release one claimed pool row, outside the measured window.
async fn release(pool: &PgPool, claimed: Uuid) {
    sqlx::query!(
        "UPDATE serving_pool SET claimed_at = NULL WHERE id = $1",
        claimed
    )
    .execute(pool)
    .await
    .unwrap();
}

// ---------------------------------------------------------------------------
// The two measured transactions
// ---------------------------------------------------------------------------

/// What one measured transaction read.
struct Read {
    /// The pool row the transaction claimed.
    claimed: Uuid,
    /// The count of event rows the OPEN read decoded. It is the length of the
    /// open session's window, not the length of the log.
    rows: usize,
    /// Whether the fold took the full-replay branch.
    replayed: bool,
    /// The `seq` an appended event took, when the transaction appended one.
    seq: i64,
    /// The `seq` the fold reached.
    folded_through: i64,
}

/// The read `cadus_web::serve::open` performs, statement for statement.
///
/// The advisory lock, `project_current`, the OPEN SESSION's own event window,
/// and the D-S6 document. No step reads the whole log.
async fn open_once(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user: Uuid,
    input: &ProjectionInput<'_>,
) -> Result<(usize, i64, Json), StoreError> {
    lock_web_state(tx, user).await?;
    let projection = project_current(tx, user, input).await?;
    let after = projection
        .view
        .session_start_seq
        .unwrap_or(0)
        .saturating_sub(1);
    let events = load_events_after(tx, user, after).await?;
    let doc = load_web_state(tx, user).await?.unwrap_or_else(|| json!({}));
    assert!(
        !projection.replayed,
        "the open read took the full-replay branch, so the fixture cache is stale"
    );
    Ok((events.len(), projection.through_seq, doc))
}

/// The D-O1 serve transaction, as `cadus_web::serve::serve` runs it.
async fn serve_once(
    pool: &PgPool,
    user: Uuid,
    input: &ProjectionInput<'_>,
    avoid: &Avoid<'_>,
) -> Result<Read, StoreError> {
    let mut tx = begin_tenant(pool, user).await?;
    let (rows, through, doc) = open_once(&mut tx, user, input).await?;
    let claimed = pop_with_ring_tx(&mut tx, user, KP_ID, avoid)
        .await?
        .claimed
        .expect("the pool holds an unclaimed row");
    save_web_state(&mut tx, user, &doc).await?;
    tx.commit().await?;
    Ok(Read {
        claimed: claimed.row.id,
        rows,
        replayed: false,
        seq: 0,
        folded_through: through,
    })
}

/// The D-O2 grade transaction, as `cadus_web::grade::answer` runs it.
async fn grade_once(
    pool: &PgPool,
    user: Uuid,
    input: &ProjectionInput<'_>,
    attempt_id: &str,
    avoid: &Avoid<'_>,
) -> Result<Read, StoreError> {
    let mut tx = begin_tenant(pool, user).await?;
    let (rows, _, doc) = open_once(&mut tx, user, input).await?;

    let event = attempt_event(SESSIONS - 1, OPEN_SESSION_EVENTS, attempt_id);
    let seq = append_event(&mut tx, user, &event, Some(attempt_id))
        .await?
        .expect("the attempt is new, so the append returns a seq");
    let projection = project_and_save(&mut tx, user, input, None).await?;
    let claimed = pop_with_ring_tx(&mut tx, user, KP_ID, avoid)
        .await?
        .claimed
        .expect("the pool holds an unclaimed row");
    save_web_state(&mut tx, user, &doc).await?;
    tx.commit().await?;

    Ok(Read {
        claimed: claimed.row.id,
        rows,
        replayed: projection.replayed,
        seq,
        folded_through: projection.through_seq,
    })
}

// ---------------------------------------------------------------------------
// The benchmarks
// ---------------------------------------------------------------------------

/// F18: the serve transaction holds its 100 ms segment at lifetime log length.
#[tokio::test]
async fn benchmark_long_log_serve_holds_the_l1_segment() {
    if std::env::var_os(TEST_DSN_VAR).is_none() {
        println!("SKIPPED long-log serve: {TEST_DSN_VAR} is not set");
        return;
    }
    TestDb::with(|db| async move {
        let graph = curriculum();
        let cfg = Config::default();
        let (user, ring, task) = seed(&db, &graph, &cfg).await;
        let input = ProjectionInput::new(&graph, &cfg, Timestamp::from_micros(BASE_US));
        let avoid = Avoid::new(&ring, &task);
        let app = db.pool_as("cadus_app", 1).await;

        for _ in 0..warmups() {
            let read = serve_once(&app, user, &input, &avoid)
                .await
                .unwrap_or_else(|err| panic!("a warm-up serve failed: {err}"));
            release(&db.admin, read.claimed).await;
        }

        let count = samples();
        let mut timings: Vec<u128> = Vec::with_capacity(count);
        let mut reads: Vec<Read> = Vec::with_capacity(count);
        for index in 0..count {
            let start = Instant::now();
            let read = serve_once(&app, user, &input, &avoid)
                .await
                .unwrap_or_else(|err| panic!("serve sample {index} failed: {err}"));
            timings.push(start.elapsed().as_nanos());
            release(&db.admin, read.claimed).await;
            reads.push(read);
        }

        let times = Percentiles::of(&timings);
        let limit = budget(SERVE_P95_BUDGET_NS);
        println!(
            "long-log serve ({}): p50 {} ns, p95 {} ns, p99 {} ns, max {} ns over {} samples, \
             log {} events, {} rows decoded per request, budget {} ns",
            profile(),
            times.p50,
            times.p95,
            times.p99,
            times.max,
            timings.len(),
            SEEDED_EVENTS,
            reads[0].rows,
            limit,
        );
        if std::env::var_os(BENCH_VAR).is_some() {
            write_artifact(
                "benchmark-b-long-log-serve.json",
                &format!(
                    "{{\n  \"benchmark\": \"B-long-log-serve\",\n  \"profile\": {:?},\n  \
                     \"log_events\": {},\n  \"rows_decoded\": {},\n  \"samples\": {},\n  \
                     \"serve_ns\": {{\"p50_ns\": {}, \"p95_ns\": {}, \"p99_ns\": {}, \
                     \"max_ns\": {}}},\n  \"p95_budget_ns\": {}\n}}\n",
                    profile(),
                    SEEDED_EVENTS,
                    reads[0].rows,
                    count,
                    times.p50,
                    times.p95,
                    times.p99,
                    times.max,
                    limit,
                ),
            );
        }

        for (index, read) in reads.iter().enumerate() {
            assert!(
                !read.replayed,
                "serve sample {index} took the full-replay branch; a read route never does"
            );
            // The open read decodes the OPEN SESSION and nothing older. A number
            // that reaches SEEDED_EVENTS means the whole-log read came back.
            assert_eq!(
                read.rows, OPEN_SESSION_EVENTS,
                "serve sample {index} decoded another window"
            );
            // A serve appends nothing, so the cursor stands at the log head.
            assert_eq!(
                read.folded_through, SEEDED_EVENTS as i64,
                "serve sample {index} folded to another cursor"
            );
        }
        assert!(
            !timed() || times.p95 < limit,
            "the p95 serve transaction over {SEEDED_EVENTS} events took {} ns, and the budget \
             is {limit} ns",
            times.p95
        );
    })
    .await;
}

/// F15: the grade transaction holds its 150 ms segment at lifetime log length.
#[tokio::test]
async fn benchmark_long_log_grade_holds_the_l2_segment() {
    if std::env::var_os(TEST_DSN_VAR).is_none() {
        println!("SKIPPED long-log grade: {TEST_DSN_VAR} is not set");
        return;
    }
    TestDb::with(|db| async move {
        let graph = curriculum();
        let cfg = Config::default();
        let (user, ring, task) = seed(&db, &graph, &cfg).await;
        let input = ProjectionInput::new(&graph, &cfg, Timestamp::from_micros(BASE_US));
        let avoid = Avoid::new(&ring, &task);
        let app = db.pool_as("cadus_app", 1).await;

        // The snapshot is taken BEFORE the first grade, so every warm-up and
        // every sample starts from the same cursor and folds the same log.
        let snap = snapshot(&db.admin, user).await;
        for index in 0..warmups() {
            let id = format!("warmup-{index}");
            let read = grade_once(&app, user, &input, &id, &avoid)
                .await
                .unwrap_or_else(|err| panic!("warm-up {index} did not grade: {err}"));
            restore(&db.admin, user, &id, read.claimed, &snap).await;
        }

        let count = samples();
        let mut timings: Vec<u128> = Vec::with_capacity(count);
        let mut reads: Vec<Read> = Vec::with_capacity(count);
        for index in 0..count {
            let id = format!("sample-{index}");
            let start = Instant::now();
            let read = grade_once(&app, user, &input, &id, &avoid)
                .await
                .unwrap_or_else(|err| panic!("grade sample {index} did not grade: {err}"));
            timings.push(start.elapsed().as_nanos());
            restore(&db.admin, user, &id, read.claimed, &snap).await;
            reads.push(read);
        }

        let times = Percentiles::of(&timings);
        let limit = budget(GRADE_P95_BUDGET_NS);
        println!(
            "long-log grade ({}): p50 {} ns, p95 {} ns, p99 {} ns, max {} ns over {} samples, \
             log {} events, {} rows decoded per request, budget {} ns",
            profile(),
            times.p50,
            times.p95,
            times.p99,
            times.max,
            timings.len(),
            SEEDED_EVENTS,
            reads[0].rows,
            limit,
        );
        if std::env::var_os(BENCH_VAR).is_some() {
            write_artifact(
                "benchmark-b-long-log-grade.json",
                &format!(
                    "{{\n  \"benchmark\": \"B-long-log-grade\",\n  \"profile\": {:?},\n  \
                     \"log_events\": {},\n  \"rows_decoded\": {},\n  \"samples\": {},\n  \
                     \"grade_ns\": {{\"p50_ns\": {}, \"p95_ns\": {}, \"p99_ns\": {}, \
                     \"max_ns\": {}}},\n  \"p95_budget_ns\": {}\n}}\n",
                    profile(),
                    SEEDED_EVENTS,
                    reads[0].rows,
                    count,
                    times.p50,
                    times.p95,
                    times.p99,
                    times.max,
                    limit,
                ),
            );
        }

        for (index, read) in reads.iter().enumerate() {
            assert_eq!(
                read.seq,
                SEEDED_EVENTS as i64 + 1,
                "grade sample {index} appended at another seq, so the fixture is growing"
            );
            assert!(
                !read.replayed,
                "grade sample {index} took the full-replay branch (spec section 4.3)"
            );
            assert_eq!(
                read.rows, OPEN_SESSION_EVENTS,
                "grade sample {index} decoded another window"
            );
            // The fold advanced onto the appended line. A cursor that stayed put
            // means the sample folded nothing and measured nothing.
            assert_eq!(
                read.folded_through,
                SEEDED_EVENTS as i64 + 1,
                "grade sample {index} folded to another cursor"
            );
        }
        assert!(
            !timed() || times.p95 < limit,
            "the p95 grade transaction over {SEEDED_EVENTS} events took {} ns, and the budget \
             is {limit} ns",
            times.p95
        );
    })
    .await;
}
