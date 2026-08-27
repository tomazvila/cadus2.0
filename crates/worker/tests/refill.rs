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
use std::time::{Duration, Instant};

use cadus_core::curriculum::{Curriculum, load_curriculum};
use cadus_core::pool::{Avoid, PoolProblem, Ring, TaskMemory};
use cadus_store::pool::{
    operator_flags, operator_flags_with_exhausted, pop_with_ring, refill_targets, unclaimed_depth,
};
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_worker::{
    EMPTY_FILLS_BEFORE_BACKOFF, EXHAUSTED_BACKOFF, REFILL_BACKOFF, RefillConfig, RefillJob,
    RefillState, batch_seed, refill_once, refill_once_at,
};
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
                .claimed
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

// --------------------------------------------------------------------------
// (5) Every instance is checked again before the pool
//     (review round 1, findings #1, #2, #15)
// --------------------------------------------------------------------------

/// The serving key of the knowledge point whose template has one bad corner.
const BIG_SUB: &str = "big-subtraction/kp1";

/// The digest of its approved template row.
const BIG_SUB_DIGEST: &str = "template-big-sub-1";

/// A template the gate accepts and one of whose 10,000 instances answers `-1`.
///
/// The declared space is above `EXHAUSTIVE_SPACE_LIMIT`, so the gate reads a
/// 4,096-tuple sample from its constant seed and never meets `a = 100, b = 100`.
/// Both authored exemplars of the knowledge point answer a non-negative whole
/// number, so the envelope refuses that instance.
const BIG_SUB_BODY: &str = r#"{
    "v": 1,
    "topic_id": "big-subtraction",
    "answer_kind": "numeric",
    "statement": "Compute $9999 - {a} \\times {b}$.",
    "params": {"a": {"kind": "int", "low": 1, "high": 100},
               "b": {"kind": "int", "low": 1, "high": 100}},
    "answer_expr": "9999 - a*b",
    "hints": ["What is the product first?"],
    "samples": [{"params": {"a": 1, "b": 1}, "expected": "9998"},
                {"params": {"a": 100, "b": 1}, "expected": "9899"},
                {"params": {"a": 1, "b": 100}, "expected": "9899"}]
}"#;

/// The base seed whose nonce-1 batch for `USER_ID` draws the violating tuple.
///
/// The seed is a literal, found by a walk over the base seeds outside this test:
/// `batch_seed(145, USER_ID, "big-subtraction/kp1", 1)` is
/// 14,794,079,017,945,678,686, and the batch of that seed holds `a = 100,
/// b = 100`.
const BIG_SUB_BASE_SEED: u64 = 145;

/// The statement of the one instance the envelope refuses.
const BIG_SUB_REFUSED_TEXT: &str = "Compute $9999 - 100 \\times 100$.";

/// Every unclaimed answer of one pair, in pop order.
async fn answers_of(admin: &PgPool, user_id: Uuid, kp_id: &str) -> Vec<String> {
    let rows = sqlx::query!(
        r#"
        SELECT expected_answer::text AS "expected!"
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
            cadus_core::pool::PoolAnswer::from_body(&row.expected)
                .expect("the document reads")
                .answer
        })
        .collect()
}

