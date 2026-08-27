//! The ten `/api/auth/*` routes.
//!
//! Spec `docs/reference/web-service-1.0-spec.md` sections 2, 3.1, 3.2, and 3.3,
//! and every auth row of section 10 (1.0 `cadus_web/authn/routes.py`,
//! `authn/service.py`).
//!
//! The routes are a thin shell: parse the body, apply the rate rule, run the
//! section 3.3 call order through `cadus_store::auth`, and render the answer.
//! No handler here calls a model and no handler here panics on a body (R4).
//!
//! # The three rules that shape every handler
//!
//! **Nothing tells a registered address from an unregistered one.** Sign-up
//! answers `{"status":"verification_required"}` for both, and never
//! `409 email_taken`. Forgot-password and verify-resend answer `{"ok":true}` for
//! both. A login refusal is one `401 invalid_credentials` for an unknown
//! address, a wrong password, an unverified address, a disabled account, and an
//! OAuth-only account. An unknown address still spends one real Argon2 verify
//! against [`DUMMY_PASSWORD`], so the answer time of the unknown path stays
//! inside 3x of the known path.
//!
//! One timing residual stays, and 1.0 records the same one
//! (`authn/routes.py:312-317`): a NEW address writes the account row and mints
//! the verification link, and a registered address writes nothing. Every
//! account-state write of sign-up, forgot, and resend sits BEHIND the Argon2
//! cost, which is 3 passes over 65536 KiB in the `prod` profile, so the
//! difference is a few milliseconds against tens of them. It is not a practical
//! oracle, and it is not zero.
//!
//! **Unbound first, bound after.** The pre-tenant lookups run on the pool. Every
//! write runs inside a `begin_tenant` transaction, and the `WITH CHECK` clause
//! of the tenant policy is what refuses a forged `user_id` (C3).
//!
//! **The raw token leaves the process once.** A session token reaches the client
//! in the cookie, and in the body only when the caller sends
//! `Accept-Session-Token: true` (D-M5-5). The database holds the SHA-256 digest
//! alone.
//!
//! # What M5 does not carry
//!
//! The reset and verification tokens land in `auth_tokens`. M5 has no unit that
//! delivers mail, so nothing sends the link yet; `email_outbox` and its drain
//! are the place that work belongs.

use std::net::SocketAddr;

use axum::body::Bytes;
use axum::extract::{ConnectInfo, FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use cadus_store::auth::{
    AccountProfile, NewSession, PURPOSE_RESET, PURPOSE_VERIFY, SignUp, TokenConsumed, TokenEffect,
    account_profile, consume_token_tx, delete_all_sessions, delete_other_sessions, delete_session,
    delete_tokens_for_purpose, insert_session, insert_token, mark_email_verified,
    session_by_token_hash, set_password_hash, sign_up, token_by_hash, user_by_email, user_by_id,
};
use cadus_store::{Db, begin_tenant};
use serde_json::{Value, json};
use sqlx::types::Uuid;
use sqlx::types::chrono::{DateTime, Utc};
use sqlx::{Postgres, Transaction};

use crate::AppState;
use crate::auth::email::normalize_email;
use crate::auth::guard::{Authed, current_user, plus_secs};
use crate::auth::password::{
    Argon2Profile, hash_password, needs_rehash, validate_password, verify_password,
};
use crate::auth::rate::{RateRule, client_ip, enforce};
use crate::auth::session::{
    RESET_TOKEN_TTL_SECS, SESSION_IDLE_SECS, VERIFY_TOKEN_TTL_SECS, clear_session_cookie,
    set_session_cookie,
};
use crate::auth::store_call;
use crate::auth::token::{generate_token, hash_token};
use crate::error::ApiError;

/// The header that asks for the raw session token in the body (D-M5-5).
pub const ACCEPT_SESSION_TOKEN: &str = "accept-session-token";

/// The `status` of every sign-up answer, new address or not.
pub const VERIFICATION_REQUIRED: &str = "verification_required";

/// The message beside [`VERIFICATION_REQUIRED`].
pub const VERIFICATION_REQUIRED_MESSAGE: &str =
    "This address needs a verified email address before it can sign in.";

/// The refusal message of every login failure.
pub const INVALID_LOGIN_MESSAGE: &str = "Invalid email or password.";

/// The refusal message of a wrong current password.
pub const WRONG_CURRENT_PASSWORD_MESSAGE: &str = "Current password is incorrect.";

/// The refusal message of an unspendable reset token.
pub const BAD_RESET_TOKEN_MESSAGE: &str = "This password-reset link is invalid or has expired.";

/// The refusal message of an unspendable verification token.
pub const BAD_VERIFY_TOKEN_MESSAGE: &str = "This verification link is invalid or has expired.";

/// The password of the anti-enumeration dummy hash.
///
/// The hash of this password and the password that the dummy verify tests
/// differ, so the verify is a mismatch that spends the whole Argon2 cost.
pub const DUMMY_PASSWORD: &str = "anti-enumeration-throwaway-password";

/// The password that the dummy verify tests against the dummy hash.
pub const DUMMY_MISMATCH: &str = "anti-enumeration-throwaway-mismatch";

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
        Ok(Self(
            parts
                .extensions
                .get::<ConnectInfo<SocketAddr>>()
                .map(|info| info.0),
        ))
    }
}

