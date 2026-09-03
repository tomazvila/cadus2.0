//! The serving pool, part 3: a row this build cannot decode is retired, not
//! fatal, and a pair on backoff leaves the target list.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_core::pool::Source;
use cadus_store::pool::{refill_targets, refill_targets_skipping, unclaimed_depth};
use cadus_store::test_support::TestDb;
use common::{KP, at, pop_fresh, seed_pool_row};
use sqlx::PgPool;
use sqlx::types::chrono::{DateTime, Utc};
use uuid::Uuid;

// --------------------------------------------------------------------------
// (8) One undecodable row is retired, and the pop keeps serving
//     (review round 1, finding #19)
// --------------------------------------------------------------------------

/// Seed one row whose `problem` document this build refuses.
///
/// `{"v":2,...}` is the shape a later `POOL_ROW_VERSION` writes. The reader
/// refuses it, exactly as `crates/core/tests/pool_row.rs` pins.
async fn seed_poison_row(admin: &PgPool, user_id: Uuid, kp_id: &str, index: usize) -> Uuid {
    sqlx::query_scalar!(
        r#"
        INSERT INTO serving_pool
            (user_id, kp_id, source, problem, expected_answer, instance_hash, created_at)
        VALUES ($1, $2, 'template', $3::text::jsonb, $4::text::jsonb, $5, $6)
        RETURNING id
        "#,
        user_id,
        kp_id,
        r#"{"v":2,"text":"Compute $1 + 1$.","seed":0}"#,
        r#"{"v":2,"answer":"2"}"#,
        format!("pool-poison-{index:04}"),
        at(index as i64),
    )
    .fetch_one(admin)
    .await
    .expect("the poison row inserts")
}

/// D-O1: 7 servable rows are served when 1 of 8 does not decode.
///
/// The poison row is the OLDEST, so it is candidate 1 of every pop. The old code
/// returned its decode error and threw the seven good rows away with it, so the
/// pair was dead until an operator deleted the row by hand.
#[tokio::test]
async fn one_undecodable_row_is_retired_and_the_other_seven_are_served() {
    TestDb::with(|db| async move {
        let user = db.seed_user("poison@example.test").await;
        let poison = seed_poison_row(&db.admin, user, KP, 0).await;
        for index in 1..8 {
            seed_pool_row(&db.admin, user, KP, index, Source::Template).await;
        }
        assert_eq!(
            unclaimed_depth(&db.admin, user, KP).await.unwrap(),
            8,
            "the pair holds 8 unclaimed rows, 1 of them undecodable"
        );

        // The first pop reads the poison row, retires it, and serves the oldest
        // good row behind it.
        let first = pop_fresh(&db.app, user, KP).await;
        assert_eq!(first.undecodable, 1, "the pop counted the row it retired");
        let claimed = first.claimed.expect("a good row is served");
        assert_eq!(claimed.row.instance_hash, "pool-instance-0001");
        assert_eq!(
            claimed.candidates, 7,
            "the rule read the 7 rows that decoded"
        );

        // Six more serves, one per remaining good row.
        let mut served = vec![claimed.row.instance_hash];
        for _ in 0..6 {
            let popped = pop_fresh(&db.app, user, KP).await;
            assert_eq!(
                popped.undecodable, 0,
                "the poison row is claimed, so no later pop reads it"
            );
            served.push(
                popped
                    .claimed
                    .expect("the pool still holds a servable row")
                    .row
                    .instance_hash,
            );
        }
        served.sort();
        assert_eq!(
            served,
            [
                "pool-instance-0001",
                "pool-instance-0002",
                "pool-instance-0003",
                "pool-instance-0004",
                "pool-instance-0005",
                "pool-instance-0006",
                "pool-instance-0007",
            ],
            "all 7 servable rows were served"
        );

        // The pool is empty now, and the poison row is claimed, not unclaimed.
        let last = pop_fresh(&db.app, user, KP).await;
        assert!(last.claimed.is_none(), "8 rows, 7 served and 1 retired");
        assert_eq!(last.undecodable, 0);
        assert_eq!(unclaimed_depth(&db.admin, user, KP).await.unwrap(), 0);

        let claimed_at: Option<DateTime<Utc>> =
            sqlx::query_scalar!("SELECT claimed_at FROM serving_pool WHERE id = $1", poison)
                .fetch_one(&db.admin)
                .await
                .unwrap();
        assert!(
            claimed_at.is_some(),
            "the undecodable row is claimed, so it leaves the unclaimed set for good"
        );
    })
    .await;
}

