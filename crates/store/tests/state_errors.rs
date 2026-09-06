//! The error paths of the state module: a refused statement on every table,
//! a lock that another tab holds, an event that does not write, a payload
//! that does not read, and a fold that the projector refuses.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_core::event::{Event, SchemaVersion, SessionStart, Timestamp};
use cadus_core::projector::ProjectionInput;
use cadus_store::begin_tenant;
use cadus_store::state::{
    append_event, clear_web_state, load_events, load_events_after, load_learner_model,
    load_session_view, load_web_state, lock_web_state, project_and_save, project_current,
    save_web_state,
};
use cadus_store::test_support::TestDb;
use common::events::{BASE_US, Fixture, attempt, review, start};
use common::fault::{poison_event, revoke, revoke_set_config};
use common::{sqlstate_in_tx, store_sqlstate};
use serde_json::json;
use uuid::Uuid;

/// Assert that the fold and the view read of `user` both fail, each in a
/// tenant transaction of its own.
async fn both_reads_fail(db: &TestDb, user: Uuid, input: &ProjectionInput<'_>) {
    let mut tx = begin_tenant(&db.app, user).await.unwrap();
    assert!(project_current(&mut tx, user, input).await.is_err());
    tx.rollback().await.unwrap();
    let mut tx = begin_tenant(&db.app, user).await.unwrap();
    assert!(load_session_view(&mut tx, user).await.is_err());
    tx.rollback().await.unwrap();
}

/// Append the fixture log of `count` events for `user` and fold it once, so
/// the `learner_models` row stands with its cursor at the head.
async fn seed_log(db: &TestDb, user: Uuid, count: usize) {
    let fixture = Fixture::micro();
    let input = fixture.input();
    let mut tx = begin_tenant(&db.app, user).await.unwrap();
    append_event(&mut tx, user, &start("s_2026-01-01a"), None)
        .await
        .unwrap();
    for index in 1..count {
        let id = format!("t-{index}");
        append_event(&mut tx, user, &attempt(&id), Some(&id))
            .await
            .unwrap();
    }
    project_and_save(&mut tx, user, &input, None).await.unwrap();
    tx.commit().await.unwrap();
}

/// The two statements of the lock report a refused `set_config` and a lock
/// that another transaction holds past the timeout.
#[tokio::test]
async fn the_lock_reports_a_refused_set_config_and_a_held_lock() {
    TestDb::with(|db| async move {
        let user = db.seed_user("lock@example.test").await;
        let mut holder = begin_tenant(&db.app, user).await.unwrap();
        lock_web_state(&mut holder, user).await.unwrap();
        let mut waiter = begin_tenant(&db.app, user).await.unwrap();
        let err = lock_web_state(&mut waiter, user).await.unwrap_err();
        assert_eq!(store_sqlstate(&err), "55P03");
        waiter.rollback().await.unwrap();
        holder.rollback().await.unwrap();

        revoke_set_config(&db).await;
        let mut tx = db.app.begin().await.unwrap();
        let err = lock_web_state(&mut tx, user).await.unwrap_err();
        assert_eq!(store_sqlstate(&err), "42501");
        tx.rollback().await.unwrap();
    })
    .await;
}

/// Every statement on `web_states` reports the privilege it lacks.
#[tokio::test]
async fn every_web_state_statement_reports_a_refused_privilege() {
    TestDb::with(|db| async move {
        let user = db.seed_user("scratch@example.test").await;
        revoke(&db, "SELECT, INSERT, DELETE", "web_states").await;
        // A refused statement aborts its transaction, so each one gets its
        // own.
        assert_eq!(
            sqlstate_in_tx!(db, user, |tx| load_web_state(&mut tx, user)),
            "42501"
        );
        assert_eq!(
            sqlstate_in_tx!(db, user, |tx| save_web_state(&mut tx, user, &json!({}))),
            "42501"
        );
        assert_eq!(
            sqlstate_in_tx!(db, user, |tx| clear_web_state(&mut tx, user)),
            "42501"
        );
    })
    .await;
}

