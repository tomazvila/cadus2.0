//! M4 U4 acceptance: the pool refill job (D-O4, A6, A7, C6).
//!
//! Every expected value here is a LITERAL: a literal depth, a literal row count,
//! a literal source, a literal seed. Nothing is read back from the code under
//! test.
//!
//! The seed literals come from an independent SplitMix64 and FNV-1a computed in
//! Python from the documented formula of `batch_seed`, not from a call to the
//! function itself.
//!
//! The template body is the 1.0 perfect-squares template in the 2.0 document
//! shape (`docs/reference/serving-1.0-spec.md` section 2.1): `a` runs 1..12, so
//! the template has exactly 12 distinct instances.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::collections::BTreeSet;
use std::path::PathBuf;

use cadus_core::curriculum::{Curriculum, load_curriculum};
use cadus_core::pool::{Avoid, PoolProblem, Ring, TaskMemory};
use cadus_store::pool::{pop_with_ring, refill_targets, unclaimed_depth};
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_worker::{RefillConfig, RefillJob, RefillState, batch_seed, refill_once};
use sqlx::PgPool;
use sqlx::types::Uuid;

/// The serving key of the templated knowledge point.
const SQUARES: &str = "perfect-squares/kp1";

/// The serving key of the knowledge point that falls back to exemplars (A6).
const ADDING: &str = "adding-two-digits/kp1";

/// The serving key of the knowledge point with nothing authored at all.
const BARE: &str = "bare/kp1";

/// The digest of the approved template row.
const SQUARES_DIGEST: &str = "template-squares-1";

/// The learner of the seed tests. A fixed id makes the batch seed a literal.
const USER_ID: &str = "11111111-2222-3333-4444-555555555555";

/// The 1.0 perfect-squares template in the 2.0 document shape.
///
/// `a` runs 1..12, so the declared space is 12 tuples and the fill walks all of
/// them.
const SQUARES_BODY: &str = r#"{
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

/// A document the gate refuses: `a` runs 1..3, which is under `MIN_SPACE_SIZE`.
const TOO_SMALL_BODY: &str = r#"{
    "v": 1,
    "topic_id": "perfect-squares",
    "answer_kind": "numeric",
    "statement": "Compute ${a}^{{2}}$.",
    "params": {"a": {"kind": "int", "low": 1, "high": 3}},
    "answer_expr": "a**2",
    "hints": ["What does squaring a number mean?"],
    "samples": [{"params": {"a": 1}, "expected": "1"},
                {"params": {"a": 3}, "expected": "9"}]
}"#;

/// The fixture curriculum of this file.
fn arena() -> Curriculum {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pool");
    let (curriculum, _findings) = load_curriculum(&root).expect("the fixture curriculum loads");
    curriculum
}

/// Wrap the admin pool in the `Db` the refill takes.
fn db_of(pool: &PgPool) -> Db {
    Db::new(pool.clone(), DEFAULT_CLIENT_TIMEOUT_MS)
}

/// Insert a learner with a fixed id, so the batch seed is a literal.
async fn seed_fixed_user(admin: &PgPool) -> Uuid {
    let id: Uuid = USER_ID.parse().expect("the literal id parses");
    sqlx::query!(
        "INSERT INTO users (id, email) VALUES ($1, $2::text::citext)",
        id,
        "refill@example.test",
    )
    .execute(admin)
    .await
    .expect("the learner inserts");
    id
}

/// Insert one approved template document (C6).
async fn seed_approved(admin: &PgPool, digest: &str, kp_id: &str, body: &str) {
    sqlx::query!(
        r#"
        INSERT INTO content_store (digest, kp_id, kind, body, status, approved_at)
        VALUES ($1, $2, 'template', $3::text::jsonb, 'approved', now())
        "#,
        digest,
        kp_id,
        body,
    )
    .execute(admin)
    .await
    .expect("the content row inserts");
}

/// Put one claimed row into the pool, so the `(user, kp)` pair exists.
///
/// The refill target list reads `serving_pool`, so a pair reaches it after its
/// first row. This row is claimed already, so the unclaimed depth of the pair is
/// 0: the pool is empty in the sense the refill measures.
async fn seed_drained_pair(admin: &PgPool, user_id: Uuid, kp_id: &str) {
    sqlx::query!(
        r#"
        INSERT INTO serving_pool
            (user_id, kp_id, source, problem, expected_answer, instance_hash, claimed_at)
        VALUES ($1, $2, 'exemplar', $3::text::jsonb, $4::text::jsonb, $5, now())
        "#,
        user_id,
        kp_id,
        r#"{"v":1,"text":"Compute $1 + 1$.","seed":0}"#,
        r#"{"v":1,"answer":"2"}"#,
        "drained-seed-row",
    )
    .execute(admin)
    .await
    .expect("the drained row inserts");
}

