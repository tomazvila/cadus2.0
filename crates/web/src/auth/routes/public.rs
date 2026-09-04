//! The two extractors of the public auth routes: the peer address, and the
//! whole public request with its JSON body.

use std::net::SocketAddr;

use axum::extract::{ConnectInfo, FromRequest, FromRequestParts, Request};
use axum::http::HeaderMap;
use axum::http::request::Parts;
use cadus_store::auth::{AuthUser, NewSession, user_by_email};
use serde_json::Value;
use sqlx::types::chrono::{DateTime, Utc};

use super::support::{email_field, field, new_session_row, object, session_window, user_agent};
use crate::AppState;
use crate::auth::body::LimitedBody;
use crate::auth::guard::plus_secs;
use crate::auth::rate::{RateRule, client_ip, enforce};
use crate::auth::session::SESSION_IDLE_SECS;
use crate::auth::store_call;
use crate::error::ApiError;

/// The peer address of one request, when the server wired one.
fn peer_of(parts: &Parts) -> Option<SocketAddr> {
    parts
        .extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0)
}

/// The peer address of the request, when the server wired one.
///
/// `axum::serve` carries it in `ConnectInfo` only when the binary calls
/// `into_make_service_with_connect_info`. A test that drives the router in
/// process carries none, so the extractor answers `None` instead of a rejection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientAddr(pub Option<SocketAddr>);

impl<S: Send + Sync> FromRequestParts<S> for ClientAddr {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(Self(peer_of(parts)))
    }
}

/// A public auth request with a JSON object body: the state, the client
/// address, the headers, and the parsed body.
///
/// The body refusals keep their order: `413` past the body cap, then
/// `422 invalid_request` for a body that is not a JSON object.
pub struct Public {
    /// The shared state of the process.
    pub state: AppState,
    /// The client address the rate rules key on.
    pub ip: String,
    /// The request headers.
    pub headers: HeaderMap,
    /// The body, as a JSON object.
    pub value: Value,
}

impl FromRequest<AppState> for Public {
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &AppState) -> Result<Self, ApiError> {
        let (parts, body) = request.into_parts();
        let ip = client_ip(&parts.headers, peer_of(&parts));
        let headers = parts.headers.clone();
        let request = Request::from_parts(parts, body);
        let LimitedBody(bytes) = LimitedBody::from_request(request, state).await?;
        let value = object(&bytes)?;
        Ok(Self {
            state: state.clone(),
            ip,
            headers,
            value,
        })
    }
}

impl Public {
    /// One string field of the body, or `422 invalid_request`.
    pub(super) fn field(&self, name: &str) -> Result<&str, ApiError> {
        field(&self.value, name)
    }

    /// The normalized `email` field of the body. See [`email_field`].
    pub(super) fn email(&self) -> Result<String, ApiError> {
        email_field(&self.value)
    }

    /// Apply `rule` to the pair of this address and this client.
    pub(super) async fn enforce(
        &self,
        rule: RateRule,
        email: &str,
        now: DateTime<Utc>,
    ) -> Result<(), ApiError> {
        enforce(&self.state.db, rule, email, &self.ip, now).await
    }

    /// Apply `rule`, then look the account of `email` up.
    pub(super) async fn rate_limited_lookup(
        &self,
        rule: RateRule,
        email: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<AuthUser>, ApiError> {
        self.enforce(rule, email, now).await?;
        store_call(
            &self.state.db,
            "account lookup",
            user_by_email(self.state.db.pool(), email),
        )
        .await
    }

    /// The session row of a sign-in at `now`, with the token digest `token_hash`.
    pub(super) fn session_row<'a>(
        &'a self,
        token_hash: &'a str,
        now: DateTime<Utc>,
    ) -> Result<NewSession<'a>, ApiError> {
        let expires_at = plus_secs(now, SESSION_IDLE_SECS).ok_or_else(session_window)?;
        Ok(new_session_row(
            token_hash,
            now,
            expires_at,
            &self.ip,
            user_agent(&self.headers),
        ))
    }
}
