//! The entropy-failure arms of the routes that draw a token. The kernel never
//! refuses entropy on the build box, so the thread-local seam of
//! `cadus_web::auth::token` is the one way to reach them. Each test sets the
//! seam, sends its request on the same thread, and the route answers `500`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_web::auth::token::refuse_entropy_after;
use common::*;

/// The OAuth start route draws the state and the verifier of the handshake:
/// a refused draw makes it `500 internal_error`.
#[tokio::test]
async fn a_refused_draw_is_500_on_the_oauth_start() {
    TestDb::with(|db| async move {
        let app = oauth_app(&db, google_config(Arc::new(FakeProvider::new())));
        refuse_entropy_after(Some(0));
        let answer = send(&app, get("/api/auth/oauth/google/start")).await;
        refuse_entropy_after(None);
        assert_eq!(answer.status.as_u16(), 500, "{}", answer.body);
        assert_eq!(answer.code(), "internal_error");
    })
    .await;
}

/// The OAuth callback mints a session token after it resolves the account: a
/// refused draw makes it `500 internal_error`.
#[tokio::test]
async fn a_refused_draw_is_500_on_the_oauth_callback() {
    TestDb::with(|db| async move {
        let app = google_app(&db, Arc::new(google_verified()));
        refuse_entropy_after(Some(0));
        let answer = google_callback(&app, GOOGLE_QUERY).await;
        refuse_entropy_after(None);
        assert_eq!(answer.status.as_u16(), 500, "{}", answer.body);
        assert_eq!(answer.code(), "internal_error");
    })
    .await;
}

/// A login mints a session token: a refused draw makes it `500`. The account
/// and its verification are set up before the seam, so their own draws run.
#[tokio::test]
async fn a_refused_draw_is_500_on_the_login() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        verified_login(&db, &app, "entropy-login@example.com", GOOD_PASSWORD).await;
        refuse_entropy_after(Some(0));
        let body = json!({ "email": "entropy-login@example.com", "password": GOOD_PASSWORD });
        let answer = assert_internal_answer(&app, post("/api/auth/login", &body)).await;
        refuse_entropy_after(None);
        let _ = answer;
    })
    .await;
}

/// A password-reset mint draws an out-of-band token: a refused draw makes the
/// forgot route `500`. The account is created before the seam.
#[tokio::test]
async fn a_refused_draw_is_500_on_the_forgot() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        signup(&app, "entropy-forgot@example.com", GOOD_PASSWORD).await;
        refuse_entropy_after(Some(0));
        let body = json!({ "email": "entropy-forgot@example.com" });
        let answer = assert_internal_answer(&app, post("/api/auth/password/forgot", &body)).await;
        refuse_entropy_after(None);
        let _ = answer;
    })
    .await;
}