/// A pair whose every row is undecodable gives no row, and every one is retired.
#[tokio::test]
async fn a_pair_of_undecodable_rows_gives_no_row_and_retires_them_all() {
    TestDb::with(|db| async move {
        let user = db.seed_user("allpoison@example.test").await;
        for index in 0..3 {
            seed_poison_row(&db.admin, user, KP, index).await;
        }

        let popped = pop_fresh(&db.app, user, KP).await;

        assert!(
            popped.claimed.is_none(),
            "no row decoded, so none is served"
        );
        assert_eq!(popped.undecodable, 3, "all three rows were retired");
        assert_eq!(
            unclaimed_depth(&db.admin, user, KP).await.unwrap(),
            0,
            "a retired row never counts toward the refill depth again"
        );
    })
    .await;
}

// --------------------------------------------------------------------------
// (9) The refill target list skips the pairs the worker names
//     (review round 1, finding #3)
// --------------------------------------------------------------------------

/// D-O4: a skipped pair costs no slot of the per-tick budget.
///
/// Three pairs sit at depth 0. With a limit of 1 the query returns the first one
/// in `(depth, user_id, kp_id)` order. Named as a skip, that pair leaves the list
/// and the slot goes to the next pair.
#[tokio::test]
async fn a_skipped_pair_leaves_the_target_list_and_frees_its_slot() {
    TestDb::with(|db| async move {
        let user = db.seed_user("skip@example.test").await;
        // Three drained pairs: every row claimed, so every depth is 0.
        for kp in ["a-topic/kp1", "b-topic/kp1", "c-topic/kp1"] {
            let id = seed_pool_row(&db.admin, user, kp, 0, Source::Exemplar).await;
            sqlx::query!(
                "UPDATE serving_pool SET claimed_at = now() WHERE id = $1",
                id
            )
            .execute(&db.admin)
            .await
            .unwrap();
        }

        let all = refill_targets(&db.admin, 12, 10).await.unwrap();
        let keys: Vec<&str> = all.iter().map(|target| target.kp_id.as_str()).collect();
        assert_eq!(keys, ["a-topic/kp1", "b-topic/kp1", "c-topic/kp1"]);

        let one = refill_targets(&db.admin, 12, 1).await.unwrap();
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].kp_id, "a-topic/kp1", "the head of the list");

        let skip = vec![(user, "a-topic/kp1".to_string())];
        let next = refill_targets_skipping(&db.admin, 12, 1, &skip)
            .await
            .unwrap();
        assert_eq!(next.len(), 1, "the skipped pair did not spend the one slot");
        assert_eq!(next[0].kp_id, "b-topic/kp1");

        let two_skipped = vec![
            (user, "a-topic/kp1".to_string()),
            (user, "b-topic/kp1".to_string()),
        ];
        let last = refill_targets_skipping(&db.admin, 12, 2, &two_skipped)
            .await
            .unwrap();
        assert_eq!(last.len(), 1, "only one pair is left to fill");
        assert_eq!(last[0].kp_id, "c-topic/kp1");

        // The skip is keyed by the PAIR, not by the knowledge point: another
        // learner of the same knowledge point stays in the list.
        let other = db.seed_user("other@example.test").await;
        let id = seed_pool_row(&db.admin, other, "a-topic/kp1", 0, Source::Exemplar).await;
        sqlx::query!(
            "UPDATE serving_pool SET claimed_at = now() WHERE id = $1",
            id
        )
        .execute(&db.admin)
        .await
        .unwrap();
        let both = refill_targets_skipping(&db.admin, 12, 10, &skip)
            .await
            .unwrap();
        let pairs: Vec<(Uuid, &str)> = both
            .iter()
            .map(|target| (target.user_id, target.kp_id.as_str()))
            .collect();
        assert!(
            pairs.contains(&(other, "a-topic/kp1")),
            "the other learner's pair is not skipped"
        );
        assert!(
            !pairs.contains(&(user, "a-topic/kp1")),
            "the named pair is skipped"
        );
    })
    .await;
}
