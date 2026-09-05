//! The two `/api/auth/oauth` routes: start and callback.
//!
//! Spec `docs/reference/web-service-1.0-spec.md` section 3.1, rows "OAuth
//! providers" and "OAuth handshake"; section 3.3, "OAuth callback"; section 10,
//! row "OAuth" (1.0 `authn/oauth_routes.py`, `authn/service.py:260-319`).
//!
//! Both routes are `GET`, so the CSRF origin layer never reads them and the
//! provider's top-level callback navigation is never refused. Neither route
//! needs a session.
//!
//! # Unconfigured means absent
//!
//! A provider this deployment does not serve answers `404 not_found` on BOTH
//! routes, so the router is mounted unconditionally and an unconfigured
//! deployment presents no OAuth surface. "Serves" means credentials AND an
//! installed transport ([`crate::auth::oauth::OAuthConfig::enabled`]): a start
//! route that redirects to a provider the callback cannot reach would strand
//! every sign-in at a `500`.
//!
//! # One rejection for two failures
//!
//! [`OAUTH_REJECTED_MESSAGE`] answers a mismatched `state` AND an account that
//! resolved but cannot sign in. The two bodies are byte-identical by
//! construction, so the answer carries no account-state signal.
//!
//! This is not full indistinguishability, and it does not have to be: a
//! mismatched `state` is refused before any provider call, and the disabled
//! branch costs two provider round trips, so the two differ in latency. To reach
//! the second one at all, a caller must control the provider identity, and at
//! that point the caller IS the account holder.
//!
//! # The callback order (section 3.3)
//!
//! Unbound first, bound after, and the guard before every write:
//!
//! 1. unbound `oauth_account_lookup(provider, subject)`, then unbound
//!    `auth_user_by_id` — a returning federated account;
//! 2. otherwise unbound `auth_user_by_email` — an account that owns the
//!    provider-verified address;
//! 3. otherwise the sign-up steps with a NULL `password_hash`.
//! 4. THE guard: a disabled account is refused here, before anything is written.
//! 5. bind the tenant, then, in ONE transaction: when `email_verified_at` is
//!    absent, clear `password_hash`, delete every session of the account, and
//!    stamp `email_verified_at`; insert the `oauth_accounts` row when step 1
//!    found none; and insert the session row.
//!
//! Step 5 runs only for a verified provider email. Step 4 sits above every
//! write, so a refused account is never stamped verified and its link is never
//! re-pointed.
//!
//! # The stamp carries no old credential with it
//!
//! Sign-up writes a password on an address it never proves, and login refuses
//! that password for ONE reason: `email_verified_at IS NULL`. A stamp that keeps
//! the password therefore hands the account to whoever pre-registered the
//! address. The three writes above run in the fixed order clear, delete, stamp,
//! inside the one transaction, so the verified account holds only the
//! credentials the provider just proved.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;
use sqlx::types::chrono::Utc;

use crate::AppState;
use crate::auth::oauth::{
    Credentials, HANDSHAKE_COOKIE, HANDSHAKE_PATH, Handshake, Identity, OAuthFailure, Provider,
    ProviderTransport, authorize_url, callback_redirect_uri, decode_handshake, encode_handshake,
    exchange_and_fetch_identity, safe_next,
};
use crate::auth::rate::client_ip;
use crate::auth::routes::{ClientAddr, cookie_failed, new_session_row, new_token, user_agent};
use crate::auth::session::{
    OAUTH_HANDSHAKE_TTL_SECS, SESSION_IDLE_SECS, clear_auth_cookie, set_auth_cookie,
    set_session_cookie,
};
use crate::auth::token::{EntropyError, hash_token, tokens_equal};
use crate::cookie::CookiePosture;
use crate::cookie::read_session_cookie;
use crate::error::ApiError;
use crate::origin::own_origin;
use crate::path::ApiPath;

mod account;

use account::{resolve_account, write_sign_in};

/// The message of a provider this deployment does not serve.
pub const PROVIDER_NOT_ENABLED_MESSAGE: &str = "This OAuth provider is not enabled.";

