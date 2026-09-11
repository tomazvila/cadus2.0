//! Canonical, engine, and prospective-bank review contexts fail closed.
#![allow(clippy::unwrap_used)]

use cadus_store::content::{
    Admin, ApprovalContext, CurrentContext, approve_current, approved_document_current,
    approved_document_for_source, template_review_context,
};
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};

const KP: &str = "effective-context/kp1";
const CURRICULUM: &str = "curriculum-v1";
const ENGINE: &str = "engine-v1";

fn current<'a>() -> CurrentContext<'a> {
    CurrentContext {
        policy_digest: None,
        curriculum_digest: CURRICULUM,
        review_engine_digest: ENGINE,
    }
}

async fn seed(db: &TestDb, digest: &str, kind: &str, status: &str) {
    sqlx::query(
        "INSERT INTO content_store
         (digest,kp_id,kind,body,status,approved_at,approved_curriculum_digest,
          approved_review_engine_digest)
         VALUES ($1,$2,$3,'{}',$4,CASE WHEN $4='approved' THEN now() END,$5,$6)",
    )
    .bind(digest)
    .bind(KP)
    .bind(kind)
    .bind(status)
    .bind((status == "approved").then_some(CURRICULUM))
    .bind((status == "approved").then_some(ENGINE))
    .execute(&db.admin)
    .await
    .unwrap();
}

#[tokio::test]
async fn canonical_and_engine_changes_expire_approval_and_live_source_membership() {
    TestDb::with(|db| async move {
        seed(&db, "template", "template", "approved").await;
        seed(&db, "hint", "hint_ladder", "pending").await;
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
        let (bank, members) = template_review_context(&db.app, KP, current(), None)
            .await
            .unwrap();
        assert_eq!(members, ["template"]);
        approve_current(
            Admin::new(&handle),
            "hint",
            None,
            ApprovalContext {
                current: current(),
                template_context_digest: bank.as_deref(),
            },
        )
        .await
        .unwrap()
        .unwrap();
        assert!(
            approved_document_for_source(
                &db.app,
                KP,
                "hint_ladder",
                current(),
                Some("template"),
                Some((CURRICULUM, ENGINE)),
            )
            .await
            .unwrap()
            .is_some()
        );
        for stale in [
            CurrentContext {
                curriculum_digest: "curriculum-v2",
                ..current()
            },
            CurrentContext {
                review_engine_digest: "engine-v2",
                ..current()
            },
        ] {
            assert!(
                approved_document_current(&db.app, KP, "hint_ladder", stale)
                    .await
                    .unwrap()
                    .is_none()
            );
        }
        assert!(
            approved_document_for_source(
                &db.app,
                KP,
                "hint_ladder",
                current(),
                Some("template"),
                Some((CURRICULUM, "engine-v0")),
            )
            .await
            .unwrap()
            .is_none()
        );
        assert!(
            approved_document_for_source(
                &db.app,
                KP,
                "hint_ladder",
                current(),
                Some("template"),
                None,
            )
            .await
            .unwrap()
            .is_none(),
            "source-bound reads fail closed without generation context"
        );
    })
    .await;
}

#[tokio::test]
async fn concurrent_approvals_from_one_bank_snapshot_allow_exactly_one_candidate() {
    TestDb::with(|db| async move {
        seed(&db, "t1", "template", "approved").await;
        seed(&db, "t2", "template", "pending").await;
        seed(&db, "t3", "template", "pending").await;
        let (context2, members) = template_review_context(&db.admin, KP, current(), Some("t2"))
            .await
            .unwrap();
        assert_eq!(members, ["t1"]);
        let (context3, _) = template_review_context(&db.admin, KP, current(), Some("t3"))
            .await
            .unwrap();
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
        let first = approve_current(
            Admin::new(&handle),
            "t2",
            None,
            ApprovalContext {
                current: current(),
                template_context_digest: context2.as_deref(),
            },
        );
        let second = approve_current(
            Admin::new(&handle),
            "t3",
            None,
            ApprovalContext {
                current: current(),
                template_context_digest: context3.as_deref(),
            },
        );
        let (first, second) = tokio::join!(first, second);
        let successes = [first.unwrap(), second.unwrap()]
            .into_iter()
            .filter(Option::is_some)
            .count();
        assert_eq!(successes, 1);
        let (_, members) = template_review_context(&db.admin, KP, current(), None)
            .await
            .unwrap();
        assert_eq!(members.len(), 2);
        assert!(members.contains(&"t1".to_owned()));
    })
    .await;
}
