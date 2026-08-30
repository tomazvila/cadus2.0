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
//! The read takes any executor, because `cadus_app` holds SELECT on the table
//! and the serve path is a request-tier read. The three writes take [`Admin`],
//! because `cadus_app` holds nothing else (`docs/SCHEMA.md`, finding #14).

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
    /// The content address of `body`. Approval binds to it (C6).
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
/// an error and not a rewrite: the digest is the content address of the body,
/// so the body is the same body, and a human may have approved or rejected it
/// already. `ON CONFLICT DO NOTHING` leaves that verdict alone.
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
            (digest, kp_id, kind, body, status, authoring_attempts, authoring_cost_usd)
        VALUES ($1, $2, $3, $4, $5, $6, $7::text::numeric)
        ON CONFLICT (digest) DO NOTHING
        "#,
        doc.digest,
        doc.kp_id,
        doc.kind,
        doc.body,
        STATUS_PENDING,
        i32::try_from(doc.authoring_attempts).unwrap_or(i32::MAX),
        doc.cost_usd,
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

/// The typed answer to a review write whose digest is not in the table.
fn missing(digest: &str) -> StoreError {
    StoreError::NotFound {
        entity: ENTITY,
        key: digest.to_string(),
    }
}
