//! M5 U6 acceptance, the D-S6 document and the pure event scans.
//!
//! Requirements: D-S6, D5, C3. Spec `docs/reference/web-service-1.0-spec.md`
//! sections 4.1 and 4.2, and section 11 row U6.
//!
//! Every expected value is a LITERAL: a literal status code, a literal error
//! code, a literal session id, a literal count. Nothing is read back from the
//! code under test.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::review_problem;

use axum::http::StatusCode;
use cadus_core::pool::{Ring, TaskMemory};
use cadus_web::error::ApiError;
use cadus_web::state::{TASK_COMPLETE, TaskProgress, UNKNOWN_PROBLEM, ValidateError, WebState};

// --------------------------------------------------------------------------- //
// The document
// --------------------------------------------------------------------------- //

/// The two D5 windows carry the M4 capacities: 20 per topic, 12 per task.
#[test]
fn the_state_document_carries_the_m4_ring_and_the_task_memory() {
    assert_eq!(Ring::capacity(), 20);
    assert_eq!(TaskMemory::capacity(), 12);

    let mut state = WebState::for_session("s_2026-01-01a");
    for index in 0..25 {
        state.record_served(
            "addition",
            "s_2026-01-01a-review-addition",
            &format!("h{index}"),
        );
    }
    assert_eq!(state.ring("addition").len(), 20);
    assert_eq!(state.memory("s_2026-01-01a-review-addition").len(), 12);
    // The oldest entries fell out of each window and the newest one stands.
    assert!(!state.ring("addition").contains("h4"));
    assert!(state.ring("addition").contains("h5"));
    assert!(
        !state
            .memory("s_2026-01-01a-review-addition")
            .contains("h12")
    );
    assert!(
        state
            .memory("s_2026-01-01a-review-addition")
            .contains("h13")
    );
    assert!(state.ring("addition").contains("h24"));

    // An unknown topic and an unknown task both give empty windows.
    assert_eq!(state.ring("fractions").len(), 0);
    assert_eq!(state.memory("no-such-task").len(), 0);
}

/// The document round-trips through the `web_states.doc` column, and a session
/// drift resets it (`state.py:142-166`).
#[test]
fn a_session_drift_resets_the_document() {
    let mut state = WebState::for_session("s_2026-01-01a");
    state.active_secs = 42.0;
    state.record_served("addition", "s_2026-01-01a-review-addition", "h1");

    let doc = state.to_doc();
    let read = WebState::from_doc(&doc).unwrap();
    assert_eq!(read, state);

    // The same session id changes nothing.
    let mut same = read.clone();
    assert!(!same.bind("s_2026-01-01a"));
    assert_eq!(same.active_secs, 42.0);
    assert_eq!(same.ring("addition").len(), 1);

    // A different session id resets every field.
    let mut drifted = read;
    assert!(drifted.bind("s_2026-01-02a"));
    assert_eq!(drifted.session.as_deref(), Some("s_2026-01-02a"));
    assert_eq!(drifted.active_secs, 0.0);
    assert_eq!(drifted.ring("addition").len(), 0);
    assert_eq!(drifted.served.len(), 0);
    assert_eq!(drifted.tasks.len(), 0);
}

/// `plan_progress` is a pure LOOKUP. Trap W3: it must never install a row for a
/// task that the plan merely lists.
#[test]
fn plan_progress_never_installs_a_row() {
    let mut state = WebState::for_session("s_2026-01-01a");
    state.tasks.insert(
        "s_2026-01-01a-review-addition".to_string(),
        TaskProgress {
            task_id: "s_2026-01-01a-review-addition".to_string(),
            task_type: "review".to_string(),
            total: 3,
            served: 3,
            answered: 2,
            done: true,
            current_kp: None,
        },
    );

    assert_eq!(
        state.plan_progress("s_2026-01-01a-review-addition"),
        (2, true)
    );
    assert_eq!(
        state.plan_progress("s_2026-01-01a-lesson-fractions"),
        (0, false)
    );
    // The lookup of an absent task left the map at its one entry.
    assert_eq!(state.tasks.len(), 1);
}

/// The validate rule of section 4.2, in its two refusals and their literals.
#[test]
fn validate_gives_409_task_complete_and_404_unknown_problem() {
    let mut state = WebState::for_session("s_2026-01-01a");
    state.served.insert(
        "s_2026-01-01a-review-addition".to_string(),
        review_problem("s_2026-01-01a-review-addition"),
    );

    // The live problem passes.
    let live = state
        .validate("s_2026-01-01a-review-addition", "p1")
        .unwrap();
    assert_eq!(live.problem_id, "p1");

    // A superseded problem id is unknown.
    assert_eq!(
        state.validate("s_2026-01-01a-review-addition", "p0"),
        Err(ValidateError::UnknownProblem)
    );
    // A task that serves nothing is unknown too.
    assert_eq!(
        state.validate("s_2026-01-01a-lesson-fractions", "p1"),
        Err(ValidateError::UnknownProblem)
    );

    // A closed task refuses BEFORE the problem id is compared.
    state.tasks.insert(
        "s_2026-01-01a-review-addition".to_string(),
        TaskProgress {
            done: true,
            ..TaskProgress::default()
        },
    );
    assert_eq!(
        state.validate("s_2026-01-01a-review-addition", "p1"),
        Err(ValidateError::TaskComplete)
    );

    let complete = ApiError::from(ValidateError::TaskComplete);
    assert_eq!(complete.status, StatusCode::CONFLICT);
    assert_eq!(complete.code, "task_complete");
    assert_eq!(TASK_COMPLETE, "task_complete");

    let unknown = ApiError::from(ValidateError::UnknownProblem);
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);
    assert_eq!(unknown.code, "unknown_problem");
    assert_eq!(UNKNOWN_PROBLEM, "unknown_problem");
}
