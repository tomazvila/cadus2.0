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

use cadus_core::event::Event;
use cadus_core::learner::LearnerModel;
use cadus_core::projector::{PROJECTOR_VERSION, ProjectionInput, project, project_incremental};
use serde_json::Value as Json;
use sqlx::types::chrono::{DateTime, Utc};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::StoreError;

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

/// One row of the append-only log, with its dense per-user line number.
#[derive(Debug, Clone, PartialEq)]
pub struct EventRow {
    /// The dense per-user line number.
    pub seq: i64,
    /// The event document.
    pub event: Event,
}

/// Read the whole log of one tenant, in `seq` order.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails and
/// [`StoreError::Document`] when a payload is not an event of this build.
pub async fn load_events(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> Result<Vec<EventRow>, StoreError> {
    let rows = sqlx::query!(
        r#"
        SELECT seq AS "seq!", payload AS "payload!"
        FROM events WHERE user_id = $1 ORDER BY seq
        "#,
        user_id
    )
    .fetch_all(&mut **tx)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let event: Event = serde_json::from_value(row.payload)
            .map_err(|err| StoreError::Document(format!("events.seq {}: {err}", row.seq)))?;
        out.push(EventRow {
            seq: row.seq,
            event,
        });
    }
    Ok(out)
}

