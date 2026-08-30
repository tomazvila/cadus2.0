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

use std::collections::{BTreeMap, BTreeSet};

use cadus_core::event::{EnrollReason, Event, TaskType};
use cadus_core::learner::LearnerModel;
use cadus_core::projector::{PROJECTOR_VERSION, ProjectionInput, project, project_incremental};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use sqlx::types::chrono::{DateTime, NaiveDate, Utc};
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

// --------------------------------------------------------------------------- //
// The session view (D4, second cached document)
// --------------------------------------------------------------------------- //

/// A quiz scored at or above this counts as aced (`service.py:1096`).
pub const QUIZ_HIGH_SCORE: f64 = 0.9;

/// The document version of [`SessionView`].
///
/// A bump makes every stored view stale, so the next read replays the whole log,
/// the way a `projector_version` bump does for the model (D4).
pub const SESSION_VIEW_VERSION: i64 = 1;

/// The whole-log maps of one tenant, folded once and cached beside the model.
///
/// Every field is a LEFT FOLD over the log in `seq` order, so
/// [`SessionView::fold`] over the events after a cursor gives the same document
/// as [`SessionView::of_log`] over the whole log. That property is what lets a
/// request read no event row at all, and
/// `crates/store/src/state.rs::tests::the_forward_fold_equals_the_whole_log_fold`
/// holds it.
///
/// The fold reads the RAW log. A `regraded` event changes no field here, and the
/// three replay rules of [`project_current`] still replay the view, so the view
/// and the model always describe the same `through_seq`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionView {
    /// The document version. It is [`SESSION_VIEW_VERSION`] on a fresh fold.
    pub v: i64,
    /// The open session id: a `session_start` with no later `session_end`.
    pub current_session: Option<String>,
    /// The `events.seq` of the `session_start` that opened [`Self::current_session`].
    pub session_start_seq: Option<i64>,
    /// The sessions that started and did not end. `session_xp` credits all of
    /// them, which is what the per-session scan of 1.0 does.
    pub open_sessions: BTreeSet<String>,
    /// Every session id the log ever opened. `new_session_id` reads it.
    pub session_ids: BTreeSet<String>,
    /// The XP each session credited, before the two-place rounding.
    pub session_xp: BTreeMap<String, f64>,
    /// The enrollment stack, base first and effective last (`service.py:1142`).
    pub enrollment_stack: Vec<String>,
    /// Topic id to the instant it was FIRST passed (`service.py:1086`).
    pub learned_at: BTreeMap<String, i64>,
    /// Topic id to the instant of its last served drill (`service.py:1120`).
    pub last_drill_at: BTreeMap<String, i64>,
    /// The task ids a `review_result` already closed (`service.py:1261`).
    pub closed_task_ids: BTreeSet<String>,
    /// Topic id to the knowledge points a FAILED `lesson_result` stopped at,
    /// over the whole log (`_topic_already_failed`, `service.py:495-513`).
    ///
    /// The repeat-fail peel-back of section 8 reads it. A second failure of one
    /// lesson is only reachable in a LATER session, because a lesson task id is
    /// `{session}-lesson-{topic}` and the closed task stays done for the rest of
    /// its session, so the open-session window cannot answer the question and
    /// this whole-log map is the only place the earlier failure stands (M5
    /// review 2, finding V2).
    ///
    /// A result with no `failed_at_kp` folds nothing: the peel-back names the
    /// key prerequisites of ONE knowledge point, and a topic that authors none
    /// has no prerequisite to peel back to.
    ///
    /// The field carries no `serde` default ON PURPOSE. A document written
    /// before this map does not read back, [`load_learner_model`] then treats it
    /// as absent, and the next fold rebuilds the view from the whole log. That
    /// is why migration 0010 needs no backfill.
    pub lesson_failures: BTreeMap<String, BTreeSet<String>>,
    /// The distinct UTC dates that carry an attempt (`service.py:1099`).
    pub active_study_days: BTreeSet<NaiveDate>,
    /// The trailing run of quizzes scored at or above [`QUIZ_HIGH_SCORE`].
    pub quiz_high_score_streak: i64,
    /// Whether any `diagnostic_placed` event stands in the log.
    pub has_diagnostic: bool,
}

