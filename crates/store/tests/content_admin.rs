//! M6 R4: the C6 admin write path of `content_store`.
//!
//! Requirements: C6 (a human approves a document before it serves, and the
//! approval binds to the digest), C5 (content is reviewed), R4 (the request tier
//! never writes this table), T3 (the attempts and the money of one document).
//!
//! Spec: `docs/reference/authoring-and-spa-1.0-spec.md` section 3.2 and row R4
//! of section 7.
//!
//! Every expected value is a LITERAL.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use cadus_store::content::{
    Admin, KIND_TEACH, KIND_TEMPLATE, NewDocument, approve, approved_document, insert_pending,
    reject,
};
use cadus_store::test_support::TestDb;
use cadus_store::{Db, StoreError};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

/// The serving key of these tests.
const KP: &str = "perfect-squares/kp1";

/// The digest of the document under review.
const DIGEST: &str = "sha256:0123456789abcdef";

/// Wrap a pool in a `Db` with no client-side bound. The tests measure the
/// statement, not the timeout, and unit `client_timeout.rs` pins the bound.
fn handle(pool: &PgPool) -> Db {
    Db::new(pool.clone(), 0)
}

/// One template document to insert.
fn template<'a>(digest: &'a str, body: &'a serde_json::Value) -> NewDocument<'a> {
    NewDocument {
        digest,
        kp_id: KP,
        kind: KIND_TEMPLATE,
        body,
        authoring_attempts: 2,
        cost_usd: Some("0.004500"),
    }
}

/// The SQLSTATE of a database error, or `"none"` when the error carries none.
fn sqlstate(err: &sqlx::Error) -> String {
    match err.as_database_error().and_then(|db| db.code()) {
        Some(code) => code.to_string(),
        None => "none".to_string(),
    }
}

// --------------------------------------------------------------------------
// The acceptance check: the runtime role holds SELECT and nothing else
// --------------------------------------------------------------------------

/// C6, finding #14: `cadus_app` cannot INSERT, UPDATE, or DELETE
/// `content_store`, and the store's own write path gets the same answer when a
/// caller hands it the runtime pool.
///
/// The three raw statements pin the grant. The fourth call pins the store: an
/// `Admin` built over the request tier's pool writes nothing, so the type names
/// the path and the grant enforces it.
#[tokio::test]
async fn the_app_role_cannot_insert_update_or_delete_content_store() {
    TestDb::with(|db| async move {
        let body = json!({"statement": "reviewed"});
        let inserted = insert_pending(Admin::new(&handle(&db.admin)), &template(DIGEST, &body))
            .await
            .unwrap();
        assert!(inserted, "the admin path inserts the seed row");

        let insert_err = sqlx::query!(
            "INSERT INTO content_store (digest, kp_id, kind, body, status)
             VALUES ('sha256:smuggled', 'perfect-squares/kp1', 'template',
                     '{}'::jsonb, 'approved')"
        )
        .execute(&db.app)
        .await
        .unwrap_err();
        assert_eq!(sqlstate(&insert_err), "42501");

        let update_err = sqlx::query!("UPDATE content_store SET status = 'approved'")
            .execute(&db.app)
            .await
            .unwrap_err();
        assert_eq!(sqlstate(&update_err), "42501");

        let delete_err = sqlx::query!("DELETE FROM content_store")
            .execute(&db.app)
            .await
            .unwrap_err();
        assert_eq!(sqlstate(&delete_err), "42501");

        // The store's write path is not a way around the grant.
        let through_store = insert_pending(
            Admin::new(&handle(&db.app)),
            &template("sha256:through-the-store", &body),
        )
        .await
        .unwrap_err();
        match through_store {
            StoreError::Db(err) => assert_eq!(sqlstate(&err), "42501"),
            other => panic!("the app role wrote through the store path: {other}"),
        }

        // The runtime role still reads, and it reads exactly the seeded row.
        let rows =
            sqlx::query!(r#"SELECT digest AS "digest!", status AS "status!" FROM content_store"#)
                .fetch_all(&db.app)
                .await
                .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].digest, DIGEST);
        assert_eq!(rows[0].status, "pending");
    })
    .await;
}

// --------------------------------------------------------------------------
// insert_pending
// --------------------------------------------------------------------------

