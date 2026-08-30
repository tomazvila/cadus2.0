//! M4 U4 acceptance: the serving-pool operations (D-O1, D-O4, D7, A6, C3).
//!
//! Every expected value here is a LITERAL: a literal row count, a literal pop
//! width, a literal digest, a literal flag. Nothing is read back from the code
//! under test.
//!
//! The literals come from three places:
//!
//! - `docs/reference/serving-1.0-spec.md` section 5.5 and section 7.2: the pop
//!   takes 8 candidates, skips the ring hits, and serves the last candidate on
//!   exhaustion;
//! - `migrations/0005_content.sql`: `UNIQUE (user_id, kp_id, instance_hash)` and
//!   the three `source` values;
//! - `migrations/0006_grants_rls.sql`: the tenant policy on `serving_pool`.
//!
//! Every test takes its own throwaway database, so a failure drops the database
//! instead of leaving it on the shared cluster.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::collections::BTreeSet;

use cadus_core::pool::{Avoid, PoolAnswer, PoolProblem, Ring, Source, TaskMemory};
use cadus_store::pool::{
    ApprovedTemplate, NewInstance, POP_LIMIT, approved_template, insert_batch_for_user,
    operator_flags, operator_flags_with_exhausted, pop_with_ring, refill_targets,
    refill_targets_skipping, retire_unapproved, unclaimed_depth,
};
use cadus_store::test_support::TestDb;
use sqlx::PgPool;
use sqlx::types::chrono::{DateTime, Utc};
use uuid::Uuid;

/// The serving key of the tests: `"<topic_id>/<kp_id>"`.
const KP: &str = "perfect-squares/kp1";

/// A second serving key, for the flag query.
const OTHER_KP: &str = "adding-two-digits/kp1";

/// The Unix instant of 2026-01-01T00:00:00Z.
const BASE_INSTANT: i64 = 1_767_225_600;

/// The instant the seeded row at `index` carries. Every later row adds one
/// second, so `ORDER BY created_at` is the seeding order and the pop is
/// deterministic.
fn at(index: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(BASE_INSTANT + index, 0).expect("the instant is inside the range")
}

/// The digest of the row at `index`: `pool-instance-0007`.
fn digest(index: usize) -> String {
    format!("pool-instance-{index:04}")
}