/// The ten routes, on the paths that the CSRF layer names.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/signup", post(signup))
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/logout-all", post(logout_all))
        .route("/api/auth/me", get(me))
        .route("/api/auth/password/change", post(change_password))
        .route("/api/auth/password/forgot", post(forgot_password))
        .route("/api/auth/password/reset", post(reset_password))
        .route("/api/auth/verify-email", post(verify_email))
        .route("/api/auth/verify-email/resend", post(resend_verification))
}

// ---------------------------------------------------------------------------
// Body and answer helpers
// ---------------------------------------------------------------------------

/// Read the body as a JSON object, or answer `422 invalid_request`.
fn object(body: &Bytes) -> Result<Value, ApiError> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|_| ApiError::invalid_request("The body is not JSON."))?;
    if !value.is_object() {
        return Err(ApiError::invalid_request("The body is not a JSON object."));
    }
    Ok(value)
}

/// Read one string field, or answer `422 invalid_request`.
fn field<'v>(value: &'v Value, name: &str) -> Result<&'v str, ApiError> {
    value
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| ApiError::invalid_request(format!("The body needs a string {name}.")))
}

/// Whether the caller asked for the raw session token in the body.
fn wants_bearer(headers: &HeaderMap) -> bool {
    headers
        .get(ACCEPT_SESSION_TOKEN)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("true"))
}

/// The public view of one account. Sign-up, login, verify-email, and `me` share
/// it, so one shape reaches the client from all four.
fn user_public(profile: &AccountProfile) -> Value {
    json!({
        "id": profile.id.to_string(),
        "email": profile.email,
        "email_verified": profile.email_verified_at.is_some(),
        "created_at": profile.created_at.to_rfc3339(),
    })
}

/// Attach one `Set-Cookie` header to an answer.
fn with_cookie(mut response: Response, cookie: HeaderValue) -> Response {
    response.headers_mut().append(header::SET_COOKIE, cookie);
    response
}

/// The `200` that opens a session: the cookie, the user, and the raw token when
/// the caller asked for it.
fn session_answer(
    state: &AppState,
    headers: &HeaderMap,
    profile: &AccountProfile,
    raw: &str,
) -> Result<Response, ApiError> {
    let cookie = set_session_cookie(state.posture, raw).map_err(|err| {
        tracing::error!(error = %err, "auth: the session cookie did not build");
        ApiError::internal("session cookie")
    })?;
    let mut payload = json!({ "user": user_public(profile) });
    if wants_bearer(headers) {
        payload["session_token"] = json!(raw);
    }
    Ok(with_cookie(
        (StatusCode::OK, Json(payload)).into_response(),
        cookie,
    ))
}

