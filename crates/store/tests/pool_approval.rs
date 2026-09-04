//! The serving pool, part 4: the approval is read on every serve (C6), the
//! retire of the rows a revoked approval wrote, and the exhausted flags.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_core::pool::Source;
use cadus_store::pool::{
    operator_flags, operator_flags_with_exhausted, retire_unapproved, unclaimed_depth,
};
use cadus_store::test_support::TestDb;
use common::{
    KP, at, claim_fresh, pool_digest, pool_documents, pop_fresh, seed_pool_row, seed_template,
};
use sqlx::PgPool;
use std::collections::BTreeSet;
use uuid::Uuid;

/// A second serving key, for the flag query.
const OTHER_KP: &str = "adding-two-digits/kp1";

// --------------------------------------------------------------------------
// (8) The approval decides the serve, on every serve
//     (C6, M4 review 2, finding #4)
// --------------------------------------------------------------------------

/// Seed one pool row that names a `content_store` digest.
///
/// The row order is the seeding order: `at(index)` adds one second per index, so
/// the pop reads the rows in index order.
async fn seed_row_of_digest(
    admin: &PgPool,
    user_id: Uuid,
    kp_id: &str,
    index: usize,
    content_digest: &str,
) -> Uuid {
    let (problem, expected) = pool_documents(index);
    sqlx::query_scalar!(
        r#"
        INSERT INTO serving_pool
            (user_id, kp_id, source, content_digest, problem, expected_answer,
             instance_hash, created_at)
        VALUES ($1, $2, 'template', $3, $4::text::jsonb, $5::text::jsonb, $6, $7)
        RETURNING id
        "#,
        user_id,
        kp_id,
        content_digest,
        problem,
        expected,
        pool_digest(index),
        at(index as i64),
    )
    .fetch_one(admin)
    .await
    .expect("the seeded pool row inserts")
}

/// Set the status of one `content_store` row, as an operator does (C6).
async fn set_status(admin: &PgPool, content_digest: &str, status: &str) {
    let changed = sqlx::query!(
        "UPDATE content_store SET status = $2 WHERE digest = $1",
        content_digest,
        status,
    )
    .execute(admin)
    .await
    .expect("the status update runs")
    .rows_affected();
    assert_eq!(changed, 1, "the operator changed exactly one content row");
}

/// C6: the pop serves a template row only while its digest is `approved`.
///
/// The pool holds four rows in this age order: a `pending` digest, a `rejected`
/// digest, an `approved` digest, and a row with NO digest. Two of the four are
/// candidates, and the oldest of those two wins, so the pop reads 2 candidates
/// and serves `pool-instance-0002`. Every number here is a literal.
#[tokio::test]
async fn the_pop_serves_a_template_row_only_while_its_digest_is_approved() {
    TestDb::with(|db| async move {
        let user = db.seed_user("approval-pop@example.test").await;
        seed_template(&db.admin, "pending-d", KP, "pending", "wait").await;
        seed_template(&db.admin, "rejected-d", KP, "rejected", "no").await;
        seed_template(&db.admin, "approved-d", KP, "approved", "ok").await;

        seed_row_of_digest(&db.admin, user, KP, 0, "pending-d").await;
        seed_row_of_digest(&db.admin, user, KP, 1, "rejected-d").await;
        seed_row_of_digest(&db.admin, user, KP, 2, "approved-d").await;
        // Index 3 carries no digest: an exemplar rotation (A6).
        seed_pool_row(&db.admin, user, KP, 3, Source::Exemplar).await;

        let popped = pop_fresh(&db.app, user, KP).await;
        let claimed = popped.claimed.expect("one row is servable");

        assert_eq!(popped.undecodable, 0, "every row decodes");
        assert_eq!(
            claimed.candidates, 2,
            "the pending and the rejected digest are not candidates (C6)"
        );
        assert_eq!(
            claimed.row.instance_hash, "pool-instance-0002",
            "the approved digest is the oldest servable row"
        );
        assert_eq!(claimed.row.content_digest.as_deref(), Some("approved-d"));

        // The second pop takes the row with no digest.
        let second = claim_fresh(&db.app, user, KP).await;
        assert_eq!(second.candidates, 1);
        assert_eq!(second.row.instance_hash, "pool-instance-0003");
        assert_eq!(
            second.row.content_digest, None,
            "A6: an exemplar row names no content document and is served"
        );

        // The two rows behind the unapproved digests are gone, so the pool has
        // nothing left to serve, and the unapproved rows are still unclaimed.
        let third = pop_fresh(&db.app, user, KP).await;
        assert_eq!(
            third.claimed, None,
            "an unapproved digest is never served (C6)"
        );
        assert_eq!(
            unclaimed_depth(&db.admin, user, KP).await.unwrap(),
            2,
            "the pop leaves the unapproved rows in place; the refill retires them"
        );
    })
    .await;
}