/// Every unclaimed row of one pair, as `(source, content_digest, seed, text)`.
async fn rows_of(
    admin: &PgPool,
    user_id: Uuid,
    kp_id: &str,
) -> Vec<(String, Option<String>, u64, String)> {
    let rows = sqlx::query!(
        r#"
        SELECT source AS "source!", content_digest, problem::text AS "problem!"
        FROM serving_pool
        WHERE user_id = $1 AND kp_id = $2 AND claimed_at IS NULL
        ORDER BY created_at, id
        "#,
        user_id,
        kp_id,
    )
    .fetch_all(admin)
    .await
    .expect("the rows read");

    rows.into_iter()
        .map(|row| {
            let problem = PoolProblem::from_body(&row.problem).expect("the document reads");
            (row.source, row.content_digest, problem.seed, problem.text)
        })
        .collect()
}

// --------------------------------------------------------------------------
// (1) The acceptance check: the worker fills an empty pool to depth N.
// --------------------------------------------------------------------------

/// D-O4: one refill pass fills a drained pair to a depth of 12.
///
/// The template has exactly 12 distinct instances, and the target depth is 12,
/// so the pass inserts 12 rows. Both numbers are literals here.
#[tokio::test]
async fn the_worker_fills_an_empty_pool_to_the_target_depth() {
    TestDb::with(|db| async move {
        let user = db.seed_user("fill@example.test").await;
        seed_approved(&db.admin, SQUARES_DIGEST, SQUARES, SQUARES_BODY).await;
        seed_drained_pair(&db.admin, user, SQUARES).await;

        let curriculum = arena();
        let job = RefillJob::new(&curriculum).with_config(RefillConfig {
            target_depth: 12,
            targets_per_tick: 32,
            base_seed: 0,
        });
        let mut state = RefillState::new();
        let report = refill_once(&db_of(&db.admin), &job, &mut state, 0)
            .await
            .expect("the pass runs");

        assert_eq!(report.targets, 1, "one pair is under the depth");
        assert_eq!(report.inserted, 12, "the pass fills the pool to 12");
        assert_eq!(report.from_template, 12, "every row came from the template");
        assert_eq!(report.from_exemplar, 0);
        assert_eq!(report.without_source, 0);
        assert_eq!(report.failed, 0);

        assert_eq!(
            unclaimed_depth(&db.admin, user, SQUARES).await.unwrap(),
            12,
            "the unclaimed depth is the target depth"
        );

        let rows = rows_of(&db.admin, user, SQUARES).await;
        assert_eq!(rows.len(), 12);
        for (source, digest, _seed, _text) in &rows {
            assert_eq!(source, "template", "A7: the row records its source");
            assert_eq!(
                digest.as_deref(),
                Some("template-squares-1"),
                "the row names the approved document it came from (C6)"
            );
        }

        // The 12 statements are the 12 instances of the template, all distinct.
        let texts: BTreeSet<&str> = rows.iter().map(|row| row.3.as_str()).collect();
        assert_eq!(texts.len(), 12, "every statement is a different problem");
        assert!(texts.contains("Compute $7^{2}$."));
        assert!(texts.contains("Compute $12^{2}$."));

        // The pair is no longer a refill target at this depth.
        let targets = refill_targets(&db.admin, 12, 32).await.unwrap();
        assert!(targets.is_empty(), "a full pair is not a target");
    })
    .await;
}

// --------------------------------------------------------------------------
// (2) The acceptance check: a knowledge point without an approved template.
// --------------------------------------------------------------------------

