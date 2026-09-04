//! Proof tests for the rest of M5 U1: the error envelope, the section 3.1
//! security headers, the request-metrics layer, `/metrics`, and the D-M5-6
//! fields of `/api/ready`.
//!
//! Spec `docs/reference/web-service-1.0-spec.md` section 2 (the envelope),
//! section 3.1 (row "Security headers"), section 10 (row "Ready"), section 11
//! (unit U1: "an unmatched path is one `__unmatched__` metric label"), and
//! ruling D-M5-6 of `docs/plans/M5.md`.
//!
//! Every header value below is a literal of this file, copied from the 1.0
//! source (`cadus_web/app.py:65-89`), never read back from the constant under
//! test. Every metric line is a literal too.
//!
//! Each test builds its own application, so each one gets its own metrics
//! registry and no test can read another test's counts.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use common::skeleton::*;

use axum::body::Body;
use axum::http::Request;

// ---------------------------------------------------------------------------
// The security headers (spec section 3.1)
// ---------------------------------------------------------------------------

/// (1) Every section 3.1 header literal is on a `200`.
#[tokio::test]
async fn every_security_header_literal_is_on_a_200() {
    let app = offline_app();

    let (status, headers, _body) = send(&app, get("/api/health")).await;

    assert_eq!(status.as_u16(), 200);
    for (name, value) in SECURITY_HEADERS {
        assert_eq!(header_of(&headers, name), value, "header {name}");
    }
}

/// (2) The `403` of the CSRF layer carries the headers too.
///
/// The security-header layer sits OUTSIDE the CSRF layer for exactly this
/// reason. A refusal is still an answer a browser renders.
#[tokio::test]
async fn the_csrf_refusal_carries_the_security_headers() {
    let app = offline_app();

    let request = Request::builder()
        .method("POST")
        .uri("/api/task/t-1/answer")
        .header("host", "tutor.example")
        .header("cookie", "__Host-cadus_session=s3cr3t")
        .header("sec-fetch-site", "cross-site")
        .body(Body::empty())
        .unwrap();
    let (status, headers, _body) = send(&app, request).await;

    assert_eq!(status.as_u16(), 403);
    for (name, value) in SECURITY_HEADERS {
        assert_eq!(header_of(&headers, name), value, "header {name}");
    }
}

/// (3) The `404` fallback and the `405` fallback carry them as well.
#[tokio::test]
async fn both_fallbacks_carry_the_security_headers() {
    let app = offline_app();

    let (not_found, headers, _body) = send(&app, get("/no-such-path")).await;
    assert_eq!(not_found.as_u16(), 404);
    for (name, value) in SECURITY_HEADERS {
        assert_eq!(header_of(&headers, name), value, "404, header {name}");
    }

    let request = Request::builder()
        .method("POST")
        .uri("/api/health")
        .body(Body::empty())
        .unwrap();
    let (not_allowed, headers, _body) = send(&app, request).await;
    assert_eq!(not_allowed.as_u16(), 405);
    for (name, value) in SECURITY_HEADERS {
        assert_eq!(header_of(&headers, name), value, "405, header {name}");
    }
}

/// (4) The layer never overwrites a header the handler set.
///
/// `/api/ready` reports a live reading, so it sets `Cache-Control: no-store`,
/// which is stronger than the layer's `no-cache`. A layer that overwrote it
/// would let a proxy hold a stale readiness answer.
#[tokio::test]
async fn a_handler_keeps_its_own_cache_control() {
    let app = offline_app();

    let (_status, headers, _body) = send(&app, get("/api/ready")).await;

    assert_eq!(header_of(&headers, "cache-control"), "no-store");
    assert_eq!(header_of(&headers, "x-frame-options"), "DENY");
}

// ---------------------------------------------------------------------------
// The error envelope (spec section 2)
// ---------------------------------------------------------------------------

/// (5) An unmatched path answers `404 not_found` inside the envelope.
#[tokio::test]
async fn an_unmatched_path_is_404_not_found_in_the_envelope() {
    let app = offline_app();

    let (status, headers, body) = send(&app, get("/no-such-path")).await;

    assert_eq!(status.as_u16(), 404);
    assert_eq!(header_of(&headers, "content-type"), "application/json");
    assert_eq!(
        body,
        r#"{"error":{"code":"not_found","message":"This path serves nothing."}}"#
    );
}

/// (6) A known path with an unserved method answers `405 method_not_allowed`
/// inside the envelope.
///
/// axum's own fallback answers `405` with an EMPTY body, so a client that
/// branches on `error.code` gets nothing to read. This is the one place that
/// fixes it.
#[tokio::test]
async fn a_wrong_method_is_405_method_not_allowed_in_the_envelope() {
    let app = offline_app();

    let request = Request::builder()
        .method("POST")
        .uri("/api/health")
        .body(Body::empty())
        .unwrap();
    let (status, headers, body) = send(&app, request).await;

    assert_eq!(status.as_u16(), 405);
    assert_eq!(header_of(&headers, "content-type"), "application/json");
    assert_eq!(
        body,
        r#"{"error":{"code":"method_not_allowed","message":"This path does not serve that method."}}"#
    );
}