/// C4: the instance the gate's sample never read does not reach the pool.
///
/// The document passes the gate, a human approves it (C6), and the refill draws
/// the corner the gate missed. The per-instance re-check refuses that one
/// instance, counts it, and writes the other twelve.
#[tokio::test]
async fn an_instance_the_re_check_refuses_never_reaches_the_pool() {
    TestDb::with(|db| async move {
        let user = seed_fixed_user(&db.admin).await;
        seed_approved(&db.admin, BIG_SUB_DIGEST, BIG_SUB, BIG_SUB_BODY).await;
        seed_drained_pair(&db.admin, user, BIG_SUB).await;

        let curriculum = arena();
        let job = RefillJob::new(&curriculum).with_config(RefillConfig {
            target_depth: 12,
            targets_per_tick: 32,
            base_seed: BIG_SUB_BASE_SEED,
        });
        let mut state = RefillState::new();
        let report = refill_once(&db_of(&db.admin), &job, &mut state, 1)
            .await
            .expect("the pass runs");

        assert_eq!(report.targets, 1);
        assert_eq!(report.inserted, 12, "the twelve good instances are written");
        assert_eq!(report.from_template, 12);
        assert_eq!(
            report.refused_instances, 1,
            "one instance broke the exemplar envelope"
        );
        assert_eq!(
            report.flagged_refusals, 0,
            "1 refusal of 13 checked is 7 percent, under the 10 percent limit"
        );
        assert_eq!(report.failed, 0);
        assert_eq!(
            state.refusal(BIG_SUB_DIGEST),
            None,
            "the DOCUMENT is accepted; one INSTANCE of it is not"
        );

        let rows = rows_of(&db.admin, user, BIG_SUB).await;
        assert_eq!(rows.len(), 12);
        for (_source, _digest, _seed, text) in &rows {
            assert_ne!(
                text, BIG_SUB_REFUSED_TEXT,
                "the refused statement must not be a pool row"
            );
        }

        let answers = answers_of(&db.admin, user, BIG_SUB).await;
        assert_eq!(answers.len(), 12);
        assert!(
            !answers.iter().any(|answer| answer == "-1"),
            "no pool row carries the answer the envelope refuses, the answers were {answers:?}"
        );
    })
    .await;
}

// --------------------------------------------------------------------------
// (6) A pair that cannot fill leaves the target list
//     (review round 1, finding #3)
// --------------------------------------------------------------------------

/// The serving key of a pair the curriculum does not name and no template serves.
const UNFILLABLE: &str = "aaa-unknown/kp1";

/// D-O4: a starved pair does not spend the per-tick budget.
///
/// Three pairs sit at depth 0, and the budget is ONE pair per tick. The target
/// query orders by `(depth, user_id, kp_id)`, so the unfillable pair is the head
/// of the list on every tick. Without the backoff it takes the only slot forever
/// and neither fillable pair ever gains a row.
#[tokio::test]
async fn a_starved_pair_does_not_consume_the_tick_budget() {
    TestDb::with(|db| async move {
        let user = seed_fixed_user(&db.admin).await;
        seed_approved(&db.admin, SQUARES_DIGEST, SQUARES, SQUARES_BODY).await;
        seed_drained_pair(&db.admin, user, UNFILLABLE).await;
        seed_drained_pair(&db.admin, user, ADDING).await;
        seed_drained_pair(&db.admin, user, SQUARES).await;

        let curriculum = arena();
        let job = RefillJob::new(&curriculum).with_config(RefillConfig {
            target_depth: 12,
            targets_per_tick: 1,
            base_seed: 0,
        });
        let mut state = RefillState::new();
        let now = Instant::now();

        // Tick 1: the head of the list is the pair that can never fill.
        let first = refill_once_at(&db_of(&db.admin), &job, &mut state, 1, now)
            .await
            .expect("the pass runs");
        assert_eq!(first.targets, 1);
        assert_eq!(first.without_source, 1);
        assert_eq!(first.inserted, 0);
        assert_eq!(first.skipped_starved, 0, "nothing was on backoff yet");
        assert_eq!(state.starved_len(), 1, "the pair left the target list");
        assert!(state.is_starved(user, UNFILLABLE, now));

        // Tick 2: the slot goes to the first fillable pair.
        let second = refill_once_at(&db_of(&db.admin), &job, &mut state, 2, now)
            .await
            .expect("the pass runs");
        assert_eq!(second.targets, 1);
        assert_eq!(second.skipped_starved, 1, "the starved pair is held out");
        assert_eq!(second.inserted, 3, "adding-two-digits has 3 exemplars");
        assert_eq!(second.from_exemplar, 3);

        // Tick 3: the slot goes to the second fillable pair.
        let third = refill_once_at(&db_of(&db.admin), &job, &mut state, 3, now)
            .await
            .expect("the pass runs");
        assert_eq!(third.targets, 1);
        assert_eq!(third.inserted, 12, "the perfect-squares template has 12");
        assert_eq!(third.from_template, 12);

        assert_eq!(unclaimed_depth(&db.admin, user, ADDING).await.unwrap(), 3);
        assert_eq!(unclaimed_depth(&db.admin, user, SQUARES).await.unwrap(), 12);
        assert_eq!(
            unclaimed_depth(&db.admin, user, UNFILLABLE).await.unwrap(),
            0,
            "the pair that cannot fill still holds no row"
        );
    })
    .await;
}