/// C6: a revoked approval stops the serve of the rows the digest already wrote.
///
/// The old pop read `serving_pool` alone, so the 3 unclaimed rows of a revoked
/// digest kept being served with the answer the operator rejected (M4 review 2,
/// finding #4).
#[tokio::test]
async fn a_revoked_approval_stops_the_serve_of_the_rows_it_wrote() {
    TestDb::with(|db| async move {
        let user = db.seed_user("revoke@example.test").await;
        seed_template(&db.admin, "wrong-d", KP, "approved", "ok").await;
        seed_row_of_digest(&db.admin, user, KP, 0, "wrong-d").await;
        seed_row_of_digest(&db.admin, user, KP, 1, "wrong-d").await;
        seed_row_of_digest(&db.admin, user, KP, 2, "wrong-d").await;

        // While the approval holds, the pool serves.
        let served = claim_fresh(&db.app, user, KP).await;
        assert_eq!(served.row.instance_hash, "pool-instance-0000");

        // The operator reads a wrong answer in the log and revokes the approval.
        set_status(&db.admin, "wrong-d", "rejected").await;

        let after = pop_fresh(&db.app, user, KP).await;
        assert_eq!(
            after.claimed, None,
            "C6: no row of the revoked digest is served"
        );
        assert_eq!(
            unclaimed_depth(&db.admin, user, KP).await.unwrap(),
            2,
            "the two rows are still in the pool, and no longer servable"
        );

        // The refill retires them, so the pair falls under its target depth.
        let retired = retire_unapproved(&db.admin).await.unwrap();
        assert_eq!(retired.len(), 2, "the two unclaimed rows are retired");
        for row in &retired {
            assert_eq!(row.user_id, user);
            assert_eq!(row.kp_id, "perfect-squares/kp1");
            assert_eq!(row.content_digest, "wrong-d");
            assert_eq!(row.status, "rejected", "the log names the new status");
        }
        let hashes: BTreeSet<&str> = retired
            .iter()
            .map(|row| row.content_digest.as_str())
            .collect();
        assert_eq!(hashes.len(), 1);
        assert_eq!(
            unclaimed_depth(&db.admin, user, KP).await.unwrap(),
            0,
            "the pair is empty, so the refill fills it from an approved source"
        );

        // A second call retires nothing: a claimed row leaves the unclaimed set.
        let again = retire_unapproved(&db.admin).await.unwrap();
        assert!(again.is_empty(), "the retire is idempotent");
    })
    .await;
}

/// The negative fixture of the retire: an approved row and an exemplar row stay.
///
/// A retire that claimed every row of a pair would empty a healthy pool on every
/// tick, so this test pins what the rule must NOT take.
#[tokio::test]
async fn the_retire_leaves_the_approved_and_the_exemplar_rows_alone() {
    TestDb::with(|db| async move {
        let user = db.seed_user("retire-negative@example.test").await;
        seed_template(&db.admin, "approved-d", KP, "approved", "ok").await;
        seed_template(&db.admin, "pending-d", OTHER_KP, "pending", "wait").await;

        seed_row_of_digest(&db.admin, user, KP, 0, "approved-d").await;
        seed_row_of_digest(&db.admin, user, KP, 1, "approved-d").await;
        seed_pool_row(&db.admin, user, KP, 2, Source::Exemplar).await;
        seed_row_of_digest(&db.admin, user, OTHER_KP, 3, "pending-d").await;

        let retired = retire_unapproved(&db.admin).await.unwrap();
        assert_eq!(retired.len(), 1, "only the pending digest is retired");
        assert_eq!(retired[0].kp_id, "adding-two-digits/kp1");
        assert_eq!(retired[0].content_digest, "pending-d");
        assert_eq!(retired[0].status, "pending");

        assert_eq!(
            unclaimed_depth(&db.admin, user, KP).await.unwrap(),
            3,
            "the two approved rows and the exemplar row are untouched"
        );
        assert_eq!(unclaimed_depth(&db.admin, user, OTHER_KP).await.unwrap(), 0);
    })
    .await;
}

/// A6: `operator_flags_with_exhausted` carries the flag the refill state holds.
///
/// The plain `operator_flags` reads `false` for every row, because the backoff
/// map lives in the worker process and no table carries it.
#[tokio::test]
async fn the_operator_flags_carry_the_exhausted_knowledge_points() {
    TestDb::with(|db| async move {
        let user = db.seed_user("exhausted-flag@example.test").await;
        seed_pool_row(&db.admin, user, KP, 0, Source::Template).await;
        seed_pool_row(&db.admin, user, OTHER_KP, 1, Source::Exemplar).await;

        let plain = operator_flags(&db.admin).await.unwrap();
        assert_eq!(plain.len(), 2);
        assert!(
            plain.iter().all(|flag| !flag.source_exhausted),
            "the plain query carries no worker state"
        );

        let exhausted = vec!["adding-two-digits/kp1".to_string()];
        let flags = operator_flags_with_exhausted(&db.admin, &exhausted)
            .await
            .unwrap();
        assert_eq!(flags.len(), 2);
        assert_eq!(flags[0].kp_id, "adding-two-digits/kp1");
        assert!(
            flags[0].source_exhausted,
            "A6: the source of this knowledge point ran dry"
        );
        assert_eq!(flags[1].kp_id, "perfect-squares/kp1");
        assert!(
            !flags[1].source_exhausted,
            "the knowledge point that is not in the list is not flagged"
        );
    })
    .await;
}