/// The insert writes `pending`, the T3 columns, and nothing else. A second
/// insert of the same digest writes no row and reports `false`.
#[tokio::test]
async fn the_insert_writes_pending_once_per_digest() {
    TestDb::with(|db| async move {
        let admin = handle(&db.admin);
        let body = json!({"statement": "Compute $a + $b$."});

        assert!(
            insert_pending(Admin::new(&admin), &template(DIGEST, &body))
                .await
                .unwrap()
        );
        assert!(
            !insert_pending(Admin::new(&admin), &template(DIGEST, &body))
                .await
                .unwrap(),
            "C6: the same digest is the same body, so the second insert writes nothing"
        );

        let row = sqlx::query!(
            r#"
            SELECT status AS "status!", kind AS "kind!", kp_id AS "kp_id!",
                   body AS "body!", authoring_attempts AS "attempts!",
                   authoring_cost_usd::text AS "cost", review_reason,
                   approved_by, approved_at
            FROM content_store WHERE digest = $1
            "#,
            DIGEST
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();

        assert_eq!(row.status, "pending");
        assert_eq!(row.kind, "template");
        assert_eq!(row.kp_id, "perfect-squares/kp1");
        assert_eq!(row.body, json!({"statement": "Compute $a + $b$."}));
        assert_eq!(row.attempts, 2);
        assert_eq!(row.cost.as_deref(), Some("0.004500"));
        assert_eq!(row.review_reason, None);
        assert_eq!(row.approved_by, None);
        assert_eq!(row.approved_at, None);

        // C6: a pending document does not serve.
        assert_eq!(
            approved_document(&db.app, KP, KIND_TEMPLATE).await.unwrap(),
            None
        );
    })
    .await;
}

/// A second insert never overwrites the verdict a human already gave. The
/// authoring job runs nightly, so this is the ordinary case and not an edge.
#[tokio::test]
async fn a_repeat_insert_leaves_an_approval_alone() {
    TestDb::with(|db| async move {
        let admin = handle(&db.admin);
        let body = json!({"statement": "reviewed"});
        insert_pending(Admin::new(&admin), &template(DIGEST, &body))
            .await
            .unwrap();
        approve(Admin::new(&admin), DIGEST, None).await.unwrap();

        assert!(
            !insert_pending(Admin::new(&admin), &template(DIGEST, &body))
                .await
                .unwrap()
        );

        let status = sqlx::query_scalar!(
            r#"SELECT status AS "status!" FROM content_store WHERE digest = $1"#,
            DIGEST
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(status, "approved");
    })
    .await;
}

// --------------------------------------------------------------------------
// approve
// --------------------------------------------------------------------------

/// The acceptance check: an approve on a digest the table does not hold is a
/// typed `NotFound`, which the R5 route answers with 404.
#[tokio::test]
async fn a_review_write_on_an_absent_digest_is_a_typed_not_found() {
    TestDb::with(|db| async move {
        let admin = handle(&db.admin);

        let approve_err = approve(Admin::new(&admin), "sha256:absent", None)
            .await
            .unwrap_err();
        match approve_err {
            StoreError::NotFound { entity, key } => {
                assert_eq!(entity, "content_store");
                assert_eq!(key, "sha256:absent");
            }
            other => panic!("the approve of an absent digest gave {other}"),
        }
        assert_eq!(
            approve(Admin::new(&admin), "sha256:absent", None)
                .await
                .unwrap_err()
                .to_string(),
            "no content_store row with key sha256:absent"
        );

        let reject_err = reject(Admin::new(&admin), "sha256:absent", "no reason to keep it")
            .await
            .unwrap_err();
        assert!(
            matches!(reject_err, StoreError::NotFound { .. }),
            "the reject of an absent digest is not a NotFound"
        );

        // Neither write created a row.
        let rows = sqlx::query_scalar!(r#"SELECT count(*) AS "n!" FROM content_store"#)
            .fetch_one(&db.admin)
            .await
            .unwrap();
        assert_eq!(rows, 0);
    })
    .await;
}

/// The acceptance check: approving twice is idempotent. The second call keeps
/// the first reviewer and the first instant, and the document serves.
#[tokio::test]
async fn approving_twice_keeps_the_first_stamp() {
    TestDb::with(|db| async move {
        let admin = handle(&db.admin);
        let first = db.seed_user("first-reviewer@example.test").await;
        let second = db.seed_user("second-reviewer@example.test").await;
        let body = json!({"concept": "the page"});
        insert_pending(
            Admin::new(&admin),
            &NewDocument {
                digest: DIGEST,
                kp_id: KP,
                kind: KIND_TEACH,
                body: &body,
                authoring_attempts: 1,
                cost_usd: None,
            },
        )
        .await
        .unwrap();

        let one = approve(Admin::new(&admin), DIGEST, Some(first))
            .await
            .unwrap();
        assert_eq!(one.digest, DIGEST);
        assert_eq!(one.status, "approved");
        assert_eq!(one.approved_by, Some(first));
        let stamped = one.approved_at.expect("the approval carries an instant");

        let two = approve(Admin::new(&admin), DIGEST, Some(second))
            .await
            .unwrap();
        assert_eq!(two.status, "approved");
        assert_eq!(
            two.approved_by,
            Some(first),
            "the second approval took the row from the first reviewer"
        );
        assert_eq!(
            two.approved_at,
            Some(stamped),
            "the second approval moved the instant"
        );
        assert_eq!(one, two);

        // One row, one approval, and it serves.
        let served = approved_document(&db.app, KP, KIND_TEACH)
            .await
            .unwrap()
            .expect("the approved page serves");
        assert_eq!(served.digest, DIGEST);
        assert_eq!(served.body, json!({"concept": "the page"}));

        let count = sqlx::query_scalar!(
            r#"SELECT count(*) AS "n!" FROM content_store WHERE status = 'approved'"#
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(count, 1);
    })
    .await;
}

// --------------------------------------------------------------------------
// reject
// --------------------------------------------------------------------------

/// A rejection keeps the body, records the reason, drops the approval stamp,
/// and stops the document serving (C6).
#[tokio::test]
async fn a_rejection_stops_the_document_and_keeps_the_reason() {
    TestDb::with(|db| async move {
        let admin = handle(&db.admin);
        let reviewer = db.seed_user("reviewer@example.test").await;
        let body = json!({"statement": "Compute $12 - 70$."});
        insert_pending(Admin::new(&admin), &template(DIGEST, &body))
            .await
            .unwrap();
        approve(Admin::new(&admin), DIGEST, Some(reviewer))
            .await
            .unwrap();

        let decision = reject(
            Admin::new(&admin),
            DIGEST,
            "the low edge of the domain gives a negative difference",
        )
        .await
        .unwrap();
        assert_eq!(decision.digest, DIGEST);
        assert_eq!(decision.status, "rejected");
        assert_eq!(decision.approved_by, None);
        assert_eq!(decision.approved_at, None);

        assert_eq!(
            approved_document(&db.app, KP, KIND_TEMPLATE).await.unwrap(),
            None,
            "C6: a rejected document is never served"
        );

        let row = sqlx::query!(
            r#"
            SELECT status AS "status!", body AS "body!", review_reason,
                   approved_by, approved_at
            FROM content_store WHERE digest = $1
            "#,
            DIGEST
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(row.status, "rejected");
        assert_eq!(row.body, json!({"statement": "Compute $12 - 70$."}));
        assert_eq!(
            row.review_reason.as_deref(),
            Some("the low edge of the domain gives a negative difference")
        );
        assert_eq!(row.approved_by, None);
        assert_eq!(row.approved_at, None);
    })
    .await;
}

/// A reviewer who refused a digest by mistake approves it again, and the row
/// carries a fresh stamp instead of the one the rejection cleared.
#[tokio::test]
async fn an_approval_after_a_rejection_stamps_the_row_again() {
    TestDb::with(|db| async move {
        let admin = handle(&db.admin);
        let reviewer = db.seed_user("second-look@example.test").await;
        let body = json!({"statement": "reviewed twice"});
        insert_pending(Admin::new(&admin), &template(DIGEST, &body))
            .await
            .unwrap();
        reject(Admin::new(&admin), DIGEST, "refused on the first read")
            .await
            .unwrap();

        let decision = approve(Admin::new(&admin), DIGEST, Some(reviewer))
            .await
            .unwrap();
        assert_eq!(decision.status, "approved");
        assert_eq!(decision.approved_by, Some(reviewer));
        assert!(
            decision.approved_at.is_some(),
            "the second approval carries no instant"
        );

        let served = approved_document(&db.app, KP, KIND_TEMPLATE)
            .await
            .unwrap()
            .expect("the re-approved template serves");
        assert_eq!(served.digest, DIGEST);
    })
    .await;
}

/// The write addresses one digest and no other row of the same knowledge point
/// and kind (C6: an edited body is a new row with its own approval).
#[tokio::test]
async fn a_review_write_touches_one_digest_only() {
    TestDb::with(|db| async move {
        let admin = handle(&db.admin);
        let first = json!({"statement": "the first body"});
        let second = json!({"statement": "the edited body"});
        insert_pending(Admin::new(&admin), &template(DIGEST, &first))
            .await
            .unwrap();
        insert_pending(Admin::new(&admin), &template("sha256:edited", &second))
            .await
            .unwrap();

        approve(Admin::new(&admin), DIGEST, None).await.unwrap();

        let rows = sqlx::query!(
            r#"
            SELECT digest AS "digest!", status AS "status!"
            FROM content_store ORDER BY digest
            "#
        )
        .fetch_all(&db.admin)
        .await
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].digest, "sha256:0123456789abcdef");
        assert_eq!(rows[0].status, "approved");
        assert_eq!(rows[1].digest, "sha256:edited");
        assert_eq!(rows[1].status, "pending");

        // The approved body is the one that serves, and the edit waits.
        let served = approved_document(&db.app, KP, KIND_TEMPLATE)
            .await
            .unwrap()
            .expect("the approved template serves");
        assert_eq!(served.body, json!({"statement": "the first body"}));

        // A reviewer id that names no user is a foreign-key violation, so the
        // approval fails and the row stays pending.
        let err = approve(Admin::new(&admin), "sha256:edited", Some(Uuid::nil()))
            .await
            .unwrap_err();
        match err {
            StoreError::Db(err) => assert_eq!(sqlstate(&err), "23503"),
            other => panic!("an unknown reviewer gave {other}"),
        }
        let status = sqlx::query_scalar!(
            r#"SELECT status AS "status!" FROM content_store WHERE digest = 'sha256:edited'"#
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(status, "pending");
    })
    .await;
}
