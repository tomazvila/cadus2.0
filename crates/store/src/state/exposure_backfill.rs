//! Bounded administrative ingestion of legacy Attempt text identity.

use super::lock_web_state;
use crate::{StoreError, begin_tenant, content::Admin};
use cadus_core::learner::problem_text_hash;
use serde::Serialize;
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

const PAGE_ROWS: i64 = 500;
const TEXT_LIMIT: i32 = 65_536;
const PAGE_BYTES: usize = 1_048_576;

/// One committed page; issues retain event sequence references for AI resolution.
#[derive(Debug, Default, Serialize)]
pub struct BackfillPage {
    pub target_seq: i64,
    pub through_seq: i64,
    pub inspected: usize,
    pub bytes_processed: usize,
    pub filled: usize,
    pub already_correct: usize,
    pub supplied_digest_discrepancies: Vec<i64>,
    pub unresolved: Vec<i64>,
    pub total_unresolved: i64,
    pub finished: bool,
}

/// Fill derived metadata only, retaining every original payload and grade.
/// Requires an administrative pool. Each call locks one tenant and commits at
/// most 500 rows / 1 MiB of exact text. Restart an unresolved completed scan
/// explicitly after its source issue is resolved; interruption resumes normally.
pub async fn backfill_attempt_digests(
    admin: Admin<'_>,
    user_id: Uuid,
    restart: bool,
) -> Result<BackfillPage, StoreError> {
    let mut tx = begin_tenant(admin.db().pool(), user_id).await?;
    lock_web_state(&mut tx, user_id).await?;
    let (target, through, unresolved) = checkpoint(&mut tx, user_id, restart).await?;
    let mut page = BackfillPage {
        target_seq: target,
        through_seq: through,
        total_unresolved: unresolved,
        ..BackfillPage::default()
    };
    let rows = sqlx::query(
        r#"SELECT seq, type,
             CASE WHEN type = 'attempt' AND
               jsonb_typeof(payload #> '{problem,text}') = 'string'
               THEN octet_length(payload #>> '{problem,text}') END AS text_bytes
           FROM events WHERE user_id = $1 AND seq > $2 AND seq <= $3
           ORDER BY seq LIMIT $4"#,
    )
    .bind(user_id)
    .bind(through)
    .bind(target)
    .bind(PAGE_ROWS)
    .fetch_all(&mut *tx)
    .await?;
    for row in rows {
        let seq: i64 = row.try_get("seq")?;
        let kind: String = row.try_get("type")?;
        if kind == "attempt" {
            let size: Option<i32> = row.try_get("text_bytes")?;
            if size.is_some_and(|n| n >= 0 && n <= TEXT_LIMIT) {
                let bytes = size.unwrap_or_default() as usize;
                if page.bytes_processed + bytes > PAGE_BYTES {
                    break;
                }
                page.bytes_processed += bytes;
                ingest(&mut tx, user_id, seq, &mut page).await?;
            } else {
                page.unresolved.push(seq);
            }
        }
        page.inspected += 1;
        page.through_seq = seq;
    }
    // A deleted/non-dense historical range still terminates at its fixed bound.
    if page.inspected < PAGE_ROWS as usize && page.bytes_processed < PAGE_BYTES {
        let more = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM events WHERE user_id = $1 AND seq > $2 AND seq <= $3)",
        )
        .bind(user_id)
        .bind(page.through_seq)
        .bind(target)
        .fetch_one(&mut *tx)
        .await?;
        if !more {
            page.through_seq = target;
        }
    }
    page.total_unresolved += page.unresolved.len() as i64;
    page.finished = page.through_seq == target;
    let status = if !page.finished {
        "pending"
    } else if page.total_unresolved == 0 {
        "complete"
    } else {
        "unresolved"
    };
    sqlx::query(
        "UPDATE exposure_history_progress SET through_seq = $2, status = $3,
         unresolved_count = $4, updated_at = now() WHERE user_id = $1 AND scope_key = '*'",
    )
    .bind(user_id)
    .bind(page.through_seq)
    .bind(status)
    .bind(page.total_unresolved)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(page)
}

async fn checkpoint(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    restart: bool,
) -> Result<(i64, i64, i64), StoreError> {
    sqlx::query(
        r#"INSERT INTO exposure_history_progress
          (user_id, scope_key, algorithm_version, target_seq, through_seq, status)
          SELECT $1, '*', 1, COALESCE(MAX(seq), 0), 0, 'pending'
          FROM events WHERE user_id = $1
          ON CONFLICT (user_id, scope_key) DO NOTHING"#,
    )
    .bind(user_id)
    .execute(&mut **tx)
    .await?;
    if restart {
        sqlx::query(
            "UPDATE exposure_history_progress SET algorithm_version = 1,
             target_seq = (SELECT COALESCE(MAX(seq), 0) FROM events WHERE user_id = $1),
             through_seq = 0, status = 'pending', unresolved_count = 0,
             review_ref = NULL, updated_at = now() WHERE user_id = $1 AND scope_key = '*'",
        )
        .bind(user_id)
        .execute(&mut **tx)
        .await?;
    }
    let row = sqlx::query(
        "SELECT target_seq, through_seq, unresolved_count FROM exposure_history_progress
         WHERE user_id = $1 AND scope_key = '*' AND algorithm_version = 1",
    )
    .bind(user_id)
    .fetch_one(&mut **tx)
    .await?;
    Ok((
        row.try_get("target_seq")?,
        row.try_get("through_seq")?,
        row.try_get("unresolved_count")?,
    ))
}

async fn ingest(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    seq: i64,
    page: &mut BackfillPage,
) -> Result<(), StoreError> {
    let row = sqlx::query(
        "SELECT payload #>> '{problem,text}' AS text, payload->>'item_digest' AS claimed,
         attempt_problem_digest AS derived FROM events WHERE user_id = $1 AND seq = $2",
    )
    .bind(user_id)
    .bind(seq)
    .fetch_one(&mut **tx)
    .await?;
    let text: String = row.try_get("text")?;
    let digest = problem_text_hash(&text);
    let claimed: Option<String> = row.try_get("claimed")?;
    if claimed.as_ref().is_some_and(|old| old != &digest) {
        page.supplied_digest_discrepancies.push(seq);
    }
    let derived: Option<String> = row.try_get("derived")?;
    match derived {
        Some(old) if old == digest => page.already_correct += 1,
        Some(_) => page.unresolved.push(seq),
        None => {
            let result = sqlx::query(
                "UPDATE events SET attempt_problem_digest = $3 WHERE user_id = $1 AND seq = $2
                 AND type = 'attempt' AND attempt_problem_digest IS NULL
                 AND jsonb_typeof(payload #> '{problem,text}') = 'string'
                 AND payload #>> '{problem,text}' = $4",
            )
            .bind(user_id)
            .bind(seq)
            .bind(&digest)
            .bind(&text)
            .execute(&mut **tx)
            .await?;
            if result.rows_affected() == 1 {
                page.filled += 1;
            } else {
                page.unresolved.push(seq);
            }
        }
    }
    Ok(())
}
