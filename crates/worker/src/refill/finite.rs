//! Complete finite-case materialization for the refill worker.

use cadus_core::curriculum::FiniteObjectiveDomain;
use cadus_core::pool::{PoolAnswer, PoolProblem, ProblemSource, Source, TemplateSource};
use cadus_core::template::{FiniteGateSpec, Instance};
use cadus_store::Db;
use cadus_store::pool::{
    FiniteEligibility, GenerationContext, NewFiniteInstance, NewInstance, PoolTarget,
};

use super::target::{Filled, report_refusals};
use crate::WorkerError;

fn refill_error(err: impl std::fmt::Display) -> WorkerError {
    WorkerError::Refill(err.to_string())
}

/// Materialize the complete reviewed practice set once, then rotate it forever.
pub(super) async fn fill_finite(
    db: &Db,
    target: &PoolTarget,
    digest: &str,
    source: TemplateSource<'_>,
    policy: &FiniteObjectiveDomain,
    seed: u64,
    generation_context: &GenerationContext,
) -> Result<Filled, WorkerError> {
    let gate = FiniteGateSpec::new(&target.kp_id, policy).map_err(refill_error)?;
    let wanted = policy
        .cases
        .iter()
        .filter(|case| case.role.is_practice())
        .count();
    let source = source
        .with_finite_policy(&target.kp_id, policy)
        .map_err(refill_error)?;
    let batch = source
        .fill(&target.kp_id, wanted, seed)
        .map_err(refill_error)?;
    let flagged = report_refusals(&target.kp_id, Some(digest), &batch);
    let refused = u64::try_from(batch.refusals().len()).unwrap_or(u64::MAX);
    let rows = finite_rows(
        batch.instances(),
        digest,
        seed,
        &gate,
        wanted,
        generation_context,
    )?;
    let hashes: Vec<String> = rows
        .iter()
        .map(|row| row.instance.instance_hash.clone())
        .collect();
    let case_ids: Vec<String> = rows.iter().map(|row| row.case_id.clone()).collect();
    let eligibility = FiniteEligibility {
        content_digest: digest,
        policy_digest: gate.policy_fingerprint(),
        generation_context,
        allowed_instance_hashes: &hashes,
        allowed_case_ids: &case_ids,
    };
    let mut tx = db.pool().begin().await?;
    if !cadus_store::pool::finite_approval_current_tx(&mut tx, &target.kp_id, &eligibility).await? {
        tx.commit().await?;
        return Ok(Filled::NoSource);
    }
    if cadus_store::pool::finite_pool_complete_tx(
        &mut tx,
        target.user_id,
        &target.kp_id,
        &eligibility,
    )
    .await?
    {
        tx.commit().await?;
        return Ok(Filled::RotationalComplete);
    }
    let Some(inserted) = cadus_store::pool::insert_current_finite_tx(
        &mut tx,
        target.user_id,
        &target.kp_id,
        &eligibility,
        &rows,
    )
    .await?
    else {
        tx.commit().await?;
        return Ok(Filled::NoSource);
    };
    tx.commit().await?;
    Ok(Filled::Rows {
        source: Source::Template,
        inserted,
        refused,
        flagged,
    })
}

fn finite_rows(
    instances: &[Instance],
    digest: &str,
    seed: u64,
    gate: &FiniteGateSpec<'_>,
    wanted: usize,
    generation_context: &GenerationContext,
) -> Result<Vec<NewFiniteInstance>, WorkerError> {
    let mut cases = std::collections::BTreeSet::new();
    let mut rows = Vec::with_capacity(instances.len());
    for instance in instances {
        let evidence = gate
            .match_practice_instance(instance)
            .map_err(|rejection| refill_error(rejection.message))?;
        if !cases.insert(evidence.case_id.clone()) {
            return Err(refill_error("finite source repeated one semantic case"));
        }
        rows.push(NewFiniteInstance {
            case_id: evidence.case_id,
            instance: new_instance(instance, Source::Template, Some(digest), seed)
                .with_generation_context(generation_context),
        });
    }
    if rows.len() != wanted {
        return Err(refill_error(
            "finite source did not materialize every practice case",
        ));
    }
    Ok(rows)
}

