//! Completeness of an approved finite pool across claimed and unclaimed rows.
use super::FiniteEligibility;
use crate::StoreError;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

/// Whether every currently verified case already exists under this digest.
///
/// Claimed rows count: a complete finite set rotates without refill retries.
///
/// # Errors
/// Returns a database error if the read fails.
pub async fn finite_pool_complete_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    kp_id: &str,
    eligibility: &FiniteEligibility<'_>,
) -> Result<bool, StoreError> {
    if eligibility.allowed_instance_hashes.is_empty()
        || eligibility.allowed_instance_hashes.len() != eligibility.allowed_case_ids.len()
    {
        return Ok(false);
    }
    let complete = sqlx::query_scalar!(
        r#"
        SELECT NOT EXISTS (
            SELECT 1 FROM unnest($6::text[], $7::text[]) AS wanted(hash, case_id)
            WHERE NOT EXISTS (
                SELECT 1 FROM serving_pool AS sp
                WHERE sp.user_id = $1 AND sp.kp_id = $2
                  AND sp.source = 'template' AND sp.content_digest = $3
                  AND sp.source_curriculum_digest = $4
                  AND sp.source_review_engine_digest = $5
                  AND sp.instance_hash = wanted.hash AND sp.finite_case_id = wanted.case_id
            )
        ) AS "complete!"
        "#,
        user_id,
        kp_id,
        eligibility.content_digest,
        eligibility.generation_context.curriculum_digest,
        eligibility.generation_context.review_engine_digest,
        eligibility.allowed_instance_hashes,
        eligibility.allowed_case_ids,
    )
    .fetch_one(&mut **tx)
    .await?;
    Ok(complete)
}

/// Check whether the chosen digest is still the newest approval for this policy.
///
/// Completeness is a structural pool property; this separate result prevents an
/// ineligible source from being reported as rotationally complete. Insertion
/// checks approval atomically again, and pop rechecks before handoff.
///
/// # Errors
/// Returns a database read error.
pub async fn finite_approval_current_tx(
    tx: &mut Transaction<'_, Postgres>,
    kp_id: &str,
    eligibility: &FiniteEligibility<'_>,
) -> Result<bool, StoreError> {
    let approved = super::approved_template_current(
        &mut **tx,
        kp_id,
        crate::content::CurrentContext {
            policy_digest: Some(eligibility.policy_digest),
            curriculum_digest: &eligibility.generation_context.curriculum_digest,
            review_engine_digest: &eligibility.generation_context.review_engine_digest,
        },
    )
    .await?;
    Ok(approved.is_some_and(|row| row.digest == eligibility.content_digest))
}
