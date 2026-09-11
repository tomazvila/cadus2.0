//! Current-policy finite template draws and least-recently-served rotation.

use cadus_core::pool::{Avoid, pick};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use super::{RawRow, decode_rows};
use crate::StoreError;
use crate::pool::{Claimed, GenerationContext, POP_LIMIT, Pop};

/// Eligibility derived from the current approved document and trusted gate.
pub struct FiniteEligibility<'a> {
    /// Exact current approved template body digest.
    pub content_digest: &'a str,
    /// Current curriculum-owned policy fingerprint.
    pub policy_digest: &'a str,
    /// Exhaustively verified practice/rehearsal instances only.
    pub allowed_instance_hashes: &'a [String],
    /// Semantic case IDs paired in the same order as the instance hashes.
    pub allowed_case_ids: &'a [String],
    /// Trusted context that rendered and checked every allowed row.
    pub generation_context: &'a GenerationContext,
}

/// Which half of a finite pool the draw may use.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FiniteDraw {
    /// A previously unclaimed approved instance.
    Unclaimed,
    /// A previously claimed instance, ordered by least recent handoff.
    Repeat,
}

/// Draw an eligible finite template under the caller's tenant transaction.
///
/// The query rechecks approval and current-policy selection on every handoff.
/// An exhausted anti-repeat window chooses the oldest eligible candidate.
///
/// # Errors
/// Returns database errors or a pool error if a locked row vanishes.
pub async fn pop_finite_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    kp_id: &str,
    eligibility: &FiniteEligibility<'_>,
    avoid: &Avoid<'_>,
    mode: FiniteDraw,
) -> Result<Pop, StoreError> {
    if eligibility.allowed_instance_hashes.len() != eligibility.allowed_case_ids.len() {
        return Err(StoreError::PoolRow(
            "finite eligibility case/hash lengths differ".to_owned(),
        ));
    }
    let repeat = mode == FiniteDraw::Repeat;
    let rows = candidate_read!(
        r#"
        FROM serving_pool AS sp
        JOIN content_store AS cs ON cs.digest = sp.content_digest
        WHERE sp.user_id = $1 AND sp.kp_id = $2
          AND sp.source = 'template'
          AND (sp.claimed_at IS NOT NULL) = $3
          AND cs.digest = $4 AND cs.status = 'approved'
          AND cs.approved_policy_digest = $5
          AND cs.approved_curriculum_digest = $9
          AND cs.approved_review_engine_digest = $10
          AND sp.source_curriculum_digest = $9
          AND sp.source_review_engine_digest = $10
          AND (sp.instance_hash, sp.finite_case_id) IN (
              SELECT * FROM unnest($6::text[], $7::text[])
          )
          AND cs.digest = (
              SELECT digest FROM content_store
              WHERE kp_id = $2 AND kind = 'template' AND status = 'approved'
                AND approved_policy_digest = $5
                AND approved_curriculum_digest = $9
                AND approved_review_engine_digest = $10
              ORDER BY approved_at DESC NULLS LAST, created_at DESC, digest
              LIMIT 1
          )
        ORDER BY CASE WHEN $3 THEN sp.claimed_at ELSE sp.created_at END, sp.id
        FOR UPDATE OF sp SKIP LOCKED
        LIMIT $8
        "#,
        user_id,
        kp_id,
        repeat,
        eligibility.content_digest,
        eligibility.policy_digest,
        eligibility.allowed_instance_hashes,
        eligibility.allowed_case_ids,
        POP_LIMIT,
        eligibility.generation_context.curriculum_digest,
        eligibility.generation_context.review_engine_digest,
    )
    .fetch_all(&mut **tx)
    .await?;
    let (mut candidates, refused) = decode_rows(
        rows,
        user_id,
        kp_id,
        "pool: malformed finite template instance skipped",
    );
    let undecodable = refused.len();
    for id in refused {
        super::retire_row(tx, id).await?;
    }
    let Some(mut chosen) = pick(&candidates, avoid) else {
        return Ok(Pop {
            claimed: None,
            undecodable,
        });
    };
    if chosen.exhausted {
        chosen.index = 0;
    }
    let count = candidates.len();
    let row = candidates.swap_remove(chosen.index);
    let updated = sqlx::query!(
        "UPDATE serving_pool SET claimed_at = now() WHERE id = $1",
        row.id,
    )
    .execute(&mut **tx)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(StoreError::PoolRow(format!(
            "locked finite pool row {} vanished",
            row.id
        )));
    }
    Ok(Pop {
        claimed: Some(Claimed {
            row,
            pick: chosen,
            candidates: count,
        }),
        undecodable,
    })
}
