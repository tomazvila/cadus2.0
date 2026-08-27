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

/// The code of a body that is not the JSON object the route reads. Spec section
/// 10, row "Error envelope".
pub const INVALID_REQUEST: &str = "invalid_request";

/// The code of a request with no usable session (spec section 3.3, "Cookie
/// check"). It never says WHICH of the refusals fired.
pub const UNAUTHORIZED: &str = "unauthorized";

/// The code of every login refusal and of a wrong current password (spec
/// section 10, row "Weak password / wrong current password").
pub const INVALID_CREDENTIALS: &str = "invalid_credentials";

/// The code of a new password that the section 3.1 policy refuses.
pub const WEAK_PASSWORD: &str = "weak_password";

/// The code of a reset or verification token that cannot be spent (spec section
/// 10, row "Bad / reused token").
pub const INVALID_TOKEN: &str = "invalid_token";

/// The code of a refused rate rule (spec section 3.2).
pub const RATE_LIMITED: &str = "rate_limited";

/// The code of a fault inside the service. The message names no account, no
/// address, and no token.
pub const INTERNAL_ERROR: &str = "internal_error";

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

    /// `422 invalid_request` — the body is not the JSON object the route reads.
    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            INVALID_REQUEST,
            message.into(),
        )
    }

    /// `401 unauthorized` — the request carries no usable session.
    ///
    /// The message stays generic on purpose. A caller must not learn whether the
    /// session is absent, expired, or attached to a disabled account.
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, UNAUTHORIZED, message.into())
    }

    /// `401 invalid_credentials` — one uniform refusal for every login failure.
    pub fn invalid_credentials(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            INVALID_CREDENTIALS,
            message.into(),
        )
    }

    /// `422 weak_password` — the new password breaks the length policy.
    pub fn weak_password(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            WEAK_PASSWORD,
            message.into(),
        )
    }

    /// `400 invalid_token` — the token is unknown, spent, stale, or of another
    /// purpose. One code covers all four, so nothing tells the four apart.
    pub fn invalid_token(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, INVALID_TOKEN, message.into())
    }

    /// `429 rate_limited` — this call passed the ceiling of its rate rule.
    pub fn rate_limited() -> Self {
        Self::new(
            StatusCode::TOO_MANY_REQUESTS,
            RATE_LIMITED,
            "Too many requests; please wait a bit and try again.",
        )
    }

    /// `500 internal_error` — the service failed, and the caller did nothing
    /// wrong.
    ///
    /// The argument names the failing STEP, never a value of it. A store error
    /// text can carry a token hash or an address, so it goes to the log and
    /// never into the body.
    pub fn internal(step: &'static str) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            INTERNAL_ERROR,
            format!("The service could not finish this request ({step})."),
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
