//! `/api/admin/outbox*` — the operator drain of the mail queue (ISSUE-1).
//!
//! A new sign-up answers `verification_required` and stores the SHA-256 digest
//! of the verification token alone. Before this unit there was no reader for the
//! raw token, so a real learner could never verify and never sign in. The mint
//! path now queues the raw token and its link in `email_outbox`, and these two
//! routes are the operator path that drains it.
//!
//! Every expected value below is a LITERAL: the status codes, the error codes,
//! the message kind, and the link shape.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod common;
use common::admin::{app, seed_account, seed_admin};
use common::{
    GOOD_PASSWORD, SESSION_TOKEN_ONE, SESSION_TOKEN_TWO, TestDb, get, get_bearer, json,
    post_bearer, send, signup,
};

/// The drain list path.
const LIST: &str = "/api/admin/outbox";

/// The address a sign-up registers in this file.
const NEW_EMAIL: &str = "outbox-learner@example.test";

/// The one pending `(kind, payload)` row of `email`, read with the admin pool.
async fn pending_row(db: &TestDb, email: &str) -> (String, serde_json::Value) {
    sqlx::query_as::<_, (String, serde_json::Value)>(
        "SELECT kind, payload FROM email_outbox
         WHERE to_addr = $1 AND status = 'pending'
         ORDER BY created_at DESC, id LIMIT 1",
    )
    .bind(email)
    .fetch_one(&db.admin)
    .await
    .expect("the outbox holds a pending row")
}

// --------------------------------------------------------------------------- //
// The queue at sign-up
// --------------------------------------------------------------------------- //

/// Sign-up queues one pending verification message, and the raw token it
/// carries is the token the real verify route spends.
#[tokio::test]
async fn signup_queues_a_verification_message_with_a_usable_token() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;

        let answer = signup(&app, NEW_EMAIL, GOOD_PASSWORD).await;
        assert_eq!(answer.status.as_u16(), 200, "{}", answer.body);
        assert_eq!(answer.body["status"], "verification_required");

        let (kind, payload) = pending_row(&db, NEW_EMAIL).await;
        assert_eq!(kind, "verify");
        assert_eq!(payload["purpose"], "verify");
        let token = payload["token"]
            .as_str()
            .expect("the payload carries the raw token")
            .to_string();
        assert!(!token.is_empty(), "the raw token is not empty");
        assert_eq!(payload["link"], json!(format!("/?verify={token}")));

        // The queued token is live: the real route stamps the address and opens
        // a session.
        let verified = send(
            &app,
            common::post("/api/auth/verify-email", &json!({ "token": token })),
        )
        .await;
        assert_eq!(verified.status.as_u16(), 200, "{}", verified.body);
        assert_eq!(verified.body["user"]["email"], NEW_EMAIL);
        assert_eq!(verified.body["user"]["email_verified"], json!(true));
    })
    .await;
}

/// A second sign-up for the same address queues no second message, because the
/// account is not created twice.
#[tokio::test]
async fn a_second_signup_for_one_address_queues_no_second_message() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;
        signup(&app, NEW_EMAIL, GOOD_PASSWORD).await;
        signup(&app, NEW_EMAIL, GOOD_PASSWORD).await;

        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM email_outbox WHERE to_addr = $1")
            .bind(NEW_EMAIL)
            .fetch_one(&db.admin)
            .await
            .unwrap();
        assert_eq!(count, 1, "one address owns one queued verification");
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The drain
// --------------------------------------------------------------------------- //

/// The admin drain lists the pending message across tenants, with its token and
/// link.
#[tokio::test]
async fn the_admin_drain_lists_the_pending_verification_message() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;
        signup(&app, NEW_EMAIL, GOOD_PASSWORD).await;

        let answer = send(&app, get_bearer(LIST, SESSION_TOKEN_ONE.0)).await;
        assert_eq!(answer.status.as_u16(), 200, "{}", answer.body);
        assert_eq!(answer.body["pending"], json!(1));
        let items = answer.body["items"].as_array().unwrap();
        assert_eq!(items.len(), 1, "{}", answer.body);

        let item = &items[0];
        assert_eq!(item["kind"], "verify");
        assert_eq!(item["status"], "pending");
        assert_eq!(item["to_addr"], NEW_EMAIL);
        let token = item["payload"]["token"]
            .as_str()
            .expect("the drain carries the raw token");
        assert!(!token.is_empty());
        assert_eq!(
            item["payload"]["link"],
            json!(format!("/?verify={token}"))
        );
    })
    .await;
}

/// Marking a message sent clears it from the drain, and a second mark is `404`.
#[tokio::test]
async fn marking_a_message_sent_clears_it_from_the_queue() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;
        signup(&app, NEW_EMAIL, GOOD_PASSWORD).await;

        let listed = send(&app, get_bearer(LIST, SESSION_TOKEN_ONE.0)).await;
        let id = listed.body["items"][0]["id"]
            .as_str()
            .expect("the drain carries an id")
            .to_string();
        let sent_path = format!("/api/admin/outbox/{id}/sent");

        let marked = send(
            &app,
            post_bearer(&sent_path, SESSION_TOKEN_ONE.0, &json!({})),
        )
        .await;
        assert_eq!(marked.status.as_u16(), 200, "{}", marked.body);
        assert_eq!(marked.body["id"], json!(id));
        assert_eq!(marked.body["status"], "sent");

        let listed = send(&app, get_bearer(LIST, SESSION_TOKEN_ONE.0)).await;
        assert_eq!(listed.body["pending"], json!(0));
        assert_eq!(listed.body["items"], json!([]));

        let again = send(
            &app,
            post_bearer(&sent_path, SESSION_TOKEN_ONE.0, &json!({})),
        )
        .await;
        assert_eq!(again.status.as_u16(), 404, "{}", again.body);
        assert_eq!(again.code(), "not_found");
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The guard
// --------------------------------------------------------------------------- //

/// Both routes refuse an anonymous caller with `401` and a non-admin with `403`.
#[tokio::test]
async fn the_drain_refuses_an_anonymous_and_a_non_admin_caller() {
    TestDb::with(|db| async move {
        let app = app(&db);

        let anonymous = send(&app, get(LIST)).await;
        assert_eq!(anonymous.status.as_u16(), 401, "{}", anonymous.body);
        assert_eq!(anonymous.code(), "unauthorized");

        seed_account(
            &db,
            "outbox-nonadmin@example.test",
            SESSION_TOKEN_TWO.1,
            false,
        )
        .await;
        let learner = send(&app, get_bearer(LIST, SESSION_TOKEN_TWO.0)).await;
        assert_eq!(learner.status.as_u16(), 403, "{}", learner.body);
        assert_eq!(learner.code(), "forbidden");
    })
    .await;
}

/// A deployment with no admin connection answers `503 admin_path_unavailable`.
#[tokio::test]
async fn a_deployment_with_no_admin_path_is_unavailable() {
    TestDb::with(|db| async move {
        // The plain auth router carries no `AppState::admin`.
        let app = common::app_of(&db);
        seed_admin(&db).await;
        let answer = send(&app, get_bearer(LIST, SESSION_TOKEN_ONE.0)).await;
        assert_eq!(answer.status.as_u16(), 503, "{}", answer.body);
        assert_eq!(answer.code(), "admin_path_unavailable");
    })
    .await;
}