//! The D-O3 content read: one approved `content_store` document per knowledge
//! point and kind.
//!
//! Requirements: C6 (a `pending` or `rejected` document is never served), L4
//! (teach under 150 ms), L5 (hint under 150 ms), R4 (the request tier does local
//! work and database I/O only), T1 (the teach and hint paths spend no token).
//!
//! Spec: `docs/reference/web-service-1.0-spec.md` section 4.3, last paragraph —
//! "D-O3 is a separate single-statement path: one `content_store` read by
//! `(kp_id, kind, status='approved')`, cacheable in memory by digest, no
//! transaction of its own."
//!
//! [`approved_template`](crate::pool::approved_template) is the same read for
//! `kind = 'template'`, and it stays where the pool code needs it. This module
//! serves the two kinds the M5 request tier reads: `teach` and `hint_ladder`.
//! Kind `diagnosis` joins them in unit U9.
//!
//! The reader gives the caller the digest with the body, so a caller that caches
//! keys the cache by the digest and never by the knowledge point: an edited body
//! is a new digest with its own approval (C6).

use serde_json::Value as Json;
use sqlx::PgExecutor;

use crate::StoreError;

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
