//! The D4 fold: the three replay rules, the cached model, and the session
//! view that resumes from its stored document.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

mod common;

use cadus_core::event::{
    Event, LessonResult, SchemaVersion, Slug, Timestamp, TopicStatus, WorkQuality,
};
use cadus_core::learner::TopicState;
use cadus_store::begin_tenant;
use cadus_store::state::{append_event, load_session_view, project_and_save, project_current};
use cadus_store::test_support::TestDb;
use common::events::{BASE_US, end, regraded, start};
use common::state::{
    Scene, append_all, open_first_session, open_locked, open_with, read_cache, read_view,
};
use serde_json::{Value, json};
use uuid::Uuid;

// --------------------------------------------------------------------------- //
// The fold
// --------------------------------------------------------------------------- //

/// `project_and_save` writes the cursor; `project_current` writes nothing.
/// A `regraded` in the events after the cursor forces the FULL REPLAY branch,
/// and an ordinary event does not (spec section 4.3, `projector-1.0-spec.md:299`).
#[tokio::test]
async fn a_regraded_event_forces_the_full_replay_branch() {
    TestDb::with(|db| async move {
        let user = db.seed_user("fold@example.com").await;
        let scene = Scene::new(&db);
        let input = scene.input();
        let mut tx = open_first_session(&scene.handle, user).await;
        // The first fold has no cache, so it replays by definition.
        let first = project_and_save(&mut tx, user, &input, None).await.unwrap();
        assert!(first.replayed);
        assert_eq!(first.through_seq, 2);
        tx.commit().await.unwrap();

        let saved: (i64, i32) = sqlx::query!(
            r#"
            SELECT through_seq AS "through_seq!", projector_version AS "projector_version!"
            FROM learner_models WHERE user_id = $1
            "#,
            user
        )
        .fetch_one(&db.admin)
        .await
        .map(|row| (row.through_seq, row.projector_version))
        .unwrap();
        assert_eq!(saved, (2, 3));

        // One ordinary event after the cursor: the fold goes incremental.
        let mut tx = open_locked(&scene.handle, user).await;
        append_all(&mut tx, user, &[(&end("s_2026-01-01a"), None)]).await;
        let incremental = project_current(&mut tx, user, &input).await.unwrap();
        assert!(
            !incremental.replayed,
            "an ordinary event must not force a replay"
        );
        assert_eq!(incremental.through_seq, 3);
        // The read wrote nothing: the row still stands at the old cursor.
        let cursor: i64 = sqlx::query_scalar!(
            r#"SELECT through_seq AS "n!" FROM learner_models WHERE user_id = $1"#,
            user
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(cursor, 2, "project_current wrote the learner model");
        project_and_save(&mut tx, user, &input, None).await.unwrap();
        tx.commit().await.unwrap();

        // A `regraded` after the cursor forces the full replay.
        let mut tx = open_locked(&scene.handle, user).await;
        append_event(&mut tx, user, &regraded("t-1"), None)
            .await
            .unwrap();
        let replayed = project_current(&mut tx, user, &input).await.unwrap();
        assert!(
            replayed.replayed,
            "a regraded event must force the full replay"
        );
        assert_eq!(replayed.through_seq, 4);
        tx.rollback().await.unwrap();
    })
    .await;
}

/// A `config_hash` that drifted forces the full replay too, and the cached model
/// comes back with its `through_seq` filled in (D4).
#[tokio::test]
async fn a_config_drift_forces_the_full_replay() {
    TestDb::with(|db| async move {
        let user = db.seed_user("drift@example.com").await;
        let scene = Scene::new(&db);
        let input = scene.input();
        let mut tx = open_with(&scene.handle, user, &[(&start("s_2026-01-01a"), None)]).await;
        project_and_save(&mut tx, user, &input, Some("curriculum-hash"))
            .await
            .unwrap();
        tx.commit().await.unwrap();

        // A default config hashes to this literal (`config.rs`, trap T16).
        let stored: String = sqlx::query_scalar!(
            r#"SELECT config_hash AS "hash!" FROM learner_models WHERE user_id = $1"#,
            user
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(stored, "797575e985c12149");

        // Rewrite the drift fields the way a config change would.
        sqlx::query!(
            "UPDATE learner_models SET config_hash = 'deadbeefdeadbeef' WHERE user_id = $1",
            user
        )
        .execute(&db.admin)
        .await
        .unwrap();

        let cached = read_cache(&scene.handle, user).await;
        let mut tx = begin_tenant(scene.handle.pool(), user).await.unwrap();
        assert_eq!(cached.through_seq, 1);
        assert_eq!(cached.model.through_seq, Some(1));
        assert_eq!(cached.config_hash, "deadbeefdeadbeef");

        let after = project_current(&mut tx, user, &input).await.unwrap();
        assert!(after.replayed, "a config drift must force the full replay");
        tx.rollback().await.unwrap();
    })
    .await;
}

/// The fold reads the topic states the row carries. A learner model written by
/// hand comes back through `load_learner_model` field for field.
#[tokio::test]
async fn the_cached_model_reads_back_field_for_field() {
    TestDb::with(|db| async move {
        let user = db.seed_user("cache@example.com").await;
        let scene = Scene::new(&db);

        let mut topics: BTreeMap<String, TopicState> = BTreeMap::new();
        topics.insert(
            "addition".to_string(),
            TopicState {
                status: TopicStatus::Learning,
                rep_num: 1.0,
                memory_base: 1.0,
                t0: Some(Timestamp::from_micros(BASE_US)),
                interval_days: 2.0,
                ability: 0.62,
                ..TopicState::default()
            },
        );
        let model = cadus_core::learner::LearnerModel {
            topics,
            ..cadus_core::learner::LearnerModel::default()
        };
        sqlx::query!(
            r#"
            INSERT INTO learner_models
                (user_id, model, through_seq, projector_version, config_hash)
            VALUES ($1, $2, 7, 3, '797575e985c12149')
            "#,
            user,
            serde_json::to_value(&model).unwrap()
        )
        .execute(&db.admin)
        .await
        .unwrap();

        let cached = read_cache(&scene.handle, user).await;

        assert_eq!(cached.through_seq, 7);
        assert_eq!(cached.projector_version, 3);
        let state = cached.model.topics.get("addition").unwrap();
        assert_eq!(state.status, TopicStatus::Learning);
        assert_eq!(state.rep_num, 1.0);
        assert_eq!(state.memory_base, 1.0);
        assert_eq!(state.interval_days, 2.0);
        assert_eq!(state.ability, 0.62);
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The session view alone (M5 review 2, finding V2)
// --------------------------------------------------------------------------- //

/// One lesson that FAILED `topic` at `kp`.
fn lesson_failure(topic: &str, kp: &str) -> Event {
    Event::LessonResult(LessonResult {
        ts: Timestamp::from_micros(BASE_US),
        session: Some("s_2026-01-01a".to_string()),
        v: SchemaVersion,
        topic: Slug::new(topic).unwrap(),
        passed: false,
        failed_at_kp: Some(Slug::new(kp).unwrap()),
        xp: 0.0,
        quality_tier: WorkQuality::NearlyPassable,
        assisted: false,
    })
}

/// The stored `session_view` document of `user`.
async fn stored_view(db: &TestDb, user: Uuid) -> Value {
    sqlx::query_scalar!(
        r#"SELECT session_view AS "session_view!" FROM learner_models WHERE user_id = $1"#,
        user
    )
    .fetch_one(&db.admin)
    .await
    .unwrap()
}

/// Overwrite the stored `session_view` document of `user`.
async fn put_view(db: &TestDb, user: Uuid, doc: &Value) {
    sqlx::query!(
        "UPDATE learner_models SET session_view = $2 WHERE user_id = $1",
        user,
        doc
    )
    .execute(&db.admin)
    .await
    .unwrap();
}

/// V2. `load_session_view` resumes from the STORED document and folds the events
/// above the cursor into it.
///
/// The stored document names a failure the log does not hold, so a fold that
/// replayed the log instead of resuming loses it. The `session_end` appended
/// after the save is above the cursor, so the answer proves the forward fold ran
/// too.
#[tokio::test]
async fn the_failure_map_resumes_from_the_stored_view() {
    TestDb::with(|db| async move {
        let user = db.seed_user("resume-view@example.com").await;
        let scene = Scene::new(&db);
        let input = scene.input();
        let mut tx = open_first_session(&scene.handle, user).await;
        let saved = project_and_save(&mut tx, user, &input, None).await.unwrap();
        assert_eq!(saved.through_seq, 2);
        assert_eq!(saved.view.lesson_failures.len(), 0);
        tx.commit().await.unwrap();

        // A failure the LOG does not hold, written straight into the cache.
        let mut doc = stored_view(&db, user).await;
        doc["lesson_failures"] = json!({"fractions": ["kp9"]});
        put_view(&db, user, &doc).await;

        let mut tx = open_locked(&scene.handle, user).await;
        append_all(&mut tx, user, &[(&end("s_2026-01-01a"), None)]).await;
        let view = load_session_view(&mut tx, user).await.unwrap();
        tx.rollback().await.unwrap();

        assert_eq!(view.lesson_failures.len(), 1);
        assert!(view.already_failed("fractions", Some("kp9")));
        // The `session_end` of line 3 folded forward into the resumed document.
        assert_eq!(view.current_session, None);
    })
    .await;
}

/// V2, and the reason migration 0010 needs no backfill: a stored document that
/// carries no `lesson_failures` map does not read back, so the fold rebuilds the
/// whole view from the log.
#[tokio::test]
async fn a_stored_view_without_the_failure_map_is_rebuilt_from_the_log() {
    TestDb::with(|db| async move {
        let user = db.seed_user("old-view@example.com").await;
        let scene = Scene::new(&db);
        let input = scene.input();
        let mut tx = open_with(&scene.handle, user, &[(&start("s_2026-01-01a"), None)]).await;
        append_event(&mut tx, user, &lesson_failure("addition", "kp1"), None)
            .await
            .unwrap();
        let saved = project_and_save(&mut tx, user, &input, None).await.unwrap();
        assert_eq!(saved.through_seq, 2);
        assert!(saved.view.already_failed("addition", Some("kp1")));
        tx.commit().await.unwrap();

        // The document of a row written before the map: every other key, and no
        // `lesson_failures`.
        let mut doc = stored_view(&db, user).await;
        doc.as_object_mut().unwrap().remove("lesson_failures");
        assert!(doc.get("lesson_failures").is_none());
        put_view(&db, user, &doc).await;

        let view = read_view(&scene.handle, user).await;

        assert_eq!(view.lesson_failures.len(), 1);
        assert!(view.already_failed("addition", Some("kp1")));
        assert_eq!(view.current_session.as_deref(), Some("s_2026-01-01a"));
    })
    .await;
}
