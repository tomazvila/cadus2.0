//! A finite template replacement preserves history and refuses changed cases.
#![allow(clippy::unwrap_used)]

use cadus_core::pool::{PoolAnswer, PoolProblem, Source};
use cadus_store::pool::{GenerationContext, NewFiniteInstance, NewInstance, insert_finite_tx};
use cadus_store::{begin_tenant, test_support::TestDb};

const CURRICULUM: &str = "finite-test-curriculum";
const ENGINE: &str = "finite-test-engine";

fn generation_context() -> GenerationContext {
    GenerationContext {
        curriculum_digest: CURRICULUM.to_owned(),
        review_engine_digest: ENGINE.to_owned(),
    }
}

fn instance(digest: &str, answer: &str, case: &str) -> NewFiniteInstance {
    NewFiniteInstance {
        case_id: case.to_owned(),
        instance: NewInstance {
            source: Source::Template,
            content_digest: Some(digest.to_owned()),
            generation_context: Some(generation_context()),
            problem: PoolProblem {
                v: 1,
                text: "Compute 2+3.".to_owned(),
                bindings: Default::default(),
                seed: 7,
            },
            expected_answer: PoolAnswer {
                v: 1,
                answer: answer.to_owned(),
                answer_contract: None,
            },
            instance_hash: "same-problem-hash".to_owned(),
        },
    }
}

async fn documents(db: &TestDb) {
    sqlx::query!("INSERT INTO content_store (digest, kp_id, kind, body) VALUES ('old', 'finite/kp1', 'template', '{}'), ('new', 'finite/kp1', 'template', '{}'), ('wrong-answer', 'finite/kp1', 'template', '{}'), ('wrong-case', 'finite/kp1', 'template', '{}')").execute(&db.admin).await.unwrap();
}

#[tokio::test]
async fn a_replacement_keeps_row_identity_and_handoff_but_updates_bindings() {
    TestDb::with(|db| async move {
        documents(&db).await;
        let user = db.seed_user("finite-rebind@example.test").await;
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        insert_finite_tx(&mut tx, user, "finite/kp1", &[instance("old", "5", "addition")]).await.unwrap();
        tx.commit().await.unwrap();
        let before = sqlx::query!(
            "UPDATE serving_pool SET claimed_at = '2000-01-01Z' WHERE user_id = $1 RETURNING id, claimed_at",
            user,
        ).fetch_one(&db.admin).await.unwrap();
        let mut replacement = instance("new", "5", "addition");
        replacement.instance.problem.bindings.insert("new_parameter".to_owned(), "3".to_owned());
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        assert_eq!(insert_finite_tx(&mut tx, user, "finite/kp1", &[replacement]).await.unwrap(), 1);
        tx.commit().await.unwrap();
        let after = sqlx::query!(
            "SELECT id, claimed_at, content_digest, finite_case_id, problem FROM serving_pool WHERE user_id = $1",
            user,
        ).fetch_one(&db.admin).await.unwrap();
        assert_eq!(before.id, after.id);
        assert_eq!(before.claimed_at, after.claimed_at);
        assert_eq!(after.content_digest.as_deref(), Some("new"));
        assert_eq!(after.finite_case_id.as_deref(), Some("addition"));
        assert_eq!(after.problem["bindings"]["new_parameter"], "3");
        for bad in [instance("wrong-answer", "6", "addition"), instance("wrong-case", "5", "other-case")] {
            let mut tx = begin_tenant(&db.app, user).await.unwrap();
            let error = insert_finite_tx(&mut tx, user, "finite/kp1", &[bad]).await.unwrap_err();
            assert!(error.to_string().contains("finite_policy_replacement_conflict"));
            tx.rollback().await.unwrap();
        }
        let digest = sqlx::query_scalar!("SELECT content_digest FROM serving_pool WHERE user_id = $1", user).fetch_one(&db.admin).await.unwrap();
        assert_eq!(digest.as_deref(), Some("new"));
    }).await;
}

