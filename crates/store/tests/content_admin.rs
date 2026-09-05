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

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_store::StoreError;
use cadus_store::content::{
    Admin, KIND_TEACH, KIND_TEMPLATE, NewDocument, approve, approved_document, insert_pending,
    reject,
};
use cadus_store::test_support::TestDb;
use common::content::{DIGEST, PROMPT_DIGEST, template};
use common::{KP, handle, sqlstate};
use serde_json::json;

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

/// F4: the insert writes `prompt_digest`, and `None` writes NULL.
///
/// Spec section 2.2, "Prompt digest": the digest is a COLUMN on the row and
/// never part of the content digest, because the C6 approval binds to the
/// content. A prompt edit therefore marks the row for re-authoring and never
/// unapproves it. A NULL means "the prompt is not recorded", which every row
/// written before migration `0012_content_prompt_digest` carries.
#[tokio::test]
async fn the_insert_writes_the_prompt_digest_column() {
    TestDb::with(|db| async move {
        let admin = handle(&db.admin);
        let body = json!({"statement": "Compute $7^{{2}}$."});
        insert_pending(Admin::new(&admin), &template(DIGEST, &body))
            .await
            .unwrap();
        insert_pending(
            Admin::new(&admin),
            &NewDocument {
                digest: "sha256:no-prompt-digest",
                kp_id: KP,
                kind: KIND_TEACH,
                body: &body,
                authoring_attempts: 1,
                cost_usd: None,
                prompt_digest: None,
            },
        )
        .await
        .unwrap();

        let rows = sqlx::query!(
            r#"SELECT digest AS "digest!", prompt_digest
                 FROM content_store ORDER BY digest"#
        )
        .fetch_all(&db.admin)
        .await
        .unwrap();

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].digest, DIGEST);
        assert_eq!(rows[0].prompt_digest.as_deref(), Some(PROMPT_DIGEST));
        assert_eq!(rows[1].digest, "sha256:no-prompt-digest");
        assert_eq!(rows[1].prompt_digest, None);
    })
    .await;
}

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
