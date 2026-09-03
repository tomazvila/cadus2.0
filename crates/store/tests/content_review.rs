//! R5: the review reads of `content_store`, the re-gate read, the one-document
//! read, and the prompt stamp of a held row.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_store::content::{
    Admin, KIND_HINT_LADDER, KIND_TEACH, KIND_TEMPLATE, LIST_LIMIT, NewDocument, ReviewFilter,
    STATUS_APPROVED, STATUS_PENDING, STATUS_REJECTED, document, insert_pending,
    refresh_prompt_digest, regate_rows, review_list,
};
use cadus_store::test_support::TestDb;
use common::{KP, at, handle, seed_doc};
use serde_json::json;

/// Seed the six documents of the review tests: three templates (approved,
/// pending, rejected), an approved teach page, a pending teach page, and a
/// pending hint ladder.
async fn seed_review_rows(db: &TestDb) {
    let rows = [
        ("t-approved", KIND_TEMPLATE, STATUS_APPROVED, Some(at(1))),
        ("t-pending", KIND_TEMPLATE, STATUS_PENDING, None),
        ("t-rejected", KIND_TEMPLATE, STATUS_REJECTED, None),
        ("teach-approved", KIND_TEACH, STATUS_APPROVED, Some(at(2))),
        ("teach-pending", KIND_TEACH, STATUS_PENDING, None),
        ("hint-pending", KIND_HINT_LADDER, STATUS_PENDING, None),
    ];
    for (digest, kind, status, approved) in rows {
        seed_doc(
            &db.admin,
            digest,
            kind,
            status,
            json!({"d": digest}),
            approved,
        )
        .await;
    }
}

