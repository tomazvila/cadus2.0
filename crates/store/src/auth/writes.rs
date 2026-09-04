//! The bound writes, the rate counters, and the three composed orders of the
//! M5 auth contract.

use sqlx::types::chrono::{DateTime, Utc};
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

use super::{
    AccountProfile, NewSession, SignUp, TokenConsumed, TokenEffect, insert_user, user_by_email,
};
use crate::{StoreError, begin_tenant};

// ---------------------------------------------------------------------------
// The bound writes. The executor is a `begin_tenant` transaction.
// ---------------------------------------------------------------------------

/// Write one session row for the bound account (login step 3, sign-up step 3).
///
/// The `WITH CHECK` clause of the `tenant_isolation` policy refuses any other
/// `user_id` with SQLSTATE 42501, so one forged INSERT cannot mint a cookie for
/// another account.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn insert_session<'e, E>(
    executor: E,
    user_id: Uuid,
    session: &NewSession<'_>,
) -> Result<(), StoreError>
where
    E: PgExecutor<'e>,
{
    sqlx::query!(
        "INSERT INTO auth_sessions
             (token_hash, user_id, created_at, last_seen_at, expires_at, ip, user_agent)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
        session.token_hash,
        user_id,
        session.created_at,
        session.last_seen_at,
        session.expires_at,
        session.ip,
        session.user_agent
    )
    .execute(executor)
    .await?;
    Ok(())
}

/// Touch `last_seen_at` of one session (guarded request step 4).
///
/// The caller runs this at most once per hour. The return value is the count of
/// rows the statement wrote: 0 says the policy refused the row, or the session
/// is gone.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn touch_last_seen<'e, E>(executor: E, token_hash: &str) -> Result<u64, StoreError>
where
    E: PgExecutor<'e>,
{
    Ok(sqlx::query!(
        "UPDATE auth_sessions SET last_seen_at = now() WHERE token_hash = $1",
        token_hash
    )
    .execute(executor)
    .await?
    .rows_affected())
}

/// End one session (sign-out).
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn delete_session<'e, E>(executor: E, token_hash: &str) -> Result<u64, StoreError>
where
    E: PgExecutor<'e>,
{
    Ok(sqlx::query!(
        "DELETE FROM auth_sessions WHERE token_hash = $1",
        token_hash
    )
    .execute(executor)
    .await?
    .rows_affected())
}

/// End every session of the bound account (sign out everywhere).
///
/// The statement carries no `WHERE` clause; the policy holds it to the bound
/// tenant.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn delete_all_sessions<'e, E>(executor: E) -> Result<u64, StoreError>
where
    E: PgExecutor<'e>,
{
    Ok(sqlx::query!("DELETE FROM auth_sessions")
        .execute(executor)
        .await?
        .rows_affected())
}

/// End every session of the bound account except the current one (after a
/// password change).
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn delete_other_sessions<'e, E>(
    executor: E,
    keep_token_hash: &str,
) -> Result<u64, StoreError>
where
    E: PgExecutor<'e>,
{
    Ok(sqlx::query!(
        "DELETE FROM auth_sessions WHERE token_hash <> $1",
        keep_token_hash
    )
    .execute(executor)
    .await?
    .rows_affected())
}

/// Write one out-of-band token for the bound account (token creation, step 3).
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn insert_token<'e, E>(
    executor: E,
    user_id: Uuid,
    token_hash: &str,
    purpose: &str,
    expires_at: DateTime<Utc>,
) -> Result<(), StoreError>
where
    E: PgExecutor<'e>,
{
    sqlx::query!(
        "INSERT INTO auth_tokens (token_hash, user_id, purpose, expires_at)
         VALUES ($1, $2, $3, $4)",
        token_hash,
        user_id,
        purpose,
        expires_at
    )
    .execute(executor)
    .await?;
    Ok(())
}

/// Drop every live token of one purpose for the bound account.
///
/// A new reset link supersedes the older ones, so a leaked older link stops
/// working the moment a fresh one is requested (1.0
/// `cadus_web/authn/service.py:392`).
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn delete_tokens_for_purpose<'e, E>(executor: E, purpose: &str) -> Result<u64, StoreError>
where
    E: PgExecutor<'e>,
{
    Ok(
        sqlx::query!("DELETE FROM auth_tokens WHERE purpose = $1", purpose)
            .execute(executor)
            .await?
            .rows_affected(),
    )
}

