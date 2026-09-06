//! M5 U7 acceptance, the drill cadence (F17, D-M5-8) and the fold cursor
//! (M5 review 2, V1, V3, V8, V9).
//!
//! The header of `serve_routes.rs` gives the requirements and the rules.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axum::http::StatusCode;
use cadus_core::event::{Event, SchemaVersion, SessionEnd, SessionStart, Timestamp};
use cadus_store::test_support::TestDb;
use common::{
    BASE_US, DRILL, DRILL_2, SESSION, SESSION_2, close_live, drill_app as app, fold_cursor,
    log_head, plan_task_ids, seed_drill_due, seed_learner, seed_open_session, serve_task,
    status_body, stored_state, task_served_rows,
};
use sqlx::types::Uuid;
use sqlx::types::chrono::{DateTime, Utc};

/// Append one event at the next dense `seq` of `user`.
async fn append_log(db: &TestDb, user: Uuid, event: &Event, micros: i64) {
    let ts = DateTime::<Utc>::from_timestamp_micros(micros).unwrap();
    sqlx::query!(
        r#"
        INSERT INTO events (user_id, seq, ts, type, session_id, v, payload)
        SELECT $1, COALESCE(MAX(e.seq), 0) + 1, $2, $3, $4, 1, $5
        FROM events e WHERE e.user_id = $1
        "#,
        user,
        ts,
        event.type_name(),
        event.session(),
        serde_json::to_value(event).unwrap()
    )
    .execute(&db.admin)
    .await
    .unwrap();
}

/// Serve the drill, and read the `200` body.
async fn serve_drill(app: &axum::Router, user: Uuid) -> serde_json::Value {
    let (status, body) = serve_task(app, user, DRILL).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

/// Serve question `n` of the running drill: the index is `n`, and the log
/// holds ONE cadence row whatever `n` is.
async fn drill_question(db: &TestDb, app: &axum::Router, user: Uuid, n: i64) -> serde_json::Value {
    let body = serve_drill(app, user).await;
    assert_eq!(body["index"], n);
    assert_eq!(task_served_rows(db, user, DRILL).await, 1);
    body
}

/// F17 and ruling D-M5-8. The serve appends `task_served`, so the drill cadence
/// of `schedule_drills` finally has its one source. The 3.5-day window
/// (`DRILL_INTERVAL_DAYS`) has not passed when the next session opens, so the
/// plan of that session offers NO drill.
///
/// Every expected value here is a literal: one event row, the drill task id of
/// each session, and the empty drill offer of the second session.
#[tokio::test]
async fn a_served_drill_is_recorded_and_the_next_session_offers_no_drill() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "cadence@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;
        seed_drill_due(&db, user).await;

        // The drill stands in the plan of the first session.
        assert!(
            plan_task_ids(&app, user).await.contains(&DRILL.to_string()),
            "the fixture learner is not drill-eligible"
        );
        assert_eq!(task_served_rows(&db, user, DRILL).await, 0);

        let served = serve_drill(&app, user).await;
        assert_eq!(served["index"], 1);
        assert_eq!(served["countdown"], true);

        // The serve wrote the cadence event, and it wrote exactly one.
        assert_eq!(task_served_rows(&db, user, DRILL).await, 1);
        let stored = sqlx::query!(
            r#"
            SELECT session_id AS "session_id!", payload AS "payload!"
            FROM events WHERE user_id = $1 AND type = 'task_served'
            "#,
            user
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(stored.session_id, SESSION);
        assert_eq!(stored.payload["task_id"], DRILL);
        assert_eq!(stored.payload["task_type"], "drill");
        assert_eq!(stored.payload["topic"], "tables");
        assert_eq!(stored.payload["session"], SESSION);

        // Close the session and open the next one. The cadence event stays in
        // the log, so the new session's plan holds no drill at all.
        append_log(
            &db,
            user,
            &Event::SessionEnd(SessionEnd {
                ts: Timestamp::from_micros(BASE_US),
                session: Some(SESSION.to_string()),
                v: SchemaVersion::current(),
                xp_earned: 0.0,
                minutes: 0.0,
            }),
            BASE_US,
        )
        .await;
        append_log(
            &db,
            user,
            &Event::SessionStart(SessionStart {
                ts: Timestamp::from_micros(BASE_US),
                session: Some(SESSION_2.to_string()),
                v: SchemaVersion::current(),
            }),
            BASE_US,
        )
        .await;

        let ids = plan_task_ids(&app, user).await;
        assert!(
            !ids.contains(&DRILL_2.to_string()),
            "the drill came back inside the 3.5-day window: {ids:?}"
        );
        assert!(
            !ids.iter().any(|id| id.contains("-drill-")),
            "the plan still offers a drill: {ids:?}"
        );

        // The refusal is the cadence, not a missing task: a serve of a task the
        // plan does not hold is `404 unknown_task`.
        let (status, body) = serve_task(&app, user, DRILL_2).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
        assert_eq!(body["error"]["code"], "unknown_task");
    })
    .await;
}

