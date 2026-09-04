//! R5: the review reads of `content_store` (spec section 3.2).

use serde_json::Value as Json;
use sqlx::PgExecutor;
use sqlx::types::chrono::{DateTime, Utc};

use super::{KIND_HINT_LADDER, KIND_TEACH, KIND_TEMPLATE, STATUS_APPROVED, STATUS_PENDING};
use crate::StoreError;

/// The count of approved templates one knowledge point needs (spec section 3.2,
/// "The bank warning carries over").
///
/// A knowledge point serves from its APPROVED slots only, so a bank with fewer
/// than three approved templates repeats a smaller set of problem shapes than
/// the bank was sized for (1.0 `scripts/review_templates.py:100-112`).
pub const BANK_TARGET: i64 = 3;

/// The rows one [`review_list`] call returns, at most.
///
/// The review screen reads a queue, not an archive. A deployment with more
/// pending documents than this reads the rest through the `kp` filter.
pub const LIST_LIMIT: i64 = 200;

/// Which documents [`review_list`] returns.
///
/// A `None` field applies no filter of that kind.
#[derive(Debug, Clone, Copy, Default)]
pub struct ReviewFilter<'a> {
    /// One of [`STATUS_PENDING`], [`STATUS_APPROVED`], [`STATUS_REJECTED`].
    pub status: Option<&'a str>,
    /// One of [`KIND_TEMPLATE`], [`KIND_TEACH`], [`KIND_HINT_LADDER`],
    /// [`KIND_DIAGNOSIS`].
    pub kind: Option<&'a str>,
    /// The serving key `"<topic>/<kp>"`.
    pub kp_id: Option<&'a str>,
}

/// One row of the review queue (spec section 3.2, `GET /api/admin/content`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewItem {
    /// The content address of the body.
    pub digest: String,
    /// The serving key of the document.
    pub kp_id: String,
    /// The kind of the document.
    pub kind: String,
    /// The review status of the document.
    pub status: String,
    /// How many model calls the authoring pass spent (T3).
    pub authoring_attempts: i32,
    /// What the authoring pass cost, as the exact text of the column (T3).
    pub cost_usd: Option<String>,
    /// When the row entered the table.
    pub created_at: DateTime<Utc>,
    /// The body, for the one-line summary the caller writes.
    pub body: Json,
    /// How many APPROVED templates this knowledge point holds.
    ///
    /// The count is of kind [`KIND_TEMPLATE`] alone, because the bank the
    /// warning is about is the template bank.
    pub approved_templates: i64,
}

/// One `content_store` row with the two review columns, as the one read of
/// this module returns it.
struct StoredRow {
    digest: String,
    kp_id: String,
    kind: String,
    status: String,
    authoring_attempts: i32,
    cost_usd: Option<String>,
    created_at: DateTime<Utc>,
    body: Json,
    approved_templates: i64,
    review_reason: Option<String>,
    approved_at: Option<DateTime<Utc>>,
}

impl StoredRow {
    /// The queue line of the row.
    fn into_item(self) -> ReviewItem {
        ReviewItem {
            digest: self.digest,
            kp_id: self.kp_id,
            kind: self.kind,
            status: self.status,
            authoring_attempts: self.authoring_attempts,
            cost_usd: self.cost_usd,
            created_at: self.created_at,
            body: self.body,
            approved_templates: self.approved_templates,
        }
    }
}

/// The one read of the review screen: the rows of `filter`, or the one row
/// of `digest`, newest first, at most `limit` rows.
///
/// The order is `created_at` descending with the digest as the tie break, so
/// two rows written in one statement give one stable page. `digest` is the
/// primary key, so a read by digest returns at most one row.
async fn stored_rows<'e, E>(
    executor: E,
    filter: &ReviewFilter<'_>,
    digest: Option<&str>,
    limit: i64,
) -> Result<Vec<StoredRow>, StoreError>
where
    E: PgExecutor<'e>,
{
    Ok(sqlx::query_as!(
        StoredRow,
        r#"
        SELECT c.digest AS "digest!", c.kp_id AS "kp_id!", c.kind AS "kind!",
               c.status AS "status!", c.authoring_attempts AS "authoring_attempts!",
               c.authoring_cost_usd::text AS "cost_usd?",
               c.created_at AS "created_at!", c.body AS "body!",
               c.review_reason, c.approved_at,
               (SELECT count(*) FROM content_store a
                 WHERE a.kp_id = c.kp_id AND a.kind = $4 AND a.status = $5)
                 AS "approved_templates!"
        FROM content_store c
        WHERE ($1::text IS NULL OR c.status = $1)
          AND ($2::text IS NULL OR c.kind = $2)
          AND ($3::text IS NULL OR c.kp_id = $3)
          AND ($7::text IS NULL OR c.digest = $7)
        ORDER BY c.created_at DESC, c.digest
        LIMIT $6
        "#,
        filter.status,
        filter.kind,
        filter.kp_id,
        KIND_TEMPLATE,
        STATUS_APPROVED,
        limit,
        digest,
    )
    .fetch_all(executor)
    .await?)
}

