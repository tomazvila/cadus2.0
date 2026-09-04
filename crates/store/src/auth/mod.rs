//! The auth statements of the M5 call order (`docs/SCHEMA.md`, "The M5 auth
//! contract"; specification section 3.3).
//!
//! Requirements: C3 (row-level security and the tenant bind), R2 (compile-time
//! checked queries).
//!
//! # Unbound first, bound after
//!
//! An auth path starts with no tenant: it reads a key — an email address, a
//! token hash, a provider account id — to learn which `user_id` to bind. Every
//! policy of `0006_grants_rls.sql` gives an unbound caller zero rows, so five
//! SECURITY DEFINER functions serve those reads. The guard inside each body
//! tests the CALLER, not the argument, so a bound caller gets zero rows from all
//! five. The functions of this module therefore split in four groups:
//!
//! | Group | Executor | Functions |
//! |---|---|---|
//! | unbound lookups | a pool with no tenant | [`user_by_email`], [`user_by_id`], [`session_by_token_hash`], [`token_by_hash`], [`oauth_account_user`] |
//! | unbound write | a pool with no tenant | [`insert_user`] |
//! | bound writes | a [`begin_tenant`] transaction | [`insert_session`], [`touch_last_seen`], [`delete_session`], [`delete_all_sessions`], [`delete_other_sessions`], [`insert_token`], [`delete_tokens_for_purpose`], [`consume_token`], [`set_password_hash`], [`clear_password_hash`], [`mark_email_verified`], [`account_profile`], [`insert_oauth_account`] |
//! | either | any executor | [`bump_rate_counter`] |
//!
//! [`sign_up`], [`start_session`], and [`consume_token_tx`] compose the three
//! orders that a caller must not reorder, so a route of M5 U4 and U5 calls one
//! function and gets the order of the contract.
//!
//! # The email arrives normalized
//!
//! `users.email` is `citext`, so the match is case-folded, and the caller
//! normalizes the address (NFKC, trim, lowercase) with
//! `cadus_web::auth::email::normalize_email` first. This module casts the bind
//! to `citext` and does nothing else to it.
//!
//! # No panic
//!
//! Every function returns [`StoreError`]. A duplicate address is
//! [`SignUp::EmailTaken`] and never an error that the caller must decode.

mod writes;

use sqlx::PgExecutor;
use sqlx::types::chrono::{DateTime, Utc};
use uuid::Uuid;

pub use writes::{
    account_profile, bump_rate_counter, clear_password_hash, consume_token, consume_token_tx,
    delete_all_sessions, delete_other_sessions, delete_session, delete_tokens_for_purpose,
    insert_oauth_account, insert_session, insert_token, mark_email_verified, set_password_hash,
    sign_up, start_session, touch_last_seen, window_start,
};

use crate::StoreError;

/// The `auth_tokens.purpose` value of a password-reset token (1.0 parity,
/// `cadus_web/authn/models.py:35`).
pub const PURPOSE_RESET: &str = "reset";

/// The `auth_tokens.purpose` value of an email-verification token (1.0 parity,
/// `cadus_web/authn/models.py:35`).
pub const PURPOSE_VERIFY: &str = "verify";

/// One account, as the two user lookups return it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthUser {
    /// The primary key. It becomes the bound tenant.
    pub id: Uuid,
    /// `None` for an OAuth-only account.
    pub password_hash: Option<String>,
    /// `None` until the address is verified.
    pub email_verified_at: Option<DateTime<Utc>>,
    /// A non-`None` value disables the account.
    pub disabled_at: Option<DateTime<Utc>>,
    /// The admin flag. The runtime role never writes it.
    pub is_admin: bool,
}

/// One session row, as [`session_by_token_hash`] returns it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRow {
    /// The account the cookie belongs to.
    pub user_id: Uuid,
    /// The start of the absolute window. The mint stamps it and nothing slides
    /// it, so the caller tests the 90-day ceiling against this value before the
    /// tenant bind (migration 0008).
    pub created_at: DateTime<Utc>,
    /// The end of the idle window.
    pub expires_at: DateTime<Utc>,
    /// The last touch. The caller touches it at most once per hour.
    pub last_seen_at: DateTime<Utc>,
}

/// The account view that a BOUND caller reads with a plain SELECT.
///
/// `AuthUser` carries what the pre-tenant refusals need. This row carries what
/// the profile answer needs, and the M5 call order gives it plainly: "after the
/// bind a plain SELECT on `users` reads the caller's own rows"
/// (`docs/SCHEMA.md`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountProfile {
    /// The primary key, and the bound tenant.
    pub id: Uuid,
    /// The stored address, as `citext` folded it.
    pub email: String,
    /// `None` until the address is verified.
    pub email_verified_at: Option<DateTime<Utc>>,
    /// When the account was created.
    pub created_at: DateTime<Utc>,
}

/// One out-of-band token row, as [`token_by_hash`] returns it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenRow {
    /// The account the token belongs to.
    pub user_id: Uuid,
    /// [`PURPOSE_RESET`] or [`PURPOSE_VERIFY`].
    pub purpose: String,
    /// The end of the token window.
    pub expires_at: DateTime<Utc>,
    /// A non-`None` value marks the token spent. A token is single use.
    pub consumed_at: Option<DateTime<Utc>>,
}

