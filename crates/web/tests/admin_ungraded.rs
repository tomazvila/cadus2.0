//! `/api/admin/ungraded*` — the recovery path of the third outcome (D-F2).
//!
//! An UNGRADED attempt moves no learner state, so a knowledge point that only
//! collects ungraded attempts never advances. These two routes list what waits
//! and let a human give one attempt the verdict the checker could not reach.
//!
//! Every expected value here is a LITERAL: the status codes, the error codes,
//! the reasons the seeded events carry, and the fields of both answers.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod common;
use common::admin::*;
use common::{SESSION_TOKEN_TWO, get, get_bearer, post_bearer, seed_event, send};

/// The two paths of this unit, with the seeded attempt id in the path segment.
const LIST: &str = "/api/admin/ungraded";
const REGRADE: &str = "/api/admin/ungraded/t-1-0/regrade";

/// An attempt id the log does not hold.
const ABSENT_REGRADE: &str = "/api/admin/ungraded/t-9-9/regrade";

/// The session of the seeded log.
const SESS: &str = "s_2026-01-01a";

/// The instant of the first seeded event, in microseconds.
const T0_US: i64 = 1_767_258_000_000_000;

/// The reason the seeded ungraded attempt carries.
const REASON: &str = "no deterministic verdict for a proof";

/// The admin account of a test, with the seeded log behind it.
async fn scene(db: &TestDb) -> (Router, Uuid) {
    let app = app(db);
    let user = seed_admin(db).await;
    seed_log(db, user).await;
    (app, user)
}

/// Seed one log: a session start, one ungraded attempt, one decided attempt.
///
/// The topic is `band`, the one topic of the fixture curriculum.
async fn seed_log(db: &TestDb, user: Uuid) {
    seed_event(
        db,
        user,
        1,
        T0_US,
        SESS,
        json!({"type": "session_start", "ts": "2026-01-01T09:00:00Z", "session": SESS, "v": 2}),
    )
    .await;
    seed_event(
        db,
        user,
        2,
        T0_US + 60_000_000,
        SESS,
        attempt("t-1-0", true),
    )
    .await;
    seed_event(
        db,
        user,
        3,
        T0_US + 120_000_000,
        SESS,
        attempt("t-1-1", false),
    )
    .await;
}

/// One `attempt` payload on `band`, ungraded or decided.
fn attempt(attempt_id: &str, ungraded: bool) -> Value {
    let mut body = json!({
        "type": "attempt",
        "ts": "2026-01-01T09:01:00Z",
        "session": SESS,
        "v": 2,
        "attempt_id": attempt_id,
        "task_id": "t-1",
        "topic": "band",
        "kp": "kp1",
        "task_type": "lesson",
        "problem": {"text": "Prove it.", "expected": "5"},
        "given_answer": "Assume the contrary.",
        "correct": false,
        "secs": 20,
        "work_quality": "nearly_passable",
    });
    if ungraded {
        body["outcome"] = json!({"ungraded": {"reason": REASON}});
    }
    body
}

// --------------------------------------------------------------------------- //
// The two refusals
// --------------------------------------------------------------------------- //

/// A request with no credential is `401 unauthorized` on both routes.
#[tokio::test]
async fn a_request_with_no_session_is_unauthorized_on_both() {
    TestDb::with(|db| async move {
        let app = app(&db);
        for answer in [
            send(&app, get(LIST)).await,
            send(&app, common::post(REGRADE, &json!({"outcome": "correct"}))).await,
        ] {
            assert_eq!(answer.status.as_u16(), 401, "{}", answer.body);
            assert_eq!(answer.code(), "unauthorized");
        }
    })
    .await;
}

/// A live session on an account that is not an admin is `403 forbidden` on both.
#[tokio::test]
async fn a_session_that_is_not_an_admin_is_forbidden_on_both() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_account(&db, "not-admin@example.test", SESSION_TOKEN_TWO.1, false).await;
        let token = SESSION_TOKEN_TWO.0;
        for answer in [
            send(&app, get_bearer(LIST, token)).await,
            send(
                &app,
                post_bearer(REGRADE, token, &json!({"outcome": "correct"})),
            )
            .await,
        ] {
            assert_eq!(answer.status.as_u16(), 403, "{}", answer.body);
            assert_eq!(answer.code(), "forbidden");
        }
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The list
// --------------------------------------------------------------------------- //