#[tokio::test]
async fn a_legacy_exemplar_can_be_adopted_only_with_the_exact_payload() {
    TestDb::with(|db| async move {
        documents(&db).await;
        let user = db.seed_user("finite-legacy@example.test").await;
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        insert_finite_tx(&mut tx, user, "finite/kp1", &[instance("old", "5", "addition")]).await.unwrap();
        tx.commit().await.unwrap();
        sqlx::query!("UPDATE serving_pool SET source = 'exemplar', content_digest = NULL, finite_case_id = NULL, claimed_at = '2000-01-01Z' WHERE user_id = $1", user).execute(&db.admin).await.unwrap();
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        let mut mismatched = instance("new", "5", "addition");
        mismatched.instance.problem.text = "Different statement with the same stored hash".to_owned();
        assert!(insert_finite_tx(&mut tx, user, "finite/kp1", &[mismatched]).await.is_err());
        tx.rollback().await.unwrap();
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        insert_finite_tx(&mut tx, user, "finite/kp1", &[instance("new", "5", "addition")]).await.unwrap();
        tx.commit().await.unwrap();
        let row = sqlx::query!("SELECT source, finite_case_id, claimed_at FROM serving_pool WHERE user_id = $1", user).fetch_one(&db.admin).await.unwrap();
        assert_eq!(row.source, "template");
        assert_eq!(row.finite_case_id.as_deref(), Some("addition"));
        assert_eq!(row.claimed_at.unwrap().timestamp(), 946684800);
    }).await;
}

#[tokio::test]
async fn atomic_insert_refuses_revoked_wrong_policy_and_superseded_sources() {
    use cadus_store::pool::{
        FiniteEligibility, finite_approval_current_tx, finite_pool_complete_tx,
        insert_current_finite_tx,
    };
    TestDb::with(|db| async move {
        documents(&db).await;
        let user = db.seed_user("finite-atomic@example.test").await;
        sqlx::query("UPDATE content_store SET status = 'approved', approved_policy_digest = 'v1', approved_curriculum_digest=$1, approved_review_engine_digest=$2, approved_at = '2000-01-01Z' WHERE digest = 'old'").bind(CURRICULUM).bind(ENGINE).execute(&db.admin).await.unwrap();
        let hashes = vec!["same-problem-hash".to_owned()];
        let cases = vec!["addition".to_owned()];
        let context = generation_context();
        let mut eligibility = FiniteEligibility { content_digest:"old", policy_digest:"v1", allowed_instance_hashes:&hashes, allowed_case_ids:&cases, generation_context:&context };
        let mut tx = begin_tenant(&db.app,user).await.unwrap();
        assert!(finite_approval_current_tx(&mut tx,"finite/kp1",&eligibility).await.unwrap());
        assert_eq!(insert_current_finite_tx(&mut tx,user,"finite/kp1",&eligibility,&[instance("old","5","addition")]).await.unwrap(),Some(1));
        tx.commit().await.unwrap();
        sqlx::query("UPDATE content_store SET status = 'rejected' WHERE digest = 'old'").execute(&db.admin).await.unwrap();
        let mut tx = begin_tenant(&db.app,user).await.unwrap();
        assert!(!finite_approval_current_tx(&mut tx,"finite/kp1",&eligibility).await.unwrap());
        // Stored set completeness does not assert current approval.
        assert!(finite_pool_complete_tx(&mut tx,user,"finite/kp1",&eligibility).await.unwrap());
        assert_eq!(insert_current_finite_tx(&mut tx,user,"finite/kp1",&eligibility,&[instance("old","5","addition")]).await.unwrap(),None);
        tx.commit().await.unwrap();
        sqlx::query("UPDATE content_store SET status = 'approved', approved_policy_digest = 'v1', approved_curriculum_digest=$1, approved_review_engine_digest=$2, approved_at = CASE WHEN digest = 'new' THEN now() ELSE '2000-01-01Z' END WHERE digest IN ('old','new')").bind(CURRICULUM).bind(ENGINE).execute(&db.admin).await.unwrap();
        let mut tx = begin_tenant(&db.app,user).await.unwrap();
        assert_eq!(insert_current_finite_tx(&mut tx,user,"finite/kp1",&eligibility,&[instance("old","5","addition")]).await.unwrap(),None);
        eligibility.content_digest="new";
        eligibility.policy_digest="wrong";
        assert_eq!(insert_current_finite_tx(&mut tx,user,"finite/kp1",&eligibility,&[instance("new","5","addition")]).await.unwrap(),None);
        eligibility.policy_digest="v1";
        assert_eq!(insert_current_finite_tx(&mut tx,user,"finite/kp1",&eligibility,&[instance("new","5","addition")]).await.unwrap(),Some(1));
        tx.commit().await.unwrap();
        let digest: String = sqlx::query_scalar("SELECT content_digest FROM serving_pool WHERE user_id = $1").bind(user).fetch_one(&db.admin).await.unwrap();
        assert_eq!(digest,"new");
    }).await;
}

