//! The D-O3 content read and the C6 admin write path of `content_store`.
//!
//! Requirements: C6 (a `pending` or `rejected` document is never served, and
//! approval binds to the digest), L4 (teach under 150 ms), L5 (hint under
//! 150 ms), R4 (the request tier does local work and database I/O only), T1 (the
//! teach and hint paths spend no token), T3 (the attempts and the money of one
//! authored document).
//!
//! Spec: `docs/reference/web-service-1.0-spec.md` section 4.3, last paragraph —
//! "D-O3 is a separate single-statement path: one `content_store` read by
//! `(kp_id, kind, status='approved')`, cacheable in memory by digest, no
//! transaction of its own." The write half is
//! `docs/reference/authoring-and-spa-1.0-spec.md` section 3.2 and row R4 of
//! section 7.
//!
//! [`approved_template`](crate::pool::approved_template) is the same read for
//! `kind = 'template'`, and it stays where the pool code needs it. This module
//! serves the two kinds the M5 request tier reads: `teach` and `hint_ladder`.
//! Kind `diagnosis` joins them in unit U9.
//!
//! The reader gives the caller the digest with the body, so a caller that caches
//! keys the cache by the digest and never by the knowledge point: an edited body
//! is a new digest with its own approval (C6).
//!
//! # The two paths of this module
//!
//! The serve reads take any executor, because `cadus_app` holds SELECT on the
//! table and the serve path is a request-tier read. The three writes take
//! [`Admin`], because `cadus_app` holds nothing else (`docs/SCHEMA.md`, finding
//! #14). [`verdict`] stands between them: it is a read, and it takes [`Db`],
//! because the authoring job runs it on its own write path.

use serde_json::Value as Json;
use sqlx::PgExecutor;
use sqlx::types::chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{Db, StoreError};

/// The `content_store.kind` of an authored teach page (L4).
pub const KIND_TEACH: &str = "teach";

/// The `content_store.kind` of an authored hint ladder (L5).
pub const KIND_HINT_LADDER: &str = "hint_ladder";

/// One approved authored document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovedDoc {
    /// The content address the approval binds to (C6).
    pub digest: String,
    /// The document body, as `content_store.body` holds it.
    pub body: Json,
}

/// The newest approved document of one knowledge point and kind (C6, D-O3).
///
/// `kp_id` is the serving key [`cadus_core::pool::kp_key`] writes:
/// `"<topic_id>/<kp_id>"`.
///
/// The newest approval wins, and the digest breaks a tie, so two rows approved in
/// the same statement give one stable answer. The order matches
/// [`approved_template`](crate::pool::approved_template), so the two reads cannot
/// disagree about which document is current.
///
/// The statement carries no tenant binding, because `content_store` holds
/// curriculum content and not learner data: it is outside row-level security and
/// the runtime role holds SELECT on it (`0006_grants_rls.sql`, finding #14).
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn approved_document<'e, E>(
    executor: E,
    kp_id: &str,
    kind: &str,
) -> Result<Option<ApprovedDoc>, StoreError>
where
    E: PgExecutor<'e>,
{
    let row = sqlx::query!(
        r#"
        SELECT digest AS "digest!", body AS "body!"
        FROM content_store
        WHERE kp_id = $1 AND kind = $2 AND status = 'approved'
        ORDER BY approved_at DESC NULLS LAST, created_at DESC, digest
        LIMIT 1
        "#,
        kp_id,
        kind,
    )
    .fetch_optional(executor)
    .await?;

    Ok(row.map(|row| ApprovedDoc {
        digest: row.digest,
        body: row.body,
    }))
}

// --------------------------------------------------------------------------
// R4: the admin write path
// --------------------------------------------------------------------------

/// The `content_store.kind` of an authored problem template (A1).
pub const KIND_TEMPLATE: &str = "template";

/// The `content_store.kind` of an authored miss diagnosis (A4).
pub const KIND_DIAGNOSIS: &str = "diagnosis";

/// The status of a document that waits for a human (C6).
pub const STATUS_PENDING: &str = "pending";

/// The status of a document a human approved. Only this status serves (C6).
pub const STATUS_APPROVED: &str = "approved";

/// The status of a document a human refused (C6).
pub const STATUS_REJECTED: &str = "rejected";

/// The name this module gives `content_store` in [`StoreError::NotFound`].
pub const ENTITY: &str = "content_store";

