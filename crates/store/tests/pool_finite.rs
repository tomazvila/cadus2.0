//! Finite approved templates rotate without canonical fallback or stale rows.
#![allow(clippy::unwrap_used)]

use cadus_core::pool::{Avoid, Ring, TaskMemory};
use cadus_store::pool::{FiniteDraw, FiniteEligibility, GenerationContext, pop_finite_tx};
use cadus_store::{begin_tenant, test_support::TestDb};
use sqlx::types::chrono::{DateTime, Utc};
use uuid::Uuid;

const CURRICULUM: &str = "finite-test-curriculum";
const ENGINE: &str = "finite-test-engine";

fn generation_context() -> GenerationContext {
    GenerationContext {
        curriculum_digest: CURRICULUM.to_owned(),
        review_engine_digest: ENGINE.to_owned(),
    }
}

async fn seed(db: &TestDb, user: Uuid, digest: &str, hash: &str, index: i64) {
    let at = DateTime::<Utc>::from_timestamp(946684800 + index, 0).unwrap();
    sqlx::query!(
        r#"INSERT INTO serving_pool (user_id, kp_id, source, content_digest, source_curriculum_digest, source_review_engine_digest, problem, expected_answer, instance_hash, created_at, finite_case_id)
         VALUES ($1, 'finite/kp1', 'template', $2, $3, $4, '{"v":1,"text":"Compute","seed":0}', '{"v":1,"answer":"1"}', $5, $6, $5)"#,
        user, digest, CURRICULUM, ENGINE, hash, at,
    ).execute(&db.admin).await.unwrap();
}

async fn document(db: &TestDb, digest: &str, policy: &str) {
    sqlx::query!(
        "INSERT INTO content_store (digest, kp_id, kind, body, status, approved_policy_digest, approved_curriculum_digest, approved_review_engine_digest, approved_at)
         VALUES ($1, 'finite/kp1', 'template', '{}', 'approved', $2, $3, $4, now())",
        digest, policy, CURRICULUM, ENGINE,
    ).execute(&db.admin).await.unwrap();
}

#[tokio::test]
async fn five_cases_support_twenty_handoffs_without_immediate_repeats() {
    TestDb::with(|db| async move {
        let user = db.seed_user("finite-rotation@example.test").await;
        document(&db, "current", "policy-v1").await;
        let hashes: Vec<String> = (0..5).map(|i| format!("case-{i}")).collect();
        for (i, hash) in hashes.iter().enumerate() {
            seed(&db, user, "current", hash, i as i64).await;
        }
        let context = generation_context();
        let eligibility = FiniteEligibility {
            content_digest: "current",
            policy_digest: "policy-v1",
            allowed_instance_hashes: &hashes,
            allowed_case_ids: &hashes,
            generation_context: &context,
        };
        let mut ring = Ring::new();
        let task = TaskMemory::new();
        let mut served = Vec::new();
        for turn in 0..20 {
            let mut tx = begin_tenant(&db.app, user).await.unwrap();
            let mode = if turn < 5 {
                FiniteDraw::Unclaimed
            } else {
                FiniteDraw::Repeat
            };
            let row = pop_finite_tx(
                &mut tx,
                user,
                "finite/kp1",
                &eligibility,
                &Avoid::new(&ring, &task),
                mode,
            )
            .await
            .unwrap()
            .claimed
            .unwrap()
            .row;
            assert_eq!(row.instance_hash, hashes[turn % 5]);
            ring.push(&row.instance_hash);
            served.push(row.id);
            tx.commit().await.unwrap();
        }
        assert!(served.windows(2).all(|pair| pair[0] != pair[1]));
        let unique: std::collections::HashSet<_> = served.into_iter().collect();
        assert_eq!(unique.len(), 5);
    })
    .await;
}

#[tokio::test]
async fn eligibility_rechecks_policy_hash_revocation_and_current_document() {
    TestDb::with(|db| async move {
        let user = db.seed_user("finite-eligibility@example.test").await;
        document(&db, "current", "policy-v1").await;
        seed(&db, user, "current", "allowed", 0).await;
        seed(&db, user, "current", "reserved", 1).await;
        let hashes = vec!["allowed".to_owned()];
        let context = generation_context();
        let mut eligibility = FiniteEligibility { content_digest: "current", policy_digest: "wrong-policy", allowed_instance_hashes: &hashes, allowed_case_ids: &hashes, generation_context: &context };
        let ring = Ring::new(); let task = TaskMemory::new(); let avoid = Avoid::new(&ring, &task);
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        assert!(pop_finite_tx(&mut tx, user, "finite/kp1", &eligibility, &avoid, FiniteDraw::Unclaimed).await.unwrap().claimed.is_none());
        eligibility.policy_digest = "policy-v1";
        assert_eq!(pop_finite_tx(&mut tx, user, "finite/kp1", &eligibility, &avoid, FiniteDraw::Unclaimed).await.unwrap().claimed.unwrap().row.instance_hash, "allowed");
        assert!(pop_finite_tx(&mut tx, user, "finite/kp1", &eligibility, &avoid, FiniteDraw::Unclaimed).await.unwrap().claimed.is_none());
        tx.commit().await.unwrap();
        sqlx::query!("UPDATE content_store SET status = 'rejected' WHERE digest = 'current'").execute(&db.admin).await.unwrap();
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        assert!(pop_finite_tx(&mut tx, user, "finite/kp1", &eligibility, &avoid, FiniteDraw::Repeat).await.unwrap().claimed.is_none());
        tx.commit().await.unwrap();
        sqlx::query!("UPDATE content_store SET status = 'approved', approved_at = '2000-01-01Z' WHERE digest = 'current'").execute(&db.admin).await.unwrap();
        document(&db, "replacement", "policy-v1").await;
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        assert!(pop_finite_tx(&mut tx, user, "finite/kp1", &eligibility, &avoid, FiniteDraw::Repeat).await.unwrap().claimed.is_none());
        tx.commit().await.unwrap();
    }).await;
}

