//! The fixture curriculum and the `serving_pool` rows of the refill tests.

use std::time::Instant;

use cadus_core::curriculum::{Curriculum, load_curriculum};
use cadus_core::pool::PoolProblem;
use cadus_store::Db;
use cadus_store::test_support::TestDb;
use cadus_testkit::fixtures::INSERT_APPROVED_TEMPLATE;
use cadus_worker::{
    RefillConfig, RefillJob, RefillReport, RefillState, WorkerError, refill_once_at,
};
use sqlx::types::Uuid;
use sqlx::{AssertSqlSafe, PgPool};

use super::handle;

/// The learner of the seed tests. A fixed id makes the batch seed a literal.
pub const USER_ID: &str = "11111111-2222-3333-4444-555555555555";

/// The serving key of the templated knowledge point of the fixture tree.
pub const SQUARES: &str = "perfect-squares/kp1";

/// The serving key of the fixture knowledge point that falls back to exemplars.
pub const ADDING: &str = "adding-two-digits/kp1";

/// The digest of the approved template row of the refill tests.
pub const SQUARES_DIGEST: &str = "template-squares-1";

/// The 1.0 perfect-squares template in the 2.0 document shape.
///
/// `a` runs 1..12, so the declared space is 12 tuples and the fill walks all of
/// them.
pub const SQUARES_BODY: &str = r#"{
    "v": 1,
    "topic_id": "perfect-squares",
    "answer_kind": "numeric",
    "statement": "Compute ${a}^{{2}}$.",
    "params": {"a": {"kind": "int", "low": 1, "high": 12}},
    "answer_expr": "a**2",
    "solution_sketch": "${a} \\times {a}$ gives the answer.",
    "hints": ["What does squaring a number mean?"],
    "samples": [{"params": {"a": 1}, "expected": "1"},
                {"params": {"a": 12}, "expected": "144"}]
}"#;

/// The curriculum tree the process tests point `CADUS_CURRICULUM` at.
pub fn fixture_curriculum() -> String {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pool")
        .to_string_lossy()
        .into_owned()
}

/// That tree, loaded.
pub fn arena() -> Curriculum {
    load_curriculum(std::path::Path::new(&fixture_curriculum()))
        .expect("the fixture curriculum loads")
        .0
}

/// A refill job over the curriculum with this depth, budget and base seed.
pub fn refill_job(
    curriculum: &Curriculum,
    target_depth: i64,
    targets_per_tick: i64,
    base_seed: u64,
) -> RefillJob<'_> {
    RefillJob::new(curriculum).with_config(RefillConfig {
        target_depth,
        targets_per_tick,
        base_seed,
    })
}

/// Run one refill pass at this nonce and instant.
pub async fn refill_pass(
    db: &TestDb,
    job: &RefillJob<'_>,
    state: &mut RefillState,
    nonce: u64,
    now: Instant,
) -> RefillReport {
    refill_once_at(&handle(db), job, state, nonce, now)
        .await
        .expect("the pass runs")
}

/// Insert a learner with a literal id, so the batch seed and the target order
/// are literals.
pub async fn seed_user_with_id(admin: &PgPool, id: &str, email: &str) -> Uuid {
    let id: Uuid = id.parse().expect("the literal id parses");
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2::text::citext)")
        .bind(id)
        .bind(email)
        .execute(admin)
        .await
        .expect("the learner inserts");
    id
}

/// Insert the fixed learner of the seed tests.
pub async fn seed_fixed_user(admin: &PgPool) -> Uuid {
    seed_user_with_id(admin, USER_ID, "refill@example.test").await
}

/// Insert one approved template document (C6).
pub async fn seed_approved_template(admin: &PgPool, digest: &str, kp_id: &str, body: &str) {
    sqlx::query(INSERT_APPROVED_TEMPLATE)
        .bind(digest)
        .bind(kp_id)
        .bind(body)
        .execute(admin)
        .await
        .expect("the content row inserts");
}