impl Default for SessionView {
    fn default() -> Self {
        Self {
            v: SESSION_VIEW_VERSION,
            current_session: None,
            session_start_seq: None,
            open_sessions: BTreeSet::new(),
            session_ids: BTreeSet::new(),
            session_xp: BTreeMap::new(),
            enrollment_stack: Vec::new(),
            learned_at: BTreeMap::new(),
            last_drill_at: BTreeMap::new(),
            closed_task_ids: BTreeSet::new(),
            lesson_failures: BTreeMap::new(),
            active_study_days: BTreeSet::new(),
            quiz_high_score_streak: 0,
            has_diagnostic: false,
        }
    }
}

impl SessionView {
    /// Fold one event at line `seq` into the view.
    pub fn apply(&mut self, seq: i64, event: &Event) {
        match event {
            Event::SessionStart(body) => {
                self.current_session.clone_from(&body.session);
                self.session_start_seq = body.session.as_ref().map(|_| seq);
                if let Some(session) = body.session.as_ref() {
                    self.session_ids.insert(session.clone());
                    self.open_sessions.insert(session.clone());
                }
            }
            Event::SessionEnd(body) => {
                self.current_session = None;
                self.session_start_seq = None;
                if let Some(session) = body.session.as_ref() {
                    self.open_sessions.remove(session);
                }
            }
            Event::Enrolled(body) => {
                let course = body.course.as_str().to_string();
                match body.reason {
                    Some(EnrollReason::GapFill) => self.enrollment_stack.push(course),
                    Some(EnrollReason::GapReturn) => {
                        if self.enrollment_stack.len() > 1 {
                            self.enrollment_stack.pop();
                        }
                    }
                    None => self.enrollment_stack = vec![course],
                }
            }
            Event::TaskServed(body) => {
                if body.task_type == TaskType::Drill
                    && let Some(topic) = body.topic.as_ref()
                {
                    self.last_drill_at
                        .insert(topic.as_str().to_string(), body.ts.micros());
                }
            }
            Event::Attempt(body) => {
                if let Some(stamp) = DateTime::<Utc>::from_timestamp_micros(body.ts.micros()) {
                    self.active_study_days.insert(stamp.date_naive());
                }
            }
            Event::LessonResult(body) => {
                if body.passed {
                    self.learned_at
                        .entry(body.topic.as_str().to_string())
                        .or_insert_with(|| body.ts.micros());
                } else if let Some(kp) = body.failed_at_kp.as_ref() {
                    self.lesson_failures
                        .entry(body.topic.as_str().to_string())
                        .or_default()
                        .insert(kp.as_str().to_string());
                }
                self.credit(body.xp);
            }
            Event::ReviewResult(body) => {
                if let Some(task_id) = body.task_id.as_ref() {
                    self.closed_task_ids.insert(task_id.clone());
                }
                self.credit(body.xp);
            }
            Event::QuizResult(body) => {
                self.quiz_high_score_streak = if body.score >= QUIZ_HIGH_SCORE {
                    self.quiz_high_score_streak.saturating_add(1)
                } else {
                    0
                };
            }
            Event::DiagnosticPlaced(_) => self.has_diagnostic = true,
            _ => {}
        }
    }

    /// Credit `xp` to every session that is open at this point of the log.
    fn credit(&mut self, xp: f64) {
        for session in &self.open_sessions {
            *self.session_xp.entry(session.clone()).or_insert(0.0) += xp;
        }
    }

    /// Fold the events of `rows` into the view, in `seq` order.
    pub fn fold(&mut self, rows: &[EventRow]) {
        for row in rows {
            self.apply(row.seq, &row.event);
        }
    }

    /// The view of a whole log.
    #[must_use]
    pub fn of_log(rows: &[EventRow]) -> Self {
        let mut view = Self::default();
        view.fold(rows);
        view
    }

    /// Whether a FAILED `lesson_result` already stopped `topic` at `kp`.
    ///
    /// This is the repeat test of the peel-back (`_topic_already_failed`,
    /// `service.py:495-513`). A caller that names no knowledge point gets
    /// `false`: [`Self::lesson_failures`] folds no result without one.
    #[must_use]
    pub fn already_failed(&self, topic: &str, kp: Option<&str>) -> bool {
        let Some(kp) = kp else { return false };
        self.lesson_failures
            .get(topic)
            .is_some_and(|points| points.contains(kp))
    }

    /// The XP the log credits inside one session, rounded to two places.
    #[must_use]
    pub fn xp_in_session(&self, session: &str) -> f64 {
        let total = self.session_xp.get(session).copied().unwrap_or(0.0);
        (total * 100.0).round() / 100.0
    }