/// Append one event at the next dense `seq` of this tenant.
///
/// The answer is the `seq` the row took, or `None` when the partial unique index
/// on `(user_id, attempt_id)` made the INSERT a no-op (FR-14 idempotency, spec
/// section 4.3 step 6). The caller holds [`lock_web_state`], so the `MAX(seq)`
/// read and the INSERT are serialized for this tenant.
///
/// # Errors
///
/// Returns [`StoreError::Document`] when the event does not serialize and
/// [`StoreError::Db`] when the statement fails.
pub async fn append_event(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    event: &Event,
    attempt_id: Option<&str>,
) -> Result<Option<i64>, StoreError> {
    let payload = serde_json::to_value(event)
        .map_err(|err| StoreError::Document(format!("the event does not serialize: {err}")))?;
    let ts = DateTime::<Utc>::from_timestamp_micros(event.ts().micros()).ok_or_else(|| {
        StoreError::Document(format!(
            "the event timestamp {} is outside the representable range",
            event.ts().micros()
        ))
    })?;
    let version = i16::try_from(event.v().get()).map_err(|_| {
        StoreError::Document("the schema version does not fit smallint".to_string())
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
        version,
        attempt_id,
        payload
    )
    .fetch_optional(&mut **tx)
    .await?;
    Ok(seq)
}

/// The cached learner model of one tenant, with the three drift fields.
#[derive(Debug, Clone, PartialEq)]
pub struct CachedModel {
    /// The folded model.
    pub model: LearnerModel,
    /// The last `events.seq` folded in (D4).
    pub through_seq: i64,
    /// The `PROJECTOR_VERSION` the row was built with.
    pub projector_version: i32,
    /// The `config_hash` the row was built with.
    pub config_hash: String,
}

/// Read the cached learner model. `None` means the learner has none yet.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails and
/// [`StoreError::Document`] when the document is not a model of this build.
pub async fn load_learner_model(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> Result<Option<CachedModel>, StoreError> {
    let row = sqlx::query!(
        r#"
        SELECT model AS "model!", through_seq AS "through_seq!",
               projector_version AS "projector_version!", config_hash AS "config_hash!"
        FROM learner_models WHERE user_id = $1
        "#,
        user_id
    )
    .fetch_optional(&mut **tx)
    .await?;

    let Some(row) = row else { return Ok(None) };
    let mut model: LearnerModel = serde_json::from_value(row.model)
        .map_err(|err| StoreError::Document(format!("learner_models.model: {err}")))?;
    model.through_seq = Some(row.through_seq);
    Ok(Some(CachedModel {
        model,
        through_seq: row.through_seq,
        projector_version: row.projector_version,
        config_hash: row.config_hash,
    }))
}

/// The result of one fold.
#[derive(Debug, Clone, PartialEq)]
pub struct Projection {
    /// The folded model.
    pub model: LearnerModel,
    /// The `seq` the fold reached. 0 means the log is empty.
    pub through_seq: i64,
    /// Whether the fold took the full-replay branch.
    pub replayed: bool,
}

/// Fold this tenant's log into a learner model WITHOUT writing anything.
///
/// The branch rule is spec section 4.3: full replay when the cache is absent,
/// when `projector_version` or `config_hash` drifted, or when the events after
/// the cursor hold a `regraded`; incremental otherwise.
///
/// # Errors
///
/// Returns [`StoreError::Document`] when the config does not hash,
/// [`StoreError::Projector`] when the fold fails, and [`StoreError::Db`] when a
/// statement fails.
pub async fn project_current(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    input: &ProjectionInput<'_>,
) -> Result<Projection, StoreError> {
    let cached = load_learner_model(tx, user_id).await?;
    let rows = load_events(tx, user_id).await?;
    let config_hash = input
        .cfg
        .config_hash()
        .map_err(|err| StoreError::Document(format!("the config does not hash: {err}")))?;

    let through = cached.as_ref().map_or(0, |cache| cache.through_seq);
    let split = rows.partition_point(|row| row.seq <= through);
    let (prior, new) = rows.split_at(split);

    let replayed = match cached.as_ref() {
        None => true,
        Some(cache) => {
            i64::from(cache.projector_version) != PROJECTOR_VERSION
                || cache.config_hash != config_hash
                || new
                    .iter()
                    .any(|row| matches!(row.event, Event::Regraded(_)))
        }
    };

    let model = if replayed {
        let whole: Vec<Event> = rows.iter().map(|row| row.event.clone()).collect();
        project(&whole, input)?
    } else {
        let prior_events: Vec<Event> = prior.iter().map(|row| row.event.clone()).collect();
        let new_events: Vec<Event> = new.iter().map(|row| row.event.clone()).collect();
        let cache = match cached.as_ref() {
            Some(cache) => &cache.model,
            // Unreachable: `replayed` is true whenever the cache is absent. The
            // match keeps that guarantee without an unwrap (no-panic rule).
            None => return Err(StoreError::Document("the model cache vanished".to_string())),
        };
        project_incremental(cache, &prior_events, &new_events, input)?
    };

    let last = rows.last().map_or(0, |row| row.seq);
    Ok(Projection {
        model,
        through_seq: last,
        replayed,
    })
}

/// Fold the log and write the `learner_models` row (spec section 4.3 step 7).
///
/// # Errors
///
/// The errors of [`project_current`], plus [`StoreError::Db`] when the write
/// fails.
pub async fn project_and_save(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    input: &ProjectionInput<'_>,
    curriculum_hash: Option<&str>,
) -> Result<Projection, StoreError> {
    let projection = project_current(tx, user_id, input).await?;
    let config_hash = input
        .cfg
        .config_hash()
        .map_err(|err| StoreError::Document(format!("the config does not hash: {err}")))?;
    let document = serde_json::to_value(&projection.model)
        .map_err(|err| StoreError::Document(format!("the model does not serialize: {err}")))?;
    let version = i32::try_from(PROJECTOR_VERSION)
        .map_err(|_| StoreError::Document("PROJECTOR_VERSION does not fit int".to_string()))?;

    sqlx::query!(
        r#"
        INSERT INTO learner_models
            (user_id, model, through_seq, projector_version, config_hash, curriculum_hash)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (user_id) DO UPDATE SET
            model = EXCLUDED.model,
            through_seq = EXCLUDED.through_seq,
            projector_version = EXCLUDED.projector_version,
            config_hash = EXCLUDED.config_hash,
            curriculum_hash = EXCLUDED.curriculum_hash,
            built_at = now()
        "#,
        user_id,
        document,
        projection.through_seq,
        version,
        config_hash,
        curriculum_hash
    )
    .execute(&mut **tx)
    .await?;

    Ok(projection)
}
