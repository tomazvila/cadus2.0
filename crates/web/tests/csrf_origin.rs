//! Part of `tests/csrf.rs`: the header of that file gives the
//! requirements and the rules.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use common::csrf::*;

use axum::body::Body;
use axum::http::Request;
use cadus_web::cookie::CookiePosture;
use cadus_web::origin::OriginPolicy;
use tower::ServiceExt;

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
