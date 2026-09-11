//! Task-scoped integrated hint state, replayed from authoritative reveal events.
use crate::StoreError;
use cadus_core::event::Event;
use sqlx::{
    Postgres, Transaction,
    types::{Json, Uuid},
};
use std::collections::BTreeMap;

/// Read only reveal rows for this tenant/session/task/item identity.
///
/// # Errors
/// Return a database or event decoding error.
pub async fn hints_used(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    session: &str,
    task_id: &str,
    digest: &str,
) -> Result<BTreeMap<String, usize>, StoreError> {
    let rows = sqlx::query_scalar::<_, Json<Event>>(
        "SELECT payload FROM events WHERE user_id = $1 AND type = 'integrated_hint_revealed' AND payload->>'session' = $2 AND payload->>'task_id' = $3 AND payload->>'item_digest' = $4 ORDER BY seq"
    ).bind(user_id).bind(session).bind(task_id).bind(digest)
        .fetch_all(&mut **tx).await?;
    let mut counts = BTreeMap::<String, usize>::new();
    for row in rows {
        if let Event::IntegratedHintRevealed(reveal) = row.0 {
            let used = counts.entry(reveal.field).or_default();
            *used = (*used).max(reveal.index.saturating_add(1));
        }
    }
    Ok(counts)
}

/// Load the committed verdict by the same indexed idempotency key as the append.
///
/// # Errors
/// Return a database or event decoding error.
pub async fn attempt(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    key: &str,
) -> Result<Option<cadus_core::event::IntegratedAttempt>, StoreError> {
    let row = sqlx::query_scalar::<_, Json<Event>>(
        "SELECT payload FROM events WHERE user_id = $1 AND attempt_id = $2",
    )
    .bind(user_id)
    .bind(key)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.and_then(|row| match row.0 {
        Event::IntegratedAttempt(attempt) => Some(attempt),
        _ => None,
    }))
}