/// An event whose instant chrono cannot represent and a refused insert both
/// stop the append, and a payload that is not an event stops the read.
#[tokio::test]
async fn the_log_reports_a_bad_instant_a_refused_insert_and_a_bad_payload() {
    TestDb::with(|db| async move {
        let user = db.seed_user("log@example.test").await;
        seed_log(&db, user, 2).await;
        let far = Event::SessionStart(SessionStart {
            ts: Timestamp::from_micros(i64::MAX),
            session: None,
            v: SchemaVersion::current(),
        });
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        let err = append_event(&mut tx, user, &far, None).await.unwrap_err();
        assert_eq!(
            err.to_string(),
            format!(
                "document error: the event timestamp {} is outside the representable range",
                i64::MAX
            )
        );
        tx.rollback().await.unwrap();

        poison_event(&db, user, 1).await;
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        assert!(load_events_after(&mut tx, user, 1).await.is_ok());
        assert!(load_events(&mut tx, user).await.is_err());
        tx.rollback().await.unwrap();

        revoke(&db, "INSERT", "events").await;
        assert_eq!(
            sqlstate_in_tx!(db, user, |tx| append_event(
                &mut tx,
                user,
                &attempt("t-9"),
                Some("t-9")
            )),
            "42501"
        );
    })
    .await;
}

/// The fold reports a refused read of the cache row, a cached model that
/// does not decode, a refused write of the row, and a projector that refuses
/// the log.
#[tokio::test]
async fn the_fold_reports_the_cache_row_the_write_and_the_projector() {
    TestDb::with(|db| async move {
        let user = db.seed_user("fold@example.test").await;
        seed_log(&db, user, 3).await;
        let fixture = Fixture::micro();
        let input = fixture.input();

        // A zone name the projector does not know refuses both branches: the
        // resume over the cache, and the full replay.
        let bad_zone = ProjectionInput {
            tz: Some("Not/AZone"),
            ..input
        };
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        append_event(&mut tx, user, &attempt("t-9"), Some("t-9"))
            .await
            .unwrap();
        let resumed = project_current(&mut tx, user, &bad_zone).await;
        assert!(
            matches!(resumed, Err(cadus_store::StoreError::Projector(_))),
            "{resumed:?}"
        );
        tx.rollback().await.unwrap();
        sqlx::query("DELETE FROM learner_models WHERE user_id = $1")
            .bind(user)
            .execute(&db.admin)
            .await
            .unwrap();
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        let replayed = project_current(&mut tx, user, &bad_zone).await;
        assert!(
            matches!(replayed, Err(cadus_store::StoreError::Projector(_))),
            "{replayed:?}"
        );
        tx.rollback().await.unwrap();

        // The write of the row is refused after a fold that succeeded.
        revoke(&db, "INSERT", "learner_models").await;
        assert_eq!(
            sqlstate_in_tx!(db, user, |tx| project_and_save(&mut tx, user, &input, None)),
            "42501"
        );

        // A cached model of another shape is a document error.
        sqlx::query(
            "INSERT INTO learner_models (user_id, model, through_seq, projector_version, \
             config_hash) VALUES ($1, '\"text\"'::jsonb, 3, 3, 'x')",
        )
        .bind(user)
        .execute(&db.admin)
        .await
        .unwrap();
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        let err = load_learner_model(&mut tx, user).await.unwrap_err();
        assert!(
            err.to_string()
                .starts_with("document error: learner_models.model: "),
            "{err}"
        );
        tx.rollback().await.unwrap();

        // The read of the row is refused: the model read, the fold, and the
        // view read all report it.
        revoke(&db, "SELECT", "learner_models").await;
        // A refused statement aborts its transaction, so each one gets its
        // own.
        assert_eq!(
            sqlstate_in_tx!(db, user, |tx| load_learner_model(&mut tx, user)),
            "42501"
        );
        assert_eq!(
            sqlstate_in_tx!(db, user, |tx| project_current(&mut tx, user, &input)),
            "42501"
        );
        assert_eq!(
            sqlstate_in_tx!(db, user, |tx| project_and_save(&mut tx, user, &input, None)),
            "42501"
        );
        assert_eq!(
            sqlstate_in_tx!(db, user, |tx| load_session_view(&mut tx, user)),
            "42501"
        );
    })
    .await;
}

