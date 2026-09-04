//! M4 U4 acceptance: a pair that cannot fill leaves the target list, and so does
//! a pair whose source runs dry (D-O4, A6; M4 review rounds 1 and 2).
//!
//! Every expected value here is a LITERAL: a literal depth, a literal row count,
//! a literal backoff. Nothing is read back from the code under test.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::time::{Duration, Instant};

use cadus_store::pool::{operator_flags, operator_flags_with_exhausted, unclaimed_depth};
use cadus_store::test_support::TestDb;
use cadus_worker::{EMPTY_FILLS_BEFORE_BACKOFF, EXHAUSTED_BACKOFF, REFILL_BACKOFF};
use sqlx::PgPool;
use sqlx::types::Uuid;

use common::{
    ADDING, Refill, SQUARES, SQUARES_BODY, SQUARES_DIGEST, seed_approved_template,
    seed_drained_pair, seed_fixed_user, seed_squares_pair, seed_user_with_id,
};

/// The serving key of a pair the curriculum does not name and no template serves.
const UNFILLABLE: &str = "aaa-unknown/kp1";

/// The second learner of the exhausted-source test. A fixed id sorts the list.
const OTHER_USER_ID: &str = "22222222-2222-3333-4444-555555555555";

/// Claim every unclaimed row of one pair, as a run of serves does.
async fn claim_every_row(admin: &PgPool, user_id: Uuid, kp_id: &str) -> u64 {
    sqlx::query(
        "UPDATE serving_pool SET claimed_at = now()
          WHERE user_id = $1 AND kp_id = $2 AND claimed_at IS NULL",
    )
    .bind(user_id)
    .bind(kp_id)
    .execute(admin)
    .await
    .expect("the claim runs")
    .rows_affected()
}

/// Delete `count` unclaimed rows of one pair, as an M5 retention job does.
async fn delete_rows(admin: &PgPool, user_id: Uuid, kp_id: &str, count: i64) -> u64 {
    sqlx::query(
        "DELETE FROM serving_pool
          WHERE id IN (
            SELECT id FROM serving_pool
             WHERE user_id = $1 AND kp_id = $2 AND claimed_at IS NULL
             ORDER BY id
             LIMIT $3
          )",
    )
    .bind(user_id)
    .bind(kp_id)
    .bind(count)
    .execute(admin)
    .await
    .expect("the delete runs")
    .rows_affected()
}

// --------------------------------------------------------------------------
// (6) A pair that cannot fill leaves the target list
//     (review round 1, finding #3)
// --------------------------------------------------------------------------

/// The fixed learner with one drained pair nothing can fill, after the pass
/// that put the pair on backoff: the refill and the instant of that pass.
async fn starved_pair(db: &TestDb) -> (Refill, Instant) {
    let user = seed_fixed_user(&db.admin).await;
    seed_drained_pair(&db.admin, user, UNFILLABLE).await;
    let mut refill = Refill::new(12, 32, 0);
    let now = Instant::now();
    let first = refill.pass(db, 1, now).await;
    assert_eq!(first.without_source, 1);
    (refill, now)
}

