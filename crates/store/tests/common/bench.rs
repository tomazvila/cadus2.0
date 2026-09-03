//! The shared harness of the benchmarks: the gating, the percentiles, the
//! artifact, the report line, the curriculum, the pool rows, the two
//! anti-repeat windows, and the fixture restore between samples.

use std::future::Future;
use std::path::{Path, PathBuf};

use cadus_core::curriculum::{Curriculum, load_curriculum};
use cadus_core::learner::problem_text_hash;
use cadus_core::pool::{POOL_ROW_VERSION, PoolAnswer, PoolProblem, Ring, Source, TaskMemory};
use cadus_store::pool::NewInstance;
use serde_json::{Value as Json, json};
use sqlx::PgPool;
use uuid::Uuid;

/// The environment variable that turns the timing gates on.
pub const BENCH_VAR: &str = "CADUS_BENCH";

/// The environment variable that moves the artifact directory.
pub const ARTIFACT_DIR_VAR: &str = "CADUS_BENCH_DIR";

/// The environment variable that names the throwaway cluster.
pub const TEST_DSN_VAR: &str = "CADUS_TEST_DATABASE_URL";

/// How much wider a budget is in a debug build.
pub const DEBUG_BUDGET_FACTOR: u128 = 10;

/// Whether the timing gate is armed.
pub fn timed() -> bool {
    std::env::var_os(BENCH_VAR).is_some()
}

/// Whether the throwaway cluster is named. A run without it prints the
/// SKIPPED line of `name` and the caller returns.
pub fn dsn_set(name: &str) -> bool {
    if std::env::var_os(TEST_DSN_VAR).is_some() {
        return true;
    }
    println!("SKIPPED {name}: {TEST_DSN_VAR} is not set");
    false
}

/// The round count of this run: `bench` under the timing gate, `counting`
/// on a plain `cargo test`, which gates the COUNTS and not the clock.
pub fn rounds(bench: usize, counting: usize) -> usize {
    if timed() { bench } else { counting }
}

/// The budget of this build: the release number, ten times wider in debug.
pub const fn budget(release_ns: u128) -> u128 {
    if cfg!(debug_assertions) {
        release_ns * DEBUG_BUDGET_FACTOR
    } else {
        release_ns
    }
}

/// The `percent` percentile of a sorted sample, by the nearest-rank rule.
///
/// The rank is `ceil(percent * n / 100)`, counted from one. The arithmetic is
/// integer arithmetic, so no float enters a reported number (D6).
pub fn percentile(sorted: &[u128], percent: u128) -> u128 {
    assert!(!sorted.is_empty(), "a percentile needs a sample");
    let count = sorted.len() as u128;
    let rank = (percent * count).div_ceil(100).max(1);
    let index = usize::try_from(rank - 1).unwrap_or(0);
    sorted[index.min(sorted.len() - 1)]
}

/// The p50, p95, p99, and maximum of a sample of nanosecond durations.
pub struct Percentiles {
    pub p50: u128,
    pub p95: u128,
    pub p99: u128,
    pub max: u128,
}

impl Percentiles {
    /// Read the percentiles of one sample. The function sorts its own copy.
    pub fn of(samples: &[u128]) -> Self {
        let mut sorted = samples.to_vec();
        sorted.sort_unstable();
        Self {
            p50: percentile(&sorted, 50),
            p95: percentile(&sorted, 95),
            p99: percentile(&sorted, 99),
            max: *sorted.last().expect("a percentile needs a sample"),
        }
    }

    /// The JSON object of the four numbers, for the artifact.
    pub fn json(&self) -> Json {
        json!({
            "p50_ns": self.p50,
            "p95_ns": self.p95,
            "p99_ns": self.p99,
            "max_ns": self.max,
        })
    }
}

/// The name of the build profile, for the artifact.
pub fn profile() -> &'static str {
    if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    }
}

/// The artifact directory: `CADUS_BENCH_DIR`, or `target/bench`.
pub fn artifact_dir() -> PathBuf {
    match std::env::var_os(ARTIFACT_DIR_VAR) {
        Some(value) => PathBuf::from(value),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/bench"),
    }
}

/// Write one benchmark artifact into `dir` and print its path.
pub fn write_artifact_to(dir: &Path, name: &str, body: &str) {
    std::fs::create_dir_all(dir).expect("the artifact directory is created");
    let path = dir.join(name);
    std::fs::write(&path, body).expect("the artifact is written");
    println!("artifact: {}", path.display());
}

/// Write one benchmark artifact into the artifact directory.
pub fn write_artifact(name: &str, body: &str) {
    write_artifact_to(&artifact_dir(), name, body);
}

/// Print the report line of one benchmark.
pub fn report(name: &str, times: &Percentiles, count: usize, extra: &str) {
    println!(
        "{name} ({}): p50 {} ns, p95 {} ns, p99 {} ns, max {} ns over {count} samples{extra}",
        profile(),
        times.p50,
        times.p95,
        times.p99,
        times.max,
    );
}

/// The artifact document of one benchmark: the name, the profile, the
/// percentiles under `key`, the budget, and the fields of the run.
pub fn artifact_json(
    benchmark: &str,
    key: &str,
    times: &Percentiles,
    budget_ns: u128,
    fields: &[(&str, Json)],
) -> String {
    let mut doc = json!({
        "benchmark": benchmark,
        "profile": profile(),
        key: times.json(),
        "p95_budget_ns": budget_ns,
    });
    let map = doc.as_object_mut().expect("the artifact is an object");
    for (name, value) in fields {
        map.insert((*name).to_string(), value.clone());
    }
    let mut body = serde_json::to_string_pretty(&doc).expect("the artifact serializes");
    body.push('\n');
    body
}

