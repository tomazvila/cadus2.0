//! The `content_store` rows the authoring tests seed and read.

use serde_json::Value;
use sqlx::PgPool;

/// One `content_store` row, as the tests read it.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ContentRow {
    pub digest: String,
    pub kind: String,
    pub status: String,
    #[sqlx(rename = "authoring_attempts")]
    pub attempts: i32,
    pub body: Value,
    pub prompt_digest: Option<String>,
}

/// Every `content_store` row of one knowledge point, by status and digest.
pub async fn content_rows(pool: &PgPool, kp_id: &str) -> Vec<ContentRow> {
    sqlx::query_as::<_, ContentRow>(
        "SELECT digest, kind, status, authoring_attempts, body, prompt_digest
           FROM content_store WHERE kp_id = $1 ORDER BY status, digest",
    )
    .bind(kp_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// Every `content_store` row of one knowledge point and kind, by status and
/// digest.
pub async fn content_rows_of_kind(pool: &PgPool, kp_id: &str, kind: &str) -> Vec<ContentRow> {
    let mut rows = content_rows(pool, kp_id).await;
    rows.retain(|row| row.kind == kind);
    rows
}

/// The one `content_store` row of this knowledge point and kind, with these
/// columns.
pub async fn assert_one_row(
    pool: &PgPool,
    kp_id: &str,
    kind: &str,
    digest: &str,
    status: &str,
    attempts: i32,
) -> ContentRow {
    let rows = content_rows_of_kind(pool, kp_id, kind).await;
    assert_eq!(rows.len(), 1, "one row of kind {kind}: {rows:?}");
    assert_eq!(rows[0].digest, digest);
    assert_eq!(rows[0].status, status);
    assert_eq!(rows[0].attempts, attempts);
    rows[0].clone()
}

/// One `content_store` row to seed.
#[derive(Debug, Clone)]
pub struct Seed<'a> {
    pub digest: &'a str,
    pub kp_id: &'a str,
    pub kind: &'a str,
    pub status: &'a str,
    pub body: &'a str,
    pub review_reason: Option<&'a str>,
    pub prompt_digest: Option<&'a str>,
}

impl<'a> Seed<'a> {
    /// A row of this digest, knowledge point, kind and status, with an empty
    /// body and no reason and no prompt stamp.
    pub fn new(digest: &'a str, kp_id: &'a str, kind: &'a str, status: &'a str) -> Self {
        Self {
            digest,
            kp_id,
            kind,
            status,
            body: "{}",
            review_reason: None,
            prompt_digest: None,
        }
    }

    /// The same row with this body.
    pub fn body(mut self, body: &'a str) -> Self {
        self.body = body;
        self
    }

    /// The same row with this reviewer's reason.
    pub fn reason(mut self, reason: &'a str) -> Self {
        self.review_reason = Some(reason);
        self
    }

    /// The same row with this prompt stamp.
    pub fn prompt(mut self, prompt_digest: &'a str) -> Self {
        self.prompt_digest = Some(prompt_digest);
        self
    }

    /// Insert the row.
    pub async fn insert(self, pool: &PgPool) {
        sqlx::query(
            "INSERT INTO content_store
                 (digest, kp_id, kind, body, status, review_reason, prompt_digest)
             VALUES ($1, $2, $3, $4::text::jsonb, $5, $6, $7)",
        )
        .bind(self.digest)
        .bind(self.kp_id)
        .bind(self.kind)
        .bind(self.body)
        .bind(self.status)
        .bind(self.review_reason)
        .bind(self.prompt_digest)
        .execute(pool)
        .await
        .unwrap();
    }
}

/// Seed one `content_store` row of this kind and status, with an empty body.
pub async fn seed_content(pool: &PgPool, digest: &str, kp_id: &str, kind: &str, status: &str) {
    Seed::new(digest, kp_id, kind, status).insert(pool).await;
}

/// Set the status of one `content_store` row, as an operator does (C6).
pub async fn set_content_status(pool: &PgPool, digest: &str, status: &str) {
    let changed = sqlx::query("UPDATE content_store SET status = $2 WHERE digest = $1")
        .bind(digest)
        .bind(status)
        .execute(pool)
        .await
        .unwrap()
        .rows_affected();
    assert_eq!(changed, 1, "the operator changed exactly one content row");
}

/// The `model_call_log` rows of this purpose, as `(count, non-NULL user ids)`.
pub async fn ledger_shape(pool: &PgPool, purpose: &str) -> (i64, i64) {
    sqlx::query_as::<_, (i64, i64)>(
        "SELECT count(*), count(user_id) FROM model_call_log WHERE purpose = $1",
    )
    .bind(purpose)
    .fetch_one(pool)
    .await
    .unwrap()
}
