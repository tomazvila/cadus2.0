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
//! Seven of the ten routes read a body, and each one takes it through
//! [`LimitedBody`], never through the axum `Bytes` extractor. The axum rejection
//! answers a plain-text sentence, and the section 2 envelope has no exception,
//! so a body over the limit is `413 payload_too_large` and a body that fails
//! mid-read is `422 invalid_request`, both as JSON. The other three routes read
//! no body: `logout` and `logout-all` take the session alone, and `me` is a
//! `GET`. No body rejection can reach those three.
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
use axum::extract::{ConnectInfo, FromRequest, FromRequestParts, Request};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use cadus_store::auth::{
    AccountProfile, NewSession, account_profile, delete_tokens_for_purpose, insert_session,
    insert_token, set_password_hash, token_by_hash, user_by_email,
};
use cadus_store::{Db, begin_tenant};
use serde_json::{Value, json};
use sqlx::types::Uuid;
use sqlx::types::chrono::{DateTime, Utc};
use sqlx::{Postgres, Transaction};

use crate::AppState;
use crate::auth::body::LimitedBody;
use crate::auth::email::normalize_email;
use crate::auth::guard::plus_secs;
use crate::auth::password::{
    Argon2Profile, PasswordError, WeakPassword, hash_password, validate_password, verify_password,
};
use crate::auth::rate::{RateRule, client_ip, enforce};
use crate::auth::session::{CookieWriteError, clear_session_cookie, set_session_cookie};
use crate::auth::store_call;
use crate::auth::token::{EntropyError, generate_token, hash_token};
use crate::error::ApiError;

mod account;
mod recovery;

pub use account::{change_password, login, logout, logout_all, me, signup};
pub use recovery::{forgot_password, resend_verification, reset_password, verify_email};

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

/// The ceiling of the email field, in UTF-8 bytes AFTER normalization.
///
/// RFC 5321 section 4.5.3.1.3 bounds a forward path at 256 octets, and the two
/// angle brackets take two of them, so 254 is the longest address that SMTP
/// carries. It is the same kind of bound as [`MAX_PASSWORD_BYTES`]: it is not a
/// usability rule, it bounds the work that the routes do over the input.
///
/// The normalized address is the `key` of the `auth_rate_counters` primary key.
/// A btree index entry has a limit of about 2704 bytes on the deployed Postgres
/// 16, so an unbounded address makes the rate counter itself throw, and the very
/// call that the counter must refuse goes uncounted. The cap runs BEFORE the
/// counter and before every lookup, so the refusal costs one length test.
///
/// [`MAX_PASSWORD_BYTES`]: crate::auth::password::MAX_PASSWORD_BYTES
pub const MAX_EMAIL_BYTES: usize = 254;

/// The refusal message of an address past [`MAX_EMAIL_BYTES`].
///
/// It is one sentence for every over-cap address. A registered address and an
/// unknown one give the same status, the same code, and this same message, so
/// the cap tells a caller nothing about who has an account.
pub const EMAIL_TOO_LONG_MESSAGE: &str = "The email address is too long.";

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
    fn field(&self, name: &str) -> Result<&str, ApiError> {
        field(&self.value, name)
    }

    /// The normalized `email` field of the body. See [`email_field`].
    fn email(&self) -> Result<String, ApiError> {
        email_field(&self.value)
    }

    /// Apply `rule` to the pair of this address and this client.
    async fn enforce(
        &self,
        rule: RateRule,
        email: &str,
        now: DateTime<Utc>,
    ) -> Result<(), ApiError> {
        enforce(&self.state.db, rule, email, &self.ip, now).await
    }

    /// Apply `rule`, then look the account of `email` up.
    async fn rate_limited_lookup(
        &self,
        rule: RateRule,
        email: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<cadus_store::auth::AuthUser>, ApiError> {
        self.enforce(rule, email, now).await?;
        store_call(
            &self.state.db,
            "account lookup",
            user_by_email(self.state.db.pool(), email),
        )
        .await
    }

    /// The session row of a sign-in at `now`, with the token digest `token_hash`.
    fn session_row<'a>(
        &'a self,
        token_hash: &'a str,
        now: DateTime<Utc>,
    ) -> Result<NewSession<'a>, ApiError> {
        let expires_at =
            plus_secs(now, crate::auth::session::SESSION_IDLE_SECS).ok_or_else(session_window)?;
        Ok(new_session_row(
            token_hash,
            now,
            expires_at,
            &self.ip,
            user_agent(&self.headers),
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
    let value: Value = serde_json::from_slice(body).map_err(not_json)?;
    if !value.is_object() {
        return Err(ApiError::invalid_request("The body is not a JSON object."));
    }
    Ok(value)
}

/// The `422` of a body that is not JSON.
fn not_json(_: serde_json::Error) -> ApiError {
    ApiError::invalid_request("The body is not JSON.")
}

/// Read one string field, or answer `422 invalid_request`.
fn field<'v>(value: &'v Value, name: &str) -> Result<&'v str, ApiError> {
    value
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| ApiError::invalid_request(format!("The body needs a string {name}.")))
}

