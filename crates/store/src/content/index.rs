//! The approved-document index the readiness audit reads (D-F5).
//!
//! `cadus_core::readiness` names no table and opens no connection (R3), so it
//! asks a [`ContentIndex`](cadus_core::readiness::ContentIndex) instead. This is
//! that index over `content_store`.
//!
//! # One statement, and why the whole table
//!
//! The read is ONE grouped statement over the approved rows. It reads the whole
//! table on purpose: the audit answers a question about every knowledge point
//! of the plan, and the plan is composed before the caller knows which
//! knowledge points it holds. The row count is the count of documents a HUMAN
//! approved (C6), which is zero on a fresh deployment (audit finding h) and
//! four per knowledge point when a course is fully authored.
//!
//! The statement carries no tenant binding, because `content_store` holds
//! curriculum content and not learner data: it stands outside row-level
//! security and the runtime role holds SELECT on it
//! (`0006_grants_rls.sql`, `docs/SCHEMA.md` finding #14).

use cadus_core::readiness::MapContent;
use sqlx::PgExecutor;

use crate::StoreError;

/// The approved documents of every knowledge point, by kind.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn approved_index<'e, E>(executor: E) -> Result<MapContent, StoreError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query!(
        r#"
        SELECT kp_id AS "kp_id!", kind AS "kind!", count(*) AS "documents!"
        FROM content_store
        WHERE status = 'approved'
        GROUP BY kp_id, kind
        "#,
    )
    .fetch_all(executor)
    .await?;

    let mut index = MapContent::default();
    for row in rows {
        index.insert(
            row.kp_id,
            row.kind,
            usize::try_from(row.documents).unwrap_or(0),
        );
    }
    Ok(index)
}
