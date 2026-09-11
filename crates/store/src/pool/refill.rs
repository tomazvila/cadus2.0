//! The D-O4 refill reads, the C6 retire, the approved-template read, and the A6
//! operator flags.

use std::collections::BTreeSet;

use cadus_core::pool::Source;
use sqlx::PgExecutor;
use uuid::Uuid;

use super::{ApprovedTemplate, KpFlag, PoolTarget, RetiredRow};
use crate::StoreError;

/// Retire every unclaimed row whose digest is no longer approved (C6, D-O4).
///
/// The statement claims the row, exactly as a serve does: `claimed_at` leaves
/// the unclaimed set, so the row never reaches a pop again and never counts
/// toward the refill depth again. The row itself stays, because a claimed row is
/// the served-instance log of A5 and the retention job of M5 owns the delete.
///
/// A row with no digest is an exemplar rotation and is never retired here (A6).
///
/// The call returns one [`RetiredRow`] per claimed row, so the caller writes the
/// pair, the digest, and the status into the log. The D-O4 refill job is that
/// caller: it runs this before it reads the target list, so a pair whose rows
/// this statement took falls under its target depth in the SAME pass and refills
/// from the source that IS approved.
///
/// # The role decides how much this reaches
///
/// The statement carries no tenant bind, so it retires across every learner. The
/// worker connects as `cadus_admin`, which holds BYPASSRLS, so it sees them all.
/// A caller inside a tenant transaction retires that tenant's rows alone, and a
/// caller with no tenant bound retires nothing; neither one is an error.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn retire_unapproved<'e, E>(executor: E) -> Result<Vec<RetiredRow>, StoreError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query!(
        r#"
        UPDATE serving_pool AS sp
        SET claimed_at = now()
        FROM content_store AS cs
        WHERE cs.digest = sp.content_digest
          AND sp.claimed_at IS NULL
          AND cs.status <> 'approved'
        RETURNING sp.id AS "id!",
                  sp.user_id AS "user_id!",
                  sp.kp_id AS "kp_id!",
                  sp.content_digest AS "content_digest!",
                  cs.status AS "status!"
        "#
    )
    .fetch_all(executor)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| RetiredRow {
            id: row.id,
            user_id: row.user_id,
            kp_id: row.kp_id,
            content_digest: row.content_digest,
            status: row.status,
        })
        .collect())
}

/// The count of unclaimed rows of one `(user, kp)` pair.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn unclaimed_depth<'e, E>(
    executor: E,
    user_id: Uuid,
    kp_id: &str,
) -> Result<i64, StoreError>
where
    E: PgExecutor<'e>,
{
    let depth = sqlx::query_scalar!(
        r#"
        SELECT count(*) AS "depth!"
        FROM serving_pool
        WHERE user_id = $1 AND kp_id = $2 AND claimed_at IS NULL
        "#,
        user_id,
        kp_id,
    )
    .fetch_one(executor)
    .await?;
    Ok(depth)
}

/// Every `(user, kp)` pair whose unclaimed depth is under `target_depth` (D-O4).
///
/// The pairs come from `serving_pool` itself, so a pair reaches this list after
/// its first row exists. A pair the pool never held is not a refill target: 2.0
/// has no table that enrolls a learner in a knowledge point, and the serve path
/// creates the first rows when it enqueues a refill on a miss (specification
/// section 7.2).
///
/// The shallowest pair comes first, so a small `limit` refills the pairs closest
/// to an empty pool.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn refill_targets<'e, E>(
    executor: E,
    target_depth: i64,
    limit: i64,
) -> Result<Vec<PoolTarget>, StoreError>
where
    E: PgExecutor<'e>,
{
    refill_targets_skipping(executor, target_depth, limit, &[]).await
}

