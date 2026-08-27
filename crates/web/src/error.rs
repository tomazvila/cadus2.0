//! The error envelope of every 4xx and 5xx answer.
//!
//! Spec `docs/reference/web-service-1.0-spec.md` section 2 pins one shape for
//! every error the service returns:
//!
//! ```json
//! {"error": {"code": "cross_origin_rejected", "message": "..."}}
//! ```
//!
//! `code` is the machine half. A client branches on it, so it belongs to the
//! contract and each value is a literal in a test. `message` is the human half.
//!
//! Every error answer in `cadus_web` goes through [`ApiError`]. axum's own
//! rejections carry a plain-text body, so a route that parses a body wraps the
//! rejection and returns an `ApiError` instead. The router also sets both
//! fallbacks ([`not_found`] and [`method_not_allowed`]), because the axum
//! defaults answer with an empty body and no envelope.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

/// The code of an unmatched path. Spec section 10 pins it on the OAuth routes.
pub const NOT_FOUND: &str = "not_found";

/// The code of a known path with a method the route does not serve.
pub const METHOD_NOT_ALLOWED: &str = "method_not_allowed";

/// The code of the CSRF origin layer. Spec section 10, row "CSRF".
pub const CROSS_ORIGIN_REJECTED: &str = "cross_origin_rejected";

/// One error answer: a status code, a machine code, and a human message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    /// The HTTP status of the answer.
    pub status: StatusCode,
    /// The machine-readable code inside the envelope.
    pub code: &'static str,
    /// The human-readable message inside the envelope.
    pub message: String,
}

impl ApiError {
    /// Build an error answer from its three parts.
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }

    /// `404 not_found` — no route matches this path.
    pub fn not_found() -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            NOT_FOUND,
            "This path serves nothing.",
        )
    }

    /// `405 method_not_allowed` — the path exists, the method does not.
    pub fn method_not_allowed() -> Self {
        Self::new(
            StatusCode::METHOD_NOT_ALLOWED,
            METHOD_NOT_ALLOWED,
            "This path does not serve that method.",
        )
    }

    /// `403 cross_origin_rejected` — the CSRF origin layer refused the write.
    pub fn cross_origin_rejected() -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            CROSS_ORIGIN_REJECTED,
            "The server refuses a cross-origin write that carries a session cookie. Send the \
             request same-origin, or send a bearer token.",
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({ "error": { "code": self.code, "message": self.message } })),
        )
            .into_response()
    }
}

/// The router fallback for a path that matches no route.
pub async fn not_found() -> ApiError {
    ApiError::not_found()
}

/// The router fallback for a method that the matched route does not serve.
pub async fn method_not_allowed() -> ApiError {
    ApiError::method_not_allowed()
}