/// The backoff ends after 15 minutes, and the pair is tried again.
#[tokio::test]
async fn a_starved_pair_returns_to_the_list_after_the_backoff() {
    TestDb::with(|db| async move {
        let user = seed_fixed_user(&db.admin).await;
        seed_drained_pair(&db.admin, user, UNFILLABLE).await;

        let curriculum = arena();
        let job = RefillJob::new(&curriculum).with_config(RefillConfig {
            target_depth: 12,
            targets_per_tick: 32,
            base_seed: 0,
        });
        let mut state = RefillState::new();
        let now = Instant::now();

        let first = refill_once_at(&db_of(&db.admin), &job, &mut state, 1, now)
            .await
            .expect("the pass runs");
        assert_eq!(first.without_source, 1);

        // One second before the period ends: the pair is still out.
        let inside = now + REFILL_BACKOFF - Duration::from_secs(1);
        let held = refill_once_at(&db_of(&db.admin), &job, &mut state, 2, inside)
            .await
            .expect("the pass runs");
        assert_eq!(
            held.targets, 0,
            "the pair is not a target inside the period"
        );
        assert_eq!(held.skipped_starved, 1);

        // One second after: the pair is a target again.
        let outside = now + REFILL_BACKOFF + Duration::from_secs(1);
        let retried = refill_once_at(&db_of(&db.admin), &job, &mut state, 3, outside)
            .await
            .expect("the pass runs");
        assert_eq!(retried.targets, 1, "the period ended, so the pair is back");
        assert_eq!(retried.skipped_starved, 0);
        assert_eq!(retried.without_source, 1, "it still cannot fill");
        assert_eq!(REFILL_BACKOFF, Duration::from_secs(900));
    })
    .await;
}

/// A6: `operator_flags` names the starved pair's knowledge point.
///
/// The backoff is worker-local, so the operator view is what tells a human that
/// the knowledge point needs a template. The row exists for every knowledge point
/// that holds a pool row.
#[tokio::test]
async fn a_starved_knowledge_point_is_flagged_for_the_operator() {
    TestDb::with(|db| async move {
        let user = seed_fixed_user(&db.admin).await;
        seed_drained_pair(&db.admin, user, UNFILLABLE).await;

        let curriculum = arena();
        let job = RefillJob::new(&curriculum).with_config(RefillConfig {
            target_depth: 12,
            targets_per_tick: 32,
            base_seed: 0,
        });
        let mut state = RefillState::new();
        let report = refill_once(&db_of(&db.admin), &job, &mut state, 1)
            .await
            .expect("the pass runs");
        assert_eq!(report.without_source, 1);

        let flags = operator_flags(&db.admin).await.expect("the flags read");
        let flag = flags
            .iter()
            .find(|flag| flag.kp_id == UNFILLABLE)
            .expect("the starved knowledge point has an operator row");
        assert_eq!(flag.approved_templates, 0);
        assert_eq!(flag.pool_depth, 0);
        assert!(
            flag.needs_template,
            "A6: every serve of this knowledge point would be a fallback"
        );
    })
    .await;
}

// --------------------------------------------------------------------------
// (7) The approval is read on every serve, and the refill retires the rest
//     (C6, M4 review 2, finding #4)
// --------------------------------------------------------------------------

/// The digest of the corrected template the operator approves second.
const SQUARES_FIXED_DIGEST: &str = "template-squares-2";

