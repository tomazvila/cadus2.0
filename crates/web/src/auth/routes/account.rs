//! The account routes: sign-up, login, the two logouts, `me`, and the
//! password change.

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use cadus_store::Db;
use cadus_store::auth::{
    AccountProfile, PURPOSE_VERIFY, SignUp, account_profile, delete_all_sessions,
    delete_other_sessions, delete_session, session_by_token_hash, set_password_hash, sign_up,
};
use serde_json::json;
use sqlx::types::Uuid;
use sqlx::types::chrono::Utc;

use super::support::*;
use super::{
    INVALID_LOGIN_MESSAGE, Public, VERIFICATION_REQUIRED, VERIFICATION_REQUIRED_MESSAGE,
    WRONG_CURRENT_PASSWORD_MESSAGE,
};
use crate::AppState;
use crate::auth::guard::{Authed, current_user};
use crate::auth::password::{needs_rehash, verify_password};
use crate::auth::rate::RateRule;
use crate::auth::session::{VERIFY_TOKEN_TTL_SECS, read_session_credential};
use crate::auth::store_call;
use crate::auth::token::hash_token;
use crate::error::ApiError;

/// `POST /api/auth/signup` — register an account, non-enumerable.
///
/// The answer is the same object for a new address and for a registered one, it
/// sets no cookie, and it is never `409 email_taken`. The Argon2 hash runs
/// before the INSERT, so both paths spend the dominant cost.
pub async fn signup(req: Public) -> Result<Response, ApiError> {
    let email = req.email()?;
    let password = req.field("password")?;

    let now = Utc::now();
    req.enforce(RateRule::SIGNUP, &email, now).await?;
    let password_hash = checked_hash(req.state.argon2, password)?;

    let db = &req.state.db;
    let outcome = store_call(
        db,
        "sign-up",
        sign_up(db.pool(), &email, Some(password_hash.as_str())),
    )
    .await?;
    if let SignUp::Created(user) = outcome {
        mint_token(db, user.id, PURPOSE_VERIFY, now, VERIFY_TOKEN_TTL_SECS).await?;
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
pub async fn login(req: Public) -> Result<Response, ApiError> {
    let email = req.email()?;
    let password = req.field("password")?;

    let now = Utc::now();
    let found = req
        .rate_limited_lookup(RateRule::LOGIN, &email, now)
        .await?;
    // An unknown address and an OAuth-only account take the SAME exit, and both
    // spend one real Argon2 verify on the way out (spec section 10, row "Signup
    // / unverified login").
    let candidate = found.and_then(|user| user.password_hash.clone().map(|stored| (user, stored)));
    let Some((user, stored)) = candidate else {
        dummy_verify(req.state.argon2);
        return Err(ApiError::invalid_credentials(INVALID_LOGIN_MESSAGE));
    };

    let matched = verify_password(&stored, password);
    if !matched || user.disabled_at.is_some() || user.email_verified_at.is_none() {
        return Err(ApiError::invalid_credentials(INVALID_LOGIN_MESSAGE));
    }

    let rehash = if needs_rehash(req.state.argon2, &stored) {
        Some(hash_new(req.state.argon2, password)?)
    } else {
        None
    };
    sign_in(&req, user.id, now, rehash.as_deref()).await
}

/// `POST /api/auth/logout` — end the presented session and clear the cookie.
///
/// The route carries no guard and is idempotent: a stale or absent credential
/// still answers `200` and still clears the cookie.
pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    if let Some(raw) = read_session_credential(&headers, state.posture.name) {
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
    Ok(with_cookie(ok_answer(), clearing_cookie(&state)?))
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
    // The read wrote nothing, so the transaction ends with the rollback its
    // drop runs, and the binding ends with it.
    drop(tx);
    profile.ok_or_else(|| ApiError::unauthorized(crate::auth::guard::NO_SESSION_MESSAGE))
}

/// `POST /api/auth/password/change` — change the password and keep this session.
///
/// The sweep ends every OTHER session of the account, so the person who just
/// changed the password stays signed in and every other device does not.
pub async fn change_password(req: Public) -> Result<Response, ApiError> {
    let Authed { user, token_hash } = current_user(&req.state, &req.headers).await?;
    let current = req.field("current_password")?;
    let new_password = req.field("new_password")?;

    let stored = user
        .password_hash
        .as_deref()
        .ok_or_else(|| ApiError::invalid_credentials(WRONG_CURRENT_PASSWORD_MESSAGE))?;
    if !verify_password(stored, current) {
        return Err(ApiError::invalid_credentials(
            WRONG_CURRENT_PASSWORD_MESSAGE,
        ));
    }
    let password_hash = checked_hash(req.state.argon2, new_password)?;

    let db = &req.state.db;
    let mut tx = bind(db, user.id).await?;
    store_call(
        db,
        "password write",
        set_password_hash(&mut *tx, user.id, &password_hash),
    )
    .await?;
    store_call(
        db,
        "session delete",
        delete_other_sessions(&mut *tx, &token_hash),
    )
    .await?;
    commit(db, tx, "password write").await?;
    Ok(ok_answer())
}