/// The `Set-Cookie` header that ends the session cookie.
fn clearing_cookie(state: &AppState) -> Result<HeaderValue, ApiError> {
    clear_session_cookie(state.posture).map_err(|err| {
        tracing::error!(error = %err, "auth: the cookie deletion did not build");
        ApiError::internal("session cookie")
    })
}

/// Hash `password` under the profile of this deployment.
fn hash_new(profile: Argon2Profile, password: &str) -> Result<String, ApiError> {
    hash_password(profile, password).map_err(|err| {
        tracing::error!(error = %err, "auth: the password hash failed");
        ApiError::internal("password hash")
    })
}

/// Draw one session token, or fail the request.
///
/// A predictable session token is a full account takeover, so an entropy failure
/// writes nothing and answers `500`.
fn new_token() -> Result<String, ApiError> {
    generate_token().map_err(|err| {
        tracing::error!(error = %err, "auth: the token draw failed");
        ApiError::internal("token draw")
    })
}

/// The dummy hash of one profile, computed once per profile.
///
/// The cost of the dummy verify must match the cost of a real one, so the hash
/// carries the parameters of the ACTIVE profile. A `None` answer means the
/// operating system refused entropy; the caller then skips the verify and still
/// answers `401`, because the alternative is a `500` that tells a caller the
/// address is unknown.
fn dummy_hash(profile: Argon2Profile) -> Option<&'static str> {
    use std::sync::OnceLock;

    static PROD: OnceLock<Option<String>> = OnceLock::new();
    static TEST: OnceLock<Option<String>> = OnceLock::new();

    let cell = if profile == Argon2Profile::TEST {
        &TEST
    } else {
        &PROD
    };
    cell.get_or_init(|| hash_password(profile, DUMMY_PASSWORD).ok())
        .as_deref()
}

/// Spend one Argon2 verify that cannot match (spec section 10, "Signup /
/// unverified login").
fn dummy_verify(profile: Argon2Profile) {
    if let Some(hash) = dummy_hash(profile) {
        let _ = verify_password(hash, DUMMY_MISMATCH);
    }
}

/// Start a transaction and bind the tenant to it (C3).
async fn bind(db: &Db, user_id: Uuid) -> Result<Transaction<'static, Postgres>, ApiError> {
    store_call(db, "tenant bind", begin_tenant(db.pool(), user_id)).await
}

/// Commit a bound transaction. `step` names the write for the log.
async fn commit(
    db: &Db,
    tx: Transaction<'static, Postgres>,
    step: &'static str,
) -> Result<(), ApiError> {
    store_call(db, step, async { Ok(tx.commit().await?) }).await
}

/// The session row of a login, a sign-in by verification link, or an OAuth
/// callback.
fn new_session_row<'a>(
    token_hash: &'a str,
    now: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    ip: &'a str,
    user_agent: Option<&'a str>,
) -> NewSession<'a> {
    NewSession {
        token_hash,
        created_at: now,
        last_seen_at: now,
        expires_at,
        ip: Some(ip),
        user_agent,
    }
}

/// The `User-Agent` of the request, for the session list.
fn user_agent(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
}

/// Bind the tenant, write the session row, and read the account back.
///
/// Login step 3 and the verification sign-in both end here, so the two paths
/// cannot drift.
async fn open_session(
    db: &Db,
    user_id: Uuid,
    session: &NewSession<'_>,
    rehash: Option<&str>,
) -> Result<AccountProfile, ApiError> {
    let mut tx = bind(db, user_id).await?;
    if let Some(password_hash) = rehash {
        store_call(
            db,
            "password rehash",
            set_password_hash(&mut *tx, user_id, password_hash),
        )
        .await?;
    }
    store_call(
        db,
        "session insert",
        insert_session(&mut *tx, user_id, session),
    )
    .await?;
    let profile = store_call(db, "account read", account_profile(&mut *tx))
        .await?
        .ok_or_else(|| {
            tracing::error!("auth: the bound account read gave no row");
            ApiError::internal("account read")
        })?;
    commit(db, tx, "session insert").await?;
    Ok(profile)
}