/// A6: no approved template, so the pass fills from the authored exemplars.
///
/// The fixture knowledge point has 3 exemplars, so a target depth of 12 gives 3
/// rows and no more. No model call runs, and no synchronous generation happens.
#[tokio::test]
async fn a_knowledge_point_without_a_template_fills_from_its_exemplars() {
    TestDb::with(|db| async move {
        let user = db.seed_user("fallback@example.test").await;
        seed_drained_pair(&db.admin, user, ADDING).await;

        let curriculum = arena();
        let job = RefillJob::new(&curriculum).with_config(RefillConfig {
            target_depth: 12,
            targets_per_tick: 32,
            base_seed: 0,
        });
        let mut state = RefillState::new();
        let report = refill_once(&db_of(&db.admin), &job, &mut state, 0)
            .await
            .expect("the pass runs");

        assert_eq!(report.targets, 1);
        assert_eq!(report.inserted, 3, "the knowledge point has 3 exemplars");
        assert_eq!(report.from_exemplar, 3);
        assert_eq!(report.from_template, 0);
        assert_eq!(report.without_source, 0);
        assert_eq!(report.failed, 0);

        let rows = rows_of(&db.admin, user, ADDING).await;
        assert_eq!(rows.len(), 3);
        for (source, digest, _seed, _text) in &rows {
            assert_eq!(source, "exemplar", "A6: the fallback records its source");
            assert_eq!(*digest, None, "an exemplar row names no content document");
        }
        // The three rows enter in one statement, so they share `created_at` and
        // their relative pop order is the order of their random ids. The batch
        // is therefore compared as a SET: what matters is that every authored
        // exemplar reached the pool exactly once (A6).
        let texts: BTreeSet<&str> = rows.iter().map(|row| row.3.as_str()).collect();
        let want: BTreeSet<&str> = [
            "Compute $21 + 34$.",
            "Compute $52 + 13$.",
            "Compute $41 + 27$.",
        ]
        .into_iter()
        .collect();
        assert_eq!(texts, want, "every authored exemplar reached the pool once");
    })
    .await;
}

/// A knowledge point with no template and no exemplar is counted, not hidden.
#[tokio::test]
async fn a_knowledge_point_with_no_source_is_counted() {
    TestDb::with(|db| async move {
        let user = db.seed_user("nosource@example.test").await;
        seed_drained_pair(&db.admin, user, BARE).await;
        seed_drained_pair(&db.admin, user, "unknown-topic/kp9").await;

        let curriculum = arena();
        let job = RefillJob::new(&curriculum).with_config(RefillConfig {
            target_depth: 12,
            targets_per_tick: 32,
            base_seed: 0,
        });
        let mut state = RefillState::new();
        let report = refill_once(&db_of(&db.admin), &job, &mut state, 0)
            .await
            .expect("the pass runs");

        assert_eq!(report.targets, 2);
        assert_eq!(report.inserted, 0);
        assert_eq!(
            report.without_source, 2,
            "one knowledge point has no exemplar, the other is not in the curriculum"
        );
        assert_eq!(report.failed, 0);
        assert_eq!(unclaimed_depth(&db.admin, user, BARE).await.unwrap(), 0);
    })
    .await;
}

// --------------------------------------------------------------------------
// (3) The gate runs again on the approved document.
// --------------------------------------------------------------------------

/// C6 and the 1.0 serve-time re-check: a document the gate refuses is not served.
///
/// The stored document is approved, and its declared space is 3 tuples, which is
/// under the distinct-problem floor of 12. The pass refuses it and falls back to
/// the exemplars of the knowledge point (A6), which give 2 rows.
#[tokio::test]
async fn an_approved_document_the_gate_refuses_falls_back_to_exemplars() {
    TestDb::with(|db| async move {
        let user = db.seed_user("gated@example.test").await;
        seed_approved(&db.admin, "template-too-small", SQUARES, TOO_SMALL_BODY).await;
        seed_drained_pair(&db.admin, user, SQUARES).await;

        let curriculum = arena();
        let job = RefillJob::new(&curriculum).with_config(RefillConfig {
            target_depth: 12,
            targets_per_tick: 32,
            base_seed: 0,
        });
        let mut state = RefillState::new();
        let report = refill_once(&db_of(&db.admin), &job, &mut state, 0)
            .await
            .expect("the pass runs");

        assert_eq!(report.inserted, 2, "the knowledge point has 2 exemplars");
        assert_eq!(report.from_exemplar, 2);
        assert_eq!(report.from_template, 0);

        let rows = rows_of(&db.admin, user, SQUARES).await;
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, "exemplar");

        // The refusal is remembered by digest, so the gate runs once.
        assert_eq!(state.len(), 1);
        let refusal = state
            .refusal("template-too-small")
            .expect("the digest carries its refusal");
        assert!(
            refusal.starts_with("[space-floor]"),
            "the refusal names the check that failed, it read {refusal}"
        );
        assert!(
            refusal.contains("at least 12 are needed"),
            "the refusal carries the 1.0 message, it read {refusal}"
        );
    })
    .await;
}

// --------------------------------------------------------------------------
// (4) The recorded seed (D-O4, section 3.2).
// --------------------------------------------------------------------------

