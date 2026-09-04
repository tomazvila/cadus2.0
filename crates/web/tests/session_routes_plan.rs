//! Part of `tests/session_routes.rs`: the header of that file gives the
//! requirements and the rules.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{BASE_US, SESSION, parse, seed_open_session};

use common::sessions::*;

use axum::http::{Method, StatusCode};
use cadus_core::event::{Event, SchemaVersion, SessionStart, Timestamp};
use cadus_store::test_support::TestDb;
use cadus_web::state::{TaskProgress, WebState};
use serde_json::{Value, json};

// --------------------------------------------------------------------------- //
// Acceptance 2: listing the plan writes nothing
// --------------------------------------------------------------------------- //

/// Trap W3. `GET /api/session/plan` appends no event, installs no
/// `TaskProgress`, and touches neither the `web_states` row nor the
/// `learner_models` row.
#[tokio::test]
async fn listing_the_plan_writes_nothing() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "plan@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;
        seed_due_review(&db, user, 1).await;

        let scratch = WebState::for_session(SESSION);
        sqlx::query!(
            "INSERT INTO web_states (user_id, doc) VALUES ($1, $2)",
            user,
            scratch.to_doc()
        )
        .execute(&db.admin)
        .await
        .unwrap();

        let before = sqlx::query!(
            r#"
            SELECT (SELECT count(*) FROM events WHERE user_id = $1) AS "events!",
                   (SELECT doc FROM web_states WHERE user_id = $1) AS "doc!",
                   (SELECT updated_at FROM web_states WHERE user_id = $1) AS "touched!",
                   (SELECT built_at FROM learner_models WHERE user_id = $1) AS "built!",
                   (SELECT through_seq FROM learner_models WHERE user_id = $1) AS "cursor!"
            "#,
            user
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();

        let (status, _, body) =
            call(&app, Method::GET, "/api/session/plan", Some(user), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let plan = parse(&body);
        assert_eq!(plan["session"].as_str().unwrap(), SESSION);
        assert!(
            plan["tasks"]
                .as_array()
                .is_some_and(|tasks| !tasks.is_empty()),
            "the plan composed no task: {body}"
        );

        let after = sqlx::query!(
            r#"
            SELECT (SELECT count(*) FROM events WHERE user_id = $1) AS "events!",
                   (SELECT doc FROM web_states WHERE user_id = $1) AS "doc!",
                   (SELECT updated_at FROM web_states WHERE user_id = $1) AS "touched!",
                   (SELECT built_at FROM learner_models WHERE user_id = $1) AS "built!",
                   (SELECT through_seq FROM learner_models WHERE user_id = $1) AS "cursor!"
            "#,
            user
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();

        assert_eq!(before.events, 1);
        assert_eq!(after.events, 1, "the plan appended an event");
        assert_eq!(after.doc, before.doc, "the plan rewrote the D-S6 document");
        assert_eq!(
            after.touched, before.touched,
            "the plan touched the D-S6 row"
        );
        assert_eq!(
            after.built, before.built,
            "the plan rewrote the learner model"
        );
        assert_eq!(after.cursor, 1);
    })
    .await;
}

// --------------------------------------------------------------------------- //
// Acceptance 3: progress.done for a recomposed failed review
// --------------------------------------------------------------------------- //