/// The corrected perfect-squares template: 12 statements, none of them shared
/// with `SQUARES_BODY`, so every instance is a new row in the pool (A5).
const SQUARES_FIXED_BODY: &str = r#"{
    "v": 1,
    "topic_id": "perfect-squares",
    "answer_kind": "numeric",
    "statement": "Compute the square of ${a}$.",
    "params": {"a": {"kind": "int", "low": 1, "high": 12}},
    "answer_expr": "a**2",
    "solution_sketch": "${a} \\times {a}$ gives the answer.",
    "hints": ["What does squaring a number mean?"],
    "samples": [{"params": {"a": 1}, "expected": "1"},
                {"params": {"a": 12}, "expected": "144"}]
}"#;

/// Set the status of one `content_store` row, as an operator does (C6).
async fn set_status(admin: &PgPool, digest: &str, status: &str) {
    let changed = sqlx::query!(
        "UPDATE content_store SET status = $2 WHERE digest = $1",
        digest,
        status,
    )
    .execute(admin)
    .await
    .expect("the status update runs")
    .rows_affected();
    assert_eq!(changed, 1, "the operator changed exactly one content row");
}

/// C6: a revoked approval stops the serve, and the next pass retires the rows.
///
/// The operator reads a wrong answer in the refill log, authors a corrected
/// template, approves it, and rejects the old digest. Before this fix the pop
/// read `serving_pool` alone, so the 11 unclaimed rows of the rejected digest
/// kept being served with the wrong answer and the corrected template inserted
/// nothing over them (M4 review 2, finding #4).
#[tokio::test]
async fn a_revoked_approval_stops_the_serve_and_the_next_pass_retires_the_rows() {
    TestDb::with(|db| async move {
        let user = db.seed_user("revoked@example.test").await;
        seed_approved(&db.admin, SQUARES_DIGEST, SQUARES, SQUARES_BODY).await;
        seed_drained_pair(&db.admin, user, SQUARES).await;

        let curriculum = arena();
        let job = RefillJob::new(&curriculum).with_config(RefillConfig {
            target_depth: 12,
            targets_per_tick: 32,
            base_seed: 0,
        });
        let mut state = RefillState::new();

        // Pass 1: the approved template fills the pool.
        let first = refill_once(&db_of(&db.admin), &job, &mut state, 1)
            .await
            .expect("the pass runs");
        assert_eq!(first.inserted, 12);
        assert_eq!(first.from_template, 12);
        assert_eq!(first.retired_unapproved, 0, "every digest is approved");

        let ring = Ring::new();
        let task = TaskMemory::new();
        let avoid = Avoid::new(&ring, &task);
        let served = pop_with_ring(&db.admin, user, SQUARES, &avoid)
            .await
            .expect("the pop runs")
            .claimed
            .expect("the approved digest serves");
        assert_eq!(
            served.row.content_digest.as_deref(),
            Some("template-squares-1")
        );

        // The operator approves the corrected template and rejects the old one.
        seed_approved(&db.admin, SQUARES_FIXED_DIGEST, SQUARES, SQUARES_FIXED_BODY).await;
        set_status(&db.admin, SQUARES_DIGEST, "rejected").await;

        // The 11 unclaimed rows of the rejected digest are no longer served.
        let blocked = pop_with_ring(&db.admin, user, SQUARES, &avoid)
            .await
            .expect("the pop runs");
        assert_eq!(
            blocked.claimed, None,
            "C6: no row of the rejected digest reaches a learner"
        );
        assert_eq!(
            unclaimed_depth(&db.admin, user, SQUARES).await.unwrap(),
            11,
            "the rows are still in the pool and no longer servable"
        );

        // Pass 2 retires them and fills the pair from the corrected template.
        let second = refill_once(&db_of(&db.admin), &job, &mut state, 2)
            .await
            .expect("the pass runs");
        assert_eq!(
            second.retired_unapproved, 11,
            "every unclaimed row of the rejected digest is retired"
        );
        assert_eq!(second.targets, 1, "the retire put the pair under the depth");
        assert_eq!(second.inserted, 12, "the corrected template fills the pool");
        assert_eq!(second.from_template, 12);
        assert_eq!(second.from_exemplar, 0);
        assert_eq!(second.failed, 0);

        let rows = rows_of(&db.admin, user, SQUARES).await;
        assert_eq!(rows.len(), 12);
        for (source, digest, _seed, text) in &rows {
            assert_eq!(source, "template");
            assert_eq!(
                digest.as_deref(),
                Some("template-squares-2"),
                "every unclaimed row now names the corrected document (C6)"
            );
            assert!(
                text.starts_with("Compute the square of $"),
                "the pool carries the corrected statement, it read {text}"
            );
        }

        // The next serve takes a corrected row.
        let corrected = pop_with_ring(&db.admin, user, SQUARES, &avoid)
            .await
            .expect("the pop runs")
            .claimed
            .expect("the corrected digest serves");
        assert_eq!(
            corrected.row.content_digest.as_deref(),
            Some("template-squares-2")
        );
        assert_eq!(corrected.candidates, 8, "the pop still reads 8 candidates");

        // A third pass retires nothing more: a claimed row is out of the pool.
        let third = refill_once(&db_of(&db.admin), &job, &mut state, 3)
            .await
            .expect("the pass runs");
        assert_eq!(third.retired_unapproved, 0, "the retire is idempotent");
    })
    .await;
}

