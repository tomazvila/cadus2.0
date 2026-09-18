//! `POST /api/admin/content/{digest}/revoke` — the S6 admin action that stops
//! serving one digest whose key is wrong, and the D-F2 regrade that then
//! recovers an affected attempt.
//!
//! Every expected value here is a LITERAL: the status codes, the error codes,
//! and the fields of the two answers. The serve-path proof is the exact read
//! the serve routes make — `cadus_store::content::approved_document`, the
//! `status = 'approved'` read of `content_store` — so the test pins the
//! production filter and not a copy of it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod common;
use cadus_store::content::approved_document;
use common::admin::*;
use common::{SESSION_TOKEN_TWO, events_of_type, post_bearer, seed_event, send};

/// The serving key of the fixture knowledge point.
const KEY: &str = "band/kp1";

/// The digest the admin revokes.
const WRONG: &str = "r6-wrong-key-digest";

/// A digest the revocation never touches.
const OTHER: &str = "r6-other-key-digest";

/// The revoke path, with [`WRONG`] in the digest segment.
const REVOKE: &str = "/api/admin/content/r6-wrong-key-digest/revoke";

/// The regrade path, with the seeded ungraded attempt id in the segment.
const REGRADE: &str = "/api/admin/ungraded/t-1-0/regrade";

/// The session of the seeded log.
const SESS: &str = "s_2026-01-01a";

/// The instant of the first seeded event, in microseconds.
const T0_US: i64 = 1_767_258_000_000_000;

/// The reason the seeded ungraded attempt carries.
const UN_GRADED_REASON: &str = "no deterministic verdict for a proof";

/// The reason the admin revokes with.
const REVOKE_REASON: &str = "the answer key is wrong";

/// The refusal of a revoke body with no usable reason.
const REASON_MESSAGE: &str = "A rejection needs a reason: send a JSON object with a non-empty \
     \"reason\" string.";