/// The append is idempotent per task id, and the cadence never takes a drill
/// away from the session that serves it (D-M5-8). The second problem of the
/// running drill comes back under the SAME task id, and the log still holds one
/// `task_served` row.
#[tokio::test]
async fn a_running_drill_keeps_its_task_and_appends_no_second_event() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "running@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;
        seed_drill_due(&db, user).await;

        drill_question(&db, &app, user, 1).await;

        // Close the live problem, the way an answer does, and ask for the next
        // one. The task must still stand in the plan of its own session.
        close_live(&db, user, DRILL).await;
        drill_question(&db, &app, user, 2).await;

        // The plan route lists the running drill too: the three routes compose
        // from `session::view_for_open_session` (V3, V9), and
        // `the_plan_and_the_status_keep_the_drill_the_session_serves` pins it.
        let progress = stored_state(&db, user).await;
        assert_eq!(progress.tasks[DRILL].served, 2);
    })
    .await;
}

/// V1 and V8 of M5 review 2. The serve appends `task_served`, so the serve folds
/// and saves in the SAME transaction. A cursor one line behind the head makes
/// `project_current` leave the "nothing new" branch, and the incremental branch
/// then reads the whole log on every later request of that learner (F15, F18).
///
/// Every expected value is a literal: the head line, the cursor line, and the
/// count of the cadence rows.
#[tokio::test]
async fn a_serve_leaves_the_fold_cursor_at_the_log_head() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "cursor@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;
        seed_drill_due(&db, user).await;

        // The fixture: one `session_start` at line 1, and a cursor on it.
        assert_eq!(log_head(&db, user).await, 1);
        assert_eq!(fold_cursor(&db, user).await, 1);

        let served = serve_drill(&app, user).await;

        // The serve appended line 2, and the cursor moved onto it.
        assert_eq!(log_head(&db, user).await, 2);
        assert_eq!(fold_cursor(&db, user).await, 2);
        assert_eq!(task_served_rows(&db, user, DRILL).await, 1);

        // A re-serve of the live problem appends nothing, so the head and the
        // cursor both stay on line 2.
        let again = serve_drill(&app, user).await;
        assert_eq!(again["problem_id"], served["problem_id"]);
        assert_eq!(log_head(&db, user).await, 2);
        assert_eq!(fold_cursor(&db, user).await, 2);
        assert_eq!(task_served_rows(&db, user, DRILL).await, 1);
    })
    .await;
}

/// V3 and V9 of M5 review 2. `POST /serve`, `GET /api/session/plan` and
/// `GET /api/status` compose from ONE session view, so the drill the open
/// session already serves stays in the plan of that session and `drill_due`
/// stays true while its questions run.
///
/// Every expected value is a literal: the drill task id, the `true` of
/// `drill_due`, and the served index of each hand-off.
#[tokio::test]
async fn the_plan_and_the_status_keep_the_drill_the_session_serves() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "one-view@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;
        seed_drill_due(&db, user).await;

        // Before the serve: the drill stands in the plan and the dashboard
        // reports it due.
        assert!(
            plan_task_ids(&app, user).await.contains(&DRILL.to_string()),
            "the fixture learner is not drill-eligible"
        );
        assert_eq!(status_body(&app, user).await["drill_due"], true);

        // Question 1 of 20 goes on screen. It stamps the cadence.
        drill_question(&db, &app, user, 1).await;

        // The plan of the SAME session still lists the drill, and the dashboard
        // still reports it due: the cadence gate reads the EARLIER sessions.
        let ids = plan_task_ids(&app, user).await;
        assert!(
            ids.contains(&DRILL.to_string()),
            "the plan dropped the drill the session is working on: {ids:?}"
        );
        assert_eq!(status_body(&app, user).await["drill_due"], true);

        // Question 2 comes back under the SAME task id, and it appends no
        // second cadence event.
        close_live(&db, user, DRILL).await;
        drill_question(&db, &app, user, 2).await;
        assert!(
            plan_task_ids(&app, user).await.contains(&DRILL.to_string()),
            "the plan dropped the drill after its second question"
        );
    })
    .await;
}