/// The queue lists every row newest first, each filter narrows it, and the
/// approved template count rides on every row.
#[tokio::test]
async fn the_queue_lists_every_row_and_each_filter_narrows_it() {
    TestDb::with(|db| async move {
        seed_review_rows(&db).await;
        assert_eq!(LIST_LIMIT, 200);

        let all = review_list(&db.app, &ReviewFilter::default())
            .await
            .unwrap();
        assert_eq!(all.len(), 6);
        assert!(all.iter().all(|item| item.approved_templates == 1));
        assert!(all.iter().all(|item| item.kp_id == KP));

        let pending = review_list(
            &db.app,
            &ReviewFilter {
                status: Some(STATUS_PENDING),
                ..ReviewFilter::default()
            },
        )
        .await
        .unwrap();
        let mut digests: Vec<&str> = pending.iter().map(|item| item.digest.as_str()).collect();
        digests.sort_unstable();
        assert_eq!(digests, vec!["hint-pending", "t-pending", "teach-pending"]);

        let teach = review_list(
            &db.app,
            &ReviewFilter {
                kind: Some(KIND_TEACH),
                kp_id: Some(KP),
                ..ReviewFilter::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(teach.len(), 2);
        let none = review_list(
            &db.app,
            &ReviewFilter {
                kp_id: Some("other/kp"),
                ..ReviewFilter::default()
            },
        )
        .await
        .unwrap();
        assert!(none.is_empty());
    })
    .await;
}

/// The re-gate reads the approved and pending templates and the pending
/// pages and ladders, in kind and digest order, and nothing rejected.
#[tokio::test]
async fn the_regate_reads_the_answer_set_and_the_pending_documents() {
    TestDb::with(|db| async move {
        seed_review_rows(&db).await;
        let rows = regate_rows(&db.app, KP).await.unwrap();
        let listed: Vec<(&str, &str, &str)> = rows
            .iter()
            .map(|row| (row.kind.as_str(), row.digest.as_str(), row.status.as_str()))
            .collect();
        assert_eq!(
            listed,
            vec![
                (KIND_HINT_LADDER, "hint-pending", STATUS_PENDING),
                (KIND_TEACH, "teach-pending", STATUS_PENDING),
                (KIND_TEMPLATE, "t-approved", STATUS_APPROVED),
                (KIND_TEMPLATE, "t-pending", STATUS_PENDING),
            ]
        );
        assert_eq!(rows[0].body, json!({"d": "hint-pending"}));
        assert!(regate_rows(&db.app, "other/kp").await.unwrap().is_empty());
    })
    .await;
}

/// One document reads with its queue line and its two review columns, and
/// an absent digest reads as `None`.
#[tokio::test]
async fn one_document_reads_with_its_review_columns() {
    TestDb::with(|db| async move {
        seed_review_rows(&db).await;
        let doc = document(&db.app, "teach-approved")
            .await
            .unwrap()
            .expect("the seeded document");
        assert_eq!(doc.item.digest, "teach-approved");
        assert_eq!(doc.item.kind, KIND_TEACH);
        assert_eq!(doc.item.status, STATUS_APPROVED);
        assert_eq!(doc.item.approved_templates, 1);
        assert_eq!(doc.item.body, json!({"d": "teach-approved"}));
        assert_eq!(doc.approved_at, Some(at(2)));
        assert_eq!(doc.review_reason, None);
        assert_eq!(document(&db.app, "absent").await.unwrap(), None);
    })
    .await;
}

/// The prompt stamp changes a held row that names another prompt, and
/// leaves a row with the same prompt, a row with no prompt, and an absent
/// digest alone.
#[tokio::test]
async fn the_prompt_stamp_changes_a_row_that_names_another_prompt() {
    TestDb::with(|db| async move {
        let admin = handle(&db.admin);
        let body = json!({"statement": "s"});
        let stamped = NewDocument {
            digest: "stamped",
            kp_id: KP,
            kind: KIND_TEMPLATE,
            body: &body,
            authoring_attempts: 1,
            cost_usd: None,
            prompt_digest: Some("prompt-1"),
        };
        let unstamped = NewDocument {
            digest: "unstamped",
            prompt_digest: None,
            ..stamped.clone()
        };
        insert_pending(Admin::new(&admin), &stamped).await.unwrap();
        insert_pending(Admin::new(&admin), &unstamped)
            .await
            .unwrap();

        let refresh = |digest: &'static str, prompt: &'static str| {
            let admin = admin.clone();
            async move {
                refresh_prompt_digest(Admin::new(&admin), digest, prompt)
                    .await
                    .unwrap()
            }
        };
        assert!(refresh("stamped", "prompt-2").await);
        assert!(!refresh("stamped", "prompt-2").await);
        assert!(!refresh("unstamped", "prompt-2").await);
        assert!(!refresh("absent", "prompt-2").await);
        let held: Option<String> =
            sqlx::query_scalar("SELECT prompt_digest FROM content_store WHERE digest = 'stamped'")
                .fetch_one(&db.admin)
                .await
                .unwrap();
        assert_eq!(held.as_deref(), Some("prompt-2"));
    })
    .await;
}

/// Every content read and every admin write reports a closed pool.
#[tokio::test]
async fn every_content_statement_reports_a_closed_pool() {
    TestDb::with(|db| async move {
        let pool = common::fault::closed_pool(&db).await;
        let closed = handle(&pool);
        let admin = Admin::new(&closed);
        assert!(review_list(&pool, &ReviewFilter::default()).await.is_err());
        assert!(regate_rows(&pool, KP).await.is_err());
        assert!(document(&pool, "d").await.is_err());
        assert!(
            cadus_store::content::approved_document(&pool, KP, KIND_TEACH)
                .await
                .is_err()
        );
        assert!(refresh_prompt_digest(admin, "d", "p").await.is_err());
        assert!(
            cadus_store::content::approve(admin, "d", None)
                .await
                .is_err()
        );
        assert!(cadus_store::content::reject(admin, "d", "r").await.is_err());
        assert!(cadus_store::content::verdict(&closed, "d").await.is_err());
        let body = json!({});
        let doc = NewDocument {
            digest: "d",
            kp_id: KP,
            kind: KIND_TEMPLATE,
            body: &body,
            authoring_attempts: 1,
            cost_usd: None,
            prompt_digest: None,
        };
        assert!(insert_pending(admin, &doc).await.is_err());
    })
    .await;
}
