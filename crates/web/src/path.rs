//! The path-parameter extractor of every route that reads a path segment.
//!
//! Spec `docs/reference/web-service-1.0-spec.md` section 2 pins one shape for
//! every 4xx and 5xx answer:
//!
//! ```json
//! {"error": {"code": "invalid_request", "message": "..."}}
//! ```
//!
//! The axum `Path` extractor has its own rejection, and that rejection answers
//! with a plain-text sentence. A path segment that holds an invalid UTF-8
//! percent escape, such as `/api/auth/oauth/%ff/start`, therefore leaves the
//! service as `400 text/plain` with no envelope. [`ApiPath`] is the same
//! extractor with the rejection mapped onto [`ApiError`], so that input answers
//! JSON like every other refusal.
//!
//! [`crate::auth::body::LimitedBody`] does the same job for the request body.
//! This module is its path half, and it reads the rejection the same way: the
//! rejection type is non-exhaustive and its fields are private, so the mapping
//! branches on the STATUS, which is the stable half of that type's contract.
//!
//! The two answers:
//!
//! - `422 invalid_request` — the segment does not read as the type the handler
//!   asks for. A bad percent escape and a segment that is not a number both
//!   land here;
//! - `500 internal_error` — the route declares fewer path parameters than the
//!   handler reads. It is a fault of this service, never of the caller, and no
//!   route of `cadus_web` reaches it.
//!
//! The extractor holds no policy of its own. The handler keeps every rule it
//! had about the VALUE of the segment: an unknown OAuth provider is still
//! `404 not_found`, and an id that is not a UUID is still the handler's own
//! answer.

use axum::extract::rejection::PathRejection;
use axum::extract::{FromRequestParts, Path};
use axum::http::request::Parts;
use serde::de::DeserializeOwned;

use crate::error::ApiError;

/// One path parameter, with the axum rejection mapped to the envelope.
///
/// A handler takes it in the place of `Path`:
///
/// ```ignore
/// pub async fn poll(ApiPath(id): ApiPath<String>) -> Result<Json<Value>, ApiError>
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApiPath<T>(pub T);

impl<T, S> FromRequestParts<S> for ApiPath<T>
where
    T: DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match Path::<T>::from_request_parts(parts, state).await {
            Ok(Path(value)) => Ok(Self(value)),
            Err(rejection) => Err(map_rejection(&rejection)),
        }
    }
}

/// Map one `Path` rejection onto the section 2 envelope.
///
/// The message names no part of the request. A caller that sends a bad path
/// learns that the path is bad and nothing else.
fn map_rejection(rejection: &PathRejection) -> ApiError {
    if rejection.status().is_server_error() {
        ApiError::internal("path parameters")
    } else {
        ApiError::invalid_request("The request path is not valid for this route.")
    }
}
