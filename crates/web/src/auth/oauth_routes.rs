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

use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use cadus_store::Db;
use cadus_store::auth::{
    AuthUser, NewSession, SignUp, clear_password_hash, delete_all_sessions, insert_oauth_account,
    insert_session, mark_email_verified, oauth_account_user, sign_up, user_by_email, user_by_id,
};
use serde_json::json;
use sqlx::types::chrono::Utc;

use crate::AppState;
use crate::auth::guard::plus_secs;
use crate::auth::oauth::{
    Credentials, HANDSHAKE_COOKIE, HANDSHAKE_PATH, Handshake, Identity, OAuthFailure, Provider,
    authorize_url, callback_redirect_uri, decode_handshake, encode_handshake,
    exchange_and_fetch_identity, safe_next,
};
use crate::auth::rate::client_ip;
use crate::auth::routes::{
    ClientAddr, bind, commit, cookie_failed, new_session_row, new_token, session_window, user_agent,
};
use crate::auth::session::{
    CookieWriteError, OAUTH_HANDSHAKE_TTL_SECS, SESSION_IDLE_SECS, clear_auth_cookie,
    set_auth_cookie, set_session_cookie,
};
use crate::auth::store_call;
use crate::auth::token::{EntropyError, hash_token, tokens_equal};
use crate::cookie::read_session_cookie;
use crate::error::ApiError;
use crate::origin::own_origin;
use crate::path::ApiPath;

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

/// The `500` of a handshake cookie that did not build.
fn handshake_cookie_failed(err: CookieWriteError) -> ApiError {
    tracing::error!(error = %err, "auth: the OAuth handshake cookie did not build");
    ApiError::internal("oauth handshake cookie")
}

