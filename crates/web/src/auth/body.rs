//! The request-body extractor of the `/api/auth/*` routes.
//!
//! Spec `docs/reference/web-service-1.0-spec.md` section 2 pins one shape for
//! every 4xx and 5xx answer:
//!
//! ```json
//! {"error": {"code": "payload_too_large", "message": "..."}}
//! ```
//!
//! The axum `Bytes` extractor has its own rejection, and that rejection answers
//! with a plain-text sentence. A handler that takes `Bytes` therefore breaks the
//! envelope on two inputs that never reach the handler body: a body over the
//! buffer limit, and a body that fails mid-read. [`LimitedBody`] is the same
//! extractor with the rejection mapped onto [`ApiError`], so both inputs answer
//! JSON like every other refusal.
//!
//! The two answers:
//!
//! - `413 payload_too_large` — the body passed the limit that the server
//!   buffers. axum applies a 2 MiB default, and a `DefaultBodyLimit` layer
//!   moves it;
//! - `422 invalid_request` — the body ended early, or the transport failed. The
//!   code is the one the routes already give for a body they cannot read.
//!
//! The extractor holds no policy of its own. It buffers what `Bytes` buffers,
//! and the handler keeps every field rule it had.

use axum::body::Bytes;
use axum::extract::FromRequest;
use axum::extract::Request;
use axum::extract::rejection::BytesRejection;
use axum::http::StatusCode;

use crate::error::ApiError;

/// The whole request body, with the axum rejection mapped to the envelope.
///
/// A handler takes it in the place of `Bytes`:
///
/// ```ignore
/// pub async fn signup(LimitedBody(body): LimitedBody) -> Result<Response, ApiError>
/// ```
#[derive(Debug, Clone)]
pub struct LimitedBody(pub Bytes);

impl<S> FromRequest<S> for LimitedBody
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Bytes::from_request(request, state).await {
            Ok(bytes) => Ok(Self(bytes)),
            Err(rejection) => Err(map_rejection(&rejection)),
        }
    }
}

/// Map one `Bytes` rejection onto the section 2 envelope.
///
/// The rejection is a non-exhaustive enum of a non-exhaustive enum, and a match
/// on the inner variants cannot name their private fields. The status that axum
/// gives is the stable half of that type's contract, so the mapping reads it:
/// `413` for the length limit, and one `422 invalid_request` for every other
/// read failure.
fn map_rejection(rejection: &BytesRejection) -> ApiError {
    if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
        ApiError::payload_too_large()
    } else {
        ApiError::invalid_request("The server could not read the request body.")
    }
}