/// Bind the tenant and mint one single-use out-of-band token.
///
/// A new link supersedes the live ones of the same purpose, so a leaked older
/// link stops working the moment a fresh one is asked for.
async fn mint_token(
    db: &Db,
    user_id: Uuid,
    purpose: &str,
    now: DateTime<Utc>,
    ttl_secs: u64,
) -> Result<String, ApiError> {
    let raw = new_token()?;
    let expires_at = plus_secs(now, ttl_secs).ok_or_else(|| ApiError::internal("token window"))?;
    let mut tx = bind(db, user_id).await?;
    store_call(
        db,
        "token insert",
        delete_tokens_for_purpose(&mut *tx, purpose),
    )
    .await?;
    store_call(
        db,
        "token insert",
        insert_token(&mut *tx, user_id, &hash_token(&raw), purpose, expires_at),
    )
    .await?;
    commit(db, tx, "token insert").await?;
    Ok(raw)
}

/// Read one out-of-band token and refuse every unspendable shape.
///
/// One `400 invalid_token` covers "no such token", "wrong purpose", "already
/// spent", and "expired", so nothing tells the four apart.
async fn spendable_token(
    db: &Db,
    raw: &str,
    purpose: &str,
    message: &'static str,
    now: DateTime<Utc>,
) -> Result<(String, Uuid), ApiError> {
    let token_hash = hash_token(raw);
    let row = store_call(db, "token lookup", token_by_hash(db.pool(), &token_hash))
        .await?
        .ok_or_else(|| ApiError::invalid_token(message))?;
    if row.purpose != purpose || row.consumed_at.is_some() || now >= row.expires_at {
        return Err(ApiError::invalid_token(message));
    }
    Ok((token_hash, row.user_id))
}

// ---------------------------------------------------------------------------
// The routes
// ---------------------------------------------------------------------------

/// `POST /api/auth/signup` — register an account, non-enumerable.
///
/// The answer is the same object for a new address and for a registered one, it
/// sets no cookie, and it is never `409 email_taken`. The Argon2 hash runs
/// before the INSERT, so both paths spend the dominant cost.
pub async fn signup(
    State(state): State<AppState>,
    ClientAddr(peer): ClientAddr,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let value = object(&body)?;
    let email = normalize_email(field(&value, "email")?);
    let password = field(&value, "password")?;

    let now = Utc::now();
    enforce(
        &state.db,
        RateRule::SIGNUP,
        &email,
        &client_ip(&headers, peer),
        now,
    )
    .await?;

    validate_password(password).map_err(|err| ApiError::weak_password(err.to_string()))?;
    let password_hash = hash_new(state.argon2, password)?;

    let outcome = store_call(
        &state.db,
        "sign-up",
        sign_up(state.db.pool(), &email, Some(password_hash.as_str())),
    )
    .await?;
    if let SignUp::Created(user) = outcome {
        mint_token(
            &state.db,
            user.id,
            PURPOSE_VERIFY,
            now,
            VERIFY_TOKEN_TTL_SECS,
        )
        .await?;
    }

    Ok((
        StatusCode::OK,
        Json(json!({
            "status": VERIFICATION_REQUIRED,
            "message": VERIFICATION_REQUIRED_MESSAGE,
        })),
    )
        .into_response())
}