/// Read the `email` field, normalize it, and hold it to [`MAX_EMAIL_BYTES`].
///
/// The cap applies AFTER normalization, because the normalized string is what
/// reaches the rate counter and the account lookup. NFKC can make a string
/// longer than the string it read, so a cap on the raw field would let a longer
/// key through.
///
/// Every caller runs this BEFORE its rate rule and before its account lookup.
/// An over-cap address therefore costs one length test, writes no counter row,
/// and reads no table.
fn email_field(value: &Value) -> Result<String, ApiError> {
    let email = normalize_email(field(value, "email")?);
    if email.len() > MAX_EMAIL_BYTES {
        return Err(ApiError::invalid_request(EMAIL_TOO_LONG_MESSAGE));
    }
    Ok(email)
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

/// The `200 {"ok":true}` of a write with nothing else to say.
fn ok_answer() -> Response {
    (StatusCode::OK, Json(json!({ "ok": true }))).into_response()
}

/// Attach one `Set-Cookie` header to an answer.
fn with_cookie(mut response: Response, cookie: HeaderValue) -> Response {
    response.headers_mut().append(header::SET_COOKIE, cookie);
    response
}

/// The `500` of a cookie header that did not build.
pub(crate) fn cookie_failed(err: CookieWriteError) -> ApiError {
    tracing::error!(error = %err, "auth: a cookie header did not build");
    ApiError::internal("session cookie")
}

/// The `500` of a session window past the calendar.
pub(crate) fn session_window() -> ApiError {
    ApiError::internal("session window")
}

/// The `422 weak_password` of a password the policy refuses.
fn weak_password(err: WeakPassword) -> ApiError {
    ApiError::weak_password(err.to_string())
}

/// The `200` that opens a session: the cookie, the user, and the raw token when
/// the caller asked for it.
fn session_answer(
    state: &AppState,
    headers: &HeaderMap,
    profile: &AccountProfile,
    raw: &str,
) -> Result<Response, ApiError> {
    let cookie = set_session_cookie(state.posture, raw).map_err(cookie_failed)?;
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
    clear_session_cookie(state.posture).map_err(cookie_failed)
}

/// The `500` of a password hash that failed.
fn hash_failed(err: PasswordError) -> ApiError {
    tracing::error!(error = %err, "auth: the password hash failed");
    ApiError::internal("password hash")
}

/// Hash `password` under the profile of this deployment.
fn hash_new(profile: Argon2Profile, password: &str) -> Result<String, ApiError> {
    hash_password(profile, password).map_err(hash_failed)
}

/// Check `password` against the policy, then hash it. Every route that sets a
/// NEW password runs this; a login rehash keeps the old one as it is.
fn checked_hash(profile: Argon2Profile, password: &str) -> Result<String, ApiError> {
    validate_password(password).map_err(weak_password)?;
    hash_new(profile, password)
}

/// The `500` of a token draw that failed.
///
/// A predictable session token is a full account takeover, so an entropy failure
/// writes nothing and answers `500`.
pub(crate) fn entropy_failed(err: EntropyError) -> ApiError {
    tracing::error!(error = %err, "auth: the token draw failed");
    ApiError::internal("token draw")
}

/// Draw one session token, or fail the request.
pub(crate) fn new_token() -> Result<String, ApiError> {
    generate_token().map_err(entropy_failed)
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
pub(crate) async fn bind(
    db: &Db,
    user_id: Uuid,
) -> Result<Transaction<'static, Postgres>, ApiError> {
    store_call(db, "tenant bind", begin_tenant(db.pool(), user_id)).await
}

/// Commit a bound transaction. `step` names the write for the log.
pub(crate) async fn commit(
    db: &Db,
    tx: Transaction<'static, Postgres>,
    step: &'static str,
) -> Result<(), ApiError> {
    store_call(db, step, async { Ok(tx.commit().await?) }).await
}

/// The session row of a login, a sign-in by verification link, or an OAuth
/// callback.
pub(crate) fn new_session_row<'a>(
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
pub(crate) fn user_agent(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
}

/// The `500` of a bound account read that gave no row.
fn no_account_row() -> ApiError {
    tracing::error!("auth: the bound account read gave no row");
    ApiError::internal("account read")
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
        .ok_or_else(no_account_row)?;
    commit(db, tx, "session insert").await?;
    Ok(profile)
}

/// Open a session for `user_id` from the request `req` at `now`, and answer
/// the `200` that carries it.
async fn sign_in(
    req: &Public,
    user_id: Uuid,
    now: DateTime<Utc>,
    rehash: Option<&str>,
) -> Result<Response, ApiError> {
    let raw = new_token()?;
    let token_hash = hash_token(&raw);
    let session = req.session_row(&token_hash, now)?;
    let profile = open_session(&req.state.db, user_id, &session, rehash).await?;
    session_answer(&req.state, &req.headers, &profile, &raw)
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