/// The row that [`insert_session`] writes.
///
/// The caller holds the raw token and stores the SHA-256 hex of it, so the
/// database never carries a live token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSession<'a> {
    /// SHA-256 of the session token, 64 lowercase hex characters.
    pub token_hash: &'a str,
    /// The start of the absolute window. The caller never slides it.
    pub created_at: DateTime<Utc>,
    /// The first touch.
    pub last_seen_at: DateTime<Utc>,
    /// The end of the idle window.
    pub expires_at: DateTime<Utc>,
    /// The client address, for the session list.
    pub ip: Option<&'a str>,
    /// The client user agent, for the session list.
    pub user_agent: Option<&'a str>,
}

/// The outcome of [`sign_up`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignUp {
    /// The INSERT wrote the row and the lookup read it back.
    Created(AuthUser),
    /// The address already has an account. The route answers the generic
    /// `{"status": "verification_required"}` and opens no session, so it never
    /// tells a caller which addresses are registered.
    EmailTaken,
}

/// The outcome of [`consume_token`] and [`consume_token_tx`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenConsumed {
    /// This transaction spent the token.
    Consumed,
    /// Another request spent it first. The caller rolls back.
    AlreadySpent,
}

/// The write that follows a consumed token in the same transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenEffect<'a> {
    /// A password reset writes the new PHC string.
    PasswordReset {
        /// The new `users.password_hash`.
        password_hash: &'a str,
    },
    /// An email verification stamps `users.email_verified_at`.
    EmailVerify,
}

// ---------------------------------------------------------------------------
// The five pre-tenant lookups. The caller is UNBOUND.
// ---------------------------------------------------------------------------

/// Read one account by its normalized email (login step 1, sign-up step 2, the
/// OAuth link by email).
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn user_by_email<'e, E>(executor: E, email: &str) -> Result<Option<AuthUser>, StoreError>
where
    E: PgExecutor<'e>,
{
    Ok(sqlx::query_as!(
        AuthUser,
        r#"
        SELECT id AS "id!", password_hash, email_verified_at, disabled_at,
               is_admin AS "is_admin!"
        FROM auth_user_by_email($1::text::citext)
        "#,
        email
    )
    .fetch_optional(executor)
    .await?)
}

/// Read one account by its id (the account status behind a session cookie).
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn user_by_id<'e, E>(executor: E, id: Uuid) -> Result<Option<AuthUser>, StoreError>
where
    E: PgExecutor<'e>,
{
    Ok(sqlx::query_as!(
        AuthUser,
        r#"
        SELECT id AS "id!", password_hash, email_verified_at, disabled_at,
               is_admin AS "is_admin!"
        FROM auth_user_by_id($1)
        "#,
        id
    )
    .fetch_optional(executor)
    .await?)
}

/// Read one session by the SHA-256 hex of its cookie (guarded request step 1).
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn session_by_token_hash<'e, E>(
    executor: E,
    token_hash: &str,
) -> Result<Option<SessionRow>, StoreError>
where
    E: PgExecutor<'e>,
{
    Ok(sqlx::query_as!(
        SessionRow,
        r#"
        SELECT user_id AS "user_id!", created_at AS "created_at!",
               expires_at AS "expires_at!", last_seen_at AS "last_seen_at!"
        FROM auth_session_by_token_hash($1)
        "#,
        token_hash
    )
    .fetch_optional(executor)
    .await?)
}

/// Read one out-of-band token by the SHA-256 hex of it (password reset and
/// email verification, step 1).
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn token_by_hash<'e, E>(
    executor: E,
    token_hash: &str,
) -> Result<Option<TokenRow>, StoreError>
where
    E: PgExecutor<'e>,
{
    Ok(sqlx::query_as!(
        TokenRow,
        r#"
        SELECT user_id AS "user_id!", purpose AS "purpose!",
               expires_at AS "expires_at!", consumed_at
        FROM auth_token_by_hash($1)
        "#,
        token_hash
    )
    .fetch_optional(executor)
    .await?)
}

/// Read the account a provider account links to (the OAuth callback, step 1).
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn oauth_account_user<'e, E>(
    executor: E,
    provider: &str,
    provider_account_id: &str,
) -> Result<Option<Uuid>, StoreError>
where
    E: PgExecutor<'e>,
{
    Ok(sqlx::query_scalar!(
        r#"SELECT user_id AS "user_id!" FROM oauth_account_lookup($1, $2)"#,
        provider,
        provider_account_id
    )
    .fetch_optional(executor)
    .await?)
}

// ---------------------------------------------------------------------------
// The sign-up INSERT. The caller is UNBOUND.
// ---------------------------------------------------------------------------

/// Insert one account, with NO `RETURNING` clause (sign-up step 1).
///
/// Postgres applies the SELECT policy of `users` to the returned row, and an
/// unbound session sees no row, so a `RETURNING` clause fails with SQLSTATE
/// 42501. The statement names neither `id` nor `is_admin`: the column grant of
/// `cadus_app` holds neither, and both come from their defaults. The caller
/// reads the new id with [`user_by_email`] next.
///
/// `password_hash` is `None` for an OAuth-only account.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails, a duplicate address
/// (SQLSTATE 23505) included. [`sign_up`] maps that one case to
/// [`SignUp::EmailTaken`].
pub async fn insert_user<'e, E>(
    executor: E,
    email: &str,
    password_hash: Option<&str>,
) -> Result<(), StoreError>
where
    E: PgExecutor<'e>,
{
    sqlx::query!(
        "INSERT INTO users (email, password_hash) VALUES ($1::text::citext, $2)",
        email,
        password_hash
    )
    .execute(executor)
    .await?;
    Ok(())
}
