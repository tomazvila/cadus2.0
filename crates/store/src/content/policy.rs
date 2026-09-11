//! Approval reads and writes bound to the current trusted curriculum policy.

use sqlx::PgExecutor;
use uuid::Uuid;

use super::{Admin, ApprovedDoc, Decision, STATUS_APPROVED};
use crate::StoreError;

/// Trusted runtime context shared by approval, serving, and readiness reads.
#[derive(Debug, Clone, Copy)]
pub struct CurrentContext<'a> {
    /// Current finite policy fingerprint, or `None` for an ordinary objective.
    pub policy_digest: Option<&'a str>,
    /// Effective semantic curriculum fingerprint.
    pub curriculum_digest: &'a str,
    /// Compiled renderer/evaluator/instruction-gate fingerprint.
    pub review_engine_digest: &'a str,
}

/// Exact review snapshot submitted to one approval write.
#[derive(Debug, Clone, Copy)]
pub struct ApprovalContext<'a> {
    /// Trusted non-template dimensions.
    pub current: CurrentContext<'a>,
    /// Current instruction bank, or prospective bank for a template candidate.
    pub template_context_digest: Option<&'a str>,
}

/// Read the newest approved document bound to exactly this policy.
///
/// `None` selects an ordinary knowledge point. A finite policy fingerprint is
/// derived by the server from the loaded curriculum, never the authored body.
///
/// # Errors
/// Returns a database error if the read fails.
pub async fn approved_document_current<'e, E>(
    executor: E,
    kp_id: &str,
    kind: &str,
    context: CurrentContext<'_>,
) -> Result<Option<ApprovedDoc>, StoreError>
where
    E: PgExecutor<'e>,
{
    read_approved(executor, kp_id, kind, context, None, None).await
}

/// Read current instruction only if its live source still belongs to the bank.
pub async fn approved_document_for_source<'e, E>(
    executor: E,
    kp_id: &str,
    kind: &str,
    context: CurrentContext<'_>,
    source_digest: Option<&str>,
    source_context: Option<(&str, &str)>,
) -> Result<Option<ApprovedDoc>, StoreError>
where
    E: PgExecutor<'e>,
{
    let Some(source_context) = source_context else {
        return Ok(None);
    };
    read_approved(
        executor,
        kp_id,
        kind,
        context,
        source_digest,
        Some(source_context),
    )
    .await
}

async fn read_approved<'e, E>(
    executor: E,
    kp_id: &str,
    kind: &str,
    context: CurrentContext<'_>,
    source_digest: Option<&str>,
    source_context: Option<(&str, &str)>,
) -> Result<Option<ApprovedDoc>, StoreError>
where
    E: PgExecutor<'e>,
{
    let row = sqlx::query!(
        r#"
        SELECT digest AS "digest!", body AS "body!"
        FROM content_store c
        WHERE kp_id = $1 AND kind = $2 AND status = 'approved'
          AND approved_policy_digest IS NOT DISTINCT FROM $3
          AND approved_curriculum_digest = $5
          AND approved_review_engine_digest = $6
          AND (kind NOT IN ('teach','hint_ladder') OR
               approved_template_context_digest IS NOT DISTINCT FROM
               public.cadus_template_context(kp_id, $3, $5, $6))
          AND ($4::text IS NULL OR $4 = ANY(public.cadus_template_bank(kp_id, $3, $5, $6)))
          AND ($7::text IS NULL OR ($7 = $5 AND $8 = $6))
        ORDER BY approved_at DESC NULLS LAST, created_at DESC, digest
        LIMIT 1
        "#,
        kp_id,
        kind,
        context.policy_digest,
        source_digest,
        context.curriculum_digest,
        context.review_engine_digest,
        source_context.map(|pair| pair.0),
        source_context.map(|pair| pair.1),
    )
    .fetch_optional(executor)
    .await?;
    Ok(row.map(|row| ApprovedDoc {
        digest: row.digest,
        body: row.body,
    }))
}

