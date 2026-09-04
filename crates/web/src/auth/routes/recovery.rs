//! The out-of-band token routes: the password reset pair and the
//! verification pair.

use axum::response::Response;
use cadus_store::auth::{
    AuthUser, PURPOSE_RESET, PURPOSE_VERIFY, TokenConsumed, TokenEffect, consume_token_tx,
    delete_all_sessions, mark_email_verified, user_by_id,
};
use sqlx::types::chrono::{DateTime, Utc};

use super::{
    BAD_RESET_TOKEN_MESSAGE, BAD_VERIFY_TOKEN_MESSAGE, Public, bind, checked_hash, commit,
    mint_token, ok_answer, sign_in, spendable_token,
};
use crate::auth::rate::RateRule;
use crate::auth::session::{RESET_TOKEN_TTL_SECS, VERIFY_TOKEN_TTL_SECS};
use crate::auth::store_call;
use crate::error::ApiError;

/// Mint a token of `purpose` for `user`, when there is one, and answer
/// `{"ok":true}` either way.
async fn mint_for(
    req: &Public,
    user: Option<&AuthUser>,
    purpose: &str,
    now: DateTime<Utc>,
    ttl_secs: u64,
) -> Result<Response, ApiError> {
    if let Some(user) = user {
        mint_token(&req.state.db, user.id, purpose, now, ttl_secs).await?;
    }
    Ok(ok_answer())
}

/// `POST /api/auth/password/forgot` — mint a reset token for a registered
/// address.
///
/// The answer is `{"ok":true}` either way. An unknown address writes nothing.
pub async fn forgot_password(req: Public) -> Result<Response, ApiError> {
    let email = req.email()?;
    let now = Utc::now();
    let found = req
        .rate_limited_lookup(RateRule::FORGOT, &email, now)
        .await?;
    mint_for(
        &req,
        found.as_ref(),
        PURPOSE_RESET,
        now,
        RESET_TOKEN_TTL_SECS,
    )
    .await
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
pub async fn reset_password(req: Public) -> Result<Response, ApiError> {
    let raw = req.field("token")?;
    let new_password = req.field("new_password")?;
    let db = &req.state.db;

    let now = Utc::now();
    let (token_hash, user_id) =
        spendable_token(db, raw, PURPOSE_RESET, BAD_RESET_TOKEN_MESSAGE, now).await?;
    let password_hash = checked_hash(req.state.argon2, new_password)?;

    let account = store_call(db, "account lookup", user_by_id(db.pool(), user_id)).await?;

    let spent = store_call(
        db,
        "token spend",
        consume_token_tx(
            db.pool(),
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

    let mut tx = bind(db, user_id).await?;
    store_call(db, "session delete", delete_all_sessions(&mut *tx)).await?;
    if account.is_some_and(|user| user.email_verified_at.is_none()) {
        store_call(
            db,
            "verification stamp",
            mark_email_verified(&mut *tx, user_id),
        )
        .await?;
    }
    commit(db, tx, "session delete").await?;
    Ok(ok_answer())
}

/// `POST /api/auth/verify-email` — spend a verification token, mark the address
/// verified, and sign in.
///
/// Login is gated on verification, so the link both activates the account and
/// opens a session. A link works exactly once: an account that is already
/// verified, gone, or disabled gets the same `400 invalid_token`, which keeps a
/// leftover link from becoming a password-free login.
pub async fn verify_email(req: Public) -> Result<Response, ApiError> {
    let raw = req.field("token")?;
    let db = &req.state.db;

    let now = Utc::now();
    let (token_hash, user_id) =
        spendable_token(db, raw, PURPOSE_VERIFY, BAD_VERIFY_TOKEN_MESSAGE, now).await?;

    let account = store_call(db, "account lookup", user_by_id(db.pool(), user_id))
        .await?
        .ok_or_else(|| ApiError::invalid_token(BAD_VERIFY_TOKEN_MESSAGE))?;
    if account.disabled_at.is_some() || account.email_verified_at.is_some() {
        return Err(ApiError::invalid_token(BAD_VERIFY_TOKEN_MESSAGE));
    }

    let spent = store_call(
        db,
        "token spend",
        consume_token_tx(db.pool(), user_id, &token_hash, TokenEffect::EmailVerify),
    )
    .await?;
    if spent == TokenConsumed::AlreadySpent {
        return Err(ApiError::invalid_token(BAD_VERIFY_TOKEN_MESSAGE));
    }
    sign_in(&req, user_id, now, None).await
}

/// `POST /api/auth/verify-email/resend` — mint a fresh verification link.
///
/// Public, because nobody can sign in before the address is verified. The answer
/// is `{"ok":true}` whether the address exists, is unknown, or is already
/// verified.
pub async fn resend_verification(req: Public) -> Result<Response, ApiError> {
    let email = req.email()?;
    let now = Utc::now();
    let found = req
        .rate_limited_lookup(RateRule::VERIFY_RESEND, &email, now)
        .await?;
    let unverified = found
        .as_ref()
        .filter(|user| user.email_verified_at.is_none() && user.disabled_at.is_none());
    mint_for(&req, unverified, PURPOSE_VERIFY, now, VERIFY_TOKEN_TTL_SECS).await
}
