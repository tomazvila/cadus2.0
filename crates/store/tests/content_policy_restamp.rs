//! The boot restamp moves the policy stamp of approved teach rows only.
#![allow(clippy::unwrap_used)]

use cadus_store::content::{Admin, Restamped, policy_drift, restamp_content_currency};
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};

const ENGINE: &str = "engine-v2";
const CURRICULUM: &str = "curriculum-v2";

async fn seed(db: &TestDb, digest: &str, kp: &str, kind: &str, policy: Option<&str>) {
    sqlx::query(
        "INSERT INTO content_store (digest,kp_id,kind,body,status,approved_policy_digest,\
         approved_curriculum_digest,approved_review_engine_digest) \
         VALUES ($1,$2,$3,'{}','approved',$4,$5,$6)",
    )
    .bind(digest)
    .bind(kp)
    .bind(kind)
    .bind(policy)
    .bind(CURRICULUM)
    .bind(ENGINE)
    .execute(&db.admin)
    .await
    .unwrap();
}

async fn stamp(db: &TestDb, digest: &str) -> Option<String> {
    sqlx::query_scalar("SELECT approved_policy_digest FROM content_store WHERE digest = $1")
        .bind(digest)
        .fetch_one(&db.admin)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_stale_teach_stamp_becomes_current_and_a_template_keeps_its_stamp() {
    TestDb::with(|db| async move {
        seed(&db, "teach-a", "t/kp1", "teach", Some("old")).await;
        seed(&db, "hint-a", "t/kp1", "hint_ladder", None).await;
        seed(&db, "tmpl-a", "t/kp1", "template", Some("old")).await;
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
        let live = vec![("t/kp1".to_string(), Some("new".to_string()))];
        assert_eq!(policy_drift(&handle, &live).await.unwrap().len(), 2);
        let counts = restamp_content_currency(Admin::new(&handle), ENGINE, CURRICULUM, &live)
            .await
            .unwrap();
        assert_eq!(
            counts,
            Restamped {
                currency: 0,
                policy: 2
            }
        );
        assert_eq!(stamp(&db, "teach-a").await.as_deref(), Some("new"));
        assert_eq!(stamp(&db, "hint-a").await.as_deref(), Some("new"));
        assert_eq!(stamp(&db, "tmpl-a").await.as_deref(), Some("old"));
        assert!(policy_drift(&handle, &live).await.unwrap().is_empty());
    })
    .await;
}

#[tokio::test]
async fn a_kp_that_lost_its_finite_domain_gets_a_null_stamp() {
    TestDb::with(|db| async move {
        seed(&db, "teach-b", "t/kp2", "teach", Some("old")).await;
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
        let live = vec![("t/kp2".to_string(), None)];
        let counts = restamp_content_currency(Admin::new(&handle), ENGINE, CURRICULUM, &live)
            .await
            .unwrap();
        assert_eq!(counts.policy, 1);
        assert_eq!(stamp(&db, "teach-b").await, None);
        let again = restamp_content_currency(Admin::new(&handle), ENGINE, CURRICULUM, &live)
            .await
            .unwrap();
        assert_eq!(again, Restamped::default());
    })
    .await;
}