/// [`refill_targets`] without the pairs of `skip` (D-O4).
///
/// A `(user, kp)` pair with no approved template and no decidable exemplar can
/// never gain a row. It stays at depth 0, and depth 0 sorts first, so such a pair
/// took the head of the list on every tick and the whole per-tick budget went to
/// pairs that could not use it. The worker holds those pairs in a backoff map and
/// passes them here, so the `LIMIT` counts pairs that CAN fill.
///
/// The exclusion runs before the grouping, so a skipped pair costs no slot at
/// all.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn refill_targets_skipping<'e, E>(
    executor: E,
    target_depth: i64,
    limit: i64,
    skip: &[(Uuid, String)],
) -> Result<Vec<PoolTarget>, StoreError>
where
    E: PgExecutor<'e>,
{
    let mut skip_users: Vec<Uuid> = Vec::with_capacity(skip.len());
    let mut skip_kps: Vec<String> = Vec::with_capacity(skip.len());
    for (user_id, kp_id) in skip {
        skip_users.push(*user_id);
        skip_kps.push(kp_id.clone());
    }

    let rows = sqlx::query!(
        r#"
        SELECT user_id AS "user_id!",
               kp_id AS "kp_id!",
               count(*) FILTER (WHERE claimed_at IS NULL) AS "depth!"
        FROM serving_pool
        WHERE NOT EXISTS (
                  SELECT 1
                  FROM unnest($3::uuid[], $4::text[]) AS skip(user_id, kp_id)
                  WHERE skip.user_id = serving_pool.user_id
                    AND skip.kp_id = serving_pool.kp_id
              )
        GROUP BY user_id, kp_id
        HAVING count(*) FILTER (WHERE claimed_at IS NULL) < $1
        ORDER BY "depth!", user_id, kp_id
        LIMIT $2
        "#,
        target_depth,
        limit,
        &skip_users,
        &skip_kps,
    )
    .fetch_all(executor)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| PoolTarget {
            user_id: row.user_id,
            kp_id: row.kp_id,
            depth: row.depth,
        })
        .collect())
}

/// The newest approved template document of one knowledge point (C6).
///
/// `status = 'approved'` is the C6 gate: a `pending` row is never served, and the
/// approval binds to the digest, so an edited body is a different row that needs
/// its own approval.
///
/// The newest approval wins, and the digest breaks a tie, so two rows approved in
/// the same statement give one stable answer.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn approved_template<'e, E>(
    executor: E,
    kp_id: &str,
) -> Result<Option<ApprovedTemplate>, StoreError>
where
    E: PgExecutor<'e>,
{
    let found =
        crate::content::approved_document(executor, kp_id, crate::content::KIND_TEMPLATE).await?;
    Ok(found.map(|doc| ApprovedTemplate {
        digest: doc.digest,
        body: doc.body.to_string(),
        generation_context: super::GenerationContext {
            curriculum_digest: String::new(),
            review_engine_digest: String::new(),
        },
    }))
}

/// Read an approved template bound to the server's current curriculum policy.
///
/// # Errors
/// Returns a database error if the read fails.
pub async fn approved_template_current<'e, E>(
    executor: E,
    kp_id: &str,
    context: crate::content::CurrentContext<'_>,
) -> Result<Option<ApprovedTemplate>, StoreError>
where
    E: PgExecutor<'e>,
{
    let found = crate::content::approved_document_current(
        executor,
        kp_id,
        crate::content::KIND_TEMPLATE,
        context,
    )
    .await?;
    Ok(found.map(|doc| ApprovedTemplate {
        digest: doc.digest,
        body: doc.body.to_string(),
        generation_context: super::GenerationContext {
            curriculum_digest: context.curriculum_digest.to_owned(),
            review_engine_digest: context.review_engine_digest.to_owned(),
        },
    }))
}

/// The A6 operator view: one row per knowledge point (A6).
///
/// The row set is every knowledge point that holds a pool row or a template
/// document, so a knowledge point with a `pending` template appears with
/// `approved_templates = 0` and `needs_template = true`.
///
/// # Errors
///
/// Returns [`StoreError::PoolRow`] when a claimed row carries a `source` value
/// this build does not know, and [`StoreError::Db`] when the statement fails.
pub async fn operator_flags<'e, E>(executor: E) -> Result<Vec<KpFlag>, StoreError>
where
    E: PgExecutor<'e>,
{
    operator_flags_with_exhausted(executor, &[]).await
}