    /// The active study days, oldest first.
    #[must_use]
    pub fn study_days(&self) -> Vec<NaiveDate> {
        self.active_study_days.iter().copied().collect()
    }

    /// The next unused session id of `today`: `s_<date><letter>`.
    ///
    /// The letters run `a` to `z` (`service.py:316`). A day that used all 26
    /// gives `z` again, which is what the 1.0 loop does when it falls off.
    #[must_use]
    pub fn new_session_id(&self, today: DateTime<Utc>) -> String {
        let prefix = format!("s_{}", today.date_naive());
        for letter in SESSION_LETTERS.chars() {
            let candidate = format!("{prefix}{letter}");
            if !self.session_ids.contains(&candidate) {
                return candidate;
            }
        }
        format!("{prefix}z")
    }
}

/// The letters a same-day session id takes, in order (`service.py:316`).
const SESSION_LETTERS: &str = "abcdefghijklmnopqrstuvwxyz";

/// The cached learner model of one tenant, with the three drift fields.
#[derive(Debug, Clone, PartialEq)]
pub struct CachedModel {
    /// The folded model.
    pub model: LearnerModel,
    /// The session view of the same `through_seq`. `None` means the row predates
    /// the column or holds a document of another version, and both force the
    /// full-replay branch.
    pub view: Option<SessionView>,
    /// The last `events.seq` folded in (D4).
    pub through_seq: i64,
    /// The `PROJECTOR_VERSION` the row was built with.
    pub projector_version: i32,
    /// The `config_hash` the row was built with.
    pub config_hash: String,
}