/// The admin write path of `content_store` (C6, `docs/SCHEMA.md` finding #14).
///
/// `cadus_app` holds SELECT on `content_store` and nothing else, so the request
/// tier cannot insert a row that already carries `status = 'approved'` and
/// cannot rewrite an approved body in place. The authoring job and the two
/// review writes therefore need a connection of `cadus_admin`.
///
/// The wrapper is that path, and it is a separate type for one reason: a call
/// site reads `Admin::new(&db)` and says which connection it takes. A function
/// that took a bare [`Db`] takes the request tier's pool with no complaint and
/// fails at the server with SQLSTATE 42501 instead.
///
/// The wrapper does not test the role: the grant does that, and
/// `crates/store/tests/content_admin.rs` pins both halves.
#[derive(Debug, Clone, Copy)]
pub struct Admin<'a> {
    db: &'a Db,
}

impl<'a> Admin<'a> {
    /// Name `db` as the admin connection of the content store.
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// The handle behind this path.
    #[must_use]
    pub const fn db(self) -> &'a Db {
        self.db
    }
}

/// One authored document to insert as `pending` (C6, T3).
#[derive(Debug, Clone)]
pub struct NewDocument<'a> {
    /// The content address of the document. Approval binds to it (C6).
    ///
    /// The key of the table is the knowledge point, the kind, AND the body, so
    /// the digest covers all three. A digest of the body alone gives two
    /// knowledge points that earn one body a single primary key, and
    /// [`insert_pending`] then drops the second document with no error (M6
    /// review finding F1). The writer computes the value in ONE function:
    /// `cadus_worker::authoring::job::document_digest`.
    pub digest: &'a str,
    /// The serving key `cadus_core::pool::kp_key` writes: `"<topic>/<kp>"`.
    pub kp_id: &'a str,
    /// One of [`KIND_TEMPLATE`], [`KIND_TEACH`], [`KIND_HINT_LADDER`],
    /// [`KIND_DIAGNOSIS`].
    pub kind: &'a str,
    /// The document body.
    pub body: &'a Json,
    /// How many model calls this document cost (T3). The column is `integer`,
    /// so a count above `i32::MAX` stores as `i32::MAX`.
    pub authoring_attempts: u32,
    /// The money the document cost, as the decimal TEXT of the provider (T3).
    /// `None` writes NULL. The value never passes through a float: the column
    /// is `numeric(12,6)` and the cast is `text::numeric`, exactly as
    /// `model_call_log.cost_usd` is written.
    pub cost_usd: Option<&'a str>,
    /// The digest of the PROMPT that authored this document (spec section 2.2,
    /// "Prompt digest").
    ///
    /// The value is `cadus_worker::authoring::prompt::prompt_digest` of the
    /// kind. It is a column and never part of [`digest`](Self::digest),
    /// because the C6 approval binds to the CONTENT: a prompt edit marks the
    /// affected rows for re-authoring and never unapproves one.
    ///
    /// `None` writes NULL, which means "the prompt is not recorded". A NULL row
    /// is never stale (M6 review finding F4).
    pub prompt_digest: Option<&'a str>,
}

/// The state of one document after a review write (C6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    /// The digest the write addressed.
    pub digest: String,
    /// The new `content_store.status`.
    pub status: String,
    /// Who approved the document. `None` after a rejection.
    pub approved_by: Option<Uuid>,
    /// When the document was approved. `None` after a rejection.
    pub approved_at: Option<DateTime<Utc>>,
}

/// Insert one verified document as `pending` (C6, spec section 2.2, step 4).
///
/// Returns `true` when the row is new. A digest the table already holds is not
/// an error and not a rewrite: the digest is the content address of the
/// knowledge point, the kind and the body, so the document is the same
/// document, and a human may have approved or rejected it already.
/// `ON CONFLICT DO NOTHING` leaves that verdict alone.
///
/// A caller that answers to an operator reads that verdict with [`verdict`]: a
/// pass that reproduces a REJECTED body stored nothing, and it must say so
/// instead of counting a document it did not write (M6 review finding F6).
///
/// The row always enters as `pending`. There is no parameter for the status,
/// because a caller that writes `approved` is the review gate C6 exists to
/// prevent.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails, which includes SQLSTATE
/// 42501 when `admin` names a connection of the runtime role, and
/// [`StoreError::Timeout`] when the client-side bound expires.
pub async fn insert_pending(admin: Admin<'_>, doc: &NewDocument<'_>) -> Result<bool, StoreError> {
    let db = admin.db();
    let query = sqlx::query!(
        r#"
        INSERT INTO content_store
            (digest, kp_id, kind, body, status, authoring_attempts, authoring_cost_usd,
             prompt_digest)
        VALUES ($1, $2, $3, $4, $5, $6, $7::text::numeric, $8)
        ON CONFLICT (digest) DO NOTHING
        "#,
        doc.digest,
        doc.kp_id,
        doc.kind,
        doc.body,
        STATUS_PENDING,
        i32::try_from(doc.authoring_attempts).unwrap_or(i32::MAX),
        doc.cost_usd,
        doc.prompt_digest,
    )
    .execute(db.pool());
    Ok(crate::bounded(db, query).await?.rows_affected() == 1)
}

