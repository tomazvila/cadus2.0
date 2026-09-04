//! The fold: the cached learner model, the three replay rules, and the
//! `learner_models` write (D4, spec section 4.3).

use cadus_core::event::Event;
use cadus_core::learner::LearnerModel;
use cadus_core::projector::{PROJECTOR_VERSION, ProjectionInput, project, project_incremental};
use serde_json::Value as Json;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use super::view::{SESSION_VIEW_VERSION, SessionView};
use super::{EventRow, load_events, load_events_after};
use crate::StoreError;

/// `PROJECTOR_VERSION` as the `integer` column `learner_models.projector_version`
/// holds it.
const PROJECTOR_VERSION_I32: i32 = PROJECTOR_VERSION as i32;

const _: () = assert!(
    PROJECTOR_VERSION <= i32::MAX as i64,
    "learner_models.projector_version is an integer"
);

// The cached learner model of one tenant, with the three drift fields.
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

/// The events from the cursor line of a cache up to the head of the log.
///
/// The read starts at the cursor LINE, not after it, so the answer proves the
/// cursor sits on a line the log still holds. A cursor the log cannot show is
/// a cache that disagrees with the log, and the fold replays. The check costs
/// one extra decoded row. A cursor of 0 sits BEFORE the first line, so the
/// whole window is new.
struct Window {
    /// The rows from the cursor line up, in `seq` order.
    rows: Vec<EventRow>,
    /// Whether the cursor sits on a line the log still holds.
    anchored: bool,
    /// The position of the first row above the cursor.
    first_new: usize,
}

impl Window {
    /// Read the window of `through_seq`.
    async fn read(
        tx: &mut Transaction<'_, Postgres>,
        user_id: Uuid,
        through_seq: i64,
    ) -> Result<Self, StoreError> {
        let rows = load_events_after(tx, user_id, through_seq.saturating_sub(1)).await?;
        let anchored = through_seq == 0 || rows.first().is_some_and(|row| row.seq == through_seq);
        Ok(Self {
            rows,
            anchored,
            first_new: usize::from(through_seq > 0),
        })
    }

    /// The rows above the cursor.
    fn new_rows(&self) -> &[EventRow] {
        self.rows.get(self.first_new..).unwrap_or_default()
    }

    /// Whether the cache resumes: the cursor is anchored and no `regraded`
    /// stands above it.
    fn resumes(&self) -> bool {
        self.anchored && !holds_regraded(self.new_rows())
    }
}

/// Whether any row of `rows` is a `regraded` event.
fn holds_regraded(rows: &[EventRow]) -> bool {
    rows.iter()
        .any(|row| matches!(row.event, Event::Regraded(_)))
}

/// The drift digest of the config of `input`.
///
/// `Config::config_hash` serializes a struct of plain fields and never fails.
/// The empty string stands in for that impossible failure: no stored row
/// carries it, so the fold replays.
fn config_hash_of(input: &ProjectionInput<'_>) -> String {
    input.cfg.config_hash().unwrap_or_default()
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

/// Branch 1: the cached model IS the projection and the cached view IS the
/// view.
///
/// It needs BOTH cached documents and a cursor at the head of the log. A
/// cursor of 0 means the row was written over an empty log, and the light
/// indices of that model were built against the wall clock of that write, so
/// it is not this request's answer.
fn resume_cached(
    cache: &CachedModel,
    window: &Window,
    now: cadus_core::event::Timestamp,
) -> Option<Projection> {
    let view = cache.view.as_ref()?;
    if !window.new_rows().is_empty() || cache.through_seq < 1 {
        return None;
    }
    let mut model = cache.model.clone();
    model.built_from_ts = Some(now);
    // A fresh fold leaves this key out of the document (D4 keeps the cursor
    // in its own column), so the resume spells it the same.
    model.through_seq = None;
    Some(Projection {
        model,
        view: view.clone(),
        through_seq: cache.through_seq,
        replayed: false,
    })
}

/// Branch 2: fold the rows above the cursor into the cached model and view.
///
/// `rows` is the whole log. A `regraded` may have arrived between the two
/// reads, because a read route holds no lock, so the whole-log read is the
/// authority and the fold replays when it holds one above the cursor.
fn fold_forward(
    cache: &CachedModel,
    rows: Vec<EventRow>,
    input: &ProjectionInput<'_>,
) -> Result<Projection, StoreError> {
    let split = rows.partition_point(|row| row.seq <= cache.through_seq);
    let last = rows.last().map_or(0, |row| row.seq);
    if holds_regraded(&rows[split..]) {
        return replay(rows, input);
    }
    let view = match cache.view.as_ref() {
        Some(cached_view) => {
            let mut view = cached_view.clone();
            view.fold(&rows[split..]);
            view
        }
        // The column is new (migration 0010), so a row written before it
        // carries no view. The whole log is already read here, so the view
        // is folded from it and the MODEL still resumes from the cache: an
        // absent view costs one fold and never a full replay of the model.
        None => SessionView::of_log(&rows),
    };
    let events: Vec<Event> = rows.into_iter().map(|row| row.event).collect();
    let (prior, new) = events.split_at(split);
    let model = project_incremental(&cache.model, prior, new, input)?;
    Ok(Projection {
        model,
        view,
        through_seq: last,
        replayed: false,
    })
}

/// Branch 3: read the whole log and replay it.
async fn replay_log(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    input: &ProjectionInput<'_>,
) -> Result<Projection, StoreError> {
    let rows = load_events(tx, user_id).await?;
    replay(rows, input)
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
/// Returns [`StoreError::Projector`] when the fold fails, and [`StoreError::Db`]
/// when a statement fails.
pub async fn project_current(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    input: &ProjectionInput<'_>,
) -> Result<Projection, StoreError> {
    let cached = load_learner_model(tx, user_id).await?;
    let config_hash = config_hash_of(input);

    // The cache serves a resume only when it was built by this projector and
    // under this config. Those are the two drift rules of spec section 4.3.
    let resume = cached.as_ref().filter(|cache| {
        i64::from(cache.projector_version) == PROJECTOR_VERSION && cache.config_hash == config_hash
    });
    let Some(cache) = resume else {
        return replay_log(tx, user_id, input).await;
    };

    let window = Window::read(tx, user_id, cache.through_seq).await?;
    if !window.resumes() {
        return replay_log(tx, user_id, input).await;
    }
    if let Some(projection) = resume_cached(cache, &window, input.now) {
        return Ok(projection);
    }
    // `project_incremental` reads the earlier events for their light indices,
    // so branch 2 needs the whole log. A cursor of 0 already read it: the
    // window IS the whole log there.
    let rows = if cache.through_seq == 0 {
        window.rows
    } else {
        load_events(tx, user_id).await?
    };
    fold_forward(cache, rows, input)
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
        let window = Window::read(tx, user_id, through_seq).await?;
        if window.resumes() {
            view.fold(window.new_rows());
            return Ok(view);
        }
    }
    let rows = load_events(tx, user_id).await?;
    Ok(SessionView::of_log(&rows))
}

/// Fold the log and write the `learner_models` row (spec section 4.3 step 7).
///
/// The two documents go into their `jsonb` columns through
/// [`sqlx::types::Json`], so the serializer runs inside the encoder of the
/// statement.
///
/// # Errors
///
/// The errors of [`project_current`], plus [`StoreError::Db`] when a document
/// does not serialize or the write fails.
pub async fn project_and_save(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    input: &ProjectionInput<'_>,
    curriculum_hash: Option<&str>,
) -> Result<Projection, StoreError> {
    let projection = project_current(tx, user_id, input).await?;
    let config_hash = config_hash_of(input);

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
        // `as _`: the macro maps a `jsonb` parameter to `serde_json::Value`,
        // and `Json<T>` writes the same wire form.
        sqlx::types::Json(&projection.model) as _,
        projection.through_seq,
        PROJECTOR_VERSION_I32,
        config_hash,
        curriculum_hash,
        sqlx::types::Json(&projection.view) as _
    )
    .execute(&mut **tx)
    .await?;

    Ok(projection)
}