/// The provider and its credentials, or the `404` of one this deployment does
/// not serve.
fn served_provider(state: &AppState, name: &str) -> Result<(Provider, Credentials), ApiError> {
    state
        .oauth
        .enabled(name)
        .map(|(provider, credentials)| (provider, credentials.clone()))
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
    let (provider, credentials) = served_provider(&state, &name)?;
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
    let cookie = set_auth_cookie(
        state.posture,
        HANDSHAKE_COOKIE,
        &encode_handshake(&handshake),
        OAUTH_HANDSHAKE_TTL_SECS,
        HANDSHAKE_PATH,
    )
    .map_err(handshake_cookie_failed)?;

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
    state: &AppState,
    provider: Provider,
    credentials: &Credentials,
    base: &str,
    code: &str,
    handshake: &Handshake,
) -> Result<Identity, ApiError> {
    let Some(transport) = state.oauth.transport.as_deref() else {
        return Err(ApiError::oauth_not_found(PROVIDER_NOT_ENABLED_MESSAGE));
    };
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

/// The writes of one federated sign-in, in ONE transaction: the first-sign-in
/// stamp, the link row, and the session row.
///
/// The stamp removes the ONE reason login refuses a password on an unverified
/// account, so every credential that predates the stamp goes first. Sign-up
/// asks for no proof of the address, so the password and the sessions of an
/// unverified account have an unproven source. The order is fixed — clear,
/// delete, then stamp — and the three writes share this transaction, so no
/// window opens in which the address is verified and the old password still
/// signs in. The session row of THIS sign-in goes in last, after the deletion.
async fn write_sign_in(
    db: &Db,
    user: &AuthUser,
    linked: bool,
    identity: &Identity,
    session: &NewSession<'_>,
) -> Result<(), ApiError> {
    let mut tx = bind(db, user.id).await?;
    if user.email_verified_at.is_none() {
        store_call(db, "password clear", clear_password_hash(&mut *tx, user.id)).await?;
        store_call(db, "session sweep", delete_all_sessions(&mut *tx)).await?;
        // The provider just proved the address, which is what the verification
        // link proves.
        store_call(
            db,
            "verification stamp",
            mark_email_verified(&mut *tx, user.id),
        )
        .await?;
    }
    if !linked {
        store_call(
            db,
            "oauth link",
            insert_oauth_account(
                &mut *tx,
                user.id,
                &identity.provider,
                &identity.subject,
                &identity.email,
            ),
        )
        .await?;
    }
    store_call(
        db,
        "session insert",
        insert_session(&mut *tx, user.id, session),
    )
    .await?;
    commit(db, tx, "session insert").await
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
    let (provider, credentials) = served_provider(&state, &name)?;
    let (handshake, code) = spent_handshake(provider, &params, &headers)?;
    let base = redirect_base(&state, &headers)?;
    let identity =
        verified_identity(&state, provider, &credentials, &base, &code, &handshake).await?;

    let (user, linked) = resolve_account(&state.db, &identity).await?;
    // THE guard. Nothing above it wrote more than a fresh, enabled account, and
    // nothing below it runs for a refused one.
    if user.disabled_at.is_some() {
        return Err(ApiError::oauth_error(OAUTH_REJECTED_MESSAGE));
    }

    let raw = new_token()?;
    let now = Utc::now();
    let expires_at = plus_secs(now, SESSION_IDLE_SECS).ok_or_else(session_window)?;
    let token_hash = hash_token(&raw);
    let ip = client_ip(&headers, peer);
    let session = new_session_row(&token_hash, now, expires_at, &ip, user_agent(&headers));
    write_sign_in(&state.db, &user, linked, &identity, &session).await?;

    let session_cookie = set_session_cookie(state.posture, &raw).map_err(cookie_failed)?;
    let spent = clear_auth_cookie(state.posture, HANDSHAKE_COOKIE, HANDSHAKE_PATH)
        .map_err(handshake_cookie_failed)?;
    // `safe_next` runs again HERE, on the value read from the cookie. That
    // second pass is what lets the handshake cookie stay unsigned; do not
    // demote it to a write-time check.
    redirect(
        &safe_next(Some(&handshake.next_url)),
        vec![session_cookie, spent],
    )
}

/// Resolve the identity to an account, in the section 3.3 order.
///
/// The answer is the account and whether step 1 already found the
/// `oauth_accounts` row. A `true` there means the caller writes no link row: the
/// table's primary key is `(provider, provider_account_id)`, so a second insert
/// of a live link is a duplicate-key error, and re-pointing a live link at
/// another account is exactly the takeover this order prevents.
///
/// The guard on `disabled_at` belongs to the CALLER, not here. A resolution
/// branch that also tests usability is a match condition, and a non-match then
/// falls through into the next branch instead of refusing.
async fn resolve_account(db: &Db, identity: &Identity) -> Result<(AuthUser, bool), ApiError> {
    let linked = store_call(
        db,
        "oauth lookup",
        oauth_account_user(db.pool(), &identity.provider, &identity.subject),
    )
    .await?;
    if let Some(user_id) = linked {
        // A dangling link — the account row is gone — resolves to nothing, and
        // the walk goes on. That is the ONLY reason step 1 continues.
        if let Some(user) = store_call(db, "account lookup", user_by_id(db.pool(), user_id)).await?
        {
            return Ok((user, true));
        }
    }

    if let Some(user) = store_call(
        db,
        "account lookup",
        user_by_email(db.pool(), &identity.email),
    )
    .await?
    {
        return Ok((user, false));
    }

    // A brand-new federated account: no password hash, so a password login
    // against it is refused by the section 3.3 login order.
    match store_call(db, "sign-up", sign_up(db.pool(), &identity.email, None)).await? {
        SignUp::Created(user) => Ok((user, false)),
        // Another request created the same address between the read above and
        // this insert. Read it back and link into it instead of failing.
        SignUp::EmailTaken => store_call(
            db,
            "account lookup",
            user_by_email(db.pool(), &identity.email),
        )
        .await?
        .map(|user| (user, false))
        .ok_or_else(|| {
            tracing::error!("auth: the OAuth sign-up raced and the address then read back empty");
            ApiError::internal("sign-up")
        }),
    }
}