/// The committed curriculum tree. The fold of every sample reads it, so the
/// benchmark folds against the arena the deployment folds against.
pub fn curriculum() -> Curriculum {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../curriculum");
    let (graph, _) = load_curriculum(&root).expect("the committed curriculum loads");
    graph
}

/// One pool row in the shape the D-O4 refill inserts (D-S5): `text` bound to
/// `a = binding`, with `answer` and the batch `seed` in the documents, and
/// the M3 `problem_text_hash` of the text as the anti-repeat digest.
pub fn instance_row(text: String, answer: String, binding: String, seed: u64) -> NewInstance {
    let mut bindings = std::collections::BTreeMap::new();
    bindings.insert("a".to_string(), binding);
    NewInstance {
        source: Source::Template,
        content_digest: None,
        instance_hash: problem_text_hash(&text),
        problem: PoolProblem {
            v: POOL_ROW_VERSION,
            text,
            bindings,
            seed,
        },
        expected_answer: PoolAnswer {
            v: POOL_ROW_VERSION,
            answer,
        },
    }
}

/// Pool row `index` of the grade fixtures: `Compute $<index+1> + 11$.`
pub fn bench_instance(index: usize, seed: u64) -> NewInstance {
    instance_row(
        format!("Compute ${} + 11$.", index + 1),
        (index + 12).to_string(),
        (index + 1).to_string(),
        seed,
    )
}

/// The two anti-repeat windows, filled with digests the pool never holds.
pub fn full_windows() -> (Ring, TaskMemory) {
    let mut ring = Ring::new();
    for filler in 0..Ring::capacity() {
        ring.push(&problem_text_hash(&format!("An older problem {filler}.")));
    }
    let mut task = TaskMemory::new();
    for filler in 0..TaskMemory::capacity() {
        task.push(&problem_text_hash(&format!("A task problem {filler}.")));
    }
    (ring, task)
}

/// Seed the D-S6 row of `user` with the two windows and nothing served.
pub async fn seed_web_state(admin: &PgPool, user: Uuid, ring: &Ring, task: &TaskMemory) {
    sqlx::query!(
        "INSERT INTO web_states (user_id, doc) VALUES ($1, $2)",
        user,
        json!({"served": {}, "ring": ring, "task_memory": task})
    )
    .execute(admin)
    .await
    .expect("the state row inserts");
}

/// Release one claimed pool row, outside the measured window.
pub async fn release(pool: &PgPool, claimed: Uuid) {
    sqlx::query!(
        "UPDATE serving_pool SET claimed_at = NULL WHERE id = $1",
        claimed
    )
    .execute(pool)
    .await
    .expect("the pool row is released");
}

/// The state of the fixture the run restores between grade samples.
///
/// It is taken BEFORE the first grade of the run, so every sample folds the same
/// log from the same cursor. A snapshot taken after a grade would carry a
/// `through_seq` one line ahead of the restored log, and every later sample
/// would then take the "nothing new" branch and measure nothing (M5 review 2,
/// finding V10).
pub struct Snapshot {
    pub model: Json,
    pub view: Option<Json>,
    pub through_seq: i64,
    pub projector_version: i32,
    pub config_hash: String,
}

/// Read the `learner_models` row of `user`.
pub async fn snapshot(pool: &PgPool, user: Uuid) -> Snapshot {
    let row = sqlx::query!(
        r#"SELECT model AS "model!", through_seq AS "through_seq!",
                  projector_version AS "projector_version!", config_hash AS "config_hash!",
                  session_view
             FROM learner_models WHERE user_id = $1"#,
        user
    )
    .fetch_one(pool)
    .await
    .expect("the learner model row reads");
    Snapshot {
        model: row.model,
        view: row.session_view,
        through_seq: row.through_seq,
        projector_version: row.projector_version,
        config_hash: row.config_hash,
    }
}

/// Put the fixture back the way the run found it, outside the measured
/// window: delete the appended attempt, release the claimed row, and write
/// the snapshot back.
pub async fn restore(pool: &PgPool, user: Uuid, attempt_id: &str, claimed: Uuid, snap: &Snapshot) {
    sqlx::query!(
        "DELETE FROM events WHERE user_id = $1 AND attempt_id = $2",
        user,
        attempt_id
    )
    .execute(pool)
    .await
    .expect("the appended attempt is deleted");
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
    .expect("the snapshot is written back");
}

/// Delete the A4 job row of one attempt, outside the measured window.
pub async fn delete_job(pool: &PgPool, user: Uuid, attempt_id: &str) {
    sqlx::query!(
        "DELETE FROM diagnosis_jobs WHERE user_id = $1 AND attempt_id = $2",
        user,
        attempt_id
    )
    .execute(pool)
    .await
    .expect("the job row is deleted");
}

/// Run `round` `count` times and collect the nanoseconds and the value of
/// each round. Every round times its own measured window.
pub async fn timed_rounds<T, F, Fut>(count: usize, mut round: F) -> (Vec<u128>, Vec<T>)
where
    F: FnMut(usize) -> Fut,
    Fut: Future<Output = (u128, T)>,
{
    let mut timings: Vec<u128> = Vec::with_capacity(count);
    let mut values: Vec<T> = Vec::with_capacity(count);
    for index in 0..count {
        let (nanos, value) = round(index).await;
        timings.push(nanos);
        values.push(value);
    }
    (timings, values)
}