/// The one generic sign-in rejection. It answers a mismatched `state` and an
/// account that cannot sign in.
pub const OAUTH_REJECTED_MESSAGE: &str = "OAuth state mismatch; please start sign-in again.";

/// The message of a callback with no usable handshake cookie, `code`, or
/// `state`.
pub const OAUTH_HANDSHAKE_MESSAGE: &str =
    "OAuth handshake is missing or invalid; please try again.";

/// The message of a callback the provider itself marked failed.
pub const OAUTH_CANCELLED_MESSAGE: &str = "OAuth sign-in was cancelled or failed.";

/// The message of a token exchange or identity read that did not finish.
pub const OAUTH_EXCHANGE_MESSAGE: &str = "OAuth sign-in failed; please try again.";

/// The message of an address the provider did not verify.
pub const OAUTH_UNVERIFIED_MESSAGE: &str = "Your provider email address is not verified; verify it with the provider, or sign up with an \
     email address and a password, and try again.";

/// The two routes, plus the list the sign-in page reads.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/oauth/providers", get(providers))
        .route("/api/auth/oauth/{provider}/start", get(start))
        .route("/api/auth/oauth/{provider}/callback", get(callback))
}

/// The query parameters, or an empty map when the query does not parse.
///
/// A query that does not parse carries no `code` and no `state`, so the callback
/// answers its own `400 oauth_error` envelope. Without this the axum rejection
/// would answer a plain-text body and break the section 2 shape.
fn params(
    query: Result<Query<HashMap<String, String>>, QueryRejection>,
) -> HashMap<String, String> {
    query.map(|Query(map)| map).unwrap_or_default()
}

/// One non-empty query value.
fn value<'p>(params: &'p HashMap<String, String>, key: &str) -> Option<&'p str> {
    params
        .get(key)
        .map(String::as_str)
        .filter(|found| !found.is_empty())
}

/// The `500` of a deployment with no redirect base.
fn no_redirect_base() -> ApiError {
    tracing::error!("auth: the OAuth redirect_uri has no base; set OAUTH_REDIRECT_BASE_URL");
    ApiError::internal("oauth redirect base")
}

/// The external origin the `redirect_uri` is built on.
///
/// `OAUTH_REDIRECT_BASE_URL` wins. Without it the origin comes from the same
/// resolver the CSRF layer uses, so the two never disagree about what this
/// deployment is called.
fn redirect_base(state: &AppState, headers: &HeaderMap) -> Result<String, ApiError> {
    state
        .oauth
        .redirect_base
        .clone()
        .or_else(|| own_origin(&state.origin, headers))
        .ok_or_else(no_redirect_base)
}

/// The `500` of a redirect target that is not a header value.
fn bad_redirect(err: axum::http::header::InvalidHeaderValue) -> ApiError {
    tracing::error!(error = %err, "auth: the OAuth redirect target is not a header value");
    ApiError::internal("oauth redirect")
}

/// A `302` to `location` that carries `cookies`.
///
/// The status is 302 and not 303: the provider handshake is what 1.0 serves, and
/// both hops are `GET` either way.
fn redirect(location: &str, cookies: Vec<HeaderValue>) -> Result<Response, ApiError> {
    let target = HeaderValue::from_str(location).map_err(bad_redirect)?;
    let mut response = StatusCode::FOUND.into_response();
    response.headers_mut().insert(header::LOCATION, target);
    for cookie in cookies {
        response.headers_mut().append(header::SET_COOKIE, cookie);
    }
    Ok(response)
}

/// `GET /api/auth/oauth/providers` — the providers this deployment serves.
///
/// The sign-in page reads it to decide which buttons to draw. The answer names
/// no credential.
pub async fn providers(State(state): State<AppState>) -> Response {
    (
        StatusCode::OK,
        Json(json!({ "providers": state.oauth.enabled_names() })),
    )
        .into_response()
}

/// The `500` of an OAuth handshake that did not build.
fn handshake_failed(err: EntropyError) -> ApiError {
    tracing::error!(error = %err, "auth: the OAuth handshake draw failed");
    ApiError::internal("oauth handshake")
}