/// Read one stored `session_view` document.
///
/// A document of another shape or another version is treated as ABSENT, not as
/// a failure: the caller then folds the view from the log and writes a fresh
/// one. That rule is why a new field of [`SessionView`] needs no migration.
fn decode_view(doc: Option<Json>) -> Option<SessionView> {
    doc.and_then(|doc| serde_json::from_value::<SessionView>(doc).ok())
        .filter(|view| view.v == SESSION_VIEW_VERSION)
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
               projector_version AS "projector_version!", config_hash AS "config_hash!",
               session_view
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
    let view = decode_view(row.session_view);
    Ok(Some(CachedModel {
        model,
        view,
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
    /// The whole-log maps of the same `through_seq`.
    pub view: SessionView,
    /// The `seq` the fold reached. 0 means the log is empty.
    pub through_seq: i64,
    /// Whether the fold took the full-replay branch.
    pub replayed: bool,
}

/// Fold the whole log into a model and a view. The full-replay branch.
fn replay(rows: Vec<EventRow>, input: &ProjectionInput<'_>) -> Result<Projection, StoreError> {
    let view = SessionView::of_log(&rows);
    let last = rows.last().map_or(0, |row| row.seq);
    // The rows are CONSUMED into the event vector. An earlier version cloned
    // them, which cost one clone of every event of the log on every request.
    let events: Vec<Event> = rows.into_iter().map(|row| row.event).collect();
    let model = project(&events, input)?;
    Ok(Projection {
        model,
        view,
        through_seq: last,
        replayed: true,
    })
}

/// Fold this tenant's log into a learner model WITHOUT writing anything.
///
/// The branch rule is spec section 4.3: full replay when the cache is absent,
/// when `projector_version` or `config_hash` drifted, or when the events after
/// the cursor hold a `regraded`; incremental otherwise. This function adds ONE
/// more replay rule: a `through_seq` that names no line of the log is a cache
/// that disagrees with the log. A row that carries no [`SessionView`] is NOT a
/// replay rule — the view folds from the log the incremental branch already
/// reads — so migration 0010 needs no backfill.
///
/// # The three branches, by cost (F15, F18)
///
/// 1. **Nothing new.** The cursor is already at the head of the log and the row
///    carries both documents, so the cached model IS the projection and the
///    cached view IS the view. The read is one `learner_models` row plus a range
///    read of `events` that returns the ONE cursor line. Every read route and
///    the opening of every serve, teach and hint takes this branch.
/// 2. **Incremental.** New events stand after the cursor. `project_incremental`
///    seeds the FIRe states from the cache and replays the earlier events for
///    their light indices, so it needs the whole log. The view folds forward
///    over the new events alone.
/// 3. **Full replay.** One whole-log read and one whole-log fold of both
///    documents.
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
    let config_hash = input
        .cfg
        .config_hash()
        .map_err(|err| StoreError::Document(format!("the config does not hash: {err}")))?;

    // The cache serves a resume only when it was built by this projector and
    // under this config. Those are the two drift rules of spec section 4.3.
    let resume = cached.as_ref().filter(|cache| {
        i64::from(cache.projector_version) == PROJECTOR_VERSION && cache.config_hash == config_hash
    });

    if let Some(cache) = resume {
        // The read starts at the cursor LINE, not after it, so the answer proves
        // the cursor sits on a line the log still holds. A cursor the log cannot
        // show is a cache that disagrees with the log, and the fold replays. The
        // check costs one extra decoded row.
        let window = load_events_after(tx, user_id, cache.through_seq.saturating_sub(1)).await?;
        let anchored = cache.through_seq == 0
            || window
                .first()
                .is_some_and(|row| row.seq == cache.through_seq);
        // A cursor of 0 sits BEFORE the first line, so the whole window is new.
        let first_new = usize::from(cache.through_seq > 0);
        let new_rows: &[EventRow] = window.get(first_new..).unwrap_or_default();
        let corrected = new_rows
            .iter()
            .any(|row| matches!(row.event, Event::Regraded(_)));
        let nothing_new = new_rows.is_empty();
        if anchored && !corrected {
            // Branch 1. It needs BOTH cached documents. A cursor of 0 means
            // the row was written over an empty log, and the light indices of
            // that model were built against the wall clock of that write, so it
            // is not this request's answer.
            if let Some(view) = cache.view.as_ref()
                && nothing_new
                && cache.through_seq >= 1
            {
                let mut model = cache.model.clone();
                model.built_from_ts = Some(input.now);
                // A fresh fold leaves this key out of the document (D4 keeps the
                // cursor in its own column), so the resume spells it the same.
                model.through_seq = None;
                return Ok(Projection {
                    model,
                    view: view.clone(),
                    through_seq: cache.through_seq,
                    replayed: false,
                });
            }

            // Branch 2. `project_incremental` reads the earlier events for
            // their light indices, so this branch needs the whole log. A cursor
            // of 0 already read it: `window` IS the whole log there.
            let rows = if cache.through_seq == 0 {
                window
            } else {
                load_events(tx, user_id).await?
            };
            let split = rows.partition_point(|row| row.seq <= cache.through_seq);
            let last = rows.last().map_or(0, |row| row.seq);
            // A `regraded` may have arrived between the two reads, because a
            // read route holds no lock. The whole-log read is the authority.
            if !rows[split..]
                .iter()
                .any(|row| matches!(row.event, Event::Regraded(_)))
            {
                let view = match cache.view.as_ref() {
                    Some(cached_view) => {
                        let mut view = cached_view.clone();
                        view.fold(&rows[split..]);
                        view
                    }
                    // The column is new (migration 0010), so a row written
                    // before it carries no view. The whole log is already read
                    // here, so the view is folded from it and the MODEL still
                    // resumes from the cache: an absent view costs one fold and
                    // never a full replay of the model.
                    None => SessionView::of_log(&rows),
                };
                let events: Vec<Event> = rows.into_iter().map(|row| row.event).collect();
                let (prior, new) = events.split_at(split);
                let model = project_incremental(&cache.model, prior, new, input)?;
                return Ok(Projection {
                    model,
                    view,
                    through_seq: last,
                    replayed: false,
                });
            }
            return replay(rows, input);
        }
    }

    // Branch 3.
    let rows = load_events(tx, user_id).await?;
    replay(rows, input)
}

/// Read this tenant's [`SessionView`], folded through the head of the log.
///
/// The grade path calls this for the whole-log answers its verdict needs, which
/// the open-session window cannot give (M5 review 2, finding V2). It folds no
/// learner model, so it costs less than [`project_current`].
///
/// # The two branches
///
/// 1. **Resume.** The row carries a view and its cursor sits on a line the log
///    still holds. The read is the `through_seq` and the `session_view` of one
///    `learner_models` row, plus the events from the cursor line up, and the
///    view folds forward over them, so the cost never grows with the lifetime
///    event count (F15, F18).
/// 2. **Full replay.** No row, no view, a cursor the log cannot show, or a
///    `regraded` above the cursor. One whole-log read and one whole-log fold.
///
/// The `projector_version` and the `config_hash` gate the MODEL and not this
/// document. [`SessionView`] folds the raw log and reads no config, so a drift
/// of either one leaves the stored view correct through its own cursor.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when a statement fails or when a payload is not an
/// event of this build.
pub async fn load_session_view(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> Result<SessionView, StoreError> {
    // The read asks for the cursor and the view alone. It leaves the model
    // document in the row, so this call never decodes the FIRe states a second
    // time on a path that already folded them.
    let row = sqlx::query!(
        r#"
        SELECT through_seq AS "through_seq!", session_view
        FROM learner_models WHERE user_id = $1
        "#,
        user_id
    )
    .fetch_optional(&mut **tx)
    .await?;
    let cached = row.and_then(|row| {
        let cursor = row.through_seq;
        decode_view(row.session_view).map(|view| (cursor, view))
    });

    if let Some((through_seq, mut view)) = cached {
        // The read starts at the cursor LINE, not after it, so the answer proves
        // the cursor sits on a line the log still holds. That is the anchor rule
        // of `project_current`, spelled the same way here.
        let window = load_events_after(tx, user_id, through_seq.saturating_sub(1)).await?;
        let anchored = through_seq == 0 || window.first().is_some_and(|row| row.seq == through_seq);
        // A cursor of 0 sits BEFORE the first line, so the whole window is new.
        let first_new = usize::from(through_seq > 0);
        let new_rows: &[EventRow] = window.get(first_new..).unwrap_or_default();
        let corrected = new_rows
            .iter()
            .any(|row| matches!(row.event, Event::Regraded(_)));
        if anchored && !corrected {
            view.fold(new_rows);
            return Ok(view);
        }
    }
    let rows = load_events(tx, user_id).await?;
    Ok(SessionView::of_log(&rows))
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
    let view = serde_json::to_value(&projection.view)
        .map_err(|err| StoreError::Document(format!("the view does not serialize: {err}")))?;
    let version = i32::try_from(PROJECTOR_VERSION)
        .map_err(|_| StoreError::Document("PROJECTOR_VERSION does not fit int".to_string()))?;

    sqlx::query!(
        r#"
        INSERT INTO learner_models
            (user_id, model, through_seq, projector_version, config_hash, curriculum_hash,
             session_view)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (user_id) DO UPDATE SET
            model = EXCLUDED.model,
            through_seq = EXCLUDED.through_seq,
            projector_version = EXCLUDED.projector_version,
            config_hash = EXCLUDED.config_hash,
            curriculum_hash = EXCLUDED.curriculum_hash,
            session_view = EXCLUDED.session_view,
            built_at = now()
        "#,
        user_id,
        document,
        projection.through_seq,
        version,
        config_hash,
        curriculum_hash,
        view
    )
    .execute(&mut **tx)
    .await?;

    Ok(projection)
}

#[cfg(test)]
mod tests {
    use cadus_core::event::{
        Attempt, AttemptProblem, DiagnosticPlaced, EnrollReason, Enrolled, Event, LessonResult,
        QuizResult, ReviewResult, SchemaVersion, Secs, SessionEnd, SessionStart, Slug, TaskServed,
        TaskType, Timestamp, WorkQuality,
    };

    use std::collections::BTreeSet;

    use super::{EventRow, SESSION_VIEW_VERSION, SessionView};

    /// The Unix microsecond instant of 2026-01-01T00:00:00Z.
    const BASE_US: i64 = 1_767_225_600_000_000;

    /// One event at line `seq`.
    fn row(seq: i64, event: Event) -> EventRow {
        EventRow { seq, event }
    }

    /// The instant `minutes` minutes after [`BASE_US`].
    fn at(minutes: i64) -> Timestamp {
        Timestamp::from_micros(BASE_US + minutes * 60_000_000)
    }

    /// A slug of a test fixture.
    fn slug(id: &str) -> Slug {
        Slug::new(id).expect("the fixture slug is valid")
    }

    /// A log that touches every branch of [`SessionView::apply`].
    fn sample_log() -> Vec<EventRow> {
        vec![
            row(
                1,
                Event::Enrolled(Enrolled {
                    ts: at(0),
                    session: None,
                    v: SchemaVersion,
                    course: slug("algebra-1"),
                    reason: None,
                    return_to: None,
                }),
            ),
            row(
                2,
                Event::SessionStart(SessionStart {
                    ts: at(1),
                    session: Some("s_2026-01-01a".to_string()),
                    v: SchemaVersion,
                }),
            ),
            row(
                3,
                Event::TaskServed(TaskServed {
                    ts: at(2),
                    session: Some("s_2026-01-01a".to_string()),
                    v: SchemaVersion,
                    task_id: "s_2026-01-01a-drill-adding-integers".to_string(),
                    task_type: TaskType::Drill,
                    topic: Some(slug("adding-integers")),
                    kp: None,
                    problems: Vec::new(),
                    component_topics: Vec::new(),
                    seed: None,
                }),
            ),
            row(
                4,
                Event::Attempt(Attempt {
                    ts: at(3),
                    session: Some("s_2026-01-01a".to_string()),
                    v: SchemaVersion,
                    attempt_id: "s_2026-01-01a-lesson-adding-integers-1".to_string(),
                    task_id: "s_2026-01-01a-lesson-adding-integers".to_string(),
                    topic: slug("adding-integers"),
                    kp: None,
                    task_type: TaskType::Lesson,
                    problem: AttemptProblem {
                        text: "Compute $1 + 1$.".to_string(),
                        expected: "2".to_string(),
                    },
                    given_answer: "2".to_string(),
                    work: None,
                    answer_kind: None,
                    correct: true,
                    secs: Secs::new(9).expect("nine seconds"),
                    error_tags: Vec::new(),
                    work_quality: WorkQuality::NearlyPerfect,
                    grader_note: None,
                    assisted: false,
                }),
            ),
            row(
                5,
                Event::LessonResult(LessonResult {
                    ts: at(4),
                    session: Some("s_2026-01-01a".to_string()),
                    v: SchemaVersion,
                    topic: slug("adding-integers"),
                    passed: true,
                    failed_at_kp: None,
                    xp: 5.5,
                    quality_tier: WorkQuality::NearlyPerfect,
                    assisted: false,
                }),
            ),
            row(
                6,
                Event::QuizResult(QuizResult {
                    ts: at(5),
                    session: Some("s_2026-01-01a".to_string()),
                    v: SchemaVersion,
                    quiz_id: "q1".to_string(),
                    score: 0.95,
                    per_topic: Vec::new(),
                    xp: 0.0,
                }),
            ),
            row(
                7,
                Event::DiagnosticPlaced(DiagnosticPlaced {
                    ts: at(6),
                    session: Some("s_2026-01-01a".to_string()),
                    v: SchemaVersion,
                    balances: Default::default(),
                    conditional: Vec::new(),
                    refresh: false,
                }),
            ),
            row(
                8,
                Event::SessionEnd(SessionEnd {
                    ts: at(7),
                    session: Some("s_2026-01-01a".to_string()),
                    v: SchemaVersion,
                    xp_earned: 5.5,
                    minutes: 7.0,
                }),
            ),
            row(
                9,
                Event::SessionStart(SessionStart {
                    ts: at(1441),
                    session: Some("s_2026-01-02a".to_string()),
                    v: SchemaVersion,
                }),
            ),
            row(
                10,
                Event::Enrolled(Enrolled {
                    ts: at(1442),
                    session: Some("s_2026-01-02a".to_string()),
                    v: SchemaVersion,
                    course: slug("pre-algebra"),
                    reason: Some(EnrollReason::GapFill),
                    return_to: None,
                }),
            ),
            row(
                11,
                Event::ReviewResult(ReviewResult {
                    ts: at(1443),
                    session: Some("s_2026-01-02a".to_string()),
                    v: SchemaVersion,
                    topic: slug("adding-integers"),
                    passed: true,
                    weighted_score: 1.0,
                    xp: 2.25,
                    quality_tier: WorkQuality::NearlyPerfect,
                    assisted: false,
                    task_id: Some("s_2026-01-02a-review-adding-integers".to_string()),
                }),
            ),
            row(
                12,
                Event::LessonResult(LessonResult {
                    ts: at(1444),
                    session: Some("s_2026-01-02a".to_string()),
                    v: SchemaVersion,
                    topic: slug("adding-integers"),
                    passed: false,
                    failed_at_kp: Some(slug("kp2")),
                    xp: 0.0,
                    quality_tier: WorkQuality::NearlyPassable,
                    assisted: false,
                }),
            ),
        ]
    }

    /// The view of the whole log holds the literal values of the 1.0 scans.
    #[test]
    fn the_whole_log_fold_holds_its_literal_values() {
        let view = SessionView::of_log(&sample_log());
        assert_eq!(view.v, SESSION_VIEW_VERSION);
        assert_eq!(view.current_session.as_deref(), Some("s_2026-01-02a"));
        assert_eq!(view.session_start_seq, Some(9));
        assert_eq!(
            view.enrollment_stack,
            vec!["algebra-1".to_string(), "pre-algebra".to_string()]
        );
        assert_eq!(
            view.learned_at.get("adding-integers"),
            Some(&at(4).micros())
        );
        assert_eq!(
            view.last_drill_at.get("adding-integers"),
            Some(&at(2).micros())
        );
        assert!(
            view.closed_task_ids
                .contains("s_2026-01-02a-review-adding-integers")
        );
        // The failed lesson of line 12 folds into the map; the passed lesson of
        // line 5 folds into `learned_at` and never here (V2).
        assert_eq!(view.lesson_failures.len(), 1);
        assert_eq!(
            view.lesson_failures.get("adding-integers"),
            Some(&BTreeSet::from(["kp2".to_string()]))
        );
        assert_eq!(view.study_days().len(), 1);
        assert_eq!(view.quiz_high_score_streak, 1);
        assert!(view.has_diagnostic);
        // The lesson XP falls inside session a and the review XP inside b.
        assert_eq!(view.xp_in_session("s_2026-01-01a"), 5.5);
        assert_eq!(view.xp_in_session("s_2026-01-02a"), 2.25);
        assert_eq!(view.xp_in_session("s_2026-01-09z"), 0.0);
    }

    /// The forward fold from ANY cut equals the fold of the whole log.
    ///
    /// This is the property [`project_current`] rests on: a request that adds no
    /// event reads the cached view, and a request that adds events folds only
    /// those events into it (F15, F18).
    #[test]
    fn the_forward_fold_equals_the_whole_log_fold() {
        let log = sample_log();
        let whole = SessionView::of_log(&log);
        for cut in 0..=log.len() {
            let mut resumed = SessionView::of_log(&log[..cut]);
            resumed.fold(&log[cut..]);
            assert_eq!(
                resumed, whole,
                "the fold resumed at line {cut} left another document"
            );
        }
    }

    /// One failed lesson, with and without a knowledge point.
    fn failed_lesson(topic: &str, kp: Option<&str>) -> Event {
        Event::LessonResult(LessonResult {
            ts: at(8),
            session: Some("s_2026-01-01a".to_string()),
            v: SchemaVersion,
            topic: slug(topic),
            passed: false,
            failed_at_kp: kp.map(slug),
            xp: 0.0,
            quality_tier: WorkQuality::NearlyPassable,
            assisted: false,
        })
    }

    /// The map folds every FAILED `lesson_result` that names a knowledge point,
    /// and [`SessionView::already_failed`] answers the repeat test from it (V2).
    #[test]
    fn the_failure_map_holds_every_failed_knowledge_point() {
        let log = vec![
            row(1, failed_lesson("adding-integers", Some("kp1"))),
            row(2, failed_lesson("adding-integers", Some("kp2"))),
            row(3, failed_lesson("adding-integers", Some("kp1"))),
            row(4, failed_lesson("fractions", None)),
        ];
        let view = SessionView::of_log(&log);

        assert_eq!(view.lesson_failures.len(), 1);
        assert_eq!(
            view.lesson_failures.get("adding-integers"),
            Some(&BTreeSet::from(["kp1".to_string(), "kp2".to_string()]))
        );
        assert!(view.already_failed("adding-integers", Some("kp1")));
        assert!(view.already_failed("adding-integers", Some("kp2")));
        assert!(!view.already_failed("adding-integers", Some("kp3")));
        // A result with no knowledge point folds nothing, so it never repeats.
        assert!(!view.already_failed("adding-integers", None));
        assert!(!view.already_failed("fractions", Some("kp1")));
        assert!(!view.already_failed("subtracting-integers", Some("kp1")));
    }

    /// The document round-trips through the `session_view` column shape.
    #[test]
    fn the_view_round_trips_through_json() {
        let view = SessionView::of_log(&sample_log());
        let doc = serde_json::to_value(&view).expect("the view serializes");
        let back: SessionView = serde_json::from_value(doc).expect("the view reads back");
        assert_eq!(back, view);
    }
}
