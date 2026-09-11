//! Review context follows the effective template bank and trusted semantics.
#![allow(clippy::unwrap_used)]

use cadus_store::content::{
    Admin, ApprovalContext, CurrentContext, approve_current, approved_document_current,
    approved_document_for_source, reject, template_review_context,
};
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};

const KP: &str = "context/kp1";
const CURRICULUM: &str = "curriculum-v1";
const ENGINE: &str = "engine-v1";

fn current(policy_digest: Option<&str>) -> CurrentContext<'_> {
    CurrentContext {
        policy_digest,
        curriculum_digest: CURRICULUM,
        review_engine_digest: ENGINE,
    }
}

async fn seed(db: &TestDb, digest: &str, kind: &str, policy: Option<&str>) {
    sqlx::query(
        "INSERT INTO content_store
         (digest,kp_id,kind,body,status,approved_at,approved_policy_digest,
          approved_curriculum_digest,approved_review_engine_digest)
         VALUES ($1,$2,$3,'{}','approved',now(),$4,$5,$6)",
    )
    .bind(digest)
    .bind(KP)
    .bind(kind)
    .bind(policy)
    .bind(CURRICULUM)
    .bind(ENGINE)
    .execute(&db.admin)
    .await
    .unwrap();
}

fn approval<'a>(context: CurrentContext<'a>, bank: Option<&'a str>) -> ApprovalContext<'a> {
    ApprovalContext {
        current: context,
        template_context_digest: bank,
    }
}

#[tokio::test]
async fn ordinary_instruction_expires_when_any_template_enters_or_leaves_the_bank() {
    TestDb::with(|db| async move {
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
        seed(&db, "t1", "template", None).await;
        seed(&db, "hint", "hint_ladder", None).await;
        let (first, members) = template_review_context(&db.app, KP, current(None), None)
            .await
            .unwrap();
        assert_eq!(members, ["t1"]);
        approve_current(
            Admin::new(&handle),
            "hint",
            None,
            approval(current(None), first.as_deref()),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(
            approved_document_for_source(
                &db.app,
                KP,
                "hint_ladder",
                current(None),
                Some("t1"),
                Some((CURRICULUM, ENGINE)),
            )
            .await
            .unwrap()
            .is_some()
        );

        seed(&db, "t2", "template", None).await;
        assert!(
            approved_document_current(&db.app, KP, "hint_ladder", current(None))
                .await
                .unwrap()
                .is_none()
        );
        let (second, members) = template_review_context(&db.app, KP, current(None), None)
            .await
            .unwrap();
        assert_eq!(members, ["t1", "t2"]);
        assert!(
            approve_current(
                Admin::new(&handle),
                "hint",
                None,
                approval(current(None), first.as_deref()),
            )
            .await
            .unwrap()
            .is_none()
        );
        approve_current(
            Admin::new(&handle),
            "hint",
            None,
            approval(current(None), second.as_deref()),
        )
        .await
        .unwrap()
        .unwrap();
        reject(Admin::new(&handle), "t1", "retired source")
            .await
            .unwrap();
        assert!(
            approved_document_for_source(
                &db.app,
                KP,
                "hint_ladder",
                current(None),
                Some("t1"),
                Some((CURRICULUM, ENGINE)),
            )
            .await
            .unwrap()
            .is_none()
        );
    })
    .await;
}

#[tokio::test]
async fn finite_bank_selects_only_the_newest_template_in_the_exact_context() {
    TestDb::with(|db| async move {
        seed(&db, "older", "template", Some("finite-v1")).await;
        sqlx::query("UPDATE content_store SET approved_at='2000-01-01Z' WHERE digest='older'")
            .execute(&db.admin)
            .await
            .unwrap();
        seed(&db, "newest", "template", Some("finite-v1")).await;
        seed(&db, "other", "template", Some("finite-v2")).await;
        let (_, members) = template_review_context(&db.app, KP, current(Some("finite-v1")), None)
            .await
            .unwrap();
        assert_eq!(members, ["newest"]);
        let (_, other) = template_review_context(&db.app, KP, current(Some("finite-v2")), None)
            .await
            .unwrap();
        assert_eq!(other, ["other"]);
        assert_eq!(
            template_review_context(&db.app, KP, current(None), None)
                .await
                .unwrap(),
            (None, vec![]),
        );
    })
    .await;
}

#[tokio::test]
async fn canonical_only_instruction_expires_when_a_template_bank_appears() {
    TestDb::with(|db| async move {
        seed(&db, "teach", "teach", None).await;
        assert!(
            approved_document_current(&db.app, KP, "teach", current(None))
                .await
                .unwrap()
                .is_some()
        );
        seed(&db, "template", "template", None).await;
        assert!(
            approved_document_current(&db.app, KP, "teach", current(None))
                .await
                .unwrap()
                .is_none()
        );
    })
    .await;
}
