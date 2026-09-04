//! M5 U7, the store half: the D-O3 content read and the A6 exemplar rotation.
//!
//! Requirements: A6 (the fallback never runs dry and stays visible to an
//! operator), C6 (only an approved document is served), D5, D-O3, L4, L5.
//!
//! Spec: `docs/reference/web-service-1.0-spec.md` section 4.3 (the D-O3 read)
//! and `docs/reference/serving-1.0-spec.md` section 6 (the A6 fallback).
//!
//! Every expected value is a LITERAL.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_core::pool::{Avoid, Ring, Source, TaskMemory};
use cadus_store::begin_tenant;
use cadus_store::content::{KIND_HINT_LADDER, KIND_TEACH, approved_document};
use cadus_store::pool::{PoolRow, reclaim_exemplar_tx};
use cadus_store::test_support::TestDb;
use common::fault::closed_pool;
use common::{KP, at, seed_doc};
use serde_json::json;
use sqlx::PgPool;
use sqlx::types::chrono::{DateTime, Utc};
use uuid::Uuid;

/// One A6 rotation of `user` with `ring` and an empty task memory, in a
/// tenant transaction of its own.
async fn rotate(db: &TestDb, user: Uuid, ring: Ring) -> Option<PoolRow> {
    let task = TaskMemory::new();
    let avoid = Avoid::new(&ring, &task);
    let mut tx = begin_tenant(&db.app, user).await.unwrap();
    let row = reclaim_exemplar_tx(&mut tx, user, KP, &avoid)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    row
}

