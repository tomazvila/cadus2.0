//! The serving pool, part 1: the D7 concurrency proof, the tenant bound of
//! the pop, the anti-repeat rule over the popped candidates, and the batch
//! insert with its A5 digest invariant.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_core::pool::{Ring, Source};
use cadus_store::pool::{NewInstance, POP_LIMIT, insert_batch_for_user, unclaimed_depth};
use cadus_store::test_support::TestDb;
use common::{
    KP, claim_fresh, new_instance, pool_digest, pop_fresh, pop_with, seed_pool_rows, seed_template,
};
use sqlx::PgPool;
use std::collections::BTreeSet;
use uuid::Uuid;

/// A second serving key, for the flag query.
const OTHER_KP: &str = "adding-two-digits/kp1";

// --------------------------------------------------------------------------
// (1) The acceptance check: two concurrent pops never return one row.
// --------------------------------------------------------------------------

/// D7: `FOR UPDATE SKIP LOCKED` never gives one row to two concurrent pops.
///
/// The pool holds exactly 200 rows and the two tasks take exactly 200 pops
/// between them, so a single repeated row makes the distinct count fall under
/// the serve count and the test fails.
///
/// The serve count itself is NOT 200. One open pop holds
/// `FOR UPDATE ... SKIP LOCKED LIMIT 8` on up to 8 unclaimed rows until it
/// commits, and it claims one of them, so a pop of the other task answers
/// `claimed: None` whenever the rows left over are all locked. That answer is
/// correct, and it happens only at the tail: it needs 8 or fewer unclaimed rows
/// left. The run therefore serves at least `200 - 8` rows, and every served row
/// is a different row (M5 review 2, finding V11). Every literal here is written
/// out; none is read from the code.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_concurrent_pops_never_return_the_same_row() {
    TestDb::with(|db| async move {
        let user = db.seed_user("pop-race@example.test").await;
        seed_pool_rows(&db.admin, user, KP, 200).await;

        let one = {
            let pool = db.app.clone();
            tokio::spawn(async move { pop_many(&pool, user, 100).await })
        };
        let two = {
            let pool = db.app.clone();
            tokio::spawn(async move { pop_many(&pool, user, 100).await })
        };

        let mut served: Vec<Uuid> = one.await.expect("task one finishes");
        served.extend(two.await.expect("task two finishes"));

        assert!(
            served.len() <= 200,
            "the pool holds 200 rows, so it cannot serve {} of them",
            served.len()
        );
        assert!(
            served.len() >= 200 - 8,
            "a pop answers no row only when 8 or fewer unclaimed rows are left, so the run \
             serves at least 192 rows; it served {}",
            served.len()
        );
        let distinct: BTreeSet<Uuid> = served.iter().copied().collect();
        assert_eq!(
            distinct.len(),
            served.len(),
            "every serve must be a different row; {} rows came back twice",
            served.len() - distinct.len()
        );

        let left = unclaimed_depth(&db.admin, user, KP).await.unwrap();
        assert_eq!(
            usize::try_from(left).unwrap() + served.len(),
            200,
            "every one of the 200 rows is either claimed or still in the pool"
        );
    })
    .await;
}

/// Pop `count` times and return the claimed row ids.
///
/// A pop that answers `claimed: None` adds no id, and the loop goes on. Two
/// conditions give that answer, and neither one is a failure of the pop: the
/// pool is empty, or a concurrent transaction holds every unclaimed row under
/// `FOR UPDATE ... SKIP LOCKED`. The caller states the property over the rows
/// the helper got (M5 review 2, finding V11).
async fn pop_many(pool: &PgPool, user: Uuid, count: usize) -> Vec<Uuid> {
    let mut ids = Vec::with_capacity(count);
    for _ in 0..count {
        let pop = pop_fresh(pool, user, KP).await;
        if let Some(claimed) = pop.claimed {
            ids.push(claimed.row.id);
        }
    }
    ids
}

/// D7 at the tail: a pop on a drained pool answers `claimed: None`.
///
/// The pool holds 8 rows and the run takes 9 pops. The first 8 pops claim the 8
/// rows, each one a different row, and the 9th finds nothing. `claimed: None` is
/// the answer of the pop, not a failure: a concurrent pop meets the same answer
/// whenever the other transaction holds every unclaimed row under
/// `FOR UPDATE ... SKIP LOCKED` (M5 review 2, finding V11).
#[tokio::test]
async fn a_pop_on_a_drained_pool_answers_no_row() {
    TestDb::with(|db| async move {
        let user = db.seed_user("pop-tail@example.test").await;
        seed_pool_rows(&db.admin, user, KP, 8).await;

        let served = pop_many(&db.app, user, 9).await;

        assert_eq!(
            served.len(),
            8,
            "8 rows give 8 serves, and the 9th pop finds no row"
        );
        let distinct: BTreeSet<Uuid> = served.iter().copied().collect();
        assert_eq!(distinct.len(), 8, "every serve must be a different row");
        let left = unclaimed_depth(&db.admin, user, KP).await.unwrap();
        assert_eq!(left, 0, "8 pops claim all 8 rows");
    })
    .await;
}