// --------------------------------------------------------------------------
// (8) A pair whose source runs dry leaves the target list
//     (D-O4, A6, M4 review 2, finding #8)
// --------------------------------------------------------------------------

/// The second learner of the exhausted-source test. A fixed id sorts the list.
const OTHER_USER_ID: &str = "22222222-2222-3333-4444-555555555555";

/// Insert a learner with a literal id, so the target order is a literal.
async fn seed_user_with_id(admin: &PgPool, id: &str, email: &str) -> Uuid {
    let id: Uuid = id.parse().expect("the literal id parses");
    sqlx::query!(
        "INSERT INTO users (id, email) VALUES ($1, $2::text::citext)",
        id,
        email,
    )
    .execute(admin)
    .await
    .expect("the learner inserts");
    id
}

/// Claim every unclaimed row of one pair, as a run of serves does.
async fn claim_every_row(admin: &PgPool, user_id: Uuid, kp_id: &str) -> u64 {
    sqlx::query!(
        r#"
        UPDATE serving_pool SET claimed_at = now()
        WHERE user_id = $1 AND kp_id = $2 AND claimed_at IS NULL
        "#,
        user_id,
        kp_id,
    )
    .execute(admin)
    .await
    .expect("the claim runs")
    .rows_affected()
}

/// Delete `count` unclaimed rows of one pair, as an M5 retention job does.
async fn delete_rows(admin: &PgPool, user_id: Uuid, kp_id: &str, count: i64) -> u64 {
    sqlx::query!(
        r#"
        DELETE FROM serving_pool
        WHERE id IN (
            SELECT id FROM serving_pool
            WHERE user_id = $1 AND kp_id = $2 AND claimed_at IS NULL
            ORDER BY id
            LIMIT $3
        )
        "#,
        user_id,
        kp_id,
        count,
    )
    .execute(admin)
    .await
    .expect("the delete runs")
    .rows_affected()
}