pub(super) fn new_instance(
    instance: &Instance,
    source: Source,
    digest: Option<&str>,
    seed: u64,
) -> NewInstance {
    NewInstance {
        source,
        content_digest: digest.map(str::to_string),
        generation_context: None,
        problem: PoolProblem::from_instance(instance, seed),
        expected_answer: PoolAnswer::from_instance(instance),
        instance_hash: instance.instance_hash.clone(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use cadus_core::curriculum::{
        FiniteCaseRole, FiniteCaseVariant, FiniteObjectiveCase, FiniteObjectiveDomain, Slug,
    };
    use cadus_core::pool::TemplateSource;
    use cadus_core::template::from_body;
    use cadus_store::Db;
    use cadus_store::pool::PoolTarget;
    use cadus_store::test_support::TestDb;

    use super::*;

    const KP: &str = "finite-index/kp1";
    const DIGEST: &str = "finite-template-v1";
    const CURRICULUM: &str = "test-curriculum";
    const ENGINE: &str = "test-engine";
    const BODY: &str = r#"{
        "v":1,
        "topic_id":"finite-index",
        "answer_kind":"numeric",
        "statement":"Return {a}.",
        "params":{"a":{"kind":"choice","values":[1,2,3]}},
        "constraints":[],
        "answer_expr":"a",
        "solution_sketch":"Return the displayed index.",
        "hints":["Read the displayed index."],
        "samples":[
            {"params":{"a":1},"expected":"1"},
            {"params":{"a":2},"expected":"2"},
            {"params":{"a":3},"expected":"3"}
        ]
    }"#;

    fn policy() -> FiniteObjectiveDomain {
        FiniteObjectiveDomain {
            schema_version: 1,
            review_ref: "sha256:finite-refill-review".to_owned(),
            cases: (1..=3)
                .map(|value| FiniteObjectiveCase {
                    id: Slug::new(format!("case-{value}")).unwrap(),
                    role: FiniteCaseRole::PracticeFresh,
                    variants: vec![FiniteCaseVariant {
                        problem: format!("Return {value}."),
                        answer: value.to_string(),
                        answer_contract: None,
                    }],
                })
                .collect(),
        }
    }

    fn generation_context() -> GenerationContext {
        GenerationContext {
            curriculum_digest: CURRICULUM.to_owned(),
            review_engine_digest: ENGINE.to_owned(),
        }
    }

    #[tokio::test]
    async fn complete_finite_set_is_inserted_once_and_claimed_rows_still_rotate() {
        TestDb::with(|db| async move {
            let user = db.seed_user("finite-refill@example.test").await;
            let policy = policy();
            let fingerprint = policy.fingerprint(KP).unwrap();
            sqlx::query(
                "INSERT INTO content_store
                    (digest, kp_id, kind, body, status, approved_at, approved_policy_digest,
                     approved_curriculum_digest, approved_review_engine_digest)
                 VALUES ($1, $2, 'template', $3::text::jsonb, 'approved', now(), $4, $5, $6)",
            )
            .bind(DIGEST)
            .bind(KP)
            .bind(BODY)
            .bind(&fingerprint)
            .bind(CURRICULUM)
            .bind(ENGINE)
            .execute(&db.admin)
            .await
            .unwrap();
            let doc = from_body(BODY).unwrap();
            let target = PoolTarget {
                user_id: user,
                kp_id: KP.to_owned(),
                depth: 0,
            };
            let handle = Db::new(db.admin.clone(), 100);
            let source = TemplateSource::new(KP, &doc).unwrap().with_digest(DIGEST);
            let context = generation_context();
            let first = fill_finite(&handle, &target, DIGEST, source, &policy, 7, &context)
                .await
                .unwrap();
            assert!(matches!(first, Filled::Rows { inserted: 3, .. }));
            let rows: Vec<(String, Option<String>)> = sqlx::query_as(
                "SELECT instance_hash, finite_case_id FROM serving_pool
                 WHERE user_id = $1 AND kp_id = $2 ORDER BY finite_case_id",
            )
            .bind(user)
            .bind(KP)
            .fetch_all(&db.admin)
            .await
            .unwrap();
            assert_eq!(rows.len(), 3);
            assert_eq!(
                rows.iter()
                    .filter_map(|row| row.1.as_deref())
                    .collect::<Vec<_>>(),
                vec!["case-1", "case-2", "case-3"]
            );
            sqlx::query("UPDATE serving_pool SET claimed_at = now() WHERE user_id = $1")
                .bind(user)
                .execute(&db.admin)
                .await
                .unwrap();
            let source = TemplateSource::new(KP, &doc).unwrap().with_digest(DIGEST);
            let second = fill_finite(&handle, &target, DIGEST, source, &policy, 11, &context)
                .await
                .unwrap();
            assert_eq!(second, Filled::RotationalComplete);

            sqlx::query(
                "INSERT INTO content_store
                    (digest, kp_id, kind, body, status, approved_at, approved_policy_digest,
                     approved_curriculum_digest, approved_review_engine_digest)
                 VALUES ($$finite-template-v2$$, $1, $$template$$, $2::text::jsonb,
                         $$approved$$, now() + interval $$1 second$$, $3, $4, $5)",
            )
            .bind(KP)
            .bind(BODY)
            .bind(&fingerprint)
            .bind(CURRICULUM)
            .bind(ENGINE)
            .execute(&db.admin)
            .await
            .unwrap();
            let source = TemplateSource::new(KP, &doc).unwrap().with_digest(DIGEST);
            let stale = fill_finite(&handle, &target, DIGEST, source, &policy, 13, &context)
                .await
                .unwrap();
            assert_eq!(stale, Filled::NoSource);
        })
        .await;
    }
}
