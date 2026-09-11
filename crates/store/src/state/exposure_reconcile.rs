//! Bounded AI-review packets and compare-and-set finite history reconciliation.

use super::lock_web_state;
use crate::{StoreError, begin_tenant, content::Admin};
use serde::Serialize;
use serde_json::Value;
use sqlx::{Postgres, Row, Transaction};
use std::collections::BTreeMap;
use uuid::Uuid;

/// Review one current page; decisions bind to the exact returned event fingerprint.
/// An irrelevant decision attests that the item exposes none of this KP's cases.
/// Actual case mappings must first enter the reviewed global alias ledger.
pub struct ReconciliationRequest<'a> {
    pub user_id: Uuid,
    pub kp_id: &'a str,
    pub context_digest: &'a str,
    pub reviewed_irrelevant: &'a BTreeMap<i64, String>,
    pub review_ref: &'a str,
    pub commit: bool,
    pub restart: bool,
}

/// One bounded historical item for independent semantic review.
#[derive(Debug, Serialize)]
pub struct ReconciliationItem {
    pub seq: i64,
    pub fingerprint: String,
    pub payload: Option<Value>,
    pub resolved: bool,
}

/// A checkpoint preview or committed page, retaining unresolved source references.
#[derive(Debug, Serialize)]
pub struct ReconciliationPage {
    pub target_seq: i64,
    pub through_seq: i64,
    pub context_digest: String,
    pub items: Vec<ReconciliationItem>,
    pub unresolved_count: i64,
    pub complete: bool,
    pub committed: bool,
}

/// Read at most 500 item rows / 1 MiB of payload under the tenant lock.
/// Preview leaves the checkpoint cursor unchanged. Commit re-reads source hashes;
/// stale or extraneous AI decisions abort the entire page. Unknown items remain
/// unresolved until a reviewed alias or exact irrelevant decision accounts for them.
pub async fn reconcile_exposure_page(
    admin: Admin<'_>,
    request: ReconciliationRequest<'_>,
) -> Result<ReconciliationPage, StoreError> {
    if request.review_ref.trim().is_empty() {
        return Err(StoreError::Document(
            "Finite reconciliation needs a review reference".into(),
        ));
    }
    let mut tx = begin_tenant(admin.db().pool(), request.user_id).await?;
    lock_web_state(&mut tx, request.user_id).await?;
    let (target, through, previous_unresolved) = checkpoint(&mut tx, &request).await?;
    let items = read_items(&mut tx, &request, through, target).await?;
    for (seq, expected) in request.reviewed_irrelevant {
        if !items
            .iter()
            .any(|item| item.seq == *seq && item.fingerprint == *expected && item.payload.is_some())
        {
            return Err(StoreError::Document(format!(
                "Stale or unavailable exposure review for event {seq}"
            )));
        }
    }
    let through_seq = items.last().map_or(target, |item| item.seq);
    let more = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM events WHERE user_id = $1 AND seq > $2 AND seq <= $3
         AND type IN ('attempt', 'ordinary_problem_served'))",
    )
    .bind(request.user_id)
    .bind(through_seq)
    .bind(target)
    .fetch_one(&mut *tx)
    .await?;
    let unresolved_count =
        previous_unresolved + items.iter().filter(|item| !item.resolved).count() as i64;
    let finished = !more;
    let through_seq = if finished { target } else { through_seq };
    let status = if !finished {
        "pending"
    } else if unresolved_count == 0 {
        "complete"
    } else {
        "unresolved"
    };
    if request.commit {
        sqlx::query(
            "UPDATE exposure_history_progress SET through_seq = $3, status = $4,
             unresolved_count = $5, review_ref = $6, updated_at = now()
             WHERE user_id = $1 AND scope_key = $2 AND context_digest = $7",
        )
        .bind(request.user_id)
        .bind(request.kp_id)
        .bind(through_seq)
        .bind(status)
        .bind(unresolved_count)
        .bind(request.review_ref)
        .bind(request.context_digest)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
    } else {
        tx.rollback().await?;
    }
    Ok(ReconciliationPage {
        target_seq: target,
        through_seq,
        context_digest: request.context_digest.to_owned(),
        items,
        unresolved_count,
        complete: finished && unresolved_count == 0,
        committed: request.commit,
    })
}