/// D-O4: an exhausted pair stops taking a tick slot, and the pairs behind fill.
///
/// The learner worked through all 3 exemplars of `adding-two-digits/kp1`, so the
/// pair sits at depth 0 and the unique index of A5 blocks every re-insert of
/// those 3 statements: the fill succeeds, inserts 0, and depth 0 is the head of
/// the depth-ordered target list forever. The budget is ONE pair per tick, so
/// without the rule the two templated pairs behind it never gain a row
/// (M4 review 2, finding #8).
#[tokio::test]
async fn an_exhausted_pair_stops_taking_a_tick_slot() {
    TestDb::with(|db| async move {
        let user = seed_fixed_user(&db.admin).await;
        let other = seed_user_with_id(&db.admin, OTHER_USER_ID, "second@example.test").await;
        seed_drained_pair(&db.admin, user, ADDING).await;

        let curriculum = arena();
        let job = RefillJob::new(&curriculum).with_config(RefillConfig {
            target_depth: 24,
            targets_per_tick: 1,
            base_seed: 0,
        });
        let mut state = RefillState::new();
        let now = Instant::now();

        // Tick 1: the exemplar pair takes its 3 rows.
        let one = refill_once_at(&db_of(&db.admin), &job, &mut state, 1, now)
            .await
            .expect("the pass runs");
        assert_eq!(one.targets, 1);
        assert_eq!(one.inserted, 3, "the knowledge point has 3 exemplars");
        assert_eq!(one.exhausted, 0);

        // The learner works through all 3. The pair is empty, and the 3
        // statements are locked in the A5 unique index for good.
        assert_eq!(claim_every_row(&db.admin, user, ADDING).await, 3);

        // Two templated pairs enter the list behind it, both at depth 0.
        seed_approved(&db.admin, SQUARES_DIGEST, SQUARES, SQUARES_BODY).await;
        seed_drained_pair(&db.admin, user, SQUARES).await;
        seed_drained_pair(&db.admin, other, SQUARES).await;

        // Tick 2: the exemplar pair is the head of the list and inserts nothing.
        // ONE empty fill is not enough: the pair keeps its slot.
        let two = refill_once_at(&db_of(&db.admin), &job, &mut state, 2, now)
            .await
            .expect("the pass runs");
        assert_eq!(two.targets, 1);
        assert_eq!(two.inserted, 0, "every statement is in the pool already");
        assert_eq!(two.from_exemplar, 0);
        assert_eq!(two.without_source, 0, "the pair HAS a source");
        assert_eq!(two.failed, 0, "the fill and the insert both succeeded");
        assert_eq!(
            two.exhausted, 0,
            "one empty fill is not an exhausted source"
        );
        assert_eq!(state.exhausted_len(), 0);
        assert_eq!(state.starved_len(), 0);

        // Tick 3: the second empty fill in a row exhausts the pair.
        let three = refill_once_at(&db_of(&db.admin), &job, &mut state, 3, now)
            .await
            .expect("the pass runs");
        assert_eq!(three.targets, 1);
        assert_eq!(three.inserted, 0);
        assert_eq!(three.exhausted, 1, "two empty fills in a row exhaust it");
        assert_eq!(state.exhausted_len(), 1);
        assert!(state.is_exhausted(user, ADDING));
        assert!(
            !state.is_exhausted(user, SQUARES),
            "the template pair is not"
        );
        assert_eq!(state.starved_len(), 1, "the pair left the target list");

        // Tick 4: the slot goes to the first templated pair.
        let four = refill_once_at(&db_of(&db.admin), &job, &mut state, 4, now)
            .await
            .expect("the pass runs");
        assert_eq!(four.skipped_starved, 1, "the exhausted pair is held out");
        assert_eq!(four.targets, 1);
        assert_eq!(four.inserted, 12, "the perfect-squares template has 12");
        assert_eq!(four.from_template, 12);
        assert_eq!(four.exhausted, 0);

        // Tick 5: the slot goes to the second templated pair.
        let five = refill_once_at(&db_of(&db.admin), &job, &mut state, 5, now)
            .await
            .expect("the pass runs");
        assert_eq!(five.skipped_starved, 1);
        assert_eq!(five.targets, 1);
        assert_eq!(five.inserted, 12);
        assert_eq!(five.from_template, 12);

        assert_eq!(unclaimed_depth(&db.admin, user, ADDING).await.unwrap(), 0);
        assert_eq!(unclaimed_depth(&db.admin, user, SQUARES).await.unwrap(), 12);
        assert_eq!(
            unclaimed_depth(&db.admin, other, SQUARES).await.unwrap(),
            12,
            "both pairs behind the exhausted one are filled"
        );

        // The backoff is one hour, not the fifteen minutes of a starved pair.
        assert_eq!(EXHAUSTED_BACKOFF, Duration::from_secs(3600));
        assert!(state.is_starved(user, ADDING, now + Duration::from_secs(3599)));
        assert!(!state.is_starved(user, ADDING, now + Duration::from_secs(3601)));

        // A6: the operator view names the knowledge point that ran dry.
        assert_eq!(state.exhausted_kps(), vec!["adding-two-digits/kp1"]);
        let flags = operator_flags_with_exhausted(&db.admin, &state.exhausted_kps())
            .await
            .expect("the flags read");
        let dry = flags
            .iter()
            .find(|flag| flag.kp_id == ADDING)
            .expect("the exhausted knowledge point has an operator row");
        assert!(
            dry.source_exhausted,
            "A6: the operator sees that this knowledge point needs more content"
        );
        let full = flags
            .iter()
            .find(|flag| flag.kp_id == SQUARES)
            .expect("the templated knowledge point has an operator row");
        assert!(
            !full.source_exhausted,
            "the knowledge point that still fills is not flagged"
        );
    })
    .await;
}