/// D-O4: a starved pair does not spend the per-tick budget.
///
/// Three pairs sit at depth 0, and the budget is ONE pair per tick. The target
/// query orders by `(depth, user_id, kp_id)`, so the unfillable pair is the head
/// of the list on every tick. Without the backoff it takes the only slot forever
/// and neither fillable pair ever gains a row.
#[tokio::test]
async fn a_starved_pair_does_not_consume_the_tick_budget() {
    TestDb::with(|db| async move {
        let user = seed_squares_pair(&db).await;
        seed_drained_pair(&db.admin, user, UNFILLABLE).await;
        seed_drained_pair(&db.admin, user, ADDING).await;

        let mut refill = Refill::new(12, 1, 0);
        let now = Instant::now();

        // Tick 1: the head of the list is the pair that can never fill.
        let first = refill.pass(&db, 1, now).await;
        assert_eq!(first.targets, 1);
        assert_eq!(first.without_source, 1);
        assert_eq!(first.inserted, 0);
        assert_eq!(first.skipped_starved, 0, "nothing was on backoff yet");
        assert_eq!(
            refill.state.starved_len(),
            1,
            "the pair left the target list"
        );
        assert!(refill.state.is_starved(user, UNFILLABLE, now));

        // Tick 2: the slot goes to the first fillable pair.
        let second = refill.pass(&db, 2, now).await;
        assert_eq!(second.targets, 1);
        assert_eq!(second.skipped_starved, 1, "the starved pair is held out");
        assert_eq!(second.inserted, 3, "adding-two-digits has 3 exemplars");
        assert_eq!(second.from_exemplar, 3);

        // Tick 3: the slot goes to the second fillable pair.
        let third = refill.pass(&db, 3, now).await;
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
        let (mut refill, now) = starved_pair(&db).await;

        // One second before the period ends: the pair is still out.
        let inside = now + REFILL_BACKOFF - Duration::from_secs(1);
        let held = refill.pass(&db, 2, inside).await;
        assert_eq!(
            held.targets, 0,
            "the pair is not a target inside the period"
        );
        assert_eq!(held.skipped_starved, 1);

        // One second after: the pair is a target again.
        let outside = now + REFILL_BACKOFF + Duration::from_secs(1);
        let retried = refill.pass(&db, 3, outside).await;
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
        starved_pair(&db).await;

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
// (8) A pair whose source runs dry leaves the target list
//     (D-O4, A6, M4 review 2, finding #8)
// --------------------------------------------------------------------------

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

        let mut refill = Refill::new(24, 1, 0);
        let now = Instant::now();

        // Tick 1: the exemplar pair takes its 3 rows.
        let one = refill.pass(&db, 1, now).await;
        assert_eq!(one.targets, 1);
        assert_eq!(one.inserted, 3, "the knowledge point has 3 exemplars");
        assert_eq!(one.exhausted, 0);

        // The learner works through all 3. The pair is empty, and the 3
        // statements are locked in the A5 unique index for good.
        assert_eq!(claim_every_row(&db.admin, user, ADDING).await, 3);

        // Two templated pairs enter the list behind it, both at depth 0.
        seed_approved_template(&db.admin, SQUARES_DIGEST, SQUARES, SQUARES_BODY).await;
        seed_drained_pair(&db.admin, user, SQUARES).await;
        seed_drained_pair(&db.admin, other, SQUARES).await;

        // Tick 2: the exemplar pair is the head of the list and inserts nothing.
        // ONE empty fill is not enough: the pair keeps its slot.
        let two = refill.pass(&db, 2, now).await;
        assert_eq!(two.targets, 1);
        assert_eq!(two.inserted, 0, "every statement is in the pool already");
        assert_eq!(two.from_exemplar, 0);
        assert_eq!(two.without_source, 0, "the pair HAS a source");
        assert_eq!(two.failed, 0, "the fill and the insert both succeeded");
        assert_eq!(
            two.exhausted, 0,
            "one empty fill is not an exhausted source"
        );
        assert_eq!(refill.state.exhausted_len(), 0);
        assert_eq!(refill.state.starved_len(), 0);

        // Tick 3: the second empty fill in a row exhausts the pair.
        let three = refill.pass(&db, 3, now).await;
        assert_eq!(three.targets, 1);
        assert_eq!(three.inserted, 0);
        assert_eq!(three.exhausted, 1, "two empty fills in a row exhaust it");
        assert_eq!(refill.state.exhausted_len(), 1);
        assert!(refill.state.is_exhausted(user, ADDING));
        assert!(
            !refill.state.is_exhausted(user, SQUARES),
            "the template pair is not"
        );
        assert_eq!(
            refill.state.starved_len(),
            1,
            "the pair left the target list"
        );

        // Tick 4: the slot goes to the first templated pair.
        let four = refill.pass(&db, 4, now).await;
        assert_eq!(four.skipped_starved, 1, "the exhausted pair is held out");
        assert_eq!(four.targets, 1);
        assert_eq!(four.inserted, 12, "the perfect-squares template has 12");
        assert_eq!(four.from_template, 12);
        assert_eq!(four.exhausted, 0);

        // Tick 5: the slot goes to the second templated pair.
        let five = refill.pass(&db, 5, now).await;
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
        assert!(
            refill
                .state
                .is_starved(user, ADDING, now + Duration::from_secs(3599))
        );
        assert!(
            !refill
                .state
                .is_starved(user, ADDING, now + Duration::from_secs(3601))
        );

        // A6: the operator view names the knowledge point that ran dry.
        assert_eq!(refill.state.exhausted_kps(), vec!["adding-two-digits/kp1"]);
        let flags = operator_flags_with_exhausted(&db.admin, &refill.state.exhausted_kps())
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
        let user = seed_squares_pair(&db).await;
        let mut refill = Refill::new(24, 32, 0);
        let now = Instant::now();

        // Tick 1: the whole space of the template enters the pool.
        let one = refill.pass(&db, 1, now).await;
        assert_eq!(one.inserted, 12);
        assert_eq!(one.exhausted, 0);

        // Tick 2: the space is in the pool, so the fill writes nothing. Count 1.
        let two = refill.pass(&db, 2, now).await;
        assert_eq!(two.inserted, 0);
        assert_eq!(two.exhausted, 0);

        // A retention job deletes 6 rows, so 6 statements are free again.
        assert_eq!(delete_rows(&db.admin, user, SQUARES, 6).await, 6);

        // Tick 3: the pass writes 6 rows, which clears the count.
        let three = refill.pass(&db, 3, now).await;
        assert_eq!(three.inserted, 6);
        assert_eq!(three.exhausted, 0);
        assert_eq!(refill.state.exhausted_len(), 0);

        // Tick 4: the FIRST empty fill after that write. A cumulative count
        // would reach 2 here and exhaust the pair.
        let four = refill.pass(&db, 4, now).await;
        assert_eq!(four.inserted, 0);
        assert_eq!(four.exhausted, 0, "the count restarted at the write");
        assert_eq!(refill.state.exhausted_len(), 0);
        assert!(!refill.state.is_exhausted(user, SQUARES));

        // Tick 5: the second empty fill in a row exhausts it.
        let five = refill.pass(&db, 5, now).await;
        assert_eq!(five.inserted, 0);
        assert_eq!(five.exhausted, 1);
        assert!(refill.state.is_exhausted(user, SQUARES));
        assert_eq!(EMPTY_FILLS_BEFORE_BACKOFF, 2);
    })
    .await;
}