async fn checkpoint(
    tx: &mut Transaction<'_, Postgres>,
    request: &ReconciliationRequest<'_>,
) -> Result<(i64, i64, i64), StoreError> {
    let valid = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM finite_exposure_contexts WHERE kp_id = $1 AND context_digest = $2)
         AND EXISTS (SELECT 1 FROM exposure_history_progress WHERE user_id = $3 AND scope_key = '*'
                     AND status = 'complete' AND algorithm_version = 1)
         AND NOT EXISTS (SELECT 1 FROM events WHERE user_id = $3 AND type = 'attempt'
                         AND attempt_problem_digest IS NULL)",
    ).bind(request.kp_id).bind(request.context_digest).bind(request.user_id)
    .fetch_one(&mut **tx).await?;
    if !valid {
        return Err(StoreError::Document(
            "Exposure context or digest ingestion is incomplete".into(),
        ));
    }
    sqlx::query(
        "INSERT INTO exposure_history_progress
          (user_id, scope_key, algorithm_version, context_digest, target_seq, through_seq, status)
         SELECT $1, $2, 1, $3, COALESCE(MAX(seq), 0), 0, 'pending' FROM events WHERE user_id = $1
         ON CONFLICT (user_id, scope_key) DO NOTHING",
    )
    .bind(request.user_id)
    .bind(request.kp_id)
    .bind(request.context_digest)
    .execute(&mut **tx)
    .await?;
    if request.restart {
        sqlx::query(
            "UPDATE exposure_history_progress SET context_digest = $3, algorithm_version = 1,
             target_seq = (SELECT COALESCE(MAX(seq), 0) FROM events WHERE user_id = $1),
             through_seq = 0, unresolved_count = 0, status = 'pending', review_ref = NULL
             WHERE user_id = $1 AND scope_key = $2",
        )
        .bind(request.user_id)
        .bind(request.kp_id)
        .bind(request.context_digest)
        .execute(&mut **tx)
        .await?;
    }
    let row = sqlx::query(
        "SELECT target_seq, through_seq, unresolved_count FROM exposure_history_progress
         WHERE user_id = $1 AND scope_key = $2 AND context_digest = $3 AND algorithm_version = 1",
    )
    .bind(request.user_id)
    .bind(request.kp_id)
    .bind(request.context_digest)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| {
        StoreError::Document("Finite history checkpoint needs an explicit context restart".into())
    })?;
    Ok((
        row.try_get("target_seq")?,
        row.try_get("through_seq")?,
        row.try_get("unresolved_count")?,
    ))
}

async fn read_items(
    tx: &mut Transaction<'_, Postgres>,
    request: &ReconciliationRequest<'_>,
    through: i64,
    target: i64,
) -> Result<Vec<ReconciliationItem>, StoreError> {
    let rows = sqlx::query(
        "SELECT seq, octet_length(payload::text) AS bytes FROM events
         WHERE user_id = $1 AND seq > $2 AND seq <= $3
           AND type IN ('attempt', 'ordinary_problem_served') ORDER BY seq LIMIT 500",
    )
    .bind(request.user_id)
    .bind(through)
    .bind(target)
    .fetch_all(&mut **tx)
    .await?;
    let mut items = Vec::new();
    let mut bytes = 0;
    for row in rows {
        let seq: i64 = row.try_get("seq")?;
        let size: i32 = row.try_get("bytes")?;
        let available = (0..=65_536).contains(&size);
        if available && bytes + size > 1_048_576 {
            break;
        }
        if available {
            bytes += size;
        }
        let item = read_item(tx, request, seq, available).await?;
        items.push(item);
    }
    Ok(items)
}

async fn read_item(
    tx: &mut Transaction<'_, Postgres>,
    request: &ReconciliationRequest<'_>,
    seq: i64,
    available: bool,
) -> Result<ReconciliationItem, StoreError> {
    let row = sqlx::query(
        r#"SELECT CASE WHEN $3 THEN payload END AS body,
          encode(sha256(convert_to(jsonb_build_array(payload, attempt_problem_digest)::text, 'UTF8')), 'hex') AS fingerprint,
          EXISTS (SELECT 1 FROM finite_exposure_aliases AS a WHERE a.kp_id = $4
            AND (a.item_digest = e.ordinary_item_digest OR a.item_digest = e.attempt_problem_digest
                 OR (e.ordinary_kp_id = $4 AND a.case_id = e.ordinary_finite_case_id))) AS known
          FROM events AS e WHERE user_id = $1 AND seq = $2"#,
    ).bind(request.user_id).bind(seq).bind(available).bind(request.kp_id)
    .fetch_one(&mut **tx).await?;
    let fingerprint: String = row.try_get("fingerprint")?;
    let payload: Option<Value> = row.try_get("body")?;
    let known: bool = row.try_get("known")?;
    let reviewed = request.reviewed_irrelevant.get(&seq) == Some(&fingerprint) && available;
    Ok(ReconciliationItem {
        seq,
        fingerprint,
        payload,
        resolved: known || reviewed,
    })
}