/// Seed one pool row. `claimed` non-`None` marks it served at that instant.
async fn seed_row(
    admin: &PgPool,
    user_id: Uuid,
    source: Source,
    hash: &str,
    text: &str,
    claimed: Option<DateTime<Utc>>,
) {
    let problem = format!(r#"{{"v":1,"text":"{text}","seed":0}}"#);
    sqlx::query!(
        r#"
        INSERT INTO serving_pool
            (user_id, kp_id, source, problem, expected_answer, instance_hash,
             created_at, claimed_at)
        VALUES ($1, $2, $3, $4::text::jsonb, '{"v":1,"answer":"2"}'::jsonb, $5, $6, $7)
        "#,
        user_id,
        KP,
        source.as_str(),
        problem,
        hash,
        at(0),
        claimed,
    )
    .execute(admin)
    .await
    .expect("the seeded pool row inserts");
}

// --------------------------------------------------------------------------
// D-O3: the content read
// --------------------------------------------------------------------------

/// C6. A `pending` row and a `rejected` row are never served, and the newest
/// approval of the same knowledge point and kind wins.
#[tokio::test]
async fn only_an_approved_document_is_read_and_the_newest_approval_wins() {
    TestDb::with(|db| async move {
        seed_doc(
            &db.admin,
            "digest-pending",
            KIND_TEACH,
            "pending",
            json!({"concept": "never served"}),
            None,
        )
        .await;
        seed_doc(
            &db.admin,
            "digest-rejected",
            KIND_TEACH,
            "rejected",
            json!({"concept": "never served"}),
            Some(at(9)),
        )
        .await;

        assert_eq!(
            approved_document(&db.app, KP, KIND_TEACH).await.unwrap(),
            None,
            "C6: an unapproved document is not servable"
        );

        seed_doc(
            &db.admin,
            "digest-old",
            KIND_TEACH,
            "approved",
            json!({"concept": "the old page"}),
            Some(at(1)),
        )
        .await;
        seed_doc(
            &db.admin,
            "digest-new",
            KIND_TEACH,
            "approved",
            json!({"concept": "the new page"}),
            Some(at(2)),
        )
        .await;

        let found = approved_document(&db.app, KP, KIND_TEACH)
            .await
            .unwrap()
            .expect("the approved page is servable");
        assert_eq!(found.digest, "digest-new");
        assert_eq!(found.body, json!({"concept": "the new page"}));
    })
    .await;
}

/// The kind selects the document: one knowledge point carries a teach page and a
/// hint ladder, and neither read sees the other.
#[tokio::test]
async fn the_kind_selects_the_document() {
    TestDb::with(|db| async move {
        seed_doc(
            &db.admin,
            "digest-teach",
            KIND_TEACH,
            "approved",
            json!({"concept": "the page"}),
            Some(at(1)),
        )
        .await;
        seed_doc(
            &db.admin,
            "digest-hints",
            KIND_HINT_LADDER,
            "approved",
            json!({"hints": ["the first rung"]}),
            Some(at(1)),
        )
        .await;

        let teach = approved_document(&db.app, KP, KIND_TEACH)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(teach.digest, "digest-teach");
        let hints = approved_document(&db.app, KP, KIND_HINT_LADDER)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(hints.digest, "digest-hints");
        assert_eq!(hints.body, json!({"hints": ["the first rung"]}));

        assert_eq!(
            approved_document(&db.app, "no-such-topic/kp1", KIND_TEACH)
                .await
                .unwrap(),
            None
        );
    })
    .await;
}

// --------------------------------------------------------------------------
// A6: the exemplar rotation
// --------------------------------------------------------------------------

/// A pair whose whole authored list is claimed serves the LEAST recently served
/// exemplar again, and the re-claim moves `claimed_at`, so the A6 view keeps
/// showing the knowledge point as a live fallback.
#[tokio::test]
async fn the_rotation_serves_the_oldest_claimed_exemplar_and_restamps_it() {
    TestDb::with(|db| async move {
        let user = db.seed_user("rotate@example.com").await;
        seed_row(
            &db.admin,
            user,
            Source::Exemplar,
            "hash-oldest",
            "Compute 1 + 1.",
            Some(at(1)),
        )
        .await;
        seed_row(
            &db.admin,
            user,
            Source::Exemplar,
            "hash-newest",
            "Compute 2 + 2.",
            Some(at(2)),
        )
        .await;

        // Every hash is inside the ring: the steady state of a knowledge point
        // whose exemplar count is under the ring size.
        let row = rotate(&db, user, Ring::from_hashes(["hash-oldest", "hash-newest"]))
            .await
            .expect("the rotation serves a row");

        assert_eq!(row.instance_hash, "hash-oldest");
        assert_eq!(row.problem.text, "Compute 1 + 1.");
        assert_eq!(row.source, Source::Exemplar);

        // The re-claim moved the instant off the seeded one, so
        // `operator_flags.last_exemplar_at` follows the newest fallback serve.
        let stamps = sqlx::query!(
            r#"
            SELECT instance_hash AS "hash!", claimed_at AS "claimed!"
            FROM serving_pool WHERE user_id = $1 ORDER BY instance_hash
            "#,
            user
        )
        .fetch_all(&db.admin)
        .await
        .unwrap();
        assert_eq!(stamps.len(), 2);
        assert_eq!(stamps[0].hash, "hash-newest");
        assert_eq!(stamps[0].claimed, at(2), "the row it did not serve moved");
        assert_eq!(stamps[1].hash, "hash-oldest");
        assert!(
            stamps[1].claimed > at(2),
            "the rotation did not re-stamp the row it served"
        );

        // The next rotation, with the same full ring, alternates.
        let row = rotate(&db, user, Ring::from_hashes(["hash-oldest", "hash-newest"]))
            .await
            .expect("the rotation serves a row");
        assert_eq!(row.instance_hash, "hash-newest");
    })
    .await;
}

/// A row the ring does not block wins over an older blocked one, and the
/// rotation reads neither a template row nor an unclaimed row.
#[tokio::test]
async fn the_rotation_reads_claimed_exemplar_rows_only() {
    TestDb::with(|db| async move {
        let user = db.seed_user("scope@example.com").await;
        seed_row(
            &db.admin,
            user,
            Source::Exemplar,
            "hash-blocked",
            "Compute 1 + 1.",
            Some(at(1)),
        )
        .await;
        seed_row(
            &db.admin,
            user,
            Source::Exemplar,
            "hash-free",
            "Compute 2 + 2.",
            Some(at(2)),
        )
        .await;
        seed_row(
            &db.admin,
            user,
            Source::Template,
            "hash-template",
            "Compute 3 + 3.",
            Some(at(0)),
        )
        .await;
        seed_row(
            &db.admin,
            user,
            Source::Exemplar,
            "hash-unclaimed",
            "Compute 4 + 4.",
            None,
        )
        .await;

        let row = rotate(&db, user, Ring::from_hashes(["hash-blocked"]))
            .await
            .expect("the rotation serves a row");

        assert_eq!(row.instance_hash, "hash-free");

        // The unclaimed row is the pop's business, and it stays unclaimed.
        let unclaimed = sqlx::query_scalar!(
            r#"
            SELECT count(*) AS "n!" FROM serving_pool
            WHERE user_id = $1 AND claimed_at IS NULL
            "#,
            user
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(unclaimed, 1);
    })
    .await;
}

/// A pair that holds no claimed exemplar row rotates nothing. The serve handler
/// then has no problem for that knowledge point and says so.
#[tokio::test]
async fn a_pair_with_no_exemplar_row_rotates_nothing() {
    TestDb::with(|db| async move {
        let user = db.seed_user("empty@example.com").await;
        let row = rotate(&db, user, Ring::new()).await;

        assert_eq!(row, None);
    })
    .await;
}

/// C3: the rotation is bound to one tenant. Another learner's claimed exemplar
/// row is invisible to it.
#[tokio::test]
async fn the_rotation_never_reaches_another_tenants_rows() {
    TestDb::with(|db| async move {
        let mine = db.seed_user("mine@example.com").await;
        let theirs = db.seed_user("theirs@example.com").await;
        seed_row(
            &db.admin,
            theirs,
            Source::Exemplar,
            "hash-theirs",
            "Compute 1 + 1.",
            Some(at(1)),
        )
        .await;

        let row = rotate(&db, mine, Ring::new()).await;

        assert_eq!(row, None);
    })
    .await;
}

/// A closed pool is the error of the document read.
#[tokio::test]
async fn a_closed_pool_is_the_error_of_the_document_read() {
    TestDb::with(|db| async move {
        let pool = closed_pool(&db).await;
        assert!(approved_document(&pool, KP, KIND_TEACH).await.is_err());
    })
    .await;
}