/// The `Set-Cookie` of the handshake cookie that carries `value`.
///
/// The name and the path are literals of this module and `value` is the
/// base64url text of [`encode_handshake`], so the build cannot fail and the
/// fallback never runs.
fn handshake_cookie(posture: CookiePosture, value: &str) -> HeaderValue {
    set_auth_cookie(
        posture,
        HANDSHAKE_COOKIE,
        value,
        OAUTH_HANDSHAKE_TTL_SECS,
        HANDSHAKE_PATH,
    )
    .unwrap_or(HeaderValue::from_static(""))
}

/// The `Set-Cookie` that ends the handshake cookie. See [`handshake_cookie`]
/// for why the build cannot fail.
fn spent_handshake_cookie(posture: CookiePosture) -> HeaderValue {
    clear_auth_cookie(posture, HANDSHAKE_COOKIE, HANDSHAKE_PATH)
        .unwrap_or(HeaderValue::from_static(""))
}

/// The provider, its credentials, and the transport that reaches it, or the
/// `404` of a provider this deployment does not serve.
fn served_provider(
    state: &AppState,
    name: &str,
) -> Result<(Provider, Credentials, Arc<dyn ProviderTransport>), ApiError> {
    state
        .oauth
        .served(name)
        .map(|(provider, credentials, transport)| (provider, credentials.clone(), transport))
        .ok_or_else(|| ApiError::oauth_not_found(PROVIDER_NOT_ENABLED_MESSAGE))
}

/// `GET /api/auth/oauth/{provider}/start` — mint a handshake and send the
/// browser to the provider.
pub async fn start(
    State(state): State<AppState>,
    ApiPath(name): ApiPath<String>,
    query: Result<Query<HashMap<String, String>>, QueryRejection>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let (provider, credentials, _transport) = served_provider(&state, &name)?;
    let params = params(query);
    let next = safe_next(value(&params, "next"));
    let handshake = Handshake::fresh(provider.name, &next).map_err(handshake_failed)?;

    let base = redirect_base(&state, &headers)?;
    let target = authorize_url(
        provider,
        &credentials,
        &callback_redirect_uri(provider.name, &base),
        &handshake,
    );
    let cookie = handshake_cookie(state.posture, &encode_handshake(&handshake));

    redirect(&target, vec![cookie])
}

/// The handshake the callback spends and the `code` it exchanges.
///
/// The three refusals are `400 oauth_error`: a provider-reported failure, a
/// callback with no handshake cookie, no `code` or no `state`, and a `state`
/// or a provider name that does not match the handshake.
fn spent_handshake(
    provider: Provider,
    params: &HashMap<String, String>,
    headers: &HeaderMap,
) -> Result<(Handshake, String), ApiError> {
    if let Some(reported) = value(params, "error") {
        tracing::info!(
            provider = provider.name,
            error = reported,
            "auth: the OAuth provider reported a failure"
        );
        return Err(ApiError::oauth_error(OAUTH_CANCELLED_MESSAGE));
    }
    // `read_session_cookie` reads ANY cookie by name; the session cookie is one
    // caller of it and this is another. One reader keeps the multi-header walk
    // of HTTP/2 in one place.
    let handshake = decode_handshake(read_session_cookie(headers, HANDSHAKE_COOKIE));
    let (Some(handshake), Some(code), Some(returned)) =
        (handshake, value(params, "code"), value(params, "state"))
    else {
        return Err(ApiError::oauth_error(OAUTH_HANDSHAKE_MESSAGE));
    };
    // The `state` compare is constant time, and the provider name is compared
    // too: a handshake minted for Google never completes a GitHub callback.
    if handshake.provider != provider.name || !tokens_equal(&handshake.state, returned) {
        return Err(ApiError::oauth_error(OAUTH_REJECTED_MESSAGE));
    }
    Ok((handshake, code.to_owned()))
}

/// The `400 oauth_error` of an exchange the provider refused.
fn exchange_failed(err: &OAuthFailure, provider: Provider) -> ApiError {
    tracing::warn!(provider = provider.name, error = %err.reason, "auth: the OAuth exchange failed");
    ApiError::oauth_error(OAUTH_EXCHANGE_MESSAGE)
}