#[tokio::test]
async fn concurrent_draws_skip_locked_rows_and_cannot_cross_tenants() {
    TestDb::with(|db| async move {
        let user = db.seed_user("finite-lock-a@example.test").await;
        let other = db.seed_user("finite-lock-b@example.test").await;
        document(&db, "current", "policy-v1").await;
        for owner in [user, other] {
            seed(&db, owner, "current", "first", 0).await;
            seed(&db, owner, "current", "second", 1).await;
        }
        let hashes = vec!["first".to_owned(), "second".to_owned()];
        let context = generation_context();
        let eligibility = FiniteEligibility { content_digest: "current", policy_digest: "policy-v1", allowed_instance_hashes: &hashes, allowed_case_ids: &hashes, generation_context: &context };
        let ring = Ring::new(); let task = TaskMemory::new(); let avoid = Avoid::new(&ring, &task);
        let mut first = begin_tenant(&db.app, user).await.unwrap();
        let mut second = begin_tenant(&db.app, user).await.unwrap();
        let a = pop_finite_tx(&mut first, user, "finite/kp1", &eligibility, &avoid, FiniteDraw::Unclaimed).await.unwrap().claimed.unwrap().row;
        // The first draw locks its bounded candidate window, including both rows.
        assert!(pop_finite_tx(&mut second, user, "finite/kp1", &eligibility, &avoid, FiniteDraw::Unclaimed).await.unwrap().claimed.is_none());
        assert!(pop_finite_tx(&mut first, other, "finite/kp1", &eligibility, &avoid, FiniteDraw::Unclaimed).await.unwrap().claimed.is_none());
        first.commit().await.unwrap();
        let b = pop_finite_tx(&mut second, user, "finite/kp1", &eligibility, &avoid, FiniteDraw::Unclaimed).await.unwrap().claimed.unwrap().row;
        assert_ne!(a.id, b.id);
        assert_eq!(a.instance_hash, "first");
        assert_eq!(b.instance_hash, "second");
        second.commit().await.unwrap();
        let count = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM serving_pool WHERE user_id = $1 AND claimed_at IS NULL"#, other).fetch_one(&db.admin).await.unwrap();
        assert_eq!(count, 2);
    }).await;
}

#[tokio::test]
async fn completeness_counts_claimed_cases_and_requires_exact_current_pairs() {
    use cadus_store::pool::finite_pool_complete_tx;
    TestDb::with(|db| async move {
        let user = db.seed_user("finite-complete@example.test").await;
        let other = db.seed_user("finite-complete-other@example.test").await;
        document(&db, "current", "policy-v1").await;
        document(&db, "old", "policy-v0").await;
        seed(&db, user, "current", "first", 0).await;
        seed(&db, user, "old", "second", 1).await;
        let hashes = vec!["first".to_owned(), "second".to_owned()];
        let context = generation_context();
        let eligibility = FiniteEligibility { content_digest: "current", policy_digest: "policy-v1", allowed_instance_hashes: &hashes, allowed_case_ids: &hashes, generation_context: &context };
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        assert!(!finite_pool_complete_tx(&mut tx, user, "finite/kp1", &eligibility).await.unwrap());
        tx.commit().await.unwrap();
        sqlx::query("UPDATE serving_pool SET content_digest = 'current', claimed_at = now() WHERE user_id = $1").bind(user).execute(&db.admin).await.unwrap();
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        assert!(finite_pool_complete_tx(&mut tx, user, "finite/kp1", &eligibility).await.unwrap());
        assert!(!finite_pool_complete_tx(&mut tx, other, "finite/kp1", &eligibility).await.unwrap());
        assert!(!finite_pool_complete_tx(&mut tx, user, "other/kp1", &eligibility).await.unwrap());
        let mismatched = FiniteEligibility { allowed_case_ids: &hashes[..1], ..eligibility };
        assert!(!finite_pool_complete_tx(&mut tx, user, "finite/kp1", &mismatched).await.unwrap());
        let reversed = vec!["second".to_owned(), "first".to_owned()];
        let wrong_pairs = FiniteEligibility { allowed_case_ids: &reversed, ..eligibility };
        assert!(!finite_pool_complete_tx(&mut tx, user, "finite/kp1", &wrong_pairs).await.unwrap());
        let empty = FiniteEligibility { allowed_instance_hashes: &[], allowed_case_ids: &[], ..eligibility };
        assert!(!finite_pool_complete_tx(&mut tx, user, "finite/kp1", &empty).await.unwrap());
        tx.commit().await.unwrap();
    }).await;
}
