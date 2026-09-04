//! M4 U4 acceptance: the pool refill job fills the pool (D-O4, A6, A7, C6).
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
//! the template has exactly 12 distinct instances. `refill_recheck.rs` and
//! `refill_backoff.rs` hold the checks of the per-instance re-check and of the
//! two backoffs.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeSet;
use std::time::Instant;

use cadus_core::pool::{Avoid, Ring, TaskMemory};
use cadus_store::pool::{pop_with_ring, refill_targets, unclaimed_depth};
use cadus_store::test_support::TestDb;
use cadus_worker::batch_seed;
use sqlx::types::Uuid;

use common::{
    ADDING, Refill, SQUARES, SQUARES_DIGEST, USER_ID, pool_rows, seed_approved_template,
    seed_drained_pair, seed_squares_pair,
};

/// The serving key of the knowledge point with nothing authored at all.
const BARE: &str = "bare/kp1";

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

/// The statements of every unclaimed row of one pair, as a set.
async fn texts_of(db: &TestDb, user: Uuid, kp_id: &str) -> BTreeSet<String> {
    pool_rows(&db.admin, user, kp_id)
        .await
        .into_iter()
        .map(|row| row.3)
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
        let user = seed_squares_pair(&db).await;

        let report = Refill::new(12, 32, 0).pass(&db, 0, Instant::now()).await;

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

        let rows = pool_rows(&db.admin, user, SQUARES).await;
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
        let texts = texts_of(&db, user, SQUARES).await;
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

        let report = Refill::new(12, 32, 0).pass(&db, 0, Instant::now()).await;

        assert_eq!(report.targets, 1);
        assert_eq!(report.inserted, 3, "the knowledge point has 3 exemplars");
        assert_eq!(report.from_exemplar, 3);
        assert_eq!(report.from_template, 0);
        assert_eq!(report.without_source, 0);
        assert_eq!(report.failed, 0);

        let rows = pool_rows(&db.admin, user, ADDING).await;
        assert_eq!(rows.len(), 3);
        for (source, digest, _seed, _text) in &rows {
            assert_eq!(source, "exemplar", "A6: the fallback records its source");
            assert_eq!(*digest, None, "an exemplar row names no content document");
        }
        // The three rows enter in one statement, so they share `created_at` and
        // their relative pop order is the order of their random ids. The batch
        // is therefore compared as a SET: what matters is that every authored
        // exemplar reached the pool exactly once (A6).
        let want: BTreeSet<String> = [
            "Compute $21 + 34$.",
            "Compute $52 + 13$.",
            "Compute $41 + 27$.",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        assert_eq!(
            texts_of(&db, user, ADDING).await,
            want,
            "every authored exemplar reached the pool once"
        );
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

        let report = no_source_pass(&db).await;

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

/// A key of a topic the curriculum names with a knowledge point it does not,
/// and a key that is no serving key at all, have no source either.
#[tokio::test]
async fn a_key_the_curriculum_cannot_resolve_has_no_source() {
    TestDb::with(|db| async move {
        let user = db.seed_user("unresolved@example.test").await;
        seed_drained_pair(&db.admin, user, "perfect-squares/kp9").await;
        seed_drained_pair(&db.admin, user, "no-slash").await;

        let report = no_source_pass(&db).await;

        assert_eq!(report.targets, 2);
        assert_eq!(report.inserted, 0);
        assert_eq!(report.without_source, 2);
        assert_eq!(report.failed, 0);
    })
    .await;
}

/// One pass at depth 12 over the fixture tree, with a fresh state.
async fn no_source_pass(db: &TestDb) -> cadus_worker::RefillReport {
    Refill::new(12, 32, 0).pass(db, 0, Instant::now()).await
}

// --------------------------------------------------------------------------
// (3) The gate runs again on the approved document.
// --------------------------------------------------------------------------

/// C6 and the 1.0 serve-time re-check: a document the gate refuses is not served.
///
/// The stored document is approved, and its declared space is 3 tuples, which is
/// under the distinct-problem floor of 12. The pass refuses it and falls back to
/// the exemplars of the knowledge point (A6), which give 2 rows. A second pass
/// reads the cached refusal and falls back again, with no new row: the two
/// exemplars are in the pool.
#[tokio::test]
async fn an_approved_document_the_gate_refuses_falls_back_to_exemplars() {
    TestDb::with(|db| async move {
        let user = db.seed_user("gated@example.test").await;
        seed_approved_template(&db.admin, "template-too-small", SQUARES, TOO_SMALL_BODY).await;
        seed_drained_pair(&db.admin, user, SQUARES).await;

        let mut refill = Refill::new(12, 32, 0);
        let report = refill.pass(&db, 0, Instant::now()).await;

        assert_eq!(report.inserted, 2, "the knowledge point has 2 exemplars");
        assert_eq!(report.from_exemplar, 2);
        assert_eq!(report.from_template, 0);

        let rows = pool_rows(&db.admin, user, SQUARES).await;
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, "exemplar");

        // The refusal is remembered by digest, so the gate runs once.
        assert_eq!(refill.state.len(), 1);
        let refusal = refill
            .state
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

        let second = refill.pass(&db, 1, Instant::now()).await;
        assert_eq!(second.targets, 1);
        assert_eq!(second.inserted, 0, "the two exemplars are in the pool");
        assert_eq!(
            second.from_template, 0,
            "the cached refusal keeps the template out"
        );
        assert_eq!(refill.state.len(), 1, "the gate did not run again");
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
        let user = seed_squares_pair(&db).await;

        let report = Refill::new(8, 32, 0).pass(&db, 0, Instant::now()).await;
        assert_eq!(report.inserted, 8);

        let rows = pool_rows(&db.admin, user, SQUARES).await;
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
        let user = seed_squares_pair(&db).await;

        let mut refill = Refill::new(8, 32, 0);
        let first = refill.pass(&db, 0, Instant::now()).await;
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

        let second = refill.pass(&db, 0, Instant::now()).await;
        assert_eq!(second.targets, 1);
        assert_eq!(
            second.inserted, 0,
            "the same nonce draws the instances the pool already holds"
        );
        assert_eq!(unclaimed_depth(&db.admin, user, SQUARES).await.unwrap(), 4);

        // The gate ran once and the verdict is cached by digest.
        assert_eq!(refill.state.len(), 1);
        assert_eq!(refill.state.refusal(SQUARES_DIGEST), None);
    })
    .await;
}

/// A pass with no target does nothing and reports nothing.
#[tokio::test]
async fn a_pass_with_no_target_is_a_no_op() {
    TestDb::with(|db| async move {
        let mut refill = Refill::new(24, 32, 0);
        let report = refill.pass(&db, 0, Instant::now()).await;
        assert_eq!(report.targets, 0);
        assert_eq!(report.inserted, 0);
        assert!(refill.state.is_empty());
    })
    .await;
}