/// Mark one token spent, once.
///
/// `consumed_at IS NULL` in the `WHERE` clause is the single-use rule: two
/// concurrent requests race on the same row, and exactly one of them writes it.
/// A zero row count is [`TokenConsumed::AlreadySpent`], and the caller then rolls
/// the transaction back instead of applying the effect twice.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn consume_token<'e, E>(
    executor: E,
    token_hash: &str,
) -> Result<TokenConsumed, StoreError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query!(
        "UPDATE auth_tokens SET consumed_at = now()
         WHERE token_hash = $1 AND consumed_at IS NULL",
        token_hash
    )
    .execute(executor)
    .await?
    .rows_affected();
    if rows == 0 {
        return Ok(TokenConsumed::AlreadySpent);
    }
    Ok(TokenConsumed::Consumed)
}

/// Write a new password hash on the bound account.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn set_password_hash<'e, E>(
    executor: E,
    user_id: Uuid,
    password_hash: &str,
) -> Result<u64, StoreError>
where
    E: PgExecutor<'e>,
{
    Ok(sqlx::query!(
        "UPDATE users SET password_hash = $2 WHERE id = $1",
        user_id,
        password_hash
    )
    .execute(executor)
    .await?
    .rows_affected())
}

/// Drop the password hash of the bound account.
///
/// The OAuth callback runs this before it stamps an address that no one
/// verified. Sign-up asks for no proof of the address, so a password on an
/// unverified account proves nothing about who set it. The stamp alone would
/// turn that password into a live credential, so the password goes first and the
/// row keeps the shape of an OAuth-only account.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn clear_password_hash<'e, E>(executor: E, user_id: Uuid) -> Result<u64, StoreError>
where
    E: PgExecutor<'e>,
{
    Ok(sqlx::query!(
        "UPDATE users SET password_hash = NULL WHERE id = $1",
        user_id
    )
    .execute(executor)
    .await?
    .rows_affected())
}

/// Stamp `email_verified_at` on the bound account.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn mark_email_verified<'e, E>(executor: E, user_id: Uuid) -> Result<u64, StoreError>
where
    E: PgExecutor<'e>,
{
    Ok(sqlx::query!(
        "UPDATE users SET email_verified_at = now() WHERE id = $1",
        user_id
    )
    .execute(executor)
    .await?
    .rows_affected())
}

/// Read the bound account's own row (the profile answer of `GET
/// /api/auth/me`).
///
/// This is the plain SELECT that the M5 call order allows after the bind. The
/// `users_read_self` policy holds the statement to the bound tenant, so the
/// answer is the caller's row or nothing.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn account_profile<'e, E>(executor: E) -> Result<Option<AccountProfile>, StoreError>
where
    E: PgExecutor<'e>,
{
    Ok(sqlx::query_as!(
        AccountProfile,
        r#"
        SELECT id, email::text AS "email!", email_verified_at, created_at
        FROM users
        "#
    )
    .fetch_optional(executor)
    .await?)
}

/// Link one provider account to the bound account (the OAuth callback).
///
/// The caller writes this row only when the provider states the email verified.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn insert_oauth_account<'e, E>(
    executor: E,
    user_id: Uuid,
    provider: &str,
    provider_account_id: &str,
    email_at_link: &str,
) -> Result<(), StoreError>
where
    E: PgExecutor<'e>,
{
    sqlx::query!(
        "INSERT INTO oauth_accounts
             (provider, provider_account_id, user_id, email_at_link)
         VALUES ($1, $2, $3, $4)",
        provider,
        provider_account_id,
        user_id,
        email_at_link
    )
    .execute(executor)
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// The rate counters. `auth_rate_counters` carries no `user_id` and no policy,
// so the upsert works bound and unbound (specification section 3.2).
// ---------------------------------------------------------------------------

/// The start of the fixed window that holds `now`.
///
/// The window is floored from the 1970 epoch, so every process of a deployment
/// derives the same bucket from the same instant (1.0 `routes.py:209-212`). A
/// `window_secs` of 0 gives `now` unchanged, and so does an instant that no
/// clock reaches.
///
/// M5 U4 layers the four paired rules on this bucket: the rule bounds the
/// normalized email first, then the client address, and it increments both
/// counters before any account lookup.
#[must_use]
pub fn window_start(now: DateTime<Utc>, window_secs: u32) -> DateTime<Utc> {
    if window_secs == 0 {
        return now;
    }
    let width = i64::from(window_secs);
    let floored = now.timestamp().div_euclid(width) * width;
    DateTime::from_timestamp(floored, 0).unwrap_or(now)
}

/// Increment one fixed-window counter and return its new count.
///
/// `scope` is `"{prefix}_email"` or `"{prefix}_ip"`, and `key` is the normalized
/// email or the client address. The first call of a window inserts the row with
/// a count of 1, and every later call of that window adds 1.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn bump_rate_counter<'e, E>(
    executor: E,
    scope: &str,
    key: &str,
    window_start: DateTime<Utc>,
) -> Result<i32, StoreError>
where
    E: PgExecutor<'e>,
{
    Ok(sqlx::query_scalar!(
        r#"
        INSERT INTO auth_rate_counters (scope, key, window_start, count)
        VALUES ($1, $2, $3, 1)
        ON CONFLICT (scope, key, window_start)
        DO UPDATE SET count = auth_rate_counters.count + 1
        RETURNING count AS "count!"
        "#,
        scope,
        key,
        window_start
    )
    .fetch_one(executor)
    .await?)
}