/// The list names every ungraded attempt of the log, and no decided one.
#[tokio::test]
async fn the_list_holds_the_ungraded_attempts_and_no_other() {
    TestDb::with(|db| async move {
        let (app, _user) = scene(&db).await;
        let answer = admin_get(&app, LIST).await;
        assert_eq!(answer.status.as_u16(), 200, "{}", answer.body);
        assert_eq!(answer.body["limit"], json!(20));
        let items = answer.body["items"].as_array().unwrap();
        assert_eq!(items.len(), 1, "{}", answer.body);
        assert_eq!(items[0]["attempt_id"], "t-1-0");
        assert_eq!(items[0]["topic"], "band");
        assert_eq!(items[0]["reason"], REASON);
    })
    .await;
}

/// A learner with no ungraded attempt reads an empty list, not a failure.
#[tokio::test]
async fn an_empty_recovery_list_is_an_empty_array() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;
        let answer = admin_get(&app, LIST).await;
        assert_eq!(answer.status.as_u16(), 200, "{}", answer.body);
        assert_eq!(answer.body["items"], json!([]));
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The regrade
// --------------------------------------------------------------------------- //

/// A regrade appends a `regraded` event, replays, and clears the entry.
#[tokio::test]
async fn a_regrade_appends_a_correction_and_clears_the_entry() {
    TestDb::with(|db| async move {
        let (app, user) = scene(&db).await;
        let answer = admin_post(&app, REGRADE, &json!({"outcome": "correct"})).await;
        assert_eq!(answer.status.as_u16(), 200, "{}", answer.body);
        assert_eq!(answer.body["attempt_id"], "t-1-0");
        assert_eq!(answer.body["outcome"], "correct");
        assert_eq!(answer.body["replayed"], json!(true));
        assert_eq!(answer.body["ungraded"], json!(0));

        // The original row stands and one `regraded` row is beside it (C2).
        assert_eq!(common::events_of_type(&db, user, "attempt").await.len(), 2);
        assert_eq!(common::events_of_type(&db, user, "regraded").await.len(), 1);

        // The entry is gone from the list, because the attempt is decided now.
        let listed = admin_get(&app, LIST).await;
        assert_eq!(listed.body["items"], json!([]));
    })
    .await;
}

/// An `incorrect` verdict is a verdict too: it clears the entry the same way.
#[tokio::test]
async fn a_regrade_to_incorrect_decides_the_attempt() {
    TestDb::with(|db| async move {
        let (app, user) = scene(&db).await;
        let answer = admin_post(&app, REGRADE, &json!({"outcome": "incorrect"})).await;
        assert_eq!(answer.status.as_u16(), 200, "{}", answer.body);
        assert_eq!(answer.body["outcome"], "incorrect");
        assert_eq!(answer.body["ungraded"], json!(0));
        assert_eq!(common::events_of_type(&db, user, "regraded").await.len(), 1);
    })
    .await;
}

/// A body that names no verdict is refused, and it appends nothing.
#[tokio::test]
async fn a_body_with_no_verdict_is_refused() {
    TestDb::with(|db| async move {
        let (app, user) = scene(&db).await;
        for body in [
            json!({}),
            json!({"outcome": "maybe"}),
            json!({"outcome": true}),
        ] {
            let answer = admin_post(&app, REGRADE, &body).await;
            assert_eq!(answer.status.as_u16(), 422, "{}", answer.body);
            assert_eq!(answer.code(), "invalid_request");
        }
        assert_eq!(common::events_of_type(&db, user, "regraded").await.len(), 0);
    })
    .await;
}

/// An attempt id the log does not hold as an ungraded attempt is `404`.
///
/// The decided attempt of the seeded log is refused too: a checker verdict is
/// never restated by this route (C4).
#[tokio::test]
async fn an_attempt_that_is_not_ungraded_is_not_found() {
    TestDb::with(|db| async move {
        let (app, user) = scene(&db).await;
        for path in [ABSENT_REGRADE, "/api/admin/ungraded/t-1-1/regrade"] {
            let answer = admin_post(&app, path, &json!({"outcome": "correct"})).await;
            assert_eq!(answer.status.as_u16(), 404, "{path}: {}", answer.body);
            assert_eq!(answer.code(), "not_found");
        }
        assert_eq!(common::events_of_type(&db, user, "regraded").await.len(), 0);
    })
    .await;
}
