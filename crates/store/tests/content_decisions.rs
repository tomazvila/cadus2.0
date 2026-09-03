//! The C6 review writes of `content_store`: the idempotent approval, the
//! rejection with its reason, the verdict read, and the one-row scope of every
//! write (spec section 3.2).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_store::content::{
    Admin, KIND_TEACH, KIND_TEMPLATE, NewDocument, approve, approved_document, insert_pending,
    reject, verdict,
};
use cadus_store::test_support::TestDb;

use cadus_store::StoreError;
use common::content::{DIGEST, template};
use common::{KP, handle, sqlstate};
use serde_json::json;
use uuid::Uuid;

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
                prompt_digest: None,
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

/// M6 review finding F26: a rejection changes EXACTLY one row.
///
/// The three seeded rows are three documents of one knowledge point and one
/// kind: one approved, one the reviewer refuses here, and one still pending. The
/// test names the status of all three after the write, so a WHERE clause that
/// took the knowledge point, the kind, or every row fails here. `reject` returns
/// one `Decision` whatever it wrote, and the route answers 200 either way, so
/// nothing else in the three crates catches that widening.
#[tokio::test]
async fn a_rejection_changes_exactly_one_row() {
    TestDb::with(|db| async move {
        let admin = handle(&db.admin);
        let reviewer = db.seed_user("three-rows@example.test").await;
        let approved = json!({"statement": "the approved body"});
        let refused = json!({"statement": "the refused body"});
        let waiting = json!({"statement": "the waiting body"});
        insert_pending(Admin::new(&admin), &template("sha256:row-a", &approved))
            .await
            .unwrap();
        insert_pending(Admin::new(&admin), &template("sha256:row-b", &refused))
            .await
            .unwrap();
        insert_pending(Admin::new(&admin), &template("sha256:row-c", &waiting))
            .await
            .unwrap();
        approve(Admin::new(&admin), "sha256:row-a", Some(reviewer))
            .await
            .unwrap();

        let decision = reject(
            Admin::new(&admin),
            "sha256:row-b",
            "the statement asks for two answers",
        )
        .await
        .unwrap();
        assert_eq!(decision.digest, "sha256:row-b");
        assert_eq!(decision.status, "rejected");

        let rows = sqlx::query!(
            r#"
            SELECT digest AS "digest!", status AS "status!", review_reason,
                   approved_by, approved_at
            FROM content_store ORDER BY digest
            "#
        )
        .fetch_all(&db.admin)
        .await
        .unwrap();
        assert_eq!(rows.len(), 3);

        assert_eq!(rows[0].digest, "sha256:row-a");
        assert_eq!(rows[0].status, "approved");
        assert_eq!(rows[0].review_reason, None);
        assert_eq!(rows[0].approved_by, Some(reviewer));
        assert!(rows[0].approved_at.is_some());

        assert_eq!(rows[1].digest, "sha256:row-b");
        assert_eq!(rows[1].status, "rejected");
        assert_eq!(
            rows[1].review_reason.as_deref(),
            Some("the statement asks for two answers")
        );
        assert_eq!(rows[1].approved_by, None);
        assert_eq!(rows[1].approved_at, None);

        assert_eq!(rows[2].digest, "sha256:row-c");
        assert_eq!(rows[2].status, "pending");
        assert_eq!(rows[2].review_reason, None);
        assert_eq!(rows[2].approved_by, None);
        assert_eq!(rows[2].approved_at, None);

        // The approved document still serves: the rejection of another digest
        // took nothing from it (C6).
        let served = approved_document(&db.app, KP, KIND_TEMPLATE)
            .await
            .unwrap()
            .expect("the approved template serves");
        assert_eq!(served.digest, "sha256:row-a");
        assert_eq!(served.body, json!({"statement": "the approved body"}));
    })
    .await;
}

/// The verdict read tells the authoring job what the table holds for a digest
/// it collided with (C6, M6 review finding F6).
///
/// The job inserts with `ON CONFLICT DO NOTHING`, so a `false` answer says only
/// that the digest is there. This read says WHAT is there: a duplicate document
/// that waits for a human, or a body the human refused, with the reason.
#[tokio::test]
async fn the_verdict_of_a_digest_names_the_status_and_the_reason() {
    TestDb::with(|db| async move {
        let admin = handle(&db.admin);
        let body = json!({"statement": "the reviewed body"});

        assert_eq!(verdict(&admin, "sha256:absent").await.unwrap(), None);

        insert_pending(Admin::new(&admin), &template(DIGEST, &body))
            .await
            .unwrap();
        let pending = verdict(&admin, DIGEST)
            .await
            .unwrap()
            .expect("the seeded row has a verdict");
        assert_eq!(pending.status, "pending");
        assert_eq!(pending.review_reason, None);

        reject(
            Admin::new(&admin),
            DIGEST,
            "the low edge gives a negative answer",
        )
        .await
        .unwrap();
        let refused = verdict(&admin, DIGEST)
            .await
            .unwrap()
            .expect("the refused row has a verdict");
        assert_eq!(refused.status, "rejected");
        assert_eq!(
            refused.review_reason.as_deref(),
            Some("the low edge gives a negative answer")
        );

        approve(Admin::new(&admin), DIGEST, None).await.unwrap();
        let approved = verdict(&admin, DIGEST)
            .await
            .unwrap()
            .expect("the approved row has a verdict");
        assert_eq!(approved.status, "approved");
        // The approval keeps the reason of the rejection it overruled.
        assert_eq!(
            approved.review_reason.as_deref(),
            Some("the low edge gives a negative answer")
        );
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
