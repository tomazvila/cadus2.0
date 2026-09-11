//! Current policy, curriculum, and engine binding prevents stale serving.
#![allow(clippy::unwrap_used)]

use cadus_store::content::{
    Admin, ApprovalContext, CurrentContext, KIND_TEMPLATE, approve_current,
    approved_document_current, reject, template_review_context,
};
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};

const KP: &str = "finite/kp1";
const CURRICULUM: &str = "curriculum-v1";
const ENGINE: &str = "engine-v1";

fn current(policy: Option<&str>) -> CurrentContext<'_> {
    CurrentContext {
        policy_digest: policy,
        curriculum_digest: CURRICULUM,
        review_engine_digest: ENGINE,
    }
}

async fn seed(db: &TestDb, digest: &str) {
    sqlx::query(
        "INSERT INTO content_store (digest,kp_id,kind,body) VALUES ($1,$2,'template','{}')",
    )
    .bind(digest)
    .bind(KP)
    .execute(&db.admin)
    .await
    .unwrap();
}

async fn approve(db: &Db, digest: &str, context: CurrentContext<'_>) {
    let (prospective, _) = template_review_context(db.pool(), KP, context, Some(digest))
        .await
        .unwrap();
    approve_current(
        Admin::new(db),
        digest,
        None,
        ApprovalContext {
            current: context,
            template_context_digest: prospective.as_deref(),
        },
    )
    .await
    .unwrap()
    .unwrap();
}

#[tokio::test]
async fn a_changed_policy_or_semantic_context_requires_review_and_rejection_revokes() {
    TestDb::with(|db| async move {
        seed(&db, "policy-content").await;
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
        approve(&handle, "policy-content", current(Some("v1"))).await;
        assert!(
            approved_document_current(&db.app, KP, KIND_TEMPLATE, current(Some("v1")))
                .await
                .unwrap()
                .is_some()
        );
        for stale in [
            current(Some("v2")),
            CurrentContext {
                curriculum_digest: "curriculum-v2",
                ..current(Some("v1"))
            },
            CurrentContext {
                review_engine_digest: "engine-v2",
                ..current(Some("v1"))
            },
        ] {
            assert!(
                approved_document_current(&db.app, KP, KIND_TEMPLATE, stale)
                    .await
                    .unwrap()
                    .is_none()
            );
        }
        reject(Admin::new(&handle), "policy-content", "mathematical error")
            .await
            .unwrap();
        assert!(
            approved_document_current(&db.app, KP, KIND_TEMPLATE, current(Some("v1")))
                .await
                .unwrap()
                .is_none()
        );
    })
    .await;
}

#[tokio::test]
async fn a_newer_stale_approval_cannot_shadow_the_current_document() {
    TestDb::with(|db| async move {
        for digest in ["current", "stale"] {
            seed(&db, digest).await;
        }
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
        approve(&handle, "current", current(Some("current-policy"))).await;
        approve(&handle, "stale", current(Some("old-policy"))).await;
        let row =
            approved_document_current(&db.app, KP, KIND_TEMPLATE, current(Some("current-policy")))
                .await
                .unwrap()
                .unwrap();
        assert_eq!(row.digest, "current");
    })
    .await;
}