/// Approve reviewed content against its server-derived current policy.
///
/// Repeating the same approval preserves its reviewer and timestamp. Reviewing
/// a changed policy records a new stamp, even when the content bytes match.
///
/// # Errors
/// Returns a database/timeout error, or NotFound for an absent digest.
pub async fn approve_current(
    admin: Admin<'_>,
    digest: &str,
    approved_by: Option<Uuid>,
    context: ApprovalContext<'_>,
) -> Result<Option<Decision>, StoreError> {
    let db = admin.db();
    let updated = crate::bounded(db, async {
        let mut tx = db.pool().begin().await?;
        if !super::lock_review_kp(&mut tx, digest).await? {
            return Ok(None);
        }
        let decision = sqlx::query_as!(
            Decision,
            r#"
        UPDATE content_store
        SET status = $2,
            approved_by = CASE
                WHEN status = $2 AND approved_policy_digest IS NOT DISTINCT FROM $4
                 AND approved_curriculum_digest = $6
                 AND approved_review_engine_digest = $7
                 AND (kind = 'template' OR
                      approved_template_context_digest IS NOT DISTINCT FROM $5)
                THEN approved_by ELSE $3 END,
            approved_at = CASE
                WHEN status = $2 AND approved_policy_digest IS NOT DISTINCT FROM $4
                 AND approved_curriculum_digest = $6
                 AND approved_review_engine_digest = $7
                 AND (kind = 'template' OR
                      approved_template_context_digest IS NOT DISTINCT FROM $5)
                THEN approved_at ELSE now() END,
            approved_policy_digest = $4,
            approved_template_context_digest = CASE
                WHEN kind IN ('teach','hint_ladder') THEN $5 ELSE NULL::text END,
            approved_curriculum_digest = $6,
            approved_review_engine_digest = $7
        WHERE digest = $1
          AND $5 IS NOT DISTINCT FROM CASE
                WHEN kind IN ('teach','hint_ladder')
                THEN public.cadus_template_context(kp_id, $4, $6, $7)
                WHEN kind = 'template'
                THEN public.cadus_template_context_after(kp_id, $4, digest, $6, $7)
                ELSE NULL::text END
        RETURNING digest AS "digest!", status AS "status!", approved_by, approved_at
        "#,
            digest,
            STATUS_APPROVED,
            approved_by,
            context.current.policy_digest,
            context.template_context_digest,
            context.current.curriculum_digest,
            context.current.review_engine_digest,
        )
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(decision)
    })
    .await?;
    Ok(updated)
}

/// The complete template bank a current instruction review must cover.
pub async fn template_context_digest<'e, E>(
    executor: E,
    kp_id: &str,
    context: CurrentContext<'_>,
) -> Result<Option<String>, StoreError>
where
    E: PgExecutor<'e>,
{
    sqlx::query_scalar::<_, Option<String>>("SELECT public.cadus_template_context($1, $2, $3, $4)")
        .bind(kp_id)
        .bind(context.policy_digest)
        .bind(context.curriculum_digest)
        .bind(context.review_engine_digest)
        .fetch_one(executor)
        .await
        .map_err(Into::into)
}

/// The context fingerprint and its exact member digests from one database snapshot.
pub async fn template_review_context<'e, E>(
    executor: E,
    kp_id: &str,
    context: CurrentContext<'_>,
    candidate_digest: Option<&str>,
) -> Result<(Option<String>, Vec<String>), StoreError>
where
    E: PgExecutor<'e>,
{
    let (digest, members) = sqlx::query_as::<_, (Option<String>, Option<Vec<String>>)>(
        "SELECT CASE WHEN $5::text IS NULL
                     THEN public.cadus_template_context($1, $2, $3, $4)
                     ELSE public.cadus_template_context_after($1, $2, $5, $3, $4) END,
                public.cadus_template_bank($1, $2, $3, $4)",
    )
    .bind(kp_id)
    .bind(context.policy_digest)
    .bind(context.curriculum_digest)
    .bind(context.review_engine_digest)
    .bind(candidate_digest)
    .fetch_one(executor)
    .await?;
    Ok((digest, members.unwrap_or_default()))
}
