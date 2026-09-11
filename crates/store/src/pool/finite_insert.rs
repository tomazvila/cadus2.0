//! Atomic adoption of a verified finite case set into a learner's pool.
use super::{FiniteEligibility, NewInstance};
use crate::StoreError;
use sqlx::{Acquire, Postgres, Transaction};
use uuid::Uuid;

/// One practice instance matched to a trusted semantic case by the core gate.
pub struct NewFiniteInstance {
    /// The approved template's rendered payload and body digest.
    pub instance: NewInstance,
    /// The stable curriculum-owned semantic case identifier.
    pub case_id: String,
}

/// Insert or rebind a gated set whose eligibility the caller establishes.
///
/// Production finite refill uses [`insert_current_finite_tx`] to bind approval
/// and all writes to one database snapshot. Rebinding preserves row identity
/// and prior handoff; it requires exact text, answer/contract and semantic case.
///
/// # Errors
/// Returns an invalid-input, conflicting-payload or database error. The caller
/// must roll back the transaction on any error.
pub async fn insert_finite_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    kp_id: &str,
    rows: &[NewFiniteInstance],
) -> Result<u64, StoreError> {
    Ok(insert_batch(tx, user_id, kp_id, None, rows)
        .await?
        .unwrap_or(0))
}

/// Atomically check current approval and insert/rebind its verified case set.
///
/// `None` means the selected template is no longer the newest approval under
/// this policy, and no pool rows were written. A later approval or revocation
/// is checked again by the pop query before any handoff.
///
/// # Errors
/// Returns an invalid-input, conflicting-payload or database error. The caller
/// must roll back the transaction on any error.
pub async fn insert_current_finite_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    kp_id: &str,
    eligibility: &FiniteEligibility<'_>,
    rows: &[NewFiniteInstance],
) -> Result<Option<u64>, StoreError> {
    validate_eligibility(eligibility, rows)?;
    insert_batch(tx, user_id, kp_id, Some(eligibility), rows).await
}

fn validate_eligibility(
    eligibility: &FiniteEligibility<'_>,
    rows: &[NewFiniteInstance],
) -> Result<(), StoreError> {
    use std::collections::BTreeSet;
    let expected: BTreeSet<_> = eligibility
        .allowed_instance_hashes
        .iter()
        .zip(eligibility.allowed_case_ids)
        .collect();
    let actual: BTreeSet<_> = rows
        .iter()
        .map(|row| (&row.instance.instance_hash, &row.case_id))
        .collect();
    if rows.is_empty()
        || eligibility.allowed_instance_hashes.len() != eligibility.allowed_case_ids.len()
        || expected.len() != eligibility.allowed_instance_hashes.len()
        || actual.len() != rows.len()
        || actual != expected
        || rows
            .iter()
            .any(|row| row.instance.content_digest.as_deref() != Some(eligibility.content_digest))
    {
        return Err(StoreError::PoolRow(
            "finite insert must match every verified case exactly once".into(),
        ));
    }
    Ok(())
}

fn input_rows(rows: &[NewFiniteInstance]) -> Result<serde_json::Value, StoreError> {
    rows.iter()
        .map(|row| {
            let instance = &row.instance;
            if row.case_id.is_empty()
                || instance.source != cadus_core::pool::Source::Template
                || instance.content_digest.is_none()
                || instance.generation_context.is_none()
            {
                return Err(StoreError::PoolRow(
                    "finite instance lacks a gated case or template digest".into(),
                ));
            }
            Ok(serde_json::json!({
                "digest":instance.content_digest, "problem":instance.problem,
                "expected":instance.expected_answer, "hash":instance.instance_hash,
                "case_id":row.case_id,
                "curriculum_digest":instance.generation_context.as_ref().map(|value| &value.curriculum_digest),
                "engine_digest":instance.generation_context.as_ref().map(|value| &value.review_engine_digest),
            }))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(serde_json::Value::Array)
}

async fn insert_batch(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    kp_id: &str,
    eligibility: Option<&FiniteEligibility<'_>>,
    rows: &[NewFiniteInstance],
) -> Result<Option<u64>, StoreError> {
    let inputs = input_rows(rows)?;
    let mut batch_tx = tx.begin().await?;
    let (eligible, written): (bool, i64) = sqlx::query_as(
        r#"
        WITH eligible AS (
            SELECT $3::text IS NULL OR $3 = (
                SELECT digest FROM content_store
                WHERE kp_id = $2 AND kind = 'template' AND status = 'approved'
                  AND approved_policy_digest = $4
                  AND approved_curriculum_digest = $6
                  AND approved_review_engine_digest = $7
                ORDER BY approved_at DESC NULLS LAST, created_at DESC, digest LIMIT 1
            ) AS current
        ), inserted AS (
            INSERT INTO serving_pool
                (user_id,kp_id,source,content_digest,source_curriculum_digest,
                 source_review_engine_digest,problem,expected_answer,instance_hash,finite_case_id)
            SELECT $1,$2,'template',candidate.digest,candidate.curriculum_digest,
                   candidate.engine_digest,candidate.problem,candidate.expected,candidate.hash,candidate.case_id
            FROM jsonb_to_recordset($5::jsonb) AS candidate(digest text,problem jsonb,
                 expected jsonb,hash text,case_id text,curriculum_digest text,engine_digest text)
            WHERE (SELECT current FROM eligible)
            ON CONFLICT (user_id,kp_id,instance_hash) DO UPDATE
            SET source = 'template', content_digest = EXCLUDED.content_digest,
                problem = EXCLUDED.problem, finite_case_id = EXCLUDED.finite_case_id
                , source_curriculum_digest = EXCLUDED.source_curriculum_digest
                , source_review_engine_digest = EXCLUDED.source_review_engine_digest
            WHERE serving_pool.source IN ('template','exemplar')
              AND serving_pool.problem->>'text' = EXCLUDED.problem->>'text'
              AND serving_pool.expected_answer = EXCLUDED.expected_answer
              AND (serving_pool.finite_case_id = EXCLUDED.finite_case_id OR serving_pool.finite_case_id IS NULL)
            RETURNING id
        )
        SELECT COALESCE((SELECT current FROM eligible),false),count(*) FROM inserted
        "#,
    ).bind(user_id).bind(kp_id)
        .bind(eligibility.map(|value| value.content_digest))
        .bind(eligibility.map(|value| value.policy_digest))
        .bind(inputs)
        .bind(eligibility.map(|value| value.generation_context.curriculum_digest.as_str()))
        .bind(eligibility.map(|value| value.generation_context.review_engine_digest.as_str()))
        .fetch_one(&mut *batch_tx).await?;
    if !eligible {
        batch_tx.rollback().await?;
        return Ok(None);
    }
    if usize::try_from(written).ok() != Some(rows.len()) {
        batch_tx.rollback().await?;
        return Err(StoreError::PoolRow(format!(
            "finite_policy_replacement_conflict: {kp_id}"
        )));
    }
    batch_tx.commit().await?;
    Ok(Some(u64::try_from(written).unwrap_or(0)))
}