// --------------------------------------------------------------------------
// (2) The acceptance check: RLS keeps a pop inside its tenant.
// --------------------------------------------------------------------------

/// C3: a pop bound to tenant A never returns tenant B's rows.
///
/// Tenant B holds 8 rows and tenant A holds 2. A pop bound to A therefore reads
/// 2 candidates, not 10, and both of them are A's.
#[tokio::test]
async fn a_pop_bound_to_one_tenant_never_returns_another_tenants_rows() {
    TestDb::with(|db| async move {
        let alice = db.seed_user("alice@example.test").await;
        let bob = db.seed_user("bob@example.test").await;
        let bob_rows = seed_pool_rows(&db.admin, bob, KP, 8).await;
        let alice_rows = seed_pool_rows(&db.admin, alice, KP, 2).await;

        let first = claim_fresh(&db.app, alice, KP).await;
        assert_eq!(
            first.candidates, 2,
            "the pop reads alice's 2 rows and none of bob's 8"
        );
        assert!(
            alice_rows.contains(&first.row.id),
            "the claimed row must be one of alice's"
        );
        assert!(
            !bob_rows.contains(&first.row.id),
            "the claimed row must not be bob's"
        );

        let second = claim_fresh(&db.app, alice, KP).await;
        assert_eq!(second.candidates, 1);

        let third = pop_fresh(&db.app, alice, KP).await.claimed;
        assert!(third.is_none(), "alice's pool is empty after 2 pops");

        // Bob's rows are untouched: the tenant boundary held on the write side
        // too.
        assert_eq!(unclaimed_depth(&db.admin, bob, KP).await.unwrap(), 8);
    })
    .await;
}

// --------------------------------------------------------------------------
// (3) The pop width and the ring filter.
// --------------------------------------------------------------------------

/// The pop reads 8 rows, never more (specification section 5.5).
#[tokio::test]
async fn one_pop_reads_eight_candidates() {
    TestDb::with(|db| async move {
        let user = db.seed_user("width@example.test").await;
        seed_pool_rows(&db.admin, user, KP, 20).await;

        let claimed = claim_fresh(&db.app, user, KP).await;

        assert_eq!(claimed.candidates, 8, "the pop takes 8 rows");
        assert_eq!(POP_LIMIT, 8, "the pop width of the M4 decision is 8");
        assert_eq!(claimed.pick.index, 0, "an empty ring serves the oldest row");
        assert_eq!(claimed.pick.skipped, 0);
        assert!(!claimed.pick.exhausted);
        assert_eq!(claimed.row.instance_hash, "pool-instance-0000");
    })
    .await;
}

/// The ring blocks the three oldest rows, so the fourth is served.
#[tokio::test]
async fn the_pop_skips_every_ring_hit() {
    TestDb::with(|db| async move {
        let user = db.seed_user("ring@example.test").await;
        seed_pool_rows(&db.admin, user, KP, 8).await;

        let claimed = pop_with(
            &db.app,
            user,
            KP,
            &Ring::from_hashes([
                "pool-instance-0000",
                "pool-instance-0001",
                "pool-instance-0002",
            ]),
        )
        .await
        .claimed
        .expect("a row comes back");

        assert_eq!(claimed.row.instance_hash, "pool-instance-0003");
        assert_eq!(claimed.pick.index, 3);
        assert_eq!(claimed.pick.skipped, 3);
        assert!(!claimed.pick.exhausted);

        // The three blocked rows stayed unclaimed: 8 rows, one claimed.
        assert_eq!(unclaimed_depth(&db.admin, user, KP).await.unwrap(), 7);
    })
    .await;
}

/// Every candidate blocked: the LAST one is served and `exhausted` says so.
///
/// 1.0 states the reason at `problem_templates.py:388-392`: "A repeat is a far
/// smaller failure than no problem."
#[tokio::test]
async fn a_fully_blocked_pop_serves_the_last_candidate() {
    TestDb::with(|db| async move {
        let user = db.seed_user("blocked@example.test").await;
        seed_pool_rows(&db.admin, user, KP, 8).await;

        let blocked: Vec<String> = (0..8).map(pool_digest).collect();
        let claimed = pop_with(&db.app, user, KP, &Ring::from_hashes(blocked))
            .await
            .claimed
            .expect("a repeat is still a serve");

        assert_eq!(claimed.row.instance_hash, "pool-instance-0007");
        assert_eq!(claimed.pick.index, 7);
        assert_eq!(claimed.pick.skipped, 8);
        assert!(claimed.pick.exhausted, "every candidate was blocked");
    })
    .await;
}