#[tokio::test]
async fn atomic_insert_requires_every_verified_case_exactly_once() {
    use cadus_store::pool::{FiniteEligibility, insert_current_finite_tx};
    TestDb::with(|db| async move {
        documents(&db).await;
        let user = db.seed_user("finite-bijection@example.test").await;
        let hashes = vec!["same-problem-hash".to_owned(), "second-hash".to_owned()];
        let cases = vec!["addition".to_owned(), "second-case".to_owned()];
        let context = generation_context();
        let eligibility = FiniteEligibility {
            content_digest: "old",
            policy_digest: "v1",
            allowed_instance_hashes: &hashes,
            allowed_case_ids: &cases,
            generation_context: &context,
        };
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        for rows in [
            vec![],
            vec![instance("old", "5", "addition")],
            vec![
                instance("old", "5", "addition"),
                instance("old", "5", "addition"),
            ],
        ] {
            assert!(
                insert_current_finite_tx(&mut tx, user, "finite/kp1", &eligibility, &rows)
                    .await
                    .is_err()
            );
        }
        let duplicate_hashes = vec!["same-problem-hash".to_owned(); 2];
        let duplicate_cases = vec!["addition".to_owned(); 2];
        let duplicate = FiniteEligibility {
            allowed_instance_hashes: &duplicate_hashes,
            allowed_case_ids: &duplicate_cases,
            ..eligibility
        };
        assert!(
            insert_current_finite_tx(
                &mut tx,
                user,
                "finite/kp1",
                &duplicate,
                &[instance("old", "5", "addition")]
            )
            .await
            .is_err()
        );
        tx.rollback().await.unwrap();
        let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM serving_pool WHERE user_id = $1")
            .bind(user)
            .fetch_one(&db.admin)
            .await
            .unwrap();
        assert_eq!(rows, 0);
    })
    .await;
}

#[tokio::test]
async fn one_conflicting_case_rolls_back_the_entire_batch_even_if_caller_continues() {
    TestDb::with(|db| async move {
        documents(&db).await;
        let user = db.seed_user("finite-batch-rollback@example.test").await;
        let first = instance("old", "5", "addition");
        let mut second = instance("old", "5", "second");
        second.instance.instance_hash = "second-hash".into();
        second.instance.problem.text = "Compute 1+4.".into();
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        insert_finite_tx(&mut tx, user, "finite/kp1", &[first, second])
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let first = instance("new", "5", "addition");
        let mut bad = instance("new", "6", "second");
        bad.instance.instance_hash = "second-hash".into();
        bad.instance.problem.text = "Compute 1+4.".into();
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        assert!(
            insert_finite_tx(&mut tx, user, "finite/kp1", &[first, bad])
                .await
                .is_err()
        );
        // The answer route may preserve the preceding attempt and continue.
        tx.commit().await.unwrap();
        let digests: Vec<String> = sqlx::query_scalar(
            "SELECT content_digest FROM serving_pool WHERE user_id = $1 ORDER BY instance_hash",
        )
        .bind(user)
        .fetch_all(&db.admin)
        .await
        .unwrap();
        assert_eq!(digests, vec!["old", "old"]);
    })
    .await;
}