/// Seed one pool row with an explicit instant and digest.
async fn seed_row(
    admin: &PgPool,
    user_id: Uuid,
    kp_id: &str,
    index: usize,
    source: Source,
) -> Uuid {
    let problem = format!(r#"{{"v":1,"text":"Compute ${index}^2$.","seed":0}}"#);
    let expected = r#"{"v":1,"answer":"2"}"#;
    sqlx::query_scalar!(
        r#"
        INSERT INTO serving_pool
            (user_id, kp_id, source, problem, expected_answer, instance_hash, created_at)
        VALUES ($1, $2, $3, $4::text::jsonb, $5::text::jsonb, $6, $7)
        RETURNING id
        "#,
        user_id,
        kp_id,
        source.as_str(),
        problem,
        expected,
        digest(index),
        at(index as i64),
    )
    .fetch_one(admin)
    .await
    .expect("the seeded pool row inserts")
}

/// Seed `count` rows, oldest first.
async fn seed_rows(admin: &PgPool, user_id: Uuid, kp_id: &str, count: usize) -> Vec<Uuid> {
    let mut ids = Vec::with_capacity(count);
    for index in 0..count {
        ids.push(seed_row(admin, user_id, kp_id, index, Source::Template).await);
    }
    ids
}

/// One instance on its way into the pool, with a literal digest.
fn new_instance(index: usize) -> NewInstance {
    NewInstance {
        source: Source::Template,
        content_digest: Some("deadbeefdeadbeef".to_string()),
        problem: PoolProblem {
            v: 1,
            text: format!("Compute ${index}^2$."),
            bindings: [("a".to_string(), index.to_string())].into_iter().collect(),
            seed: 7,
        },
        expected_answer: PoolAnswer {
            v: 1,
            answer: (index * index).to_string(),
        },
        instance_hash: digest(index),
    }
}

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
        seed_rows(&db.admin, user, KP, 200).await;

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
    let ring = Ring::new();
    let task = TaskMemory::new();
    let avoid = Avoid::new(&ring, &task);
    let mut ids = Vec::with_capacity(count);
    for _ in 0..count {
        let pop = pop_with_ring(pool, user, KP, &avoid)
            .await
            .expect("the pop runs");
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
        seed_rows(&db.admin, user, KP, 8).await;

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
        let bob_rows = seed_rows(&db.admin, bob, KP, 8).await;
        let alice_rows = seed_rows(&db.admin, alice, KP, 2).await;

        let ring = Ring::new();
        let task = TaskMemory::new();
        let avoid = Avoid::new(&ring, &task);

        let first = pop_with_ring(&db.app, alice, KP, &avoid)
            .await
            .unwrap()
            .claimed
            .expect("alice has a row");
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

        let second = pop_with_ring(&db.app, alice, KP, &avoid)
            .await
            .unwrap()
            .claimed
            .expect("alice has a second row");
        assert_eq!(second.candidates, 1);

        let third = pop_with_ring(&db.app, alice, KP, &avoid)
            .await
            .unwrap()
            .claimed;
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
        seed_rows(&db.admin, user, KP, 20).await;

        let ring = Ring::new();
        let task = TaskMemory::new();
        let avoid = Avoid::new(&ring, &task);
        let claimed = pop_with_ring(&db.app, user, KP, &avoid)
            .await
            .unwrap()
            .claimed
            .expect("a row comes back");

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
        seed_rows(&db.admin, user, KP, 8).await;

        let ring = Ring::from_hashes([
            "pool-instance-0000",
            "pool-instance-0001",
            "pool-instance-0002",
        ]);
        let task = TaskMemory::new();
        let avoid = Avoid::new(&ring, &task);
        let claimed = pop_with_ring(&db.app, user, KP, &avoid)
            .await
            .unwrap()
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
        seed_rows(&db.admin, user, KP, 8).await;

        let blocked: Vec<String> = (0..8).map(digest).collect();
        let ring = Ring::from_hashes(blocked);
        let task = TaskMemory::new();
        let avoid = Avoid::new(&ring, &task);
        let claimed = pop_with_ring(&db.app, user, KP, &avoid)
            .await
            .unwrap()
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
        let ring = Ring::new();
        let task = TaskMemory::new();
        let avoid = Avoid::new(&ring, &task);
        let claimed = pop_with_ring(&db.app, user, KP, &avoid)
            .await
            .unwrap()
            .claimed;
        assert!(claimed.is_none());
    })
    .await;
}

