//! Small history checkpoints keep incomplete legacy evidence out of fresh credit.

use crate::StoreError;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

/// Whether known item history has been indexed and, for a finite KP, reconciled.
/// The caller holds the tenant web-state lock through the subsequent hand-off.
/// Empty histories initialize a zero checkpoint; historical rows require admin
/// backfill and separately reviewed finite reconciliation.
pub async fn handoff_history_ready(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    finite: Option<(&str, &str)>,
) -> Result<bool, StoreError> {
    initialize_empty(tx, user_id).await?;
    let digests_ready = sqlx::query_scalar::<_, bool>(
        r#"SELECT EXISTS (
            SELECT 1 FROM exposure_history_progress
            WHERE user_id = $1 AND scope_key = '*'
              AND algorithm_version = 1 AND status = 'complete'
        ) AND NOT EXISTS (
            SELECT 1 FROM events WHERE user_id = $1
              AND type = 'attempt' AND attempt_problem_digest IS NULL
        )"#,
    )
    .bind(user_id)
    .fetch_one(&mut **tx)
    .await?;
    if !digests_ready {
        return Ok(false);
    }
    let Some((kp_id, policy_digest)) = finite else {
        return Ok(true);
    };
    Ok(sqlx::query_scalar::<_, bool>(
        r#"SELECT EXISTS (
            SELECT 1 FROM exposure_history_progress AS p
            JOIN finite_exposure_contexts AS c
              ON c.kp_id = p.scope_key AND c.context_digest = p.context_digest
            WHERE p.user_id = $1 AND p.scope_key = $2
              AND c.policy_digest = $3 AND p.algorithm_version = 1
              AND p.status = 'complete'
              AND NOT EXISTS (
                SELECT 1 FROM events AS e WHERE e.user_id = $1
                  AND e.ordinary_kp_id = $2
                  AND e.ordinary_item_digest IS NOT NULL
                  AND e.ordinary_finite_case_id IS NULL
                  AND e.seq > p.through_seq
              )
        )"#,
    )
    .bind(user_id)
    .bind(kp_id)
    .bind(policy_digest)
    .fetch_one(&mut **tx)
    .await?)
}

async fn initialize_empty(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> Result<(), StoreError> {
    // The partial index makes this proof independent of unrelated session events.
    sqlx::query(
        r#"INSERT INTO exposure_history_progress
            (user_id, scope_key, algorithm_version, context_digest,
             target_seq, through_seq, status, review_ref)
           SELECT $1, contexts.scope_key, 1, contexts.context_digest, 0, 0,
                  'complete', 'server:verified-empty-item-history:v1'
           FROM (
             SELECT '*'::text AS scope_key, NULL::text AS context_digest
             UNION ALL SELECT kp_id, context_digest FROM finite_exposure_contexts
           ) AS contexts
           WHERE NOT EXISTS (
             SELECT 1 FROM events WHERE user_id = $1
               AND type IN ('attempt', 'ordinary_problem_served')
           )
           ON CONFLICT (user_id, scope_key) DO NOTHING"#,
    )
    .bind(user_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