/// Trap W3. A failed review stays DUE, so the composer builds it again under the
/// SAME task id while the state row still has it closed. The plan must report
/// `progress.done: true`, or a client that tracks completion itself asks to
/// serve a closed task and gets `409 task_complete` with nothing it can do.
#[tokio::test]
async fn progress_done_is_true_for_a_recomposed_failed_review() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "recompose@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;
        // Enroll in `c1`, so `fractions` stays out of scope. An in-scope lesson
        // that encompasses `addition` at weight 0.8 would compress the review
        // out of the plan before this test could look at it.
        seed_event(
            &db,
            user,
            2,
            &Event::Enrolled(cadus_core::event::Enrolled {
                ts: Timestamp::from_micros(BASE_US + 1),
                session: Some(SESSION.to_string()),
                v: SchemaVersion,
                course: cadus_core::event::Slug::new("c1").unwrap(),
                reason: None,
                return_to: None,
            }),
        )
        .await;
        seed_due_review(&db, user, 2).await;

        // The task id the composer assigns is `{session}-{type}-{topic}`
        // (`selector.rs`, `assign_ids`).
        let task_id = format!("{SESSION}-review-addition");
        let mut scratch = WebState::for_session(SESSION);
        scratch.tasks.insert(
            task_id.clone(),
            TaskProgress {
                task_id: task_id.clone(),
                task_type: "review".to_string(),
                total: 3,
                served: 3,
                answered: 3,
                done: true,
                current_kp: None,
            },
        );
        sqlx::query!(
            "INSERT INTO web_states (user_id, doc) VALUES ($1, $2)",
            user,
            scratch.to_doc()
        )
        .execute(&db.admin)
        .await
        .unwrap();

        let (status, _, body) =
            call(&app, Method::GET, "/api/session/plan", Some(user), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let plan = parse(&body);

        let review = plan["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|task| task["task_id"] == json!(task_id.clone()))
            .unwrap_or_else(|| panic!("the failed review did not recompose: {body}"));

        assert_eq!(review["task_type"], "review");
        assert_eq!(review["topic"]["id"], "addition");
        assert_eq!(review["topic"]["module"], "M1");
        assert_eq!(review["progress"]["done"], true);
        assert_eq!(review["progress"]["answered"], 3);

        // A task the plan merely LISTED and the state row does not know reports
        // the untouched pair, and no row was installed for it.
        let fresh = plan["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|task| task["task_id"] != json!(task_id.clone()));
        if let Some(task) = fresh {
            assert_eq!(task["progress"]["done"], false);
            assert_eq!(task["progress"]["answered"], 0);
        }
        let stored: Value = sqlx::query_scalar!(
            r#"SELECT doc AS "doc!" FROM web_states WHERE user_id = $1"#,
            user
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(
            WebState::from_doc(&stored).unwrap().tasks.len(),
            1,
            "the plan installed a TaskProgress row"
        );
    })
    .await;
}

// --------------------------------------------------------------------------- //
// Acceptance 4: the export round-trips
// --------------------------------------------------------------------------- //

/// `GET /api/export` streams one canonical event per line, and every line reads
/// back through `Event::from_json` as the event that was stored.
#[tokio::test]
async fn the_export_round_trips_through_the_event_reader() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "export@example.com").await;
        let stranger = common::seed_learner(&db, "stranger@example.com").await;
        let app = app(&db);

        let written = vec![
            Event::SessionStart(SessionStart {
                ts: Timestamp::from_micros(BASE_US),
                session: Some(SESSION.to_string()),
                v: SchemaVersion,
            }),
            Event::Enrolled(cadus_core::event::Enrolled {
                ts: Timestamp::from_micros(BASE_US + 1),
                session: Some(SESSION.to_string()),
                v: SchemaVersion,
                course: cadus_core::event::Slug::new("c1").unwrap(),
                reason: None,
                return_to: None,
            }),
        ];
        for (index, event) in written.iter().enumerate() {
            seed_event(&db, user, index as i64 + 1, event).await;
        }
        // The other tenant's log must never appear in this export (C3).
        seed_event(
            &db,
            stranger,
            1,
            &Event::SessionStart(SessionStart {
                ts: Timestamp::from_micros(BASE_US),
                session: Some("s_2026-01-01z".to_string()),
                v: SchemaVersion,
            }),
        )
        .await;

        let (status, content_type, body) =
            call(&app, Method::GET, "/api/export", Some(user), None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(content_type, "application/x-ndjson");

        let lines: Vec<&str> = body.lines().collect();
        assert_eq!(lines.len(), 2, "the export carried {} lines", lines.len());
        for (line, original) in lines.iter().zip(&written) {
            let parsed = Event::from_json(line).unwrap();
            assert_eq!(&parsed, original, "the export line did not round-trip");
        }
        assert!(
            !body.contains("s_2026-01-01z"),
            "the export carried another tenant's session"
        );

        // Nothing was written by the export.
        let total: i64 = sqlx::query_scalar!(r#"SELECT count(*) AS "n!" FROM events"#)
            .fetch_one(&db.admin)
            .await
            .unwrap();
        assert_eq!(total, 3);
    })
    .await;
}