/// A claimed row never comes back, and the row stays as the served log (A5).
#[tokio::test]
async fn a_claimed_row_stays_and_never_pops_again() {
    TestDb::with(|db| async move {
        let user = db.seed_user("claim@example.test").await;
        seed_rows(&db.admin, user, KP, 2).await;

        let ring = Ring::new();
        let task = TaskMemory::new();
        let avoid = Avoid::new(&ring, &task);
        let first = pop_with_ring(&db.app, user, KP, &avoid)
            .await
            .unwrap()
            .claimed
            .unwrap();
        let second = pop_with_ring(&db.app, user, KP, &avoid)
            .await
            .unwrap()
            .claimed
            .unwrap();
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

        let ring = Ring::new();
        let task = TaskMemory::new();
        let avoid = Avoid::new(&ring, &task);
        let claimed = pop_with_ring(&db.app, user, KP, &avoid)
            .await
            .unwrap()
            .claimed
            .unwrap();

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

// --------------------------------------------------------------------------
// (5) The approved-template read (C6).
// --------------------------------------------------------------------------

/// Insert one `content_store` row.
async fn seed_template(admin: &PgPool, digest: &str, kp_id: &str, status: &str, statement: &str) {
    let body = format!(r#"{{"v":1,"statement":"{statement}"}}"#);
    sqlx::query!(
        r#"
        INSERT INTO content_store (digest, kp_id, kind, body, status, approved_at)
        VALUES ($1, $2, 'template', $3::text::jsonb, $4,
                CASE WHEN $4 = 'approved' THEN now() ELSE NULL END)
        "#,
        digest,
        kp_id,
        body,
        status,
    )
    .execute(admin)
    .await
    .expect("the content row inserts");
}

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

        seed_rows(&db.admin, user, KP, 3).await;
        seed_row(&db.admin, user, OTHER_KP, 0, Source::Exemplar).await;
        seed_row(&db.admin, user, OTHER_KP, 1, Source::Exemplar).await;

        // Serve one row of each knowledge point.
        let ring = Ring::new();
        let task = TaskMemory::new();
        let avoid = Avoid::new(&ring, &task);
        pop_with_ring(&db.app, user, KP, &avoid)
            .await
            .unwrap()
            .claimed
            .unwrap();
        pop_with_ring(&db.app, user, OTHER_KP, &avoid)
            .await
            .unwrap()
            .claimed
            .unwrap();

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
        seed_rows(&db.admin, user, KP, 5).await;
        seed_row(&db.admin, user, OTHER_KP, 0, Source::Exemplar).await;

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
        seed_rows(&db.admin, user, KP, 2).await;

        let ring = Ring::new();
        let task = TaskMemory::new();
        let avoid = Avoid::new(&ring, &task);
        pop_with_ring(&db.app, user, KP, &avoid)
            .await
            .unwrap()
            .claimed
            .unwrap();
        pop_with_ring(&db.app, user, KP, &avoid)
            .await
            .unwrap()
            .claimed
            .unwrap();

        let targets = refill_targets(&db.admin, 8, 10).await.unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].depth, 0, "an empty pool is depth 0, not absent");
        assert_eq!(targets[0].kp_id, "perfect-squares/kp1");
    })
    .await;
}

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
            seed_row(&db.admin, user, KP, index, Source::Template).await;
        }
        assert_eq!(
            unclaimed_depth(&db.admin, user, KP).await.unwrap(),
            8,
            "the pair holds 8 unclaimed rows, 1 of them undecodable"
        );

        let ring = Ring::new();
        let task = TaskMemory::new();
        let avoid = Avoid::new(&ring, &task);

        // The first pop reads the poison row, retires it, and serves the oldest
        // good row behind it.
        let first = pop_with_ring(&db.app, user, KP, &avoid).await.unwrap();
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
            let popped = pop_with_ring(&db.app, user, KP, &avoid).await.unwrap();
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
        let last = pop_with_ring(&db.app, user, KP, &avoid).await.unwrap();
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

        let ring = Ring::new();
        let task = TaskMemory::new();
        let avoid = Avoid::new(&ring, &task);
        let popped = pop_with_ring(&db.app, user, KP, &avoid).await.unwrap();

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
            let id = seed_row(&db.admin, user, kp, 0, Source::Exemplar).await;
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
        let id = seed_row(&db.admin, other, "a-topic/kp1", 0, Source::Exemplar).await;
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
    let problem = format!(r#"{{"v":1,"text":"Compute ${index}^2$.","seed":0}}"#);
    let expected = r#"{"v":1,"answer":"2"}"#;
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
        digest(index),
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
        seed_row(&db.admin, user, KP, 3, Source::Exemplar).await;

        let ring = Ring::new();
        let task = TaskMemory::new();
        let avoid = Avoid::new(&ring, &task);
        let popped = pop_with_ring(&db.app, user, KP, &avoid).await.unwrap();
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
        let second = pop_with_ring(&db.app, user, KP, &avoid)
            .await
            .unwrap()
            .claimed
            .expect("the exemplar row is servable");
        assert_eq!(second.candidates, 1);
        assert_eq!(second.row.instance_hash, "pool-instance-0003");
        assert_eq!(
            second.row.content_digest, None,
            "A6: an exemplar row names no content document and is served"
        );

        // The two rows behind the unapproved digests are gone, so the pool has
        // nothing left to serve, and the unapproved rows are still unclaimed.
        let third = pop_with_ring(&db.app, user, KP, &avoid).await.unwrap();
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

        let ring = Ring::new();
        let task = TaskMemory::new();
        let avoid = Avoid::new(&ring, &task);

        // While the approval holds, the pool serves.
        let served = pop_with_ring(&db.app, user, KP, &avoid)
            .await
            .unwrap()
            .claimed
            .expect("the approved digest serves");
        assert_eq!(served.row.instance_hash, "pool-instance-0000");

        // The operator reads a wrong answer in the log and revokes the approval.
        set_status(&db.admin, "wrong-d", "rejected").await;

        let after = pop_with_ring(&db.app, user, KP, &avoid).await.unwrap();
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
        seed_row(&db.admin, user, KP, 2, Source::Exemplar).await;
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
        seed_row(&db.admin, user, KP, 0, Source::Template).await;
        seed_row(&db.admin, user, OTHER_KP, 1, Source::Exemplar).await;

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
