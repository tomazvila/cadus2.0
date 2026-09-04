//! Part of `tests/session_state.rs`: the header of that file gives the
//! requirements and the rules.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_core::pool::PoolAnswer;
use cadus_store::state::{load_web_state, lock_web_state, save_web_state};
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db, begin_tenant};
use cadus_web::error::ApiError;
use cadus_web::state::{ServedProblem, TaskProgress, ValidateError, WebState};
use serde_json::json;

// --------------------------------------------------------------------------- //
// The two-tab race (section 11, row U6, acceptance 1)
// --------------------------------------------------------------------------- //

/// A second concurrent tab that answers a closed task gets `409 task_complete`.
///
/// The shape is section 4.2: both tabs snapshot, the first one commits the close
/// under the advisory lock, and the second one RE-VALIDATES against the row it
/// re-reads, not against the snapshot it opened with.
#[tokio::test]
async fn a_second_tab_answering_a_closed_task_gets_409_task_complete() {
    TestDb::with(|db| async move {
        let user = db.seed_user("tabs@example.com").await;
        let handle = Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
        let task = "s_2026-01-01a-review-addition";

        let mut open = WebState::for_session("s_2026-01-01a");
        open.served.insert(
            task.to_string(),
            ServedProblem {
                problem_id: "p1".to_string(),
                task_id: task.to_string(),
                topic: Some("addition".to_string()),
                serve_topic: Some("addition".to_string()),
                kp: None,
                answer_kind: Some("numeric".to_string()),
                text: "Compute $8 - 5$.".to_string(),
                expected: PoolAnswer {
                    v: 1,
                    answer: "3".to_string(),
                },
                solution_sketch: None,
                started_at: 1_767_225_600.0,
                hints_given: Vec::new(),
                index: 0,
                rework: None,
            },
        );
        let mut tx = begin_tenant(handle.pool(), user).await.unwrap();
        save_web_state(&mut tx, user, &open.to_doc()).await.unwrap();
        tx.commit().await.unwrap();

        // Tab B snapshots the OPEN task. Its snapshot says the answer is legal.
        let mut b = begin_tenant(handle.pool(), user).await.unwrap();
        lock_web_state(&mut b, user).await.unwrap();
        let b_snapshot =
            WebState::from_doc(&load_web_state(&mut b, user).await.unwrap().unwrap()).unwrap();
        assert!(b_snapshot.validate(task, "p1").is_ok());
        // Tab B lets go of the lock while it does its work.
        b.rollback().await.unwrap();

        // Tab A closes the task and commits.
        let mut a = begin_tenant(handle.pool(), user).await.unwrap();
        lock_web_state(&mut a, user).await.unwrap();
        let mut a_state =
            WebState::from_doc(&load_web_state(&mut a, user).await.unwrap().unwrap()).unwrap();
        a_state.tasks.insert(
            task.to_string(),
            TaskProgress {
                task_id: task.to_string(),
                task_type: "review".to_string(),
                total: 1,
                served: 1,
                answered: 1,
                done: true,
                current_kp: None,
            },
        );
        save_web_state(&mut a, user, &a_state.to_doc())
            .await
            .unwrap();
        a.commit().await.unwrap();

        // Tab B re-opens, re-reads, and re-validates. The verdict is the literal
        // 409 task_complete, not the stale "ok" of its own snapshot.
        let mut b = begin_tenant(handle.pool(), user).await.unwrap();
        lock_web_state(&mut b, user).await.unwrap();
        let b_fresh =
            WebState::from_doc(&load_web_state(&mut b, user).await.unwrap().unwrap()).unwrap();
        let refusal = b_fresh.validate(task, "p1").unwrap_err();
        b.rollback().await.unwrap();

        assert_eq!(refusal, ValidateError::TaskComplete);
        let answer = ApiError::from(refusal);
        assert_eq!(answer.status.as_u16(), 409);
        assert_eq!(answer.code, "task_complete");
    })
    .await;
}

/// The D-S6 document written by the production writer reads back through the
/// production reader, out of the real column (C3, D-S6).
#[tokio::test]
async fn the_document_round_trips_through_the_web_states_column() {
    TestDb::with(|db| async move {
        let user = db.seed_user("doc@example.com").await;
        let handle = Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS);

        let mut state = WebState::for_session("s_2026-01-01a");
        state.active_secs = 137.5;
        state.record_served("addition", "s_2026-01-01a-review-addition", "abc123");
        state.pending_diagnoses.insert(
            "s_2026-01-01a-review-addition-1".to_string(),
            "3f1b7d2e-0000-4000-8000-000000000001".to_string(),
        );

        let mut tx = begin_tenant(handle.pool(), user).await.unwrap();
        lock_web_state(&mut tx, user).await.unwrap();
        save_web_state(&mut tx, user, &state.to_doc())
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let mut tx = begin_tenant(handle.pool(), user).await.unwrap();
        let read =
            WebState::from_doc(&load_web_state(&mut tx, user).await.unwrap().unwrap()).unwrap();
        tx.rollback().await.unwrap();

        assert_eq!(read.session.as_deref(), Some("s_2026-01-01a"));
        assert_eq!(read.active_secs, 137.5);
        assert_eq!(read.ring("addition").hashes(), &["abc123".to_string()]);
        assert_eq!(
            read.memory("s_2026-01-01a-review-addition").hashes(),
            &["abc123".to_string()]
        );
        assert_eq!(
            read.pending_diagnoses
                .get("s_2026-01-01a-review-addition-1")
                .map(String::as_str),
            Some("3f1b7d2e-0000-4000-8000-000000000001")
        );
    })
    .await;
}

/// A stored document with a key this build does not know is a failure, not a
/// silent drop: `deny_unknown_fields` is the guard.
#[test]
fn an_unknown_key_in_the_document_is_refused() {
    let doc = json!({"session": "s_2026-01-01a", "served_texts": ["an old field"]});
    assert!(WebState::from_doc(&doc).is_err());
}