/// `POST /api/auth/login` — verify the password and open a fresh session.
///
/// Every refusal is `401 invalid_credentials`: an unknown address, an OAuth-only
/// account, a wrong password, an unverified address, and a disabled account. The
/// verify runs before the disabled test and before the verified test, so no
/// branch answers faster than another.
pub async fn login(
    State(state): State<AppState>,
    ClientAddr(peer): ClientAddr,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let value = object(&body)?;
    let email = normalize_email(field(&value, "email")?);
    let password = field(&value, "password")?;

    let now = Utc::now();
    let ip = client_ip(&headers, peer);
    enforce(&state.db, RateRule::LOGIN, &email, &ip, now).await?;

    let found = store_call(
        &state.db,
        "account lookup",
        user_by_email(state.db.pool(), &email),
    )
    .await?;
    // An unknown address and an OAuth-only account take the SAME exit, and both
    // spend one real Argon2 verify on the way out (spec section 10, row "Signup
    // / unverified login").
    let candidate = found.and_then(|user| user.password_hash.clone().map(|stored| (user, stored)));
    let Some((user, stored)) = candidate else {
        dummy_verify(state.argon2);
        return Err(ApiError::invalid_credentials(INVALID_LOGIN_MESSAGE));
    };

    let matched = verify_password(&stored, password);
    if !matched || user.disabled_at.is_some() || user.email_verified_at.is_none() {
        return Err(ApiError::invalid_credentials(INVALID_LOGIN_MESSAGE));
    }

    let rehash = if needs_rehash(state.argon2, &stored) {
        Some(hash_new(state.argon2, password)?)
    } else {
        None
    };

    let raw = new_token()?;
    let expires_at =
        plus_secs(now, SESSION_IDLE_SECS).ok_or_else(|| ApiError::internal("session window"))?;
    let token_hash = hash_token(&raw);
    let session = new_session_row(&token_hash, now, expires_at, &ip, user_agent(&headers));
    let profile = open_session(&state.db, user.id, &session, rehash.as_deref()).await?;

    session_answer(&state, &headers, &profile, &raw)
}

/// `POST /api/auth/logout` — end the presented session and clear the cookie.
///
/// The route carries no guard and is idempotent: a stale or absent credential
/// still answers `200` and still clears the cookie.
pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    if let Some(raw) = crate::auth::session::read_session_credential(&headers, state.posture.name) {
        let token_hash = hash_token(raw);
        let found = store_call(
            &state.db,
            "session lookup",
            session_by_token_hash(state.db.pool(), &token_hash),
        )
        .await?;
        if let Some(session) = found {
            let mut tx = bind(&state.db, session.user_id).await?;
            store_call(
                &state.db,
                "session delete",
                delete_session(&mut *tx, &token_hash),
            )
            .await?;
            commit(&state.db, tx, "session delete").await?;
        }
    }
    Ok(with_cookie(
        (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
        clearing_cookie(&state)?,
    ))
}

/// `POST /api/auth/logout-all` — end every session of the caller.
pub async fn logout_all(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let authed = current_user(&state, &headers).await?;
    let mut tx = bind(&state.db, authed.user.id).await?;
    let revoked = store_call(&state.db, "session delete", delete_all_sessions(&mut *tx)).await?;
    commit(&state.db, tx, "session delete").await?;

    Ok(with_cookie(
        (
            StatusCode::OK,
            Json(json!({ "ok": true, "revoked": revoked })),
        )
            .into_response(),
        clearing_cookie(&state)?,
    ))
}

