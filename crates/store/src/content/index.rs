//! The approved-document index the readiness audit reads (D-F5).
//!
//! `cadus_core::readiness` names no table and opens no connection (R3), so it
//! asks a [`ContentIndex`](cadus_core::readiness::ContentIndex) instead. This is
//! that index over `content_store`.
//!
//! # One statement, and why the whole table
//!
//! The read is ONE grouped statement over the approved rows. It reads the whole
//! table on purpose: the audit answers a question about every knowledge point
//! of the plan, and the plan is composed before the caller knows which
//! knowledge points it holds. The row count is the count of documents a reviewer
//! approved (C6), which is zero on a fresh deployment (audit finding h) and
//! four per knowledge point when a course is fully authored.
//!
//! The statement carries no tenant binding, because `content_store` holds
//! curriculum content and not learner data: it stands outside row-level
//! security and the runtime role holds SELECT on it
//! (`0006_grants_rls.sql`, `docs/SCHEMA.md` finding #14).

use cadus_core::readiness::MapContent;
use sqlx::PgExecutor;

use crate::StoreError;

/// The approved documents of every knowledge point, by kind.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn approved_index<'e, E>(executor: E) -> Result<MapContent, StoreError>
where
    E: PgExecutor<'e>,
{
    read_index(executor, &serde_json::json!({}), None).await
}

/// Read approvals valid under every currently loaded finite-objective policy.
///
/// # Errors
/// Returns configuration errors for an invalid policy and database read errors.
pub async fn approved_index_current<'e, E>(
    executor: E,
    curriculum: &cadus_core::curriculum::Curriculum,
    curriculum_digest: &str,
    review_engine_digest: &str,
) -> Result<MapContent, StoreError>
where
    E: PgExecutor<'e>,
{
    let mut policies = std::collections::BTreeMap::new();
    for topic in curriculum.topics() {
        for kp in &topic.knowledge_points {
            if let Some(policy) = &kp.finite_objective_domain {
                let key = format!("{}/{}", topic.id, kp.id);
                let digest = policy.fingerprint(&key).map_err(StoreError::Config)?;
                policies.insert(key, digest);
            }
        }
    }
    read_index(
        executor,
        &serde_json::json!(policies),
        Some((curriculum_digest, review_engine_digest)),
    )
    .await
}

async fn read_index<'e, E>(
    executor: E,
    policies: &serde_json::Value,
    effective: Option<(&str, &str)>,
) -> Result<MapContent, StoreError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query!(
        r#"
        SELECT kp_id AS "kp_id!", kind AS "kind!", count(*) AS "documents!"
        FROM content_store
        WHERE status = 'approved'
          AND approved_policy_digest IS NOT DISTINCT FROM ($1::jsonb ->> kp_id)
          AND ($2::text IS NULL OR approved_curriculum_digest = $2)
          AND ($3::text IS NULL OR approved_review_engine_digest = $3)
          AND (kind NOT IN ('teach','hint_ladder') OR
               approved_template_context_digest IS NOT DISTINCT FROM
               public.cadus_template_context(
                   kp_id, $1::jsonb ->> kp_id, $2, $3
               ))
        GROUP BY kp_id, kind
        "#,
        policies,
        effective.map(|pair| pair.0),
        effective.map(|pair| pair.1),
    )
    .fetch_all(executor)
    .await?;

    let mut index = MapContent::default();
    for row in rows {
        index.insert(
            row.kp_id,
            row.kind,
            usize::try_from(row.documents).unwrap_or(0),
        );
    }
    Ok(index)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::read_index;
    use crate::test_support::TestDb;

    #[tokio::test]
    async fn readiness_counts_only_current_policy_approvals_for_every_kind() {
        TestDb::with(|db| async move {
            for kind in ["template", "teach", "hint_ladder"] {
                for (suffix, policy, status) in [
                    ("current", Some("v2"), "approved"),
                    ("stale", Some("v1"), "approved"),
                    ("legacy", None, "approved"),
                    ("pending", Some("v2"), "pending"),
                    ("rejected", Some("v2"), "rejected"),
                ] {
                    sqlx::query("INSERT INTO content_store (digest,kp_id,kind,body,status,approved_policy_digest,approved_curriculum_digest,approved_review_engine_digest) VALUES ($1,'finite/kp1',$2,'{}',$3,$4,'curriculum-v1','engine-v1')")
                        .bind(format!("{kind}-{suffix}")).bind(kind).bind(status).bind(policy)
                        .execute(&db.admin).await.unwrap();
                }
            }
            sqlx::query("UPDATE content_store SET approved_template_context_digest = public.cadus_template_context(kp_id, approved_policy_digest, approved_curriculum_digest, approved_review_engine_digest) WHERE kind IN ('teach','hint_ladder')")
                .execute(&db.admin).await.unwrap();
            let current = read_index(
                &db.app,
                &serde_json::json!({"finite/kp1":"v2"}),
                Some(("curriculum-v1", "engine-v1")),
            )
            .await
            .unwrap();
            let changed = read_index(
                &db.app,
                &serde_json::json!({"finite/kp1":"v3"}),
                Some(("curriculum-v1", "engine-v1")),
            )
            .await
            .unwrap();
            let ordinary = read_index(
                &db.app,
                &serde_json::json!({}),
                Some(("curriculum-v1", "engine-v1")),
            )
            .await
            .unwrap();
            for kind in ["template", "teach", "hint_ladder"] {
                assert_eq!(current.count("finite/kp1", kind), 1);
                assert_eq!(changed.count("finite/kp1", kind), 0);
                assert_eq!(ordinary.count("finite/kp1", kind), 1);
            }
        }).await;
    }
}
