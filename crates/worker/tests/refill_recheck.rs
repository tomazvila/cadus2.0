//! M4 U4 acceptance: every instance is checked again before the pool, and the
//! approval is read on every serve (C4, C6; M4 review rounds 1 and 2).
//!
//! Every expected value here is a LITERAL: a literal depth, a literal row count,
//! a literal statement, a literal seed. Nothing is read back from the code under
//! test.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::time::Instant;

use cadus_core::pool::{Avoid, PoolAnswer, Ring, TaskMemory};
use cadus_store::pool::{pop_with_ring, unclaimed_depth};
use cadus_store::test_support::TestDb;
use sqlx::PgPool;
use sqlx::types::Uuid;

use common::{
    Refill, SQUARES, SQUARES_BODY, SQUARES_DIGEST, pool_rows, seed_approved_template,
    seed_drained_pair, seed_fixed_user, seed_squares_pair, set_content_status, unclaimed,
};

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

/// The digest of the corrected template the operator approves second.
const SQUARES_FIXED_DIGEST: &str = "template-squares-2";

/// The corrected perfect-squares template: 12 statements, none of them shared
/// with `SQUARES_BODY`, so every instance is a new row in the pool (A5).
fn squares_fixed_body() -> String {
    SQUARES_BODY.replace("Compute ${a}^{{2}}$.", "Compute the square of ${a}$.")
}

/// Every unclaimed answer of one pair, in pop order.
async fn answers_of(admin: &PgPool, user_id: Uuid, kp_id: &str) -> Vec<String> {
    let rows: Vec<(String,)> = unclaimed(admin, user_id, kp_id, "expected_answer::text").await;
    rows.into_iter()
        .map(|(expected,)| {
            PoolAnswer::from_body(&expected)
                .expect("the document reads")
                .answer
        })
        .collect()
}

/// The fixed learner with the approved big-subtraction template and one
/// drained pair of it, and one pass of that pair at this depth.
async fn big_sub_pass(db: &TestDb, depth: i64) -> (Uuid, Refill, cadus_worker::RefillReport) {
    let user = seed_fixed_user(&db.admin).await;
    seed_approved_template(&db.admin, BIG_SUB_DIGEST, BIG_SUB, BIG_SUB_BODY).await;
    seed_drained_pair(&db.admin, user, BIG_SUB).await;
    let mut refill = Refill::new(depth, 32, BIG_SUB_BASE_SEED);
    let report = refill.pass(db, 1, Instant::now()).await;
    (user, refill, report)
}

/// The digest of the row the next serve takes.
async fn served_digest(db: &TestDb, user: Uuid, avoid: &Avoid<'_>) -> Option<String> {
    pop_with_ring(&db.admin, user, SQUARES, avoid)
        .await
        .expect("the pop runs")
        .claimed
        .and_then(|served| served.row.content_digest)
}

// --------------------------------------------------------------------------
// (5) Every instance is checked again before the pool
//     (review round 1, findings #1, #2, #15)
// --------------------------------------------------------------------------

/// C4: the instance the gate's sample never read does not reach the pool.
///
/// The document passes the gate, a human approves it (C6), and the refill draws
/// the corner the gate missed. The per-instance re-check refuses that one
/// instance, counts it, and writes the other twelve.
#[tokio::test]
async fn an_instance_the_re_check_refuses_never_reaches_the_pool() {
    TestDb::with(|db| async move {
        let (user, refill, report) = big_sub_pass(&db, 12).await;

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
            refill.state.refusal(BIG_SUB_DIGEST),
            None,
            "the DOCUMENT is accepted; one INSTANCE of it is not"
        );

        let rows = pool_rows(&db.admin, user, BIG_SUB).await;
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

/// C4, C6: a pair whose refusal rate is above the 10 percent limit is flagged.
///
/// The same corner, drawn into a batch of eight accepted instances: one
/// refusal of nine checked is 11 percent, above the limit, so the pass counts
/// the flag and the log names the knowledge point.
#[tokio::test]
async fn a_refusal_rate_above_the_limit_flags_the_pair() {
    TestDb::with(|db| async move {
        let (_, _, report) = big_sub_pass(&db, DEPTH_FLAGGED).await;

        assert_eq!(report.inserted, u64::try_from(DEPTH_FLAGGED).unwrap());
        assert_eq!(report.refused_instances, 1);
        assert_eq!(
            report.flagged_refusals, 1,
            "one refusal of a short batch is above the limit"
        );
    })
    .await;
}

/// The target depth at which the one refused corner of `BIG_SUB_BODY` is
/// above the 10 percent limit: the corner is the ninth draw of the batch, so
/// a batch of eight accepted instances checks nine and refuses one.
const DEPTH_FLAGGED: i64 = 8;

// --------------------------------------------------------------------------
// (7) The approval is read on every serve, and the refill retires the rest
//     (C6, M4 review 2, finding #4)
// --------------------------------------------------------------------------

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
        let user = seed_squares_pair(&db).await;
        let mut refill = Refill::new(12, 32, 0);

        // Pass 1: the approved template fills the pool.
        let first = refill.pass(&db, 1, Instant::now()).await;
        assert_eq!(first.inserted, 12);
        assert_eq!(first.from_template, 12);
        assert_eq!(first.retired_unapproved, 0, "every digest is approved");

        let ring = Ring::new();
        let task = TaskMemory::new();
        let avoid = Avoid::new(&ring, &task);
        assert_eq!(
            served_digest(&db, user, &avoid).await.as_deref(),
            Some("template-squares-1"),
            "the approved digest serves"
        );

        // The operator approves the corrected template and rejects the old one.
        seed_approved_template(
            &db.admin,
            SQUARES_FIXED_DIGEST,
            SQUARES,
            &squares_fixed_body(),
        )
        .await;
        set_content_status(&db.admin, SQUARES_DIGEST, "rejected").await;

        // The 11 unclaimed rows of the rejected digest are no longer served.
        assert_eq!(
            served_digest(&db, user, &avoid).await,
            None,
            "C6: no row of the rejected digest reaches a learner"
        );
        assert_eq!(
            unclaimed_depth(&db.admin, user, SQUARES).await.unwrap(),
            11,
            "the rows are still in the pool and no longer servable"
        );

        // Pass 2 retires them and fills the pair from the corrected template.
        let second = refill.pass(&db, 2, Instant::now()).await;
        assert_eq!(
            second.retired_unapproved, 11,
            "every unclaimed row of the rejected digest is retired"
        );
        assert_eq!(second.targets, 1, "the retire put the pair under the depth");
        assert_eq!(second.inserted, 12, "the corrected template fills the pool");
        assert_eq!(second.from_template, 12);
        assert_eq!(second.from_exemplar, 0);
        assert_eq!(second.failed, 0);

        let rows = pool_rows(&db.admin, user, SQUARES).await;
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
        let third = refill.pass(&db, 3, Instant::now()).await;
        assert_eq!(third.retired_unapproved, 0, "the retire is idempotent");
    })
    .await;
}
