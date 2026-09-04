//! The helpers the auth routes share: the body readers, the answers, the
//! password and token plumbing, and the bound transaction.

use axum::Json;
use axum::body::Bytes;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use cadus_store::auth::{
    AccountProfile, NewSession, account_profile, delete_tokens_for_purpose, insert_session,
    insert_token, set_password_hash, token_by_hash,
};
use cadus_store::{Db, begin_tenant};
use serde_json::{Value, json};
use sqlx::types::Uuid;
use sqlx::types::chrono::{DateTime, Utc};
use sqlx::{Postgres, Transaction};

use super::{
    ACCEPT_SESSION_TOKEN, DUMMY_MISMATCH, DUMMY_PASSWORD, EMAIL_TOO_LONG_MESSAGE, MAX_EMAIL_BYTES,
    Public,
};
use crate::AppState;
use crate::auth::email::normalize_email;
use crate::auth::guard::plus_secs;
use crate::auth::password::{
    Argon2Profile, PasswordError, WeakPassword, hash_password, validate_password, verify_password,
};
use crate::auth::session::{CookieWriteError, clear_session_cookie, set_session_cookie};
use crate::auth::store_call;
use crate::auth::token::{EntropyError, generate_token, hash_token};
use crate::error::ApiError;

// ---------------------------------------------------------------------------
// Body and answer helpers
// ---------------------------------------------------------------------------

/// Read the body as a JSON object, or answer `422 invalid_request`.
pub(super) fn object(body: &Bytes) -> Result<Value, ApiError> {
    let value: Value = serde_json::from_slice(body).map_err(not_json)?;
    if !value.is_object() {
        return Err(ApiError::invalid_request("The body is not a JSON object."));
    }
    Ok(value)
}

/// The `422` of a body that is not JSON.
pub(super) fn not_json(_: serde_json::Error) -> ApiError {
    ApiError::invalid_request("The body is not JSON.")
}

/// Read one string field, or answer `422 invalid_request`.
pub(super) fn field<'v>(value: &'v Value, name: &str) -> Result<&'v str, ApiError> {
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
pub(super) fn email_field(value: &Value) -> Result<String, ApiError> {
    let email = normalize_email(field(value, "email")?);
    if email.len() > MAX_EMAIL_BYTES {
        return Err(ApiError::invalid_request(EMAIL_TOO_LONG_MESSAGE));
    }
    Ok(email)
}

/// Whether the caller asked for the raw session token in the body.
pub(super) fn wants_bearer(headers: &HeaderMap) -> bool {
    headers
        .get(ACCEPT_SESSION_TOKEN)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("true"))
}

/// The public view of one account. Sign-up, login, verify-email, and `me` share
/// it, so one shape reaches the client from all four.
pub(super) fn user_public(profile: &AccountProfile) -> Value {
    json!({
        "id": profile.id.to_string(),
        "email": profile.email,
        "email_verified": profile.email_verified_at.is_some(),
        "created_at": profile.created_at.to_rfc3339(),
    })
}

/// The `200 {"ok":true}` of a write with nothing else to say.
pub(super) fn ok_answer() -> Response {
    (StatusCode::OK, Json(json!({ "ok": true }))).into_response()
}

/// Attach one `Set-Cookie` header to an answer.
pub(super) fn with_cookie(mut response: Response, cookie: HeaderValue) -> Response {
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
pub(super) fn weak_password(err: WeakPassword) -> ApiError {
    ApiError::weak_password(err.to_string())
}

/// The `200` that opens a session: the cookie, the user, and the raw token when
/// the caller asked for it.
pub(super) fn session_answer(
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
pub(super) fn clearing_cookie(state: &AppState) -> Result<HeaderValue, ApiError> {
    clear_session_cookie(state.posture).map_err(cookie_failed)
}

/// The `500` of a password hash that failed.
pub(super) fn hash_failed(err: PasswordError) -> ApiError {
    tracing::error!(error = %err, "auth: the password hash failed");
    ApiError::internal("password hash")
}

/// Hash `password` under the profile of this deployment.
pub(super) fn hash_new(profile: Argon2Profile, password: &str) -> Result<String, ApiError> {
    hash_password(profile, password).map_err(hash_failed)
}

/// Check `password` against the policy, then hash it. Every route that sets a
/// NEW password runs this; a login rehash keeps the old one as it is.
pub(super) fn checked_hash(profile: Argon2Profile, password: &str) -> Result<String, ApiError> {
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
pub(super) fn dummy_hash(profile: Argon2Profile) -> Option<&'static str> {
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
pub(super) fn dummy_verify(profile: Argon2Profile) {
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
pub(super) fn no_account_row() -> ApiError {
    tracing::error!("auth: the bound account read gave no row");
    ApiError::internal("account read")
}

/// Bind the tenant, write the session row, and read the account back.
///
/// Login step 3 and the verification sign-in both end here, so the two paths
/// cannot drift.
pub(super) async fn open_session(
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
pub(super) async fn sign_in(
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
pub(super) async fn mint_token(
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
pub(super) async fn spendable_token(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::password::PasswordError;
    use crate::auth::session::CookieWriteError;
    use crate::auth::token::EntropyError;

    /// Every defensive `500` mapper answers an internal error and names its
    /// step in the message, never the cause.
    #[test]
    fn the_defensive_mappers_answer_internal_errors() {
        let mappers = [
            cookie_failed(CookieWriteError::BadValue),
            session_window(),
            hash_failed(PasswordError::Entropy {
                reason: "no pool".to_string(),
            }),
            entropy_failed(EntropyError {
                reason: "no pool".to_string(),
            }),
            no_account_row(),
        ];
        for err in mappers {
            assert_eq!(err.status, StatusCode::INTERNAL_SERVER_ERROR);
            assert!(!err.message.contains("no pool"), "{}", err.message);
        }
    }

    /// The anti-enumeration dummy hash and verify run under the production
    /// profile too, and the hash is stable across the two calls.
    #[test]
    fn the_dummy_hash_serves_the_production_profile() {
        let first = dummy_hash(Argon2Profile::PROD).expect("the prod dummy hash builds");
        let second = dummy_hash(Argon2Profile::PROD).expect("the prod dummy hash is cached");
        assert_eq!(first, second);
        dummy_verify(Argon2Profile::PROD);
    }
}
