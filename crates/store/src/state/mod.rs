//! The D-S6 state row, the per-tenant advisory lock, the event log, and the fold.
//!
//! Requirements: C2 (the event log is append-only), C3 (every statement runs
//! inside `begin_tenant`), D4 (`through_seq` is the fold cursor), D-S6 (one hot
//! state document per learner), R2 (compile-time checked queries).
//!
//! Spec: `docs/reference/web-service-1.0-spec.md` section 4. This file is the SQL
//! half of unit U6. The document itself is `cadus_web::state`, because it is web
//! scratch and no other tier reads it.
//!
//! # The lock
//!
//! Section 4.2 pins the shape: one transaction-scoped advisory lock per
//! `(user, 'web_state')`, bounded by `lock_timeout`, derived in exactly one
//! place. [`web_state_lock_key`] is that place. The lock serializes the
//! read-modify-write of a second browser tab, and it also makes the dense
//! per-user `seq` of [`append_event`] safe: two appends of one tenant never
//! compute the same next `seq`, because the second one waits.
//!
//! A row-level `FOR UPDATE` cannot replace it. A learner whose `web_states` row
//! does not exist yet has no row to lock, so two first serves both insert.
//!
//! # The fold
//!
//! [`project_current`] reads. [`project_and_save`] reads and writes the
//! `learner_models` row. A read route calls the first one, so listing a plan
//! writes nothing (spec section 9, trap W3).
//!
//! Both take the full-replay branch when the cache is absent, when the cached
//! `projector_version` or `config_hash` drifted, or when the events after the
//! cursor hold a `regraded` (spec section 4.3, `projector-1.0-spec.md:299`).
//!
//! # The session view (M5 review 1, findings F15 and F18)
//!
//! D-S2 says the log grows forever. Before this fix every learner route read
//! the WHOLE log and folded it, twice on the serve path and three times on the
//! grade path, so the cost of one request grew with the lifetime event count and
//! passed the whole 150 ms L1 budget at about 20,000 events.
//!
//! [`SessionView`] is the fix. It is the second cached document of D4: the
//! whole-log maps every route needs beside the learner model — the enrollment
//! stack, `learned_at`, `last_drill_at`, the closed task ids, the active study
//! days, and the open session. It sits in the `learner_models.session_view`
//! column, it folds forward from `through_seq` with the same cursor the model
//! uses, and it replays in full under the same rules. A request that adds no
//! event therefore reads ONE event row, the cursor line:
//! [`project_current`] answers from the cached model and the cached view.
//!
//! [`load_events_after`] is the read that makes the forward fold cheap, and
//! `crates/store/tests/bench_long_log.rs` is the gate that keeps it cheap.
//!
//! [`load_session_view`] reads the view alone, under the same two branches. The
//! grade path calls it for the repeat-fail map, which no session window holds
//! (M5 review 2, finding V2).

mod fold;
mod view;

use cadus_core::event::{Event, SchemaVersion};
use serde_json::Value as Json;
use sqlx::types::chrono::{DateTime, Utc};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

pub use fold::{
    CachedModel, Projection, load_learner_model, load_session_view, project_and_save,
    project_current,
};
pub use view::{QUIZ_HIGH_SCORE, SESSION_VIEW_VERSION, SessionView};

use crate::StoreError;

/// The `events.v` value of every event this build writes, as the `smallint`
/// column holds it.
const EVENT_VERSION: i16 = SchemaVersion.get() as i16;

const _: () = assert!(
    SchemaVersion.get() <= i16::MAX as i64,
    "events.v is a smallint"
);

/// The first key of the `web_state` advisory lock: the ASCII bytes of `web`.
///
/// The learner-event lock of 1.0 is a different namespace, so the two never
/// collide (`cadus_web/webstate.py:52-58`).
pub const WEB_STATE_LOCK_NAMESPACE: i32 = 0x0077_6562;

