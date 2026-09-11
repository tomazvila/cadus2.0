//! Admin-only reviewed mappings from retained problem text to finite cases.

use crate::{StoreError, content::Admin};
use cadus_core::{curriculum::FiniteObjectiveDomain, learner::problem_text_hash};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};

/// How a trusted old rendering relates to today's serving policy.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AliasKind {
    ActiveVariant,
    RetiredVariant,
    ExposureOnly,
}
impl AliasKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::ActiveVariant => "active_variant",
            Self::RetiredVariant => "retired_variant",
            Self::ExposureOnly => "exposure_only",
        }
    }
}

/// One independently reviewed semantic mapping. Answer equality is insufficient.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExposureAlias {
    pub case_id: String,
    pub problem_text: String,
    pub source_kind: AliasKind,
    pub source_policy_digest: Option<String>,
}

/// Publish current variants and explicitly reviewed retained mappings atomically.
/// All historical aliases remain in the ledger. A new policy/context invalidates
/// old reconciliation checkpoints until the AI background review refreshes them.
/// `expected_previous` is the context last inspected by the administrative caller.
pub async fn publish_finite_exposure(
    admin: Admin<'_>,
    kp_id: &str,
    domain: &FiniteObjectiveDomain,
    retained: &[ExposureAlias],
    review_ref: &str,
    expected_previous: Option<&str>,
) -> Result<String, StoreError> {
    domain.validate().map_err(StoreError::Document)?;
    if kp_id.trim().is_empty() || review_ref.trim().is_empty() {
        return Err(StoreError::Document(
            "Exposure publication needs a KP and review reference".into(),
        ));
    }
    let policy = domain.fingerprint(kp_id).map_err(StoreError::Document)?;
    let mut tx = admin.db().pool().begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 1163415631))")
        .bind(kp_id)
        .execute(&mut *tx)
        .await?;
    let previous = sqlx::query_scalar::<_, String>(
        "SELECT context_digest FROM finite_exposure_contexts WHERE kp_id = $1",
    )
    .bind(kp_id)
    .fetch_optional(&mut *tx)
    .await?;
    if previous.as_deref() != expected_previous {
        return Err(StoreError::Document(
            "Exposure context changed since review".into(),
        ));
    }
    for case in &domain.cases {
        for variant in &case.variants {
            insert_alias(
                &mut tx,
                kp_id,
                &ExposureAlias {
                    case_id: case.id.as_str().to_owned(),
                    problem_text: variant.problem.clone(),
                    source_kind: AliasKind::ActiveVariant,
                    source_policy_digest: Some(policy.clone()),
                },
                review_ref,
            )
            .await?;
        }
    }
    for alias in retained {
        if !domain
            .cases
            .iter()
            .any(|case| case.id.as_str() == alias.case_id)
        {
            return Err(StoreError::Document(format!(
                "Unreviewed finite case {}",
                alias.case_id
            )));
        }
        insert_alias(&mut tx, kp_id, alias, review_ref).await?;
    }
    let context = sqlx::query_scalar::<_, String>(
        r#"SELECT encode(sha256(convert_to(jsonb_build_array(
             'finite-exposure:v1', $1::text, $2::text, $3::text,
             (SELECT jsonb_agg(jsonb_build_array(case_id, item_digest, problem_sha256)
                     ORDER BY case_id, item_digest, problem_sha256)
                FROM finite_exposure_aliases WHERE kp_id = $1)
           )::text, 'UTF8')), 'hex')"#,
    )
    .bind(kp_id)
    .bind(&policy)
    .bind(review_ref)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO finite_exposure_contexts (kp_id, policy_digest, context_digest, review_ref)
         VALUES ($1, $2, $3, $4) ON CONFLICT (kp_id) DO UPDATE SET
         policy_digest = EXCLUDED.policy_digest, context_digest = EXCLUDED.context_digest,
         review_ref = EXCLUDED.review_ref, published_at = now()",
    )
    .bind(kp_id)
    .bind(policy)
    .bind(&context)
    .bind(review_ref)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(context)
}

async fn insert_alias(
    tx: &mut Transaction<'_, Postgres>,
    kp_id: &str,
    alias: &ExposureAlias,
    review_ref: &str,
) -> Result<(), StoreError> {
    if alias.problem_text.is_empty() || alias.problem_text.len() > 65_536 {
        return Err(StoreError::Document(
            "Exposure alias text must contain 1..65536 bytes".into(),
        ));
    }
    sqlx::query(
        r#"INSERT INTO finite_exposure_aliases
           (kp_id, case_id, item_digest, problem_sha256, problem_text,
            first_review_ref, source_policy_digest, source_kind)
           VALUES ($1, $2, $3, encode(sha256(convert_to($4::text, 'UTF8')), 'hex'),
                   $4, $5, $6, $7) ON CONFLICT DO NOTHING"#,
    )
    .bind(kp_id)
    .bind(&alias.case_id)
    .bind(problem_text_hash(&alias.problem_text))
    .bind(&alias.problem_text)
    .bind(review_ref)
    .bind(&alias.source_policy_digest)
    .bind(alias.source_kind.as_str())
    .execute(&mut **tx)
    .await?;
    Ok(())
}
