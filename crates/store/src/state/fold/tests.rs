//! Unit tests of the fold branches that a DB test cannot reach: the replay on
//! a `regraded` above the cursor, and the two forward-fold shapes.

use cadus_core::config::Config;
use cadus_core::curriculum::{Catalog, Curriculum, RawCurriculum};
use cadus_core::event::{
    Event, Regraded, ReviewResult, SchemaVersion, SessionStart, Slug, Timestamp, WorkQuality,
};
use cadus_core::learner::LearnerModel;
use cadus_core::projector::ProjectionInput;

use super::{
    CachedModel, EventRow, PROJECTOR_VERSION_I32, Projection, SessionView, config_hash_of,
    decode_view, fold_forward,
};
use crate::StoreError;

/// The empty curriculum and the default config that the fold reads.
fn graph() -> Curriculum {
    Curriculum::build(RawCurriculum {
        catalog: Catalog {
            courses: Vec::new(),
        },
        units: Vec::new(),
    })
    .unwrap()
}

/// One `session_start` at seq `seq` in the session `session`.
fn started(seq: i64, session: &str) -> EventRow {
    EventRow {
        seq,
        event: Event::SessionStart(SessionStart {
            ts: Timestamp::from_micros(seq),
            session: Some(session.to_string()),
            v: SchemaVersion::current(),
        }),
    }
}

/// A cache of `view` with its cursor at seq 1.
fn cache_at_one(view: Option<SessionView>, config_hash: String) -> CachedModel {
    CachedModel {
        model: LearnerModel::default(),
        view,
        through_seq: 1,
        projector_version: PROJECTOR_VERSION_I32,
        config_hash,
    }
}

/// Fold `rows` forward over a cache with its cursor at seq 1 that holds
/// `view`, against the empty curriculum and the default config.
fn fold_over(view: Option<SessionView>, rows: Vec<EventRow>) -> Result<Projection, StoreError> {
    let graph = graph();
    let cfg = Config::default();
    let input = ProjectionInput::new(&graph, &cfg, Timestamp::from_micros(0));
    let cache = cache_at_one(view, config_hash_of(&input));
    fold_forward(&cache, rows, &input)
}

/// One `session_start` at seq 1 and a second event at seq 2 in the session
/// `s_1970-01-01a`.
fn start_then(second: Event) -> Vec<EventRow> {
    vec![
        started(1, "s_1970-01-01a"),
        EventRow {
            seq: 2,
            event: second,
        },
    ]
}

/// Fold two `session_start` rows forward over a cache that holds `view`, and
/// check that the fold took the incremental branch through seq 2.
fn folds_two_starts_forward(view: Option<SessionView>) {
    let rows = vec![started(1, "s_1970-01-01a"), started(2, "s_1970-01-02a")];
    let projection = fold_over(view, rows).unwrap();
    assert!(!projection.replayed);
    assert_eq!(projection.through_seq, 2);
}

/// Branch 2 replays the whole log when a `regraded` stands above the cursor.
/// The window of the resume check held none, because a read route holds no
/// lock and the correction arrived between the two reads.
#[test]
fn a_regraded_above_the_cursor_replays_the_whole_log() {
    let rows = start_then(Event::Regraded(Regraded {
        ts: Timestamp::from_micros(2),
        session: Some("s_1970-01-01a".to_string()),
        v: SchemaVersion::current(),
        task_id: "s_1970-01-01a-review-addition".to_string(),
        topic: Slug::new("addition").unwrap(),
        attempts: Vec::new(),
        quality_tier: Some(WorkQuality::Poor),
        xp: None,
        reason: "an operator repair".to_string(),
    }));
    let projection = fold_over(Some(SessionView::default()), rows).unwrap();
    assert!(projection.replayed);
    assert_eq!(projection.through_seq, 2);
}

/// Branch 2 with a cached view folds the new rows forward over it.
#[test]
fn a_cached_view_folds_the_new_rows_forward() {
    folds_two_starts_forward(Some(SessionView::default()));
}

/// Branch 2 with no cached view folds the view from the whole log.
#[test]
fn an_absent_view_folds_from_the_whole_log() {
    folds_two_starts_forward(None);
}

/// Branch 2 reports the error of the projector when the incremental fold
/// refuses the new events.
#[test]
fn a_projector_error_stops_the_forward_fold() {
    let rows = start_then(Event::ReviewResult(ReviewResult {
        ts: Timestamp::from_micros(2),
        session: Some("s_1970-01-01a".to_string()),
        v: SchemaVersion::current(),
        topic: Slug::new("addition").unwrap(),
        passed: true,
        weighted_score: 1.0,
        xp: -1e308,
        quality_tier: WorkQuality::Perfect,
        assisted: false,
        task_id: Some("s_1970-01-01a-review-addition".to_string()),
        inconclusive: false,
    }));
    assert!(fold_over(Some(SessionView::default()), rows).is_err());
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