/// The empty-fill count is CONSECUTIVE: one fill that writes a row clears it.
///
/// A cumulative count would exhaust the pair on tick 4 below, one empty fill
/// after a pass that inserted 6 rows.
#[tokio::test]
async fn a_fill_that_writes_a_row_clears_the_empty_fill_count() {
    TestDb::with(|db| async move {
        let user = seed_fixed_user(&db.admin).await;
        seed_approved(&db.admin, SQUARES_DIGEST, SQUARES, SQUARES_BODY).await;
        seed_drained_pair(&db.admin, user, SQUARES).await;

        let curriculum = arena();
        let job = RefillJob::new(&curriculum).with_config(RefillConfig {
            target_depth: 24,
            targets_per_tick: 32,
            base_seed: 0,
        });
        let mut state = RefillState::new();
        let now = Instant::now();

        // Tick 1: the whole space of the template enters the pool.
        let one = refill_once_at(&db_of(&db.admin), &job, &mut state, 1, now)
            .await
            .expect("the pass runs");
        assert_eq!(one.inserted, 12);
        assert_eq!(one.exhausted, 0);

        // Tick 2: the space is in the pool, so the fill writes nothing. Count 1.
        let two = refill_once_at(&db_of(&db.admin), &job, &mut state, 2, now)
            .await
            .expect("the pass runs");
        assert_eq!(two.inserted, 0);
        assert_eq!(two.exhausted, 0);

        // A retention job deletes 6 rows, so 6 statements are free again.
        assert_eq!(delete_rows(&db.admin, user, SQUARES, 6).await, 6);

        // Tick 3: the pass writes 6 rows, which clears the count.
        let three = refill_once_at(&db_of(&db.admin), &job, &mut state, 3, now)
            .await
            .expect("the pass runs");
        assert_eq!(three.inserted, 6);
        assert_eq!(three.exhausted, 0);
        assert_eq!(state.exhausted_len(), 0);

        // Tick 4: the FIRST empty fill after that write. A cumulative count
        // would reach 2 here and exhaust the pair.
        let four = refill_once_at(&db_of(&db.admin), &job, &mut state, 4, now)
            .await
            .expect("the pass runs");
        assert_eq!(four.inserted, 0);
        assert_eq!(four.exhausted, 0, "the count restarted at the write");
        assert_eq!(state.exhausted_len(), 0);
        assert!(!state.is_exhausted(user, SQUARES));

        // Tick 5: the second empty fill in a row exhausts it.
        let five = refill_once_at(&db_of(&db.admin), &job, &mut state, 5, now)
            .await
            .expect("the pass runs");
        assert_eq!(five.inserted, 0);
        assert_eq!(five.exhausted, 1);
        assert!(state.is_exhausted(user, SQUARES));
        assert_eq!(EMPTY_FILLS_BEFORE_BACKOFF, 2);
    })
    .await;
}
