//! The policy health check lists a stale stamp and the boot restamp clears it.
#![allow(clippy::unwrap_used)]

mod common;

use cadus_store::content::{Admin, restamp_content_currency};
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::diag::{live_policy_map, stale_policy_rows};
use cadus_web::state::Content;

#[tokio::test]
async fn a_stale_policy_stamp_is_listed_until_the_restamp() {
    TestDb::with(|db| async move {
        let content = Content::new(common::admin::graph());
        let live = live_policy_map(&content).unwrap();
        let kp = live[0].0.clone();
        let engine = content.review_engine_digest();
        let curriculum = content.curriculum_context_digest().unwrap().to_string();
        sqlx::query(
            "INSERT INTO content_store (digest,kp_id,kind,body,status,approved_policy_digest,\
             approved_curriculum_digest,approved_review_engine_digest) \
             VALUES ('stale-teach',$1,'teach','{}','approved','stale',$2,$3)",
        )
        .bind(&kp)
        .bind(&curriculum)
        .bind(engine)
        .execute(&db.admin)
        .await
        .unwrap();
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
        let rows = stale_policy_rows(&handle, &content).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].digest, "stale-teach");
        restamp_content_currency(Admin::new(&handle), engine, &curriculum, &live)
            .await
            .unwrap();
        assert!(
            stale_policy_rows(&handle, &content)
                .await
                .unwrap()
                .is_empty()
        );
    })
    .await;
}