/// `GET /api/auth/me` — the account behind the session.
///
/// The read is the plain bound SELECT that the M5 call order allows after the
/// bind, so it needs none of the five functions.
pub async fn me(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, ApiError> {
    let authed = current_user(&state, &headers).await?;
    let profile = bound_profile(&state.db, authed.user.id).await?;
    Ok((
        StatusCode::OK,
        Json(json!({ "user": user_public(&profile) })),
    )
        .into_response())
}

/// Read the caller's own account row inside a tenant transaction.
async fn bound_profile(db: &Db, user_id: Uuid) -> Result<AccountProfile, ApiError> {
    let mut tx = bind(db, user_id).await?;
    let profile = store_call(db, "account read", account_profile(&mut *tx)).await?;
    commit(db, tx, "account read").await?;
    profile.ok_or_else(|| ApiError::unauthorized(crate::auth::guard::NO_SESSION_MESSAGE))
}

/// `POST /api/auth/password/change` — change the password and keep this session.
///
/// The sweep ends every OTHER session of the account, so the person who just
/// changed the password stays signed in and every other device does not.
pub async fn change_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let Authed { user, token_hash } = current_user(&state, &headers).await?;
    let value = object(&body)?;
    let current = field(&value, "current_password")?;
    let new_password = field(&value, "new_password")?;

    let stored = user
        .password_hash
        .as_deref()
        .ok_or_else(|| ApiError::invalid_credentials(WRONG_CURRENT_PASSWORD_MESSAGE))?;
    if !verify_password(stored, current) {
        return Err(ApiError::invalid_credentials(
            WRONG_CURRENT_PASSWORD_MESSAGE,
        ));
    }
    validate_password(new_password).map_err(|err| ApiError::weak_password(err.to_string()))?;
    let password_hash = hash_new(state.argon2, new_password)?;

    let mut tx = bind(&state.db, user.id).await?;
    store_call(
        &state.db,
        "password write",
        set_password_hash(&mut *tx, user.id, &password_hash),
    )
    .await?;
    store_call(
        &state.db,
        "session delete",
        delete_other_sessions(&mut *tx, &token_hash),
    )
    .await?;
    commit(&state.db, tx, "password write").await?;

    Ok((StatusCode::OK, Json(json!({ "ok": true }))).into_response())
}

/// `POST /api/auth/password/forgot` — mint a reset token for a registered
/// address.
///
/// The answer is `{"ok":true}` either way. An unknown address writes nothing.
pub async fn forgot_password(
    State(state): State<AppState>,
    ClientAddr(peer): ClientAddr,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let value = object(&body)?;
    let email = normalize_email(field(&value, "email")?);

    let now = Utc::now();
    enforce(
        &state.db,
        RateRule::FORGOT,
        &email,
        &client_ip(&headers, peer),
        now,
    )
    .await?;

    let found = store_call(
        &state.db,
        "account lookup",
        user_by_email(state.db.pool(), &email),
    )
    .await?;
    if let Some(user) = found {
        mint_token(&state.db, user.id, PURPOSE_RESET, now, RESET_TOKEN_TTL_SECS).await?;
    }
    Ok((StatusCode::OK, Json(json!({ "ok": true }))).into_response())
}

/// `POST /api/auth/password/reset` — spend a reset token and write the new
/// password.
///
/// The policy check runs BEFORE the token is spent, so a password that the
/// policy refuses leaves the link usable. 1.0 spent the token first and made the
/// learner ask for a second link.
///
/// A completed reset proves the person reads the inbox, which is what the
/// verification link proves, so an unverified account becomes verified here.
/// Every session of the account then ends: a reset signs out every device,
/// including an attacker's.
pub async fn reset_password(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let value = object(&body)?;
    let raw = field(&value, "token")?;
    let new_password = field(&value, "new_password")?;

    let now = Utc::now();
    let (token_hash, user_id) =
        spendable_token(&state.db, raw, PURPOSE_RESET, BAD_RESET_TOKEN_MESSAGE, now).await?;

    validate_password(new_password).map_err(|err| ApiError::weak_password(err.to_string()))?;
    let password_hash = hash_new(state.argon2, new_password)?;

    let account = store_call(
        &state.db,
        "account lookup",
        user_by_id(state.db.pool(), user_id),
    )
    .await?;

    let spent = store_call(
        &state.db,
        "token spend",
        consume_token_tx(
            state.db.pool(),
            user_id,
            &token_hash,
            TokenEffect::PasswordReset {
                password_hash: password_hash.as_str(),
            },
        ),
    )
    .await?;
    if spent == TokenConsumed::AlreadySpent {
        return Err(ApiError::invalid_token(BAD_RESET_TOKEN_MESSAGE));
    }

    let mut tx = bind(&state.db, user_id).await?;
    store_call(&state.db, "session delete", delete_all_sessions(&mut *tx)).await?;
    if account.is_some_and(|user| user.email_verified_at.is_none()) {
        store_call(
            &state.db,
            "verification stamp",
            mark_email_verified(&mut *tx, user_id),
        )
        .await?;
    }
    commit(&state.db, tx, "session delete").await?;

    Ok((StatusCode::OK, Json(json!({ "ok": true }))).into_response())
}