/// The review queue, newest first (C6).
///
/// The read takes any executor: `content_store` holds curriculum content, it is
/// outside row-level security, and `cadus_app` holds SELECT on it. The two
/// WRITE paths take [`Admin`], and they are the only ones that need it.
///
/// The order is `created_at` descending with the digest as the tie break, so two
/// rows written in one statement give one stable page.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn review_list<'e, E>(
    executor: E,
    filter: &ReviewFilter<'_>,
) -> Result<Vec<ReviewItem>, StoreError>
where
    E: PgExecutor<'e>,
{
    let rows = stored_rows(executor, filter, None, LIST_LIMIT).await?;
    Ok(rows.into_iter().map(StoredRow::into_item).collect())
}

/// One row the re-gate of a knowledge point reads (C6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegateRow {
    /// The content address of the row.
    pub digest: String,
    /// The kind of the row: [`KIND_TEMPLATE`], [`KIND_TEACH`] or
    /// [`KIND_HINT_LADDER`].
    pub kind: String,
    /// The review status of the row.
    pub status: String,
    /// The document body, as `content_store.body` holds it.
    pub body: Json,
}

/// The rows one re-gate of a knowledge point reads (C6).
///
/// The approve route re-runs the two instruction gates after a template is
/// approved, and it needs two sets of rows in one statement:
///
/// - every `template` row of the knowledge point that is `approved` or
///   `pending`. Those rows render the instances the learner is served, and they
///   are the answer set the gates judge against;
/// - every `teach` and `hint_ladder` row that is `pending`. Those are the
///   documents the re-gate moves to `rejected`. An APPROVED page or ladder is
///   not read: a human passed it, and a later template does not undo that
///   verdict.
///
/// A `rejected` row is not read at all: it serves nothing already.
///
/// The order is the kind, then the digest, so two runs over one table give one
/// list.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn regate_rows<'e, E>(executor: E, kp_id: &str) -> Result<Vec<RegateRow>, StoreError>
where
    E: PgExecutor<'e>,
{
    Ok(sqlx::query_as!(
        RegateRow,
        r#"
        SELECT digest AS "digest!", kind AS "kind!", status AS "status!", body AS "body!"
        FROM content_store
        WHERE kp_id = $1
          AND ((kind = $2 AND status IN ($3, $4))
               OR (kind IN ($5, $6) AND status = $4))
        ORDER BY kind, digest
        "#,
        kp_id,
        KIND_TEMPLATE,
        STATUS_APPROVED,
        STATUS_PENDING,
        KIND_TEACH,
        KIND_HINT_LADDER,
    )
    .fetch_all(executor)
    .await?)
}

/// One document as the review screen reads it (spec section 3.2,
/// `GET /api/admin/content/{digest}`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredDoc {
    /// The queue row of this document.
    pub item: ReviewItem,
    /// The reason of the last rejection, when a reviewer wrote one.
    pub review_reason: Option<String>,
    /// When the document was approved.
    pub approved_at: Option<DateTime<Utc>>,
}

/// One document of any status, by its digest (C6).
///
/// The read is the [`review_list`] read of one row, plus the two review columns
/// the queue line does not carry. `None` means the table holds no such digest,
/// and the caller answers 404.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn document<'e, E>(executor: E, digest: &str) -> Result<Option<StoredDoc>, StoreError>
where
    E: PgExecutor<'e>,
{
    let mut rows = stored_rows(executor, &ReviewFilter::default(), Some(digest), 1).await?;
    Ok(rows.pop().map(|row| StoredDoc {
        review_reason: row.review_reason.clone(),
        approved_at: row.approved_at,
        item: row.into_item(),
    }))
}