/// The provider-verified identity behind `code`, or the `400` of an exchange
/// that failed or an address the provider did not verify.
///
/// Link only on a provider-verified address. Without the test, a provider that
/// lets an account claim any address takes over every account of this service
/// that shares one.
async fn verified_identity(
    transport: &dyn ProviderTransport,
    provider: Provider,
    credentials: &Credentials,
    base: &str,
    code: &str,
    handshake: &Handshake,
) -> Result<Identity, ApiError> {
    let identity = exchange_and_fetch_identity(
        transport,
        provider,
        credentials,
        &callback_redirect_uri(provider.name, base),
        code,
        handshake,
    )
    .await
    .map_err(|err| exchange_failed(&err, provider))?;
    if identity.email.is_empty() || !identity.email_verified {
        return Err(ApiError::oauth_error(OAUTH_UNVERIFIED_MESSAGE));
    }
    Ok(identity)
}

/// `GET /api/auth/oauth/{provider}/callback` — spend the handshake, read the
/// provider-verified identity, open a session, and send the browser back.
pub async fn callback(
    State(state): State<AppState>,
    ApiPath(name): ApiPath<String>,
    ClientAddr(peer): ClientAddr,
    query: Result<Query<HashMap<String, String>>, QueryRejection>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let params = params(query);
    let (provider, credentials, transport) = served_provider(&state, &name)?;
    let (handshake, code) = spent_handshake(provider, &params, &headers)?;
    let base = redirect_base(&state, &headers)?;
    let identity = verified_identity(
        transport.as_ref(),
        provider,
        &credentials,
        &base,
        &code,
        &handshake,
    )
    .await?;

    let (user, linked) = resolve_account(&state.db, &identity).await?;
    // THE guard. Nothing above it wrote more than a fresh, enabled account, and
    // nothing below it runs for a refused one.
    if user.disabled_at.is_some() {
        return Err(ApiError::oauth_error(OAUTH_REJECTED_MESSAGE));
    }

    let raw = new_token()?;
    let now = Utc::now();
    let expires_at = now + Duration::from_secs(SESSION_IDLE_SECS);
    let token_hash = hash_token(&raw);
    let ip = client_ip(&headers, peer);
    let session = new_session_row(&token_hash, now, expires_at, &ip, user_agent(&headers));
    write_sign_in(&state.db, &user, linked, &identity, &session).await?;

    let session_cookie = set_session_cookie(state.posture, &raw).map_err(cookie_failed)?;
    let spent = spent_handshake_cookie(state.posture);
    // `safe_next` runs again HERE, on the value read from the cookie. That
    // second pass is what lets the handshake cookie stay unsigned; do not
    // demote it to a write-time check.
    redirect(
        &safe_next(Some(&handshake.next_url)),
        vec![session_cookie, spent],
    )
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderValue, StatusCode};

    use super::{bad_redirect, handshake_failed, no_redirect_base, redirect};
    use crate::auth::token::EntropyError;

    /// Every OAuth `500` mapper answers an internal error.
    #[test]
    fn the_oauth_mappers_answer_internal_errors() {
        let bad =
            HeaderValue::from_bytes(b"bad\nvalue").expect_err("a newline is not a header value");
        let mappers = [
            no_redirect_base(),
            bad_redirect(bad),
            handshake_failed(EntropyError {
                reason: "no pool".to_string(),
            }),
        ];
        for err in mappers {
            assert_eq!(err.status, StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    /// A redirect target that is not a header value is the `500` of the
    /// mapper, and a valid one is a `302` that carries the cookies it was
    /// given.
    #[test]
    fn a_redirect_target_that_is_not_a_header_value_is_500() {
        let err = redirect("bad\nlocation", Vec::new()).expect_err("a newline is not a target");
        assert_eq!(err.status, StatusCode::INTERNAL_SERVER_ERROR);
        let response = redirect("/next", vec![HeaderValue::from_static("a=b")]).expect("a target");
        assert_eq!(response.status(), StatusCode::FOUND);
    }
}