/// Approve one document by its digest (C6).
///
/// Approval binds to the digest and to nothing else: the statement names the
/// row by its content address, so an edited body is a new digest with its own
/// approval and no approval carries over. This replaces 1.0's re-derivation of
/// an `approved_digest` inside the payload
/// (`scripts/review_templates.py:176-198`).
///
/// The call is idempotent. `COALESCE` keeps the first stamp, so a second
/// approval of the same digest returns the same `approved_by` and the same
/// `approved_at` and writes no new value. An operator who approves a document
/// twice therefore reads one approval and not two.
///
/// An approval after a rejection is allowed and stamps the row again: a
/// reviewer who refused a digest by mistake needs a way back, and the digest
/// still addresses the body that was reviewed.
///
/// # Errors
///
/// Returns [`StoreError::NotFound`] when the table holds no row with that
/// digest. Returns [`StoreError::Db`] or [`StoreError::Timeout`] as
/// [`insert_pending`] does.
pub async fn approve(
    admin: Admin<'_>,
    digest: &str,
    approved_by: Option<Uuid>,
) -> Result<Decision, StoreError> {
    let db = admin.db();
    let query = sqlx::query!(
        r#"
        UPDATE content_store
        SET status = $2,
            approved_by = COALESCE(approved_by, $3),
            approved_at = COALESCE(approved_at, now())
        WHERE digest = $1
        RETURNING digest AS "digest!", status AS "status!", approved_by, approved_at
        "#,
        digest,
        STATUS_APPROVED,
        approved_by,
    )
    .fetch_optional(db.pool());
    let row = crate::bounded(db, query)
        .await?
        .ok_or_else(|| missing(digest))?;
    Ok(Decision {
        digest: row.digest,
        status: row.status,
        approved_by: row.approved_by,
        approved_at: row.approved_at,
    })
}

/// Reject one document by its digest, with the reason the reviewer gave (C6).
///
/// The row keeps its body, so the digest still addresses what the reviewer
/// read. The status stops it serving: [`approved_document`] reads
/// `status = 'approved'` only.
///
/// The write clears `approved_by` and `approved_at`, because those two columns
/// carry the approval and the approval no longer holds. A later [`approve`] of
/// the same digest therefore stamps a fresh approval and never an old one.
///
/// The reason overwrites an earlier reason: a second review of the same digest
/// gives the current reason, not the first one.
///
/// The statement changes ONE row: `digest` is the primary key of the table, so
/// the WHERE clause addresses one row and every other document of the same
/// knowledge point and kind keeps the status it had.
/// `crates/store/tests/content_admin.rs` pins that with three seeded rows (M6
/// review finding F26).
///
/// # Errors
///
/// Returns [`StoreError::NotFound`] when the table holds no row with that
/// digest. Returns [`StoreError::Db`] or [`StoreError::Timeout`] as
/// [`insert_pending`] does.
pub async fn reject(admin: Admin<'_>, digest: &str, reason: &str) -> Result<Decision, StoreError> {
    let db = admin.db();
    let query = sqlx::query!(
        r#"
        UPDATE content_store
        SET status = $2, review_reason = $3, approved_by = NULL, approved_at = NULL
        WHERE digest = $1
        RETURNING digest AS "digest!", status AS "status!", approved_by, approved_at
        "#,
        digest,
        STATUS_REJECTED,
        reason,
    )
    .fetch_optional(db.pool());
    let row = crate::bounded(db, query)
        .await?
        .ok_or_else(|| missing(digest))?;
    Ok(Decision {
        digest: row.digest,
        status: row.status,
        approved_by: row.approved_by,
        approved_at: row.approved_at,
    })
}