// ---------------------------------------------------------------------------
// The three composed orders.
// ---------------------------------------------------------------------------

/// Sign-up steps 1 and 2: the unbound INSERT with no `RETURNING`, then the
/// unbound lookup that reads the new id.
///
/// A duplicate address gives [`SignUp::EmailTaken`], because the route answers
/// the same generic body either way and must never tell a caller which addresses
/// are registered. The lookup runs only after an INSERT that wrote a row, so a
/// second sign-up for one address never opens a session on the first account.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when a statement fails, and [`StoreError::Auth`]
/// when the INSERT wrote a row that the lookup did not read back.
pub async fn sign_up(
    pool: &PgPool,
    email: &str,
    password_hash: Option<&str>,
) -> Result<SignUp, StoreError> {
    match insert_user(pool, email, password_hash).await {
        Ok(()) => {}
        Err(StoreError::Db(sqlx::Error::Database(err))) if err.is_unique_violation() => {
            return Ok(SignUp::EmailTaken);
        }
        Err(err) => return Err(err),
    }
    match user_by_email(pool, email).await? {
        Some(user) => Ok(SignUp::Created(user)),
        None => Err(StoreError::Auth(
            "the sign-up INSERT wrote a row that auth_user_by_email did not read back".to_string(),
        )),
    }
}

/// Login step 2 and step 3: bind the tenant, then write the session row.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the transaction does not start, the INSERT
/// fails, or the commit fails.
pub async fn start_session(
    pool: &PgPool,
    user_id: Uuid,
    session: &NewSession<'_>,
) -> Result<(), StoreError> {
    let mut tx = begin_tenant(pool, user_id).await?;
    insert_session(&mut *tx, user_id, session).await?;
    tx.commit().await?;
    Ok(())
}

/// Password reset and email verification, steps 2 and 3: bind the tenant, spend
/// the token, and apply the effect — in ONE transaction.
///
/// A zero row count on the token UPDATE means another request spent it first.
/// The function then rolls the transaction back and returns
/// [`TokenConsumed::AlreadySpent`], so the effect never lands twice.
///
/// The caller reads `purpose`, `consumed_at`, and `expires_at` with
/// [`token_by_hash`] before the bind. This function repeats the single-use check
/// alone, because that one is a race and the other two are not.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the transaction does not start, a statement
/// fails, or the commit fails.
pub async fn consume_token_tx(
    pool: &PgPool,
    user_id: Uuid,
    token_hash: &str,
    effect: TokenEffect<'_>,
) -> Result<TokenConsumed, StoreError> {
    consume_token_tx_inner(pool, user_id, token_hash, effect, false).await
}

/// [`consume_token_tx`] with a test seam: `break_backend` ends the backend
/// before the rollback so the rollback fails. Production passes `false`.
pub(crate) async fn consume_token_tx_inner(
    pool: &PgPool,
    user_id: Uuid,
    token_hash: &str,
    effect: TokenEffect<'_>,
    break_backend: bool,
) -> Result<TokenConsumed, StoreError> {
    let mut tx = begin_tenant(pool, user_id).await?;
    if consume_token(&mut *tx, token_hash).await? == TokenConsumed::AlreadySpent {
        if break_backend {
            let _ = sqlx::query("SELECT pg_terminate_backend(pg_backend_pid())")
                .execute(&mut *tx)
                .await;
        }
        tx.rollback().await?;
        return Ok(TokenConsumed::AlreadySpent);
    }
    match effect {
        TokenEffect::PasswordReset { password_hash } => {
            set_password_hash(&mut *tx, user_id, password_hash).await?;
        }
        TokenEffect::EmailVerify => {
            mark_email_verified(&mut *tx, user_id).await?;
        }
    }
    tx.commit().await?;
    Ok(TokenConsumed::Consumed)
}
#[cfg(test)]
mod tests;