/// How long a caller waits for the advisory lock before Postgres reports 55P03.
///
/// The whole grade transaction is local CPU plus Postgres now (spec section
/// 4.2), so a holder that runs longer than this is stuck, not slow.
pub const LOCK_TIMEOUT_MS: i64 = 3000;

/// The two advisory-lock keys of one tenant's `web_state`.
///
/// The second key folds the 16 bytes of the uuid into 32 bits. Two learners can
/// therefore share a key. That costs one serialized write between two accounts
/// and never a wrong answer, because every statement under the lock still runs
/// inside `begin_tenant` (C3).
#[must_use]
pub fn web_state_lock_key(user_id: Uuid) -> (i32, i32) {
    let mut folded: u32 = 0;
    for chunk in user_id.as_bytes().chunks(4) {
        let mut word: u32 = 0;
        for byte in chunk {
            word = (word << 8) | u32::from(*byte);
        }
        folded ^= word;
    }
    (WEB_STATE_LOCK_NAMESPACE, folded as i32)
}

/// Take this tenant's `web_state` advisory lock for the rest of the transaction.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the wait passes [`LOCK_TIMEOUT_MS`]
/// (SQLSTATE 55P03) or the statement fails.
pub async fn lock_web_state(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> Result<(), StoreError> {
    let (class, key) = web_state_lock_key(user_id);
    sqlx::query!(
        "SELECT set_config('lock_timeout', $1, true)",
        LOCK_TIMEOUT_MS.to_string()
    )
    .fetch_one(&mut **tx)
    .await?;
    sqlx::query!("SELECT pg_advisory_xact_lock($1, $2)", class, key)
        .fetch_one(&mut **tx)
        .await?;
    Ok(())
}

/// Read this tenant's D-S6 document. `None` means the learner has none yet.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn load_web_state(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> Result<Option<Json>, StoreError> {
    let row = sqlx::query!(
        r#"SELECT doc AS "doc!" FROM web_states WHERE user_id = $1"#,
        user_id
    )
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.map(|row| row.doc))
}

