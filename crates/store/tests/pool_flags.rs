//! The serving pool, part 2: the C6 approved-template read, the A6 operator
//! flags, and the D-O4 refill targets.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_core::pool::Source;
use cadus_store::pool::{ApprovedTemplate, approved_template, operator_flags, refill_targets};
use cadus_store::test_support::TestDb;
use common::{KP, claim_fresh, seed_pool_row, seed_pool_rows, seed_template};

/// A second serving key, for the flag query.
const OTHER_KP: &str = "adding-two-digits/kp1";

// --------------------------------------------------------------------------
// (5) The approved-template read (C6).
// --------------------------------------------------------------------------

/// C6: a pending body is never read, and an approved body is.
#[tokio::test]
async fn only_an_approved_template_is_read() {
    TestDb::with(|db| async move {
        seed_template(&db.admin, "pending-1", KP, "pending", "pending body").await;
        seed_template(&db.admin, "rejected-1", KP, "rejected", "rejected body").await;

        let none = approved_template(&db.admin, KP).await.unwrap();
        assert_eq!(none, None, "a pending template is never served (C6)");

        seed_template(&db.admin, "approved-1", KP, "approved", "approved body").await;
        let found = approved_template(&db.admin, KP).await.unwrap();
        assert_eq!(
            found,
            Some(ApprovedTemplate {
                digest: "approved-1".to_string(),
                body: r#"{"v": 1, "statement": "approved body"}"#.to_string(),
                generation_context: cadus_store::pool::GenerationContext {
                    curriculum_digest: String::new(),
                    review_engine_digest: String::new(),
                },
            })
        );
    })
    .await;
}

// --------------------------------------------------------------------------
// (6) The acceptance check: the A6 operator flags.
// --------------------------------------------------------------------------

/// A6: a knowledge point without an approved template is flagged.
///
/// `KP` has one approved template and one pending one. `OTHER_KP` has a pending
/// template only, and its last serve fell back to an exemplar. The flag row of
/// `OTHER_KP` therefore reads `approved_templates = 0`, `needs_template = true`,
/// and a `last_exemplar_at` instant.
#[tokio::test]
async fn a_knowledge_point_without_an_approved_template_is_flagged() {
    TestDb::with(|db| async move {
        let user = db.seed_user("flags@example.test").await;

        seed_template(&db.admin, "approved-1", KP, "approved", "ok").await;
        seed_template(&db.admin, "pending-1", KP, "pending", "wait").await;
        seed_template(&db.admin, "pending-2", OTHER_KP, "pending", "wait").await;

        seed_pool_rows(&db.admin, user, KP, 3).await;
        seed_pool_row(&db.admin, user, OTHER_KP, 0, Source::Exemplar).await;
        seed_pool_row(&db.admin, user, OTHER_KP, 1, Source::Exemplar).await;

        // Serve one row of each knowledge point.
        claim_fresh(&db.app, user, KP).await;
        claim_fresh(&db.app, user, OTHER_KP).await;

        let flags = operator_flags(&db.admin).await.unwrap();
        assert_eq!(flags.len(), 2, "two knowledge points have rows");

        let names: Vec<&str> = flags.iter().map(|flag| flag.kp_id.as_str()).collect();
        assert_eq!(names, [OTHER_KP, KP], "the rows come back in key order");

        let fallback = &flags[0];
        assert_eq!(fallback.kp_id, "adding-two-digits/kp1");
        assert_eq!(fallback.approved_templates, 0);
        assert!(
            fallback.needs_template,
            "A6: this knowledge point is flagged"
        );
        assert_eq!(fallback.pool_depth, 1);
        assert_eq!(fallback.last_source, Some(Source::Exemplar));
        assert!(
            fallback.last_exemplar_at.is_some(),
            "the fallback instant is recorded"
        );

        let served = &flags[1];
        assert_eq!(served.kp_id, "perfect-squares/kp1");
        assert_eq!(
            served.approved_templates, 1,
            "the pending row does not count"
        );
        assert!(!served.needs_template);
        assert_eq!(served.pool_depth, 2);
        assert_eq!(served.last_source, Some(Source::Template));
        assert_eq!(served.last_exemplar_at, None, "this KP never fell back");
    })
    .await;
}

/// A knowledge point with a template document and no pool row still appears.
#[tokio::test]
async fn a_knowledge_point_with_no_pool_row_still_appears_in_the_flags() {
    TestDb::with(|db| async move {
        seed_template(&db.admin, "pending-1", OTHER_KP, "pending", "wait").await;
        let flags = operator_flags(&db.admin).await.unwrap();
        assert_eq!(flags.len(), 1);
        assert_eq!(flags[0].kp_id, "adding-two-digits/kp1");
        assert_eq!(flags[0].approved_templates, 0);
        assert_eq!(flags[0].pool_depth, 0);
        assert_eq!(flags[0].last_source, None);
        assert!(flags[0].needs_template);
    })
    .await;
}

// --------------------------------------------------------------------------
// (7) The refill targets (D-O4).
// --------------------------------------------------------------------------

/// The target list names the pairs under the depth, shallowest first.
#[tokio::test]
async fn the_refill_targets_name_the_pairs_below_the_depth() {
    TestDb::with(|db| async move {
        let user = db.seed_user("targets@example.test").await;
        seed_pool_rows(&db.admin, user, KP, 5).await;
        seed_pool_row(&db.admin, user, OTHER_KP, 0, Source::Exemplar).await;

        let targets = refill_targets(&db.admin, 4, 10).await.unwrap();
        assert_eq!(
            targets.len(),
            1,
            "only the shallow pair is under a depth of 4"
        );
        assert_eq!(targets[0].kp_id, "adding-two-digits/kp1");
        assert_eq!(targets[0].depth, 1);
        assert_eq!(targets[0].user_id, user);

        let targets = refill_targets(&db.admin, 6, 10).await.unwrap();
        assert_eq!(targets.len(), 2, "both pairs are under a depth of 6");
        assert_eq!(targets[0].depth, 1);
        assert_eq!(targets[1].depth, 5);

        let targets = refill_targets(&db.admin, 6, 1).await.unwrap();
        assert_eq!(targets.len(), 1, "the limit bounds one pass");
        assert_eq!(targets[0].depth, 1);
    })
    .await;
}

/// A pair whose rows are all claimed has depth 0 and is a refill target.
#[tokio::test]
async fn a_fully_claimed_pair_is_a_refill_target() {
    TestDb::with(|db| async move {
        let user = db.seed_user("drained@example.test").await;
        seed_pool_rows(&db.admin, user, KP, 2).await;

        claim_fresh(&db.app, user, KP).await;
        claim_fresh(&db.app, user, KP).await;

        let targets = refill_targets(&db.admin, 8, 10).await.unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].depth, 0, "an empty pool is depth 0, not absent");
        assert_eq!(targets[0].kp_id, "perfect-squares/kp1");
    })
    .await;
}
