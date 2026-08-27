//! Proof tests for the CSRF origin layer of M5 U1, in BOTH polarities.
//!
//! Spec `docs/reference/web-service-1.0-spec.md` section 3.1 (rows "CSRF" and
//! "Same-origin test"), section 10 (rows "CSRF" and "Login-CSRF"), and the
//! acceptance check of section 11, unit U1:
//!
//! > a cookie-authed cross-site POST to `/api/*` is `403 cross_origin_rejected`
//! > and a bearer one is not; a cross-origin POST to `/api/auth/login` is 403;
//! > a header-less curl POST is not
//!
//! Every assertion names a literal: a literal status code, the literal error
//! code `cross_origin_rejected`, a literal header value.
//!
//! **These tests never depend on a route existing.** The layer runs before the
//! router picks a handler, so the paths below (`/api/task/{id}/answer`,
//! `/api/auth/login`) reach it whether or not units U4 and U8 have added their
//! routes yet. What an ALLOWED request lands on depends on that:
//!
//! - `/api/task/{id}/answer` exists since unit U8, and these requests carry no
//!   credential, so an allowed one is `401 unauthorized` from the tenant guard;
//! - `/api/auth/*` waits for unit U4, so an allowed one is the `404 not_found`
//!   fallback.
//!
//! Both are literals, and asserting the literal is stronger than asserting
//! "not 403": a layer that answered `500` would pass the weaker check. No test
//! below reaches a handler that touches the database, because the tenant guard
//! answers first.
//!
//! The pool below is lazy and points at an address with no server, so a connect
//! never starts.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::cookie::CookiePosture;
use cadus_web::origin::OriginPolicy;
use cadus_web::{AppState, create_app};
use http_body_util::BodyExt;
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

/// A session cookie of the production posture, as a browser would send it.
const SECURE_COOKIE: &str = "__Host-cadus_session=s3cr3t";

/// A session cookie of the dev posture.
const DEV_COOKIE: &str = "cadus_session=s3cr3t";

/// Build the application with the production cookie posture and the fallback
/// origin policy.
fn app() -> Router {
    app_with(CookiePosture::SECURE, OriginPolicy::default())
}

/// Build the application with a chosen posture and origin policy.
fn app_with(posture: CookiePosture, origin: OriginPolicy) -> Router {
    let pool = PgPoolOptions::new()
        .connect_lazy("postgresql://nobody@127.0.0.1:1/nodb")
        .expect("a lazy pool needs no server");
    create_app(
        AppState::new(Db::new(pool, DEFAULT_CLIENT_TIMEOUT_MS))
            .with_posture(posture)
            .with_origin(origin),
    )
}

/// Send one request and return the status code and the body as text.
async fn send(app: &Router, request: Request<Body>) -> (StatusCode, String) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

/// A request builder for a POST to `path` with the headers of `headers`.
fn post(path: &str, headers: &[(&str, &str)]) -> Request<Body> {
    let mut builder = Request::builder().method("POST").uri(path);
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    builder.body(Body::empty()).unwrap()
}

/// The refusal body of the layer, character for character.
const REJECTION_BODY: &str = concat!(
    r#"{"error":{"code":"cross_origin_rejected","message":"The server refuses a cross-origin "#,
    r#"write that carries a session cookie. Send the request same-origin, or send a bearer "#,
    r#"token."}}"#
);

/// The `422` body of a pre-auth route that ran and read an empty body.
///
/// U1 wrote tests (9) and (10) before the `/api/auth/*` routes existed, so "the
/// layer let this through" showed as the `404` of an unmatched path. M5 U4
/// mounted the three pre-auth routes, so the same request now reaches the
/// handler and the handler refuses the empty body. The MARKER changed; the rule
/// under test did not. A `403` on any of them still fails the test.
const EMPTY_BODY_REJECTION: &str =
    r#"{"error":{"code":"invalid_request","message":"The body is not JSON."}}"#;

/// The body of the `401` the `Tenant` extractor answers, character for
/// character.
///
/// Unit U8 added `POST /api/task/{task_id}/answer`, so a request the CSRF layer
/// ALLOWS now reaches that route and its tenant guard. These tests carry no
/// credential, so "not refused by the CSRF layer" is this `401` on the task
/// paths. It is a literal, and it is stronger than "not 403": a layer that
/// answered `500` would pass the weaker check. The `/api/auth/*` paths that
/// unit U4 added answer their own refusal, so they carry their own marker.
const UNAUTHORIZED_BODY: &str = concat!(
    r#"{"error":{"code":"unauthorized","message":"This route needs a session. Send the "#,
    r#"session cookie or a bearer token."}}"#
);