/// Write this tenant's D-S6 document.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn save_web_state(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    doc: &Json,
) -> Result<(), StoreError> {
    sqlx::query!(
        r#"
        INSERT INTO web_states (user_id, doc) VALUES ($1, $2)
        ON CONFLICT (user_id) DO UPDATE SET doc = EXCLUDED.doc, updated_at = now()
        "#,
        user_id,
        doc
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Drop this tenant's D-S6 document. The scratch is loss-tolerant: an enroll and
/// a session end both clear it (`cadus_web/api.py:833`, `:906`).
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn clear_web_state(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> Result<(), StoreError> {
    sqlx::query!("DELETE FROM web_states WHERE user_id = $1", user_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Read this tenant's in-progress placement diagnostic.
///
/// `None` means no diagnostic is open, which every `/api/diag/*` route answers
/// with `409 no_diagnostic`. The document is `cadus_core::diagnostic::DiagState`,
/// and the decode stays with the caller, because the store crate holds no
/// pedagogy.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn load_diag_state(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> Result<Option<Json>, StoreError> {
    let row = sqlx::query!(
        r#"SELECT state AS "state!" FROM diag_states WHERE user_id = $1"#,
        user_id
    )
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.map(|row| row.state))
}

/// Write this tenant's in-progress placement diagnostic.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn save_diag_state(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    state: &Json,
) -> Result<(), StoreError> {
    sqlx::query!(
        r#"
        INSERT INTO diag_states (user_id, state) VALUES ($1, $2)
        ON CONFLICT (user_id) DO UPDATE SET state = EXCLUDED.state, updated_at = now()
        "#,
        user_id,
        state
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Drop this tenant's in-progress placement diagnostic.
///
/// `POST /api/diag/finish` calls it in the transaction that appends
/// `diagnostic_placed`, so a placement and its scratch cannot disagree.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn clear_diag_state(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> Result<(), StoreError> {
    sqlx::query!("DELETE FROM diag_states WHERE user_id = $1", user_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// One row of the append-only log, with its dense per-user line number.
#[derive(Debug, Clone, PartialEq)]
pub struct EventRow {
    /// The dense per-user line number.
    pub seq: i64,
    /// The event document.
    pub event: Event,
}

/// Read the events of one tenant ABOVE `after_seq`, in `seq` order.
///
/// `after_seq` of 0 reads the whole log, because the first `seq` is 1. The
/// `(user_id, seq)` primary key serves the range, so the cost of the read is the
/// count of the rows it returns and never the length of the log (F15, F18).
///
/// The decode reads the `jsonb` buffer straight into [`Event`] with
/// `sqlx::types::Json`. The older form went through `serde_json::Value` and cost
/// 45 ms more per 20,000 rows, because it built a whole value tree and then
/// walked it a second time.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails or when a payload is not
/// an event of this build.
pub async fn load_events_after(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    after_seq: i64,
) -> Result<Vec<EventRow>, StoreError> {
    let rows = sqlx::query!(
        r#"
        SELECT seq AS "seq!", payload AS "payload!: sqlx::types::Json<Event>"
        FROM events WHERE user_id = $1 AND seq > $2 ORDER BY seq
        "#,
        user_id,
        after_seq
    )
    .fetch_all(&mut **tx)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| EventRow {
            seq: row.seq,
            event: row.payload.0,
        })
        .collect())
}

/// Read the whole log of one tenant, in `seq` order.
///
/// The cost of this read grows with the length of the log, so no route on a
/// learner path calls it (F15, F18). `GET /api/export` and the full-replay
/// branch of [`project_current`] are the two callers that need every row.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails or when a payload is not
/// an event of this build.
pub async fn load_events(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> Result<Vec<EventRow>, StoreError> {
    load_events_after(tx, user_id, 0).await
}

/// Append one event at the next dense `seq` of this tenant.
///
/// The answer is the `seq` the row took, or `None` when the partial unique index
/// on `(user_id, attempt_id)` made the INSERT a no-op (FR-14 idempotency, spec
/// section 4.3 step 6). The caller holds [`lock_web_state`], so the `MAX(seq)`
/// read and the INSERT are serialized for this tenant.
///
/// The event goes into the `payload` column through [`sqlx::types::Json`], so
/// the serializer runs inside the encoder of the statement.
///
/// # Errors
///
/// Returns [`StoreError::Document`] when the event timestamp is outside the
/// range of `timestamptz` and [`StoreError::Db`] when the event does not
/// serialize or the statement fails.
pub async fn append_event(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    event: &Event,
    attempt_id: Option<&str>,
) -> Result<Option<i64>, StoreError> {
    let ts = DateTime::<Utc>::from_timestamp_micros(event.ts().micros()).ok_or_else(|| {
        StoreError::Document(format!(
            "the event timestamp {} is outside the representable range",
            event.ts().micros()
        ))
    })?;

    let seq = sqlx::query_scalar!(
        r#"
        INSERT INTO events (user_id, seq, ts, type, session_id, v, attempt_id, payload)
        SELECT $1, COALESCE(MAX(e.seq), 0) + 1, $2, $3, $4, $5, $6, $7
        FROM events e WHERE e.user_id = $1
        ON CONFLICT (user_id, attempt_id) WHERE attempt_id IS NOT NULL DO NOTHING
        RETURNING seq
        "#,
        user_id,
        ts,
        event.type_name(),
        event.session(),
        EVENT_VERSION,
        attempt_id,
        // `as _`: the macro maps a `jsonb` parameter to `serde_json::Value`,
        // and `Json<T>` writes the same wire form.
        sqlx::types::Json(event) as _
    )
    .fetch_optional(&mut **tx)
    .await?;
    Ok(seq)
}