/// A payload that does not read stops the fold at whichever read meets it:
/// the cursor window when it sits on the cursor line, the whole-log read when
/// it sits below the cursor, and the full replay when no cache stands.
#[tokio::test]
async fn a_payload_that_does_not_read_stops_every_read_that_meets_it() {
    TestDb::with(|db| async move {
        let user = db.seed_user("poison@example.test").await;
        seed_log(&db, user, 3).await;
        let fixture = Fixture::micro();
        let input = fixture.input();

        // Line 1 is below the cursor (3): the window read passes, the
        // whole-log read of branch 2 fails, and so does the view fold's own
        // replay when its cursor is off the log.
        poison_event(&db, user, 1).await;
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        append_event(&mut tx, user, &attempt("t-9"), Some("t-9"))
            .await
            .unwrap();
        let err = project_current(&mut tx, user, &input).await.unwrap_err();
        assert!(err.to_string().starts_with("database error: "), "{err}");
        assert!(load_session_view(&mut tx, user).await.is_ok());
        tx.rollback().await.unwrap();

        // Line 3 is the cursor line: the window read itself fails, for the
        // fold and for the view.
        poison_event(&db, user, 3).await;
        both_reads_fail(&db, user, &input).await;

        // No cache at all: the full replay meets line 1.
        sqlx::query("DELETE FROM learner_models WHERE user_id = $1")
            .bind(user)
            .execute(&db.admin)
            .await
            .unwrap();
        both_reads_fail(&db, user, &input).await;
    })
    .await;
}

/// A cache row with no view and no new event folds the view from the log and
/// resumes the model; a cursor of 0 reads the log once; a cursor past the
/// head of the log replays the view in full.
#[tokio::test]
async fn the_cursor_shapes_choose_the_branch() {
    TestDb::with(|db| async move {
        let user = db.seed_user("cursor@example.test").await;
        seed_log(&db, user, 3).await;
        let fixture = Fixture::micro();
        let input = fixture.input();

        sqlx::query("UPDATE learner_models SET session_view = NULL WHERE user_id = $1")
            .bind(user)
            .execute(&db.admin)
            .await
            .unwrap();
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        let folded = project_current(&mut tx, user, &input).await.unwrap();
        assert!(!folded.replayed);
        assert_eq!(folded.through_seq, 3);
        assert_eq!(
            folded.view.current_session.as_deref(),
            Some("s_2026-01-01a")
        );
        tx.rollback().await.unwrap();

        sqlx::query("UPDATE learner_models SET through_seq = 0 WHERE user_id = $1")
            .bind(user)
            .execute(&db.admin)
            .await
            .unwrap();
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        let from_zero = project_current(&mut tx, user, &input).await.unwrap();
        assert!(!from_zero.replayed);
        assert_eq!(from_zero.through_seq, 3);
        tx.rollback().await.unwrap();

        sqlx::query("UPDATE learner_models SET through_seq = 9 WHERE user_id = $1")
            .bind(user)
            .execute(&db.admin)
            .await
            .unwrap();
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        let view = load_session_view(&mut tx, user).await.unwrap();
        assert_eq!(view.session_start_seq, Some(1));
        let replayed = project_current(&mut tx, user, &input).await.unwrap();
        assert!(replayed.replayed);
        tx.rollback().await.unwrap();

        // A review worth more XP than the model holds is refused by the
        // incremental fold too.
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        append_event(
            &mut tx,
            user,
            &review(Timestamp::from_micros(BASE_US), -1e308),
            None,
        )
        .await
        .unwrap();
        assert!(project_current(&mut tx, user, &input).await.is_err());
        tx.rollback().await.unwrap();
    })
    .await;
}
