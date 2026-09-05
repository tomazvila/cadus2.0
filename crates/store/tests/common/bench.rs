//! The harness of the store benchmarks: the cluster gate, the round counts,
//! the report line, the artifact document, the curriculum, the pool rows, the
//! two anti-repeat windows, and the fixture restore between samples. The
//! percentiles, the budget, and the artifact file come from
//! `cadus_testkit::bench`.

use std::future::Future;
use std::path::Path;

use cadus_core::curriculum::{Curriculum, load_curriculum};
use cadus_core::event::Event;
use cadus_core::learner::problem_text_hash;
use cadus_core::pool::{
    Avoid, POOL_ROW_VERSION, PoolAnswer, PoolProblem, Ring, Source, TaskMemory,
};
use cadus_core::projector::ProjectionInput;
use cadus_store::pool::{NewInstance, insert_batch, pop_with_ring_tx};
use cadus_store::state::{Projection, append_event, project_and_save, save_web_state};
use cadus_store::test_support::TestDb;
use cadus_testkit::bench::{Percentiles, benchmarks_are_on, profile};

use super::events::Fixture;
use cadus_store::{StoreError, begin_tenant};
use serde_json::{Value as Json, json};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

/// The environment variable that names the throwaway cluster.
pub const TEST_DSN_VAR: &str = "CADUS_TEST_DATABASE_URL";

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
    if benchmarks_are_on() { bench } else { counting }
}

/// Print the report line of one benchmark.
pub fn report(name: &str, times: &Percentiles, count: usize, extra: &str) {
    println!(
        "{name} ({}): {} over {count} samples{extra}",
        profile(),
        times.phrase(),
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

/// The one app connection of a benchmark: no concurrency, so the run measures
/// the shape of the transaction and never the contention of a pool (spec
/// 10.2).
pub async fn app_pool(db: &TestDb) -> PgPool {
    db.pool_as("cadus_app", 1).await
}

/// Insert `depth` grade-fixture rows for `(user, kp_id)` with the admin pool,
/// and check that every digest entered.
pub async fn seed_pool(admin: &PgPool, user: Uuid, kp_id: &str, depth: usize, seed: u64) {
    let rows: Vec<NewInstance> = (0..depth)
        .map(|index| bench_instance(index, seed))
        .collect();
    let inserted = insert_batch(admin, user, kp_id, &rows)
        .await
        .expect("the pool inserts");
    assert_eq!(
        inserted, depth as u64,
        "insert_batch skipped a digest, so the pool is not the depth it claims"
    );
}

/// Steps 6 and 7 of the grade transaction: append `event` under `attempt_id`
/// and fold the log into the `learner_models` row.
pub async fn append_and_fold(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
    input: &ProjectionInput<'_>,
    event: &Event,
    attempt_id: Option<&str>,
) -> Result<(i64, Projection), StoreError> {
    let seq = append_event(tx, user, event, attempt_id)
        .await?
        .expect("the event is new, so the append returns a seq");
    let projection = project_and_save(tx, user, input, None).await?;
    Ok((seq, projection))
}

/// Step 10 of the grade transaction and the end of the serve transaction: pop
/// and claim one row of `kp_id`, write the D-S6 document back, and commit.
/// The answer is the id of the claimed row.
pub async fn claim_and_commit(
    mut tx: Transaction<'static, Postgres>,
    user: Uuid,
    kp_id: &str,
    avoid: &Avoid<'_>,
    doc: &Json,
) -> Result<Uuid, StoreError> {
    let claimed = pop_with_ring_tx(&mut tx, user, kp_id, avoid)
        .await?
        .claimed
        .expect("the pool holds an unclaimed row");
    save_web_state(&mut tx, user, doc).await?;
    tx.commit().await?;
    Ok(claimed.row.id)
}

/// Time one measured step and stop the run with `what` when it fails.
pub async fn timed_step<T, Fut>(what: String, step: Fut) -> (u128, T)
where
    Fut: Future<Output = Result<T, StoreError>>,
{
    let start = std::time::Instant::now();
    let value = step
        .await
        .unwrap_or_else(|err| panic!("{what} failed: {err}"));
    (start.elapsed().as_nanos(), value)
}

/// One seeded benchmark run: the learner, the two anti-repeat windows, the
/// curriculum and config of the fold, and the one app connection.
pub struct Run {
    pub user: Uuid,
    pub ring: Ring,
    pub task: TaskMemory,
    pub fixture: Fixture,
    pub app: PgPool,
}

impl Run {
    /// The projection input and the D5 view of the windows.
    pub fn views(&self) -> (ProjectionInput<'_>, Avoid<'_>) {
        (self.fixture.input(), Avoid::new(&self.ring, &self.task))
    }
}

/// Finish a seed: fill the pool of `(user, kp_id)`, seed the D-S6 row with
/// two full windows, and open the app connection.
pub async fn finish_seed(
    db: &TestDb,
    fixture: Fixture,
    user: Uuid,
    kp_id: &str,
    depth: usize,
    seed: u64,
) -> Run {
    seed_pool(&db.admin, user, kp_id, depth, seed).await;
    let (ring, task) = full_windows();
    seed_web_state(&db.admin, user, &ring, &task).await;
    Run {
        user,
        ring,
        task,
        fixture,
        app: app_pool(db).await,
    }
}