#[cfg(test)]
mod tests {
    use cadus_core::config::Config;
    use cadus_core::curriculum::{Catalog, Curriculum, RawCurriculum};
    use cadus_core::event::{
        Event, Regraded, SchemaVersion, SessionStart, Slug, Timestamp, WorkQuality,
    };
    use cadus_core::learner::LearnerModel;
    use cadus_core::projector::ProjectionInput;

    use super::{
        CachedModel, EventRow, PROJECTOR_VERSION_I32, SessionView, config_hash_of, decode_view,
        fold_forward,
    };

    /// Branch 2 replays when a `regraded` stands above the cursor. The window
    /// of the resume check held none, because a read route holds no lock and
    /// the correction arrived between the two reads, so the whole-log read is
    /// the authority.
    #[test]
    fn a_regraded_above_the_cursor_replays_the_whole_log() {
        let graph = Curriculum::build(RawCurriculum {
            catalog: Catalog {
                courses: Vec::new(),
            },
            units: Vec::new(),
        })
        .unwrap();
        let cfg = Config::default();
        let input = ProjectionInput::new(&graph, &cfg, Timestamp::from_micros(0));
        let cache = CachedModel {
            model: LearnerModel::default(),
            view: Some(SessionView::default()),
            through_seq: 1,
            projector_version: PROJECTOR_VERSION_I32,
            config_hash: config_hash_of(&input),
        };
        let rows = vec![
            EventRow {
                seq: 1,
                event: Event::SessionStart(SessionStart {
                    ts: Timestamp::from_micros(0),
                    session: Some("s_1970-01-01a".to_string()),
                    v: SchemaVersion,
                }),
            },
            EventRow {
                seq: 2,
                event: Event::Regraded(Regraded {
                    ts: Timestamp::from_micros(0),
                    session: Some("s_1970-01-01a".to_string()),
                    v: SchemaVersion,
                    task_id: "s_1970-01-01a-review-addition".to_string(),
                    topic: Slug::new("addition").unwrap(),
                    attempts: Vec::new(),
                    quality_tier: Some(WorkQuality::Poor),
                    xp: None,
                    reason: "an operator repair".to_string(),
                }),
            },
        ];
        let projection = fold_forward(&cache, rows, &input).unwrap();
        assert!(projection.replayed);
        assert_eq!(projection.through_seq, 2);
    }

    /// A stored view of another shape or another version reads as absent.
    #[test]
    fn a_view_of_another_version_or_shape_is_absent() {
        assert!(decode_view(None).is_none());
        assert!(decode_view(Some(serde_json::json!({"v": 1}))).is_none());
        assert!(decode_view(Some(serde_json::json!("text"))).is_none());
        let stale = serde_json::to_value(super::SessionView {
            v: super::SESSION_VIEW_VERSION + 1,
            ..Default::default()
        })
        .unwrap();
        assert!(decode_view(Some(stale)).is_none());
        let fresh = serde_json::to_value(super::SessionView::default()).unwrap();
        assert_eq!(
            decode_view(Some(fresh)),
            Some(super::SessionView::default())
        );
    }
}