/// `POST /api/auth/verify-email` — spend a verification token, mark the address
/// verified, and sign in.
///
/// Login is gated on verification, so the link both activates the account and
/// opens a session. A link works exactly once: an account that is already
/// verified, gone, or disabled gets the same `400 invalid_token`, which keeps a
/// leftover link from becoming a password-free login.
pub async fn verify_email(
    State(state): State<AppState>,
    ClientAddr(peer): ClientAddr,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let value = object(&body)?;
    let raw = field(&value, "token")?;

    let now = Utc::now();
    let (token_hash, user_id) = spendable_token(
        &state.db,
        raw,
        PURPOSE_VERIFY,
        BAD_VERIFY_TOKEN_MESSAGE,
        now,
    )
    .await?;

    let account = store_call(
        &state.db,
        "account lookup",
        user_by_id(state.db.pool(), user_id),
    )
    .await?
    .ok_or_else(|| ApiError::invalid_token(BAD_VERIFY_TOKEN_MESSAGE))?;
    if account.disabled_at.is_some() || account.email_verified_at.is_some() {
        return Err(ApiError::invalid_token(BAD_VERIFY_TOKEN_MESSAGE));
    }

    let spent = store_call(
        &state.db,
        "token spend",
        consume_token_tx(
            state.db.pool(),
            user_id,
            &token_hash,
            TokenEffect::EmailVerify,
        ),
    )
    .await?;
    if spent == TokenConsumed::AlreadySpent {
        return Err(ApiError::invalid_token(BAD_VERIFY_TOKEN_MESSAGE));
    }

    let raw_session = new_token()?;
    let expires_at =
        plus_secs(now, SESSION_IDLE_SECS).ok_or_else(|| ApiError::internal("session window"))?;
    let session_hash = hash_token(&raw_session);
    let ip = client_ip(&headers, peer);
    let session = new_session_row(&session_hash, now, expires_at, &ip, user_agent(&headers));
    let profile = open_session(&state.db, user_id, &session, None).await?;

    session_answer(&state, &headers, &profile, &raw_session)
}

/// `POST /api/auth/verify-email/resend` — mint a fresh verification link.
///
/// Public, because nobody can sign in before the address is verified. The answer
/// is `{"ok":true}` whether the address exists, is unknown, or is already
/// verified.
pub async fn resend_verification(
    State(state): State<AppState>,
    ClientAddr(peer): ClientAddr,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let value = object(&body)?;
    let email = normalize_email(field(&value, "email")?);

    let now = Utc::now();
    enforce(
        &state.db,
        RateRule::VERIFY_RESEND,
        &email,
        &client_ip(&headers, peer),
        now,
    )
    .await?;

    let found = store_call(
        &state.db,
        "account lookup",
        user_by_email(state.db.pool(), &email),
    )
    .await?;
    if let Some(user) = found
        && user.email_verified_at.is_none()
        && user.disabled_at.is_none()
    {
        mint_token(
            &state.db,
            user.id,
            PURPOSE_VERIFY,
            now,
            VERIFY_TOKEN_TTL_SECS,
        )
        .await?;
    }
    Ok((StatusCode::OK, Json(json!({ "ok": true }))).into_response())
}