/// Seed one approved page of one kind on the fixture knowledge point.
async fn seed_approved(db: &TestDb, digest: &str, kind: &str) {
    seed_row(
        db,
        &Seed {
            digest,
            kp_id: KEY,
            kind,
            status: "approved",
            body: json!({"concept": "take the smaller from the larger"}),
            attempts: 1,
            cost: None,
            curriculum_digest: None,
            review_engine_digest: None,
        },
    )
    .await;
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
    let mut body = json!({
        "type": "attempt",
        "ts": "2026-01-01T09:01:00Z",
        "session": SESS,
        "v": 2,
        "attempt_id": "t-1-0",
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
    body["outcome"] = json!({"ungraded": {"reason": UN_GRADED_REASON}});
    seed_event(db, user, 2, T0_US + 60_000_000, SESS, body).await;
}

/// What the serve path reads for one kind on the fixture knowledge point.
///
/// This is `cadus_store::content::approved_document` on the runtime pool, the
/// one read the teach and hint routes make, not a test double of it.
async fn served(db: &TestDb, kind: &str) -> Option<String> {
    approved_document(&db.app, KEY, kind)
        .await
        .unwrap()
        .map(|doc| doc.digest)
}

/// The app with the admin, one wrong-key page, one other page, and the log.
async fn scene(db: &TestDb) -> (Router, Uuid) {
    let app = app(db);
    let admin = seed_admin(db).await;
    seed_approved(db, WRONG, "teach").await;
    seed_approved(db, OTHER, "hint_ladder").await;
    seed_log(db, admin).await;
    (app, admin)
}

// --------------------------------------------------------------------------- //
// The chain
// --------------------------------------------------------------------------- //

/// ACCEPTANCE. A revoke stops the serving of that one digest, and the existing
/// regrade path then emits the `regraded` event for an affected attempt.
#[tokio::test]
async fn a_revoke_stops_the_serving_and_the_regrade_emits_the_event() {
    TestDb::with(|db| async move {
        let (app, admin) = scene(&db).await;

        // The serve path returns the wrong-key content before the revocation,
        // and the untouched content beside it.
        assert_eq!(served(&db, "teach").await.as_deref(), Some(WRONG));
        assert_eq!(served(&db, "hint_ladder").await.as_deref(), Some(OTHER));

        let answer = admin_post(&app, REVOKE, &json!({"reason": REVOKE_REASON})).await;
        assert_eq!(answer.status.as_u16(), 200, "{}", answer.body);
        assert_eq!(
            answer.body,
            json!({
                "digest": WRONG,
                "status": "rejected",
                "revoked_by": admin,
                "reason": REVOKE_REASON,
            })
        );

        // The revoked digest never serves again; the other digest stands, and
        // no curriculum YAML was read or edited.
        assert_eq!(served(&db, "teach").await, None);
        assert_eq!(served(&db, "hint_ladder").await.as_deref(), Some(OTHER));

        // The row carries the reason and no approval stamp.
        assert_eq!(
            row_state(&db, WRONG).await,
            (
                "rejected".to_string(),
                Some(REVOKE_REASON.to_string()),
                None,
                false
            )
        );

        // The existing regrade path then recovers the affected attempt, and the
        // log gains the `regraded` event the fold replays.
        let regrade = admin_post(&app, REGRADE, &json!({"outcome": "incorrect"})).await;
        assert_eq!(regrade.status.as_u16(), 200, "{}", regrade.body);
        assert_eq!(regrade.body["attempt_id"], json!("t-1-0"));
        assert_eq!(regrade.body["replayed"], json!(true));
        let regraded = events_of_type(&db, admin, "regraded").await;
        assert_eq!(regraded.len(), 1, "{}", regrade.body);
    })
    .await;
}

/// A re-call is idempotent: the same answer, the same row, and still no serve.
#[tokio::test]
async fn a_second_revoke_answers_the_same_state() {
    TestDb::with(|db| async move {
        let (app, _admin) = scene(&db).await;

        let first = admin_post(&app, REVOKE, &json!({"reason": REVOKE_REASON})).await;
        assert_eq!(first.status.as_u16(), 200, "{}", first.body);
        let second = admin_post(&app, REVOKE, &json!({"reason": REVOKE_REASON})).await;
        assert_eq!(second.status.as_u16(), 200, "{}", second.body);
        assert_eq!(first.body, second.body);

        assert_eq!(served(&db, "teach").await, None);
        let (status, reason, approved_by, stamped) = row_state(&db, WRONG).await;
        assert_eq!(status, "rejected");
        assert_eq!(reason, Some(REVOKE_REASON.to_string()));
        assert_eq!(approved_by, None);
        assert!(!stamped, "the revoked row carries an approval stamp");
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The refusals
// --------------------------------------------------------------------------- //

/// A revoke with no usable reason is `422`, and it writes nothing.
#[tokio::test]
async fn a_revoke_without_a_reason_is_unprocessable() {
    TestDb::with(|db| async move {
        let (app, _admin) = scene(&db).await;

        for body in [json!({}), json!({"reason": null}), json!({"reason": "   "})] {
            let answer = admin_post(&app, REVOKE, &body).await;
            assert_invalid_request(&answer, REASON_MESSAGE);
        }

        assert_eq!(served(&db, "teach").await.as_deref(), Some(WRONG));
    })
    .await;
}

/// A revoke of a digest the table does not hold is `404 not_found`.
#[tokio::test]
async fn a_revoke_of_an_unknown_digest_is_not_found() {
    TestDb::with(|db| async move {
        let (app, _admin) = scene(&db).await;

        let answer = admin_post(
            &app,
            "/api/admin/content/r6-no-such-digest/revoke",
            &json!({"reason": REVOKE_REASON}),
        )
        .await;
        assert_eq!(answer.status.as_u16(), 404, "{}", answer.body);
        assert_eq!(answer.code(), "not_found");
        assert_eq!(served(&db, "teach").await.as_deref(), Some(WRONG));
    })
    .await;
}

/// A live session on an account that is not an admin is `403 forbidden`.
#[tokio::test]
async fn a_session_that_is_not_an_admin_is_forbidden() {
    TestDb::with(|db| async move {
        let (app, _admin) = scene(&db).await;
        seed_account(&db, "not-admin@example.test", SESSION_TOKEN_TWO.1, false).await;

        let answer = send(
            &app,
            post_bearer(
                REVOKE,
                SESSION_TOKEN_TWO.0,
                &json!({"reason": REVOKE_REASON}),
            ),
        )
        .await;
        assert_eq!(answer.status.as_u16(), 403, "{}", answer.body);
        assert_eq!(answer.code(), "forbidden");
        assert_eq!(served(&db, "teach").await.as_deref(), Some(WRONG));
    })
    .await;
}

/// A deployment with no admin connection refuses the revoke with `503`.
#[tokio::test]
async fn a_revoke_with_no_admin_connection_is_service_unavailable() {
    TestDb::with(|db| async move {
        let app = app_without_admin(&db);
        seed_admin(&db).await;
        seed_approved(&db, WRONG, "teach").await;

        let answer = admin_post(&app, REVOKE, &json!({"reason": REVOKE_REASON})).await;
        assert_eq!(answer.status.as_u16(), 503, "{}", answer.body);
        assert_eq!(answer.code(), "admin_path_unavailable");
        assert_eq!(served(&db, "teach").await.as_deref(), Some(WRONG));
    })
    .await;
}