/// The batch seed is a pure function of four inputs.
///
/// Each expected value below is an independent SplitMix64 and FNV-1a run over
/// the documented formula, computed outside this crate.
#[test]
fn the_batch_seed_is_the_documented_function() {
    let user: Uuid = USER_ID.parse().unwrap();
    let other: Uuid = "99999999-8888-7777-6666-555555555555".parse().unwrap();

    assert_eq!(
        batch_seed(0, user, "perfect-squares/kp1", 0),
        7_821_167_185_356_633_737
    );
    assert_eq!(
        batch_seed(0, user, "perfect-squares/kp1", 1),
        9_626_363_667_417_820_521,
        "a new nonce gives a new batch"
    );
    assert_eq!(
        batch_seed(7, user, "perfect-squares/kp1", 0),
        1_076_497_868_323_591_326,
        "a new base seed gives a new batch"
    );
    assert_eq!(
        batch_seed(0, user, "perfect-squares/kp2", 0),
        13_398_161_124_240_624_597,
        "a new knowledge point gives a new batch"
    );
    assert_eq!(
        batch_seed(0, other, "perfect-squares/kp1", 0),
        6_334_671_897_034_367_698,
        "a new learner gives a new batch"
    );
}

/// Every row of one batch records that batch's seed.
///
/// A reviewer reproduces the whole batch from the row alone.
#[tokio::test]
async fn every_row_records_the_seed_of_its_batch() {
    TestDb::with(|db| async move {
        let user = seed_fixed_user(&db.admin).await;
        seed_approved(&db.admin, SQUARES_DIGEST, SQUARES, SQUARES_BODY).await;
        seed_drained_pair(&db.admin, user, SQUARES).await;

        let curriculum = arena();
        let job = RefillJob::new(&curriculum).with_config(RefillConfig {
            target_depth: 8,
            targets_per_tick: 32,
            base_seed: 0,
        });
        let mut state = RefillState::new();
        let report = refill_once(&db_of(&db.admin), &job, &mut state, 0)
            .await
            .expect("the pass runs");
        assert_eq!(report.inserted, 8);

        let rows = rows_of(&db.admin, user, SQUARES).await;
        assert_eq!(rows.len(), 8);
        for (_source, _digest, seed, _text) in &rows {
            assert_eq!(
                *seed, 7_821_167_185_356_633_737,
                "the row carries the seed of its batch"
            );
        }
    })
    .await;
}

/// A repeat of one nonce inserts nothing: the seed decides the batch.
///
/// The pass runs with nonce 0, four rows are served, and the pass runs again with
/// nonce 0. The same seed gives the same candidate walk, so every drawn instance
/// is already in the pool and `ON CONFLICT DO NOTHING` writes 0 rows.
#[tokio::test]
async fn a_repeated_nonce_draws_the_batch_it_drew_before() {
    TestDb::with(|db| async move {
        let user = seed_fixed_user(&db.admin).await;
        seed_approved(&db.admin, SQUARES_DIGEST, SQUARES, SQUARES_BODY).await;
        seed_drained_pair(&db.admin, user, SQUARES).await;

        let curriculum = arena();
        let job = RefillJob::new(&curriculum).with_config(RefillConfig {
            target_depth: 8,
            targets_per_tick: 32,
            base_seed: 0,
        });
        let mut state = RefillState::new();
        let first = refill_once(&db_of(&db.admin), &job, &mut state, 0)
            .await
            .unwrap();
        assert_eq!(first.inserted, 8);

        // Serve four of the eight, so the pair is a target again at depth 4.
        let ring = Ring::new();
        let task = TaskMemory::new();
        let avoid = Avoid::new(&ring, &task);
        for _ in 0..4 {
            pop_with_ring(&db.admin, user, SQUARES, &avoid)
                .await
                .unwrap()
                .expect("the pool still holds a row");
        }
        assert_eq!(unclaimed_depth(&db.admin, user, SQUARES).await.unwrap(), 4);

        let second = refill_once(&db_of(&db.admin), &job, &mut state, 0)
            .await
            .unwrap();
        assert_eq!(second.targets, 1);
        assert_eq!(
            second.inserted, 0,
            "the same nonce draws the instances the pool already holds"
        );
        assert_eq!(unclaimed_depth(&db.admin, user, SQUARES).await.unwrap(), 4);

        // The gate ran once and the verdict is cached by digest.
        assert_eq!(state.len(), 1);
        assert_eq!(state.refusal(SQUARES_DIGEST), None);
    })
    .await;
}

/// A pass with no target does nothing and reports nothing.
#[tokio::test]
async fn a_pass_with_no_target_is_a_no_op() {
    TestDb::with(|db| async move {
        let curriculum = arena();
        let job = RefillJob::new(&curriculum);
        let mut state = RefillState::new();
        let report = refill_once(&db_of(&db.admin), &job, &mut state, 0)
            .await
            .expect("the pass runs");
        assert_eq!(report.targets, 0);
        assert_eq!(report.inserted, 0);
        assert!(state.is_empty());
    })
    .await;
}