/// Put one claimed row into the pool, so the `(user, kp)` pair exists.
///
/// The refill target list reads `serving_pool`, so a pair reaches it after its
/// first row. This row is claimed already, so the unclaimed depth of the pair is
/// 0: the pool is empty in the sense the refill measures.
pub async fn seed_drained_pair(admin: &PgPool, user_id: Uuid, kp_id: &str) {
    sqlx::query(
        "INSERT INTO serving_pool
            (user_id, kp_id, source, problem, expected_answer, instance_hash, claimed_at)
        VALUES ($1, $2, 'exemplar', $3::text::jsonb, $4::text::jsonb, $5, now())",
    )
    .bind(user_id)
    .bind(kp_id)
    .bind(r#"{"v":1,"text":"Compute $1 + 1$.","seed":0}"#)
    .bind(r#"{"v":1,"answer":"2"}"#)
    .bind("drained-seed-row")
    .execute(admin)
    .await
    .expect("the drained row inserts");
}

/// Every unclaimed row of one pair, in pop order, read as these columns.
pub async fn unclaimed<T>(admin: &PgPool, user_id: Uuid, kp_id: &str, columns: &str) -> Vec<T>
where
    T: for<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> + Send + Unpin,
{
    sqlx::query_as::<_, T>(AssertSqlSafe(format!(
        "SELECT {columns} FROM serving_pool
          WHERE user_id = $1 AND kp_id = $2 AND claimed_at IS NULL
          ORDER BY created_at, id"
    )))
    .bind(user_id)
    .bind(kp_id)
    .fetch_all(admin)
    .await
    .expect("the rows read")
}

/// Every unclaimed row of one pair, as `(source, content_digest, seed, text)`.
pub async fn pool_rows(
    admin: &PgPool,
    user_id: Uuid,
    kp_id: &str,
) -> Vec<(String, Option<String>, u64, String)> {
    let rows: Vec<(String, Option<String>, String)> = unclaimed(
        admin,
        user_id,
        kp_id,
        "source, content_digest, problem::text",
    )
    .await;
    rows.into_iter()
        .map(|(source, digest, problem)| {
            let problem = PoolProblem::from_body(&problem).expect("the document reads");
            (source, digest, problem.seed, problem.text)
        })
        .collect()
}

/// The fixed learner with the approved perfect-squares template and one
/// drained pair of it: the setup of most template tests.
pub async fn seed_squares_pair(db: &TestDb) -> Uuid {
    let user = seed_fixed_user(&db.admin).await;
    seed_approved_template(&db.admin, SQUARES_DIGEST, SQUARES, SQUARES_BODY).await;
    seed_drained_pair(&db.admin, user, SQUARES).await;
    user
}

/// One refill job over the fixture curriculum, with its state.
///
/// The job borrows the curriculum, so the two live together here and every
/// pass builds the job again from the configuration.
pub struct Refill {
    pub curriculum: Curriculum,
    pub cfg: RefillConfig,
    pub state: RefillState,
}

impl Refill {
    /// A refill over the fixture tree with this depth, budget and base seed.
    pub fn new(target_depth: i64, targets_per_tick: i64, base_seed: u64) -> Self {
        Self::over(arena(), target_depth, targets_per_tick, base_seed)
    }

    /// A refill over this curriculum with this depth, budget and base seed.
    pub fn over(
        curriculum: Curriculum,
        target_depth: i64,
        targets_per_tick: i64,
        base_seed: u64,
    ) -> Self {
        Self {
            curriculum,
            cfg: RefillConfig {
                target_depth,
                targets_per_tick,
                base_seed,
            },
            state: RefillState::new(),
        }
    }

    /// The job of this refill.
    pub fn job(&self) -> RefillJob<'_> {
        RefillJob::new(&self.curriculum).with_config(self.cfg)
    }

    /// One pass at this nonce and instant, as the superuser.
    pub async fn pass(&mut self, db: &TestDb, nonce: u64, now: Instant) -> RefillReport {
        let job = RefillJob::new(&self.curriculum).with_config(self.cfg);
        refill_once_at(&handle(db), &job, &mut self.state, nonce, now)
            .await
            .expect("the pass runs")
    }

    /// One pass at this nonce, now, through this handle.
    pub async fn pass_through(
        &mut self,
        handle: &Db,
        nonce: u64,
    ) -> Result<RefillReport, WorkerError> {
        let job = RefillJob::new(&self.curriculum).with_config(self.cfg);
        refill_once_at(handle, &job, &mut self.state, nonce, Instant::now()).await
    }
}
