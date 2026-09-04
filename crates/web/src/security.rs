//! The security-header layer.
//!
//! Spec `docs/reference/web-service-1.0-spec.md` section 3.1, row "Security
//! headers", and section 11, unit U1: "every section 10 header literal". Each
//! value below is the 1.0 value, character for character
//! (`cadus_web/app.py:65-89`).
//!
//! The layer sits OUTSIDE the CSRF origin layer, so the `403` that layer answers
//! carries the headers too. It sets a header only when the response does not
//! already carry one, so a handler that needs its own `Cache-Control` keeps it.
//!
//! `connect-src` is not redundant beside `default-src 'self'`, and its loopback
//! entries are not decoration. The learner's browser talks to AnkiConnect on the
//! learner's own machine, because the server cannot reach a desktop application
//! on someone else's network. `default-src 'self'` denies that fetch, and the
//! panel then reports "unreachable" for every user. M5 defers the Anki routes
//! (D-M5-5), and the header stays as 1.0 wrote it, because M6 wires the SPA
//! (O3) and a silent drop here is invisible until a learner complains.

use axum::extract::Request;
use axum::http::{HeaderName, HeaderValue};
use axum::middleware::Next;
use axum::response::Response;

/// The Content-Security-Policy value, exactly as 1.0 sends it.
pub const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; img-src 'self' data:; style-src \
                                           'self' 'unsafe-inline'; font-src 'self'; base-uri \
                                           'none'; frame-ancestors 'none'; connect-src 'self' \
                                           http://localhost:* http://127.0.0.1:*";

/// The header name and value of each header the layer stamps.
///
/// `Cache-Control: no-cache` makes the browser revalidate an SPA asset on every
/// load, so a shipped fix reaches the learner instead of a stale cache entry.
pub const SECURITY_HEADERS: [(&str, &str); 5] = [
    ("content-security-policy", CONTENT_SECURITY_POLICY),
    ("x-content-type-options", "nosniff"),
    ("x-frame-options", "DENY"),
    ("referrer-policy", "no-referrer"),
    ("cache-control", "no-cache"),
];

/// Stamp the security headers on every answer.
///
/// Both halves of every pair are lowercase ASCII literals of this module, so
/// [`HeaderName::from_static`] and [`HeaderValue::from_static`] carry the proof
/// that they are valid, and no branch of this layer can drop a header.
pub async fn security_headers_layer(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    for (name, value) in SECURITY_HEADERS {
        let name = HeaderName::from_static(name);
        if !headers.contains_key(&name) {
            headers.insert(name, HeaderValue::from_static(value));
        }
    }
    response
}
