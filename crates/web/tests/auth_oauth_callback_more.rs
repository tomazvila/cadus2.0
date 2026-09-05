//! The OAuth callback under the provider shapes and the faults its first
//! files left open: a transport that answers nothing, a token or an identity
//! body that is not JSON, a bind that fails, a lookup that fails, a link whose
//! account read finds nothing, the sign-up race, a cookie the posture cannot
//! write, and a deployment with no redirect base.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axum::body::Body;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::auth::oauth::{Credentials, OAuthConfig};
use cadus_web::auth::password::Argon2Profile;
use cadus_web::cookie::CookiePosture;
use cadus_web::{AppState, create_app};
use common::*;

/// Run the default Google callback on `app` and read `status` and `code` back.
async fn callback_refused(app: &Router, status: u16, code: &str) -> Answer {
    let answer = google_callback(app, GOOGLE_QUERY).await;
    assert_eq!(answer.status.as_u16(), status, "{}", answer.body);
    assert_eq!(answer.code(), code);
    answer
}

/// Link the Google identity into a fresh account on `app`, so the next callback
/// walks the link.
async fn linked(db: &TestDb) -> Router {
    let app = google_app(db, Arc::new(google_verified()));
    let answer = google_callback(&app, GOOGLE_QUERY).await;
    assert_eq!(answer.status.as_u16(), 302, "{}", answer.body);
    assert_eq!(link_rows(db).await, 1);
    app
}

/// A Google configuration on `transport` with no pinned redirect base.
fn google_config_without_base(transport: Arc<FakeProvider>) -> OAuthConfig {
    OAuthConfig {
        google: Some(Credentials {
            client_id: GOOGLE_CLIENT_ID.to_string(),
            client_secret: GOOGLE_CLIENT_SECRET.to_string(),
        }),
        github: None,
        redirect_base: None,
        transport: Some(transport),
    }
}

/// A request of `path` with the Google handshake cookie and, when given, a
/// `Host` header.
fn with_host(path: &str, host: Option<&str>) -> Request<Body> {
    let mut request = get_with_cookie(path, &google_jar());
    if let Some(host) = host {
        request
            .headers_mut()
            .insert("host", axum::http::HeaderValue::from_str(host).unwrap());
    }
    request
}

/// A transport that answers nothing, a token body that is not JSON, and an
/// identity body that is not JSON each refuse the callback as `400`.
#[tokio::test]
async fn a_provider_answer_the_exchange_cannot_read_is_400() {
    TestDb::with(|db| async move {
        refused_google_callback(&db, FakeProvider::new()).await;
        refused_google_callback(
            &db,
            FakeProvider::new().answer(GOOGLE_TOKEN_URL, 200, "not json"),
        )
        .await;
        refused_google_callback(&db, google_provider("not json")).await;
    })
    .await;
}

/// A tenant bind that fails stops the callback before its writes.
#[tokio::test]
async fn a_bind_that_fails_is_500_on_the_callback() {
    TestDb::with(|db| async move {
        let app = google_app(&db, Arc::new(google_verified()));
        fail_tenant_bind(&db).await;
        callback_refused(&app, 500, "internal_error").await;
        assert_eq!(session_rows(&db).await, 0);
    })
    .await;
}

/// A link lookup that fails, and an account read that fails behind a link,
/// each stop the callback.
#[tokio::test]
async fn a_lookup_that_fails_is_500_on_the_callback() {
    TestDb::with(|db| async move {
        let app = linked(&db).await;
        drop_function(&db, "oauth_account_lookup(text, text)").await;
        callback_refused(&app, 500, "internal_error").await;
    })
    .await;
    TestDb::with(|db| async move {
        let app = linked(&db).await;
        drop_function(&db, "auth_user_by_id(uuid)").await;
        callback_refused(&app, 500, "internal_error").await;
    })
    .await;
}

/// A link whose account read finds nothing walks on to the address.
///
/// A real dangling link cannot stand: the link row goes with its account. The
/// lookup below hides the account and leaves the link, so the walk reaches
/// the address and then meets the one fault that state holds, a second link
/// row for the same provider account. The answer is that fault, and no second
/// account.
#[tokio::test]
async fn a_dangling_link_walks_on_to_the_address() {
    TestDb::with(|db| async move {
        let app = linked(&db).await;
        hide_user_by_id(&db).await;
        callback_refused(&app, 500, "internal_error").await;
        assert_eq!(user_count(&db).await, 1);
        assert_eq!(link_rows(&db).await, 1);
    })
    .await;
}

/// The sign-up race: an address the first read misses and the insert then
/// finds taken is read back and linked into, and an address that stays unseen
/// after the insert is `500`.
#[tokio::test]
async fn a_sign_up_race_reads_the_address_back_or_fails() {
    TestDb::with(|db| async move {
        let app = google_app(&db, Arc::new(google_verified()));
        signup(&app, "learner@example.com", GOOD_PASSWORD).await;
        hide_user_by_email_first(&db, 1).await;
        let answer = google_callback(&app, GOOGLE_QUERY).await;
        assert_eq!(answer.status.as_u16(), 302, "{}", answer.body);
        assert_eq!(user_count(&db).await, 1);
    })
    .await;
    TestDb::with(|db| async move {
        let app = google_app(&db, Arc::new(google_verified()));
        signup(&app, "learner@example.com", GOOD_PASSWORD).await;
        hide_user_by_email_first(&db, i64::MAX).await;
        callback_refused(&app, 500, "internal_error").await;
    })
    .await;
}

/// A session cookie the posture cannot write is `500` after the identity.
#[tokio::test]
async fn a_cookie_the_posture_cannot_write_is_500_on_the_callback() {
    TestDb::with(|db| async move {
        let app = create_app(
            AppState::new(Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS))
                .with_argon2(Argon2Profile::TEST)
                .with_oauth(google_config(Arc::new(google_verified())))
                .with_posture(CookiePosture {
                    name: "bad name",
                    secure: true,
                }),
        );
        callback_refused(&app, 500, "internal_error").await;
    })
    .await;
}

/// With no pinned redirect base the routes build it from the request: a
/// `Host` header serves, and a request with none is `500` on the start and on
/// the callback.
#[tokio::test]
async fn a_deployment_with_no_redirect_base_reads_the_host_or_refuses() {
    TestDb::with(|db| async move {
        let app = oauth_app(&db, google_config_without_base(Arc::new(google_verified())));
        let start = "/api/auth/oauth/google/start";
        let answer = send(&app, with_host(start, Some("tutor.example"))).await;
        assert_eq!(answer.status.as_u16(), 302, "{}", answer.body);

        let answer = send(&app, with_host(start, None)).await;
        assert_eq!(answer.status.as_u16(), 500, "{}", answer.body);

        let callback = format!("/api/auth/oauth/google/callback?{GOOGLE_QUERY}");
        let answer = send(&app, with_host(&callback, None)).await;
        assert_eq!(answer.status.as_u16(), 500, "{}", answer.body);
    })
    .await;
}