/// The review verdict the table holds for one digest (C6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    /// The `content_store.status` of the row.
    pub status: String,
    /// The reason of the last rejection, when a reviewer wrote one.
    pub review_reason: Option<String>,
}

/// The verdict of one digest, for the writer that collided with it (C6).
///
/// [`insert_pending`] answers `false` when the table already holds the digest,
/// and the digest is the content address of the knowledge point, the kind and
/// the body. A collision therefore says that a reviewer saw this exact document
/// already, and this read says what the reviewer did with it. The authoring job
/// reads it to tell a duplicate `pending` document from a body a human refused
/// (M6 review finding F6).
///
/// The read takes [`Db`] and not an executor, because the caller runs it on its
/// write path and the client-side bound of that path applies to it.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails, and
/// [`StoreError::Timeout`] when the client-side bound expires.
pub async fn verdict(db: &Db, digest: &str) -> Result<Option<Verdict>, StoreError> {
    let query = sqlx::query!(
        r#"
        SELECT status AS "status!", review_reason
        FROM content_store
        WHERE digest = $1
        "#,
        digest,
    )
    .fetch_optional(db.pool());
    Ok(crate::bounded(db, query).await?.map(|row| Verdict {
        status: row.status,
        review_reason: row.review_reason,
    }))
}

/// The typed answer to a review write whose digest is not in the table.
fn missing(digest: &str) -> StoreError {
    StoreError::NotFound {
        entity: ENTITY,
        key: digest.to_string(),
    }
}

// --------------------------------------------------------------------------
// R5: the review reads
// --------------------------------------------------------------------------

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
    let rows = sqlx::query!(
        r#"
        SELECT c.digest AS "digest!", c.kp_id AS "kp_id!", c.kind AS "kind!",
               c.status AS "status!", c.authoring_attempts AS "authoring_attempts!",
               c.authoring_cost_usd::text AS "cost_usd?",
               c.created_at AS "created_at!", c.body AS "body!",
               (SELECT count(*) FROM content_store a
                 WHERE a.kp_id = c.kp_id AND a.kind = $4 AND a.status = $5)
                 AS "approved_templates!"
        FROM content_store c
        WHERE ($1::text IS NULL OR c.status = $1)
          AND ($2::text IS NULL OR c.kind = $2)
          AND ($3::text IS NULL OR c.kp_id = $3)
        ORDER BY c.created_at DESC, c.digest
        LIMIT $6
        "#,
        filter.status,
        filter.kind,
        filter.kp_id,
        KIND_TEMPLATE,
        STATUS_APPROVED,
        LIST_LIMIT,
    )
    .fetch_all(executor)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| ReviewItem {
            digest: row.digest,
            kp_id: row.kp_id,
            kind: row.kind,
            status: row.status,
            authoring_attempts: row.authoring_attempts,
            cost_usd: row.cost_usd,
            created_at: row.created_at,
            body: row.body,
            approved_templates: row.approved_templates,
        })
        .collect())
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
    let row = sqlx::query!(
        r#"
        SELECT c.digest AS "digest!", c.kp_id AS "kp_id!", c.kind AS "kind!",
               c.status AS "status!", c.authoring_attempts AS "authoring_attempts!",
               c.authoring_cost_usd::text AS "cost_usd?",
               c.created_at AS "created_at!", c.body AS "body!",
               c.review_reason, c.approved_at,
               (SELECT count(*) FROM content_store a
                 WHERE a.kp_id = c.kp_id AND a.kind = $2 AND a.status = $3)
                 AS "approved_templates!"
        FROM content_store c
        WHERE c.digest = $1
        "#,
        digest,
        KIND_TEMPLATE,
        STATUS_APPROVED,
    )
    .fetch_optional(executor)
    .await?;

    Ok(row.map(|row| StoredDoc {
        item: ReviewItem {
            digest: row.digest,
            kp_id: row.kp_id,
            kind: row.kind,
            status: row.status,
            authoring_attempts: row.authoring_attempts,
            cost_usd: row.cost_usd,
            created_at: row.created_at,
            body: row.body,
            approved_templates: row.approved_templates,
        },
        review_reason: row.review_reason,
        approved_at: row.approved_at,
    }))
}
