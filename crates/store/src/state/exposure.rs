//! Indexed lifetime lookup for ordinary problem hand-offs.

use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::StoreError;

/// Stable identity used to decide whether a hand-off repeats prior exposure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandoffIdentity<'a> {
    /// A reviewed finite semantic case, stable across rendering revisions.
    Finite { kp_id: &'a str, case_id: &'a str },
    /// An ordinary rendered item outside a reviewed finite universe.
    Digest(&'a str),
}

/// Whether indexed hand-offs or authoritative Attempt text record this identity.
/// Finite aliases are retained, reviewed global evidence, including retired text.
pub async fn handoff_seen(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    identity: HandoffIdentity<'_>,
) -> Result<bool, StoreError> {
    let digests = match identity {
        HandoffIdentity::Finite { kp_id, case_id } => {
            let stable = sqlx::query_scalar::<_, bool>(
                r#"SELECT EXISTS (
                    SELECT 1 FROM events
                    WHERE user_id = $1
                      AND ordinary_finite_case_id IS NOT NULL
                      AND ordinary_kp_id = $2
                      AND ordinary_finite_case_id = $3
                )"#,
            )
            .bind(user_id)
            .bind(kp_id)
            .bind(case_id)
            .fetch_one(&mut **tx)
            .await?;
            if stable {
                return Ok(true);
            }
            sqlx::query_scalar::<_, String>(
                "SELECT DISTINCT item_digest FROM finite_exposure_aliases
                 WHERE kp_id = $1 AND case_id = $2 ORDER BY item_digest",
            )
            .bind(kp_id)
            .bind(case_id)
            .fetch_all(&mut **tx)
            .await?
        }
        HandoffIdentity::Digest(digest) => vec![digest.to_owned()],
    };
    if digests.is_empty() {
        return Ok(false);
    }
    Ok(sqlx::query_scalar::<_, bool>(
        r#"SELECT EXISTS (
            SELECT 1 FROM events WHERE user_id = $1
              AND ordinary_item_digest IS NOT NULL
              AND ordinary_item_digest = ANY($2::text[])
        ) OR EXISTS (
            SELECT 1 FROM events WHERE user_id = $1
              AND attempt_problem_digest IS NOT NULL
              AND attempt_problem_digest = ANY($2::text[])
        )"#,
    )
    .bind(user_id)
    .bind(&digests)
    .fetch_one(&mut **tx)
    .await?)
}

/// Advance the cached projection over the exact neutral hand-off just appended.
///
/// The compare-and-set succeeds only when `seq` immediately follows the stored
/// cursor and that exact event row has type `ordinary_problem_served`. A
/// missing/sequence-stale cache, a different event type, or any intervening
/// event returns `false`, and the caller must use the projector instead.
pub async fn advance_handoff_cursor(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    seq: i64,
) -> Result<bool, StoreError> {
    let Some(previous) = seq.checked_sub(1) else {
        return Ok(false);
    };
    let result = sqlx::query(
        r#"UPDATE learner_models AS lm
              SET through_seq = $3, built_at = now()
            WHERE lm.user_id = $1 AND lm.through_seq = $2
              AND EXISTS (
                  SELECT 1 FROM events AS e
                   WHERE e.user_id = $1 AND e.seq = $3
                     AND e.type = 'ordinary_problem_served'
              )"#,
    )
    .bind(user_id)
    .bind(previous)
    .bind(seq)
    .execute(&mut **tx)
    .await?;
    Ok(result.rows_affected() == 1)
}