/// [`operator_flags`] with the exhausted knowledge points of the refill (A6).
///
/// `exhausted` holds the serving keys whose source ran dry: the refill filled the
/// pair twice in a row and inserted no new statement either time, so the pair is
/// on a one-hour backoff and the pool cannot grow (M4 review 2, finding #8). A
/// key in the slice sets [`KpFlag::source_exhausted`] on that row.
///
/// The backoff map lives in `cadus_worker::refill::RefillState`, in the worker
/// process. A caller in that process passes `RefillState::exhausted_kps()` here.
/// A caller in another process passes an empty slice and reads `false`, because
/// 2.0 has no table that carries the refill state across processes.
///
/// # Errors
///
/// Returns the errors of [`operator_flags`].
pub async fn operator_flags_with_exhausted<'e, E>(
    executor: E,
    exhausted: &[String],
) -> Result<Vec<KpFlag>, StoreError>
where
    E: PgExecutor<'e>,
{
    let exhausted: BTreeSet<&str> = exhausted.iter().map(String::as_str).collect();
    let rows = sqlx::query!(
        r#"
        WITH kps AS (
            SELECT kp_id FROM serving_pool
            UNION
            SELECT kp_id FROM content_store WHERE kind = 'template'
        ),
        approved AS (
            SELECT kp_id, count(*) AS approved
            FROM content_store
            WHERE kind = 'template' AND status = 'approved'
            GROUP BY kp_id
        ),
        depth AS (
            SELECT kp_id, count(*) AS depth
            FROM serving_pool
            WHERE claimed_at IS NULL
            GROUP BY kp_id
        ),
        last_served AS (
            SELECT DISTINCT ON (kp_id) kp_id, source, claimed_at
            FROM serving_pool
            WHERE claimed_at IS NOT NULL
            ORDER BY kp_id, claimed_at DESC, id
        ),
        last_exemplar AS (
            SELECT kp_id, max(claimed_at) AS at
            FROM serving_pool
            WHERE claimed_at IS NOT NULL AND source = 'exemplar'
            GROUP BY kp_id
        )
        SELECT kps.kp_id AS "kp_id!",
               coalesce(approved.approved, 0) AS "approved_templates!",
               coalesce(depth.depth, 0) AS "pool_depth!",
               last_served.source AS "last_source?",
               last_exemplar.at AS "last_exemplar_at?"
        FROM kps
        LEFT JOIN approved ON approved.kp_id = kps.kp_id
        LEFT JOIN depth ON depth.kp_id = kps.kp_id
        LEFT JOIN last_served ON last_served.kp_id = kps.kp_id
        LEFT JOIN last_exemplar ON last_exemplar.kp_id = kps.kp_id
        ORDER BY kps.kp_id
        "#
    )
    .fetch_all(executor)
    .await?;

    let mut flags = Vec::with_capacity(rows.len());
    for row in rows {
        let last_source = match row.last_source {
            None => None,
            Some(ref wire) => match Source::from_wire(wire) {
                Some(source) => Some(source),
                None => {
                    return Err(StoreError::PoolRow(format!(
                        "knowledge point {} last served source {wire:?}, which this build does not \
                         know",
                        row.kp_id
                    )));
                }
            },
        };
        let source_exhausted = exhausted.contains(row.kp_id.as_str());
        flags.push(KpFlag {
            kp_id: row.kp_id,
            approved_templates: row.approved_templates,
            pool_depth: row.pool_depth,
            last_source,
            last_exemplar_at: row.last_exemplar_at,
            needs_template: row.approved_templates == 0,
            source_exhausted,
        });
    }
    Ok(flags)
}