/// An empty pool gives `None`. The serve path then falls back (A6).
#[tokio::test]
async fn an_empty_pool_gives_no_row() {
    TestDb::with(|db| async move {
        let user = db.seed_user("empty@example.test").await;
        let claimed = pop_fresh(&db.app, user, KP).await.claimed;
        assert!(claimed.is_none());
    })
    .await;
}

/// A claimed row never comes back, and the row stays as the served log (A5).
#[tokio::test]
async fn a_claimed_row_stays_and_never_pops_again() {
    TestDb::with(|db| async move {
        let user = db.seed_user("claim@example.test").await;
        seed_pool_rows(&db.admin, user, KP, 2).await;

        let first = claim_fresh(&db.app, user, KP).await;
        let second = claim_fresh(&db.app, user, KP).await;
        assert_ne!(first.row.id, second.row.id);

        let rows = sqlx::query_scalar!(
            r#"SELECT count(*) AS "n!" FROM serving_pool WHERE user_id = $1"#,
            user
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(rows, 2, "a claimed row is kept as the served-instance log");
        assert_eq!(unclaimed_depth(&db.admin, user, KP).await.unwrap(), 0);
    })
    .await;
}

// --------------------------------------------------------------------------
// (4) The batch insert.
// --------------------------------------------------------------------------

/// `ON CONFLICT DO NOTHING` on `(user_id, kp_id, instance_hash)` (A5).
///
/// The second call repeats three of the five digests, so it inserts 2.
#[tokio::test]
async fn a_repeated_digest_never_enters_the_pool_twice() {
    TestDb::with(|db| async move {
        let user = db.seed_user("batch@example.test").await;
        // `serving_pool.content_digest` references `content_store.digest`, so
        // the template row must exist before a row names it.
        seed_template(&db.admin, "deadbeefdeadbeef", KP, "approved", "ok").await;

        let first: Vec<NewInstance> = (0..5).map(new_instance).collect();
        let inserted = insert_batch_for_user(&db.admin, user, KP, &first)
            .await
            .unwrap();
        assert_eq!(inserted, 5);

        let second: Vec<NewInstance> = (2..7).map(new_instance).collect();
        let inserted = insert_batch_for_user(&db.admin, user, KP, &second)
            .await
            .unwrap();
        assert_eq!(inserted, 2, "digests 2, 3, and 4 are already in the pool");

        assert_eq!(unclaimed_depth(&db.admin, user, KP).await.unwrap(), 7);

        // The same digest under a different knowledge point is a different row:
        // the unique index carries kp_id.
        let inserted = insert_batch_for_user(&db.admin, user, OTHER_KP, &first)
            .await
            .unwrap();
        assert_eq!(inserted, 5);
    })
    .await;
}

/// The two documents survive the round trip through jsonb.
#[tokio::test]
async fn the_row_documents_round_trip() {
    TestDb::with(|db| async move {
        let user = db.seed_user("roundtrip@example.test").await;
        seed_template(&db.admin, "deadbeefdeadbeef", KP, "approved", "ok").await;
        let rows = vec![new_instance(3)];
        assert_eq!(
            insert_batch_for_user(&db.admin, user, KP, &rows)
                .await
                .unwrap(),
            1
        );

        let claimed = claim_fresh(&db.app, user, KP).await;

        assert_eq!(claimed.row.problem.v, 1);
        assert_eq!(claimed.row.problem.text, "Compute $3^2$.");
        assert_eq!(claimed.row.problem.seed, 7);
        assert_eq!(
            claimed.row.problem.bindings.get("a").map(String::as_str),
            Some("3")
        );
        assert_eq!(claimed.row.expected_answer.answer, "9");
        assert_eq!(claimed.row.source, Source::Template);
        assert_eq!(
            claimed.row.content_digest.as_deref(),
            Some("deadbeefdeadbeef")
        );
        assert_eq!(claimed.row.instance_hash, "pool-instance-0003");
    })
    .await;
}

/// An empty batch writes nothing and reports 0.
#[tokio::test]
async fn an_empty_batch_inserts_nothing() {
    TestDb::with(|db| async move {
        let user = db.seed_user("nobatch@example.test").await;
        assert_eq!(
            insert_batch_for_user(&db.admin, user, KP, &[])
                .await
                .unwrap(),
            0
        );
        assert_eq!(unclaimed_depth(&db.admin, user, KP).await.unwrap(), 0);
    })
    .await;
}