/// (1) U1 acceptance: a cookie-authed cross-site POST to `/api/*` is
/// `403 cross_origin_rejected`.
///
/// `Sec-Fetch-Site: cross-site` is the signal a browser sends when a page on
/// another site posts a form here. The session cookie rides along, because it
/// is ambient. That pair is the whole shape of a CSRF write.
#[tokio::test]
async fn a_cookie_authed_cross_site_post_is_403_cross_origin_rejected() {
    let app = app();

    let (status, body) = send(
        &app,
        post(
            "/api/task/t-review-fractions/answer",
            &[
                ("host", "tutor.example"),
                ("cookie", SECURE_COOKIE),
                ("sec-fetch-site", "cross-site"),
                ("origin", "https://evil.example"),
            ],
        ),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(status.as_u16(), 403);
    assert_eq!(body, REJECTION_BODY);
}

/// (2) U1 acceptance: the same request with a bearer token is NOT refused.
///
/// A browser cannot attach an `Authorization` header cross-site, so a bearer
/// write cannot be forged. D-M5-5 keeps the channel for scripts and tests, and
/// the exemption is the reason it works.
#[tokio::test]
async fn the_same_cross_site_post_with_a_bearer_token_is_not_refused() {
    let app = app();

    let (status, body) = send(
        &app,
        post(
            "/api/task/t-review-fractions/answer",
            &[
                ("host", "tutor.example"),
                ("cookie", SECURE_COOKIE),
                ("sec-fetch-site", "cross-site"),
                ("origin", "https://evil.example"),
                ("authorization", "Bearer a-session-token"),
            ],
        ),
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(status.as_u16(), 401);
    assert_eq!(body, UNAUTHORIZED_BODY);
}

/// (3) The scheme of the bearer header is case-insensitive (RFC 9110), so
/// `bearer` earns the same exemption as `Bearer`.
#[tokio::test]
async fn a_lowercase_bearer_scheme_earns_the_exemption() {
    let app = app();

    let (status, _body) = send(
        &app,
        post(
            "/api/task/t-1/answer",
            &[
                ("host", "tutor.example"),
                ("cookie", SECURE_COOKIE),
                ("sec-fetch-site", "cross-site"),
                ("authorization", "bearer a-session-token"),
            ],
        ),
    )
    .await;

    assert_eq!(status.as_u16(), 401);
}

/// (4) A bare `Authorization: Bearer` earns NO exemption.
///
/// It authenticates nothing, so the request goes on to authenticate by cookie.
/// In 1.0 the CSRF layer read the header without reading its value and exempted
/// exactly the request it exists to refuse.
#[tokio::test]
async fn a_bearer_header_with_an_empty_value_earns_no_exemption() {
    let app = app();

    for header in ["Bearer", "Bearer ", "Bearer    "] {
        let (status, body) = send(
            &app,
            post(
                "/api/task/t-1/answer",
                &[
                    ("host", "tutor.example"),
                    ("cookie", SECURE_COOKIE),
                    ("sec-fetch-site", "cross-site"),
                    ("authorization", header),
                ],
            ),
        )
        .await;

        assert_eq!(
            status.as_u16(),
            403,
            "Authorization: {header:?} must earn no exemption; the body was {body}"
        );
        assert_eq!(body, REJECTION_BODY);
    }
}

/// (5) Another scheme is not the bearer channel and earns no exemption.
#[tokio::test]
async fn a_basic_authorization_header_earns_no_exemption() {
    let app = app();

    let (status, _body) = send(
        &app,
        post(
            "/api/task/t-1/answer",
            &[
                ("host", "tutor.example"),
                ("cookie", SECURE_COOKIE),
                ("sec-fetch-site", "cross-site"),
                ("authorization", "Basic dXNlcjpwYXNz"),
            ],
        ),
    )
    .await;

    assert_eq!(status.as_u16(), 403);
}

/// (6) Rule 1 fails CLOSED: a cookie-authed `/api/*` write with NO origin
/// signal at all is refused (trap W11).
///
/// A live session cookie is at stake here, so the absence of a signal is not
/// evidence of innocence. This is the polarity that test (9) below inverts for
/// the pre-auth routes.
#[tokio::test]
async fn a_cookie_authed_post_with_no_origin_signal_is_refused() {
    let app = app();

    let (status, body) = send(
        &app,
        post(
            "/api/task/t-1/answer",
            &[("host", "tutor.example"), ("cookie", SECURE_COOKIE)],
        ),
    )
    .await;

    assert_eq!(status.as_u16(), 403);
    assert_eq!(body, REJECTION_BODY);
}

/// (7) A request with no session cookie carries no ambient credential, so the
/// layer lets it through to the auth path, which answers `401`.
#[tokio::test]
async fn a_cross_site_post_without_a_session_cookie_is_not_refused() {
    let app = app();

    let (status, _body) = send(
        &app,
        post(
            "/api/task/t-1/answer",
            &[
                ("host", "tutor.example"),
                ("sec-fetch-site", "cross-site"),
                ("origin", "https://evil.example"),
            ],
        ),
    )
    .await;

    assert_eq!(status.as_u16(), 401);
}

/// (8) U1 acceptance: a cross-origin POST to `/api/auth/login` is `403`, and so
/// is one to `/api/auth/signup` and `/api/auth/verify-email`.
///
/// No session cookie exists yet, so rule 1 cannot see this. It is login-CSRF:
/// the attacker forces a victim into an account the attacker owns.
/// `verify-email` is on the list because it mints a session.
#[tokio::test]
async fn a_cross_origin_post_to_a_pre_auth_route_is_403() {
    let app = app();

    for path in [
        "/api/auth/login",
        "/api/auth/signup",
        "/api/auth/verify-email",
    ] {
        let (status, body) = send(
            &app,
            post(
                path,
                &[
                    ("host", "tutor.example"),
                    ("origin", "https://evil.example"),
                ],
            ),
        )
        .await;

        assert_eq!(
            status.as_u16(),
            403,
            "{path} must refuse a cross-origin POST"
        );
        assert_eq!(body, REJECTION_BODY);
    }

    // `Sec-Fetch-Site` alone is signal enough, with no `Origin` header.
    for site in ["cross-site", "same-site"] {
        let (status, _body) = send(
            &app,
            post(
                "/api/auth/login",
                &[("host", "tutor.example"), ("sec-fetch-site", site)],
            ),
        )
        .await;

        assert_eq!(
            status.as_u16(),
            403,
            "Sec-Fetch-Site: {site} must be refused"
        );
    }
}

/// (9) U1 acceptance: a header-less curl POST to a pre-auth route is NOT
/// refused.
///
/// Rule 2 fails OPEN, which is the exact opposite of rule 1 (trap W11). No
/// ambient credential is at stake before a session exists, and a browser CSRF
/// always carries one of the two signals, so a header-less programmatic client
/// — curl, a test, a native application — must go through.
#[tokio::test]
async fn a_header_less_post_to_a_pre_auth_route_is_not_refused() {
    let app = app();

    for path in [
        "/api/auth/login",
        "/api/auth/signup",
        "/api/auth/verify-email",
    ] {
        let (status, body) = send(&app, post(path, &[("host", "tutor.example")])).await;

        assert_eq!(
            status.as_u16(),
            422,
            "{path} must let a header-less POST through"
        );
        assert_eq!(body, EMPTY_BODY_REJECTION);
    }
}

/// (10) A pre-auth POST with a bearer token is exempt from rule 2 too.
#[tokio::test]
async fn a_bearer_post_to_a_pre_auth_route_is_not_refused() {
    let app = app();

    let (status, body) = send(
        &app,
        post(
            "/api/auth/login",
            &[
                ("host", "tutor.example"),
                ("origin", "https://evil.example"),
                ("authorization", "Bearer a-session-token"),
            ],
        ),
    )
    .await;

    assert_eq!(status.as_u16(), 422);
    assert_eq!(body, EMPTY_BODY_REJECTION);
}

/// (11) A same-origin cookie write goes through, by either signal.
#[tokio::test]
async fn a_same_origin_cookie_write_goes_through() {
    let app = app();

    for site in ["same-origin", "none"] {
        let (status, _body) = send(
            &app,
            post(
                "/api/task/t-1/answer",
                &[
                    ("host", "tutor.example"),
                    ("cookie", SECURE_COOKIE),
                    ("sec-fetch-site", site),
                ],
            ),
        )
        .await;

        assert_eq!(
            status.as_u16(),
            401,
            "Sec-Fetch-Site: {site} is same-origin"
        );
    }

    // The `Origin` header is the fallback signal. With no `PUBLIC_ORIGIN` and no
    // `X-Forwarded-Proto`, the layer rebuilds `http://` plus the Host header.
    let (status, _body) = send(
        &app,
        post(
            "/api/task/t-1/answer",
            &[
                ("host", "tutor.example"),
                ("cookie", SECURE_COOKIE),
                ("origin", "http://tutor.example"),
            ],
        ),
    )
    .await;

    assert_eq!(status.as_u16(), 401);
}

/// (12) A safe method is never refused, whatever it carries.
///
/// `GET`, `HEAD`, and `OPTIONS` change no state, so a forged one steals nothing.
#[tokio::test]
async fn a_safe_method_is_never_refused() {
    let app = app();

    for method in ["GET", "HEAD", "OPTIONS"] {
        let request = Request::builder()
            .method(method)
            .uri("/api/health")
            .header("host", "tutor.example")
            .header("cookie", SECURE_COOKIE)
            .header("sec-fetch-site", "cross-site")
            .header("origin", "https://evil.example")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();

        // GET answers 200. HEAD answers 200 with no body. OPTIONS matches no
        // method of the route, so it reaches the 405 fallback. None of the
        // three is 403, which is the whole claim.
        assert_ne!(
            response.status().as_u16(),
            403,
            "{method} must never be refused by the CSRF layer"
        );
    }
}

/// (13) The layer reads the cookie name of the ACTIVE posture and no other.
///
/// A cookie left over under the other name — after the dev flag flipped — must
/// be ignored, never trusted. Under the dev posture the plain name refuses and
/// the leftover `__Host-` name does not, and the production posture is the
/// mirror image.
#[tokio::test]
async fn the_layer_reads_the_cookie_name_of_the_active_posture() {
    let dev = app_with(CookiePosture::INSECURE, OriginPolicy::default());
    let production = app();

    let cross_site = |cookie: &str| {
        post(
            "/api/task/t-1/answer",
            &[
                ("host", "tutor.example"),
                ("cookie", cookie),
                ("sec-fetch-site", "cross-site"),
            ],
        )
    };

    assert_eq!(send(&dev, cross_site(DEV_COOKIE)).await.0.as_u16(), 403);
    assert_eq!(send(&dev, cross_site(SECURE_COOKIE)).await.0.as_u16(), 401);
    assert_eq!(
        send(&production, cross_site(SECURE_COOKIE))
            .await
            .0
            .as_u16(),
        403
    );
    assert_eq!(
        send(&production, cross_site(DEV_COOKIE)).await.0.as_u16(),
        401
    );
}

/// (14) The cookie reader finds the session cookie beside other cookies.
///
/// A browser sends one `Cookie` header with every cookie of the origin in it, so
/// a reader that only handles a lone pair refuses nothing on a real deployment.
#[tokio::test]
async fn the_cookie_reader_finds_the_session_cookie_in_a_full_header() {
    let app = app();

    let (status, _body) = send(
        &app,
        post(
            "/api/task/t-1/answer",
            &[
                ("host", "tutor.example"),
                (
                    "cookie",
                    "theme=dark; __Host-cadus_session=s3cr3t; locale=en-US",
                ),
                ("sec-fetch-site", "cross-site"),
            ],
        ),
    )
    .await;

    assert_eq!(status.as_u16(), 403);
}

/// (15) Trap W10: `PUBLIC_ORIGIN` pins the comparison, so no proxy header
/// decides it.
///
/// 1.0 rebuilt its own origin from the request scheme plus `Host`, and the
/// scheme was right only while `--proxy-headers` stayed in the compose command.
/// With `PUBLIC_ORIGIN` set, the `Host` header and `X-Forwarded-Proto` decide
/// nothing: only the pinned string matches.
#[tokio::test]
async fn public_origin_pins_the_comparison() {
    let app = app_with(
        CookiePosture::SECURE,
        OriginPolicy {
            public_origin: Some("https://tutor.example".to_string()),
        },
    );

    // The pinned origin matches, even though `Host` names another name and no
    // `X-Forwarded-Proto` header is present.
    let (status, _body) = send(
        &app,
        post(
            "/api/task/t-1/answer",
            &[
                ("host", "web:8080"),
                ("cookie", SECURE_COOKIE),
                ("origin", "https://tutor.example"),
            ],
        ),
    )
    .await;
    assert_eq!(status.as_u16(), 401);

    // The same name over http:// is another origin, so it is refused.
    let (status, body) = send(
        &app,
        post(
            "/api/task/t-1/answer",
            &[
                ("host", "web:8080"),
                ("cookie", SECURE_COOKIE),
                ("origin", "http://tutor.example"),
            ],
        ),
    )
    .await;
    assert_eq!(status.as_u16(), 403);
    assert_eq!(body, REJECTION_BODY);
}

/// (16) Trap W10, the fallback half: with no `PUBLIC_ORIGIN`, the scheme comes
/// from `X-Forwarded-Proto`.
///
/// This test is the visible form of the 1.0 trap. The FIRST request is what a
/// correctly configured proxy sends, and it goes through. The SECOND is the same
/// browser request after someone drops the proxy's `X-Forwarded-Proto`: the
/// layer rebuilds `http://tutor.example`, the genuine same-origin `Origin` stops
/// matching, and every browser write starts failing with `403`. Nothing in 1.0
/// asserted that; this does, and `PUBLIC_ORIGIN` is the way out.
#[tokio::test]
async fn x_forwarded_proto_decides_the_scheme_when_no_origin_is_pinned() {
    let app = app();

    let (status, _body) = send(
        &app,
        post(
            "/api/task/t-1/answer",
            &[
                ("host", "tutor.example"),
                ("cookie", SECURE_COOKIE),
                ("origin", "https://tutor.example"),
                ("x-forwarded-proto", "https"),
            ],
        ),
    )
    .await;
    assert_eq!(status.as_u16(), 401);

    let (status, _body) = send(
        &app,
        post(
            "/api/task/t-1/answer",
            &[
                ("host", "tutor.example"),
                ("cookie", SECURE_COOKIE),
                ("origin", "https://tutor.example"),
            ],
        ),
    )
    .await;
    assert_eq!(status.as_u16(), 403);
}

/// (17) A proxy chain sends `X-Forwarded-Proto: https, http`. The first value is
/// the scheme the client used.
#[tokio::test]
async fn a_forwarded_proto_list_reads_its_first_value() {
    let app = app();

    let (status, _body) = send(
        &app,
        post(
            "/api/task/t-1/answer",
            &[
                ("host", "tutor.example"),
                ("cookie", SECURE_COOKIE),
                ("origin", "https://tutor.example"),
                ("x-forwarded-proto", "https, http"),
            ],
        ),
    )
    .await;

    assert_eq!(status.as_u16(), 401);
}

/// (18) Trap W11 stated directly: the two origin tests are not each other's
/// negation.
///
/// On a request with no origin signal at all, `is_same_origin` says false and
/// `is_explicit_cross_origin` also says false. A refactor that writes one as
/// `!other` breaks either rule 1 or rule 2, and this is the test that names it.
#[test]
fn the_two_origin_tests_are_not_each_others_negation() {
    use axum::http::HeaderMap;
    use cadus_web::origin::{is_explicit_cross_origin, is_same_origin};

    let policy = OriginPolicy::default();
    let mut headers = HeaderMap::new();
    headers.insert("host", "tutor.example".parse().unwrap());

    assert!(!is_same_origin(&policy, &headers));
    assert!(!is_explicit_cross_origin(&policy, &headers));
}

/// (19) A path outside `/api/` is not rule 1's business.
///
/// Rule 1 covers the API only. `/metrics` is a `GET`, and the SPA routes of M6
/// are not credentialed writes, so a cookie on another path decides nothing.
#[tokio::test]
async fn a_cookie_write_outside_the_api_prefix_is_not_refused_by_rule_one() {
    let app = app();

    let (status, _body) = send(
        &app,
        post(
            "/some/other/path",
            &[
                ("host", "tutor.example"),
                ("cookie", SECURE_COOKIE),
                ("sec-fetch-site", "cross-site"),
            ],
        ),
    )
    .await;

    assert_eq!(status.as_u16(), 404);
}
