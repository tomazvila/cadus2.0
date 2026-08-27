//! The auth surface of `cadus_web`: the primitives of unit U2 and the
//! `/api/auth/*` routes of unit U4.
//!
//! Spec: `docs/reference/web-service-1.0-spec.md`, section 11, units U2 and U4,
//! and the literals of sections 3.1, 3.2, 3.3, and 10.
//!
//! The four PRIMITIVE modules are pure functions over their arguments. Nothing
//! there reads the environment, opens a connection, or serves a route:
//!
//! - [`password`] — the two Argon2id parameter profiles, the NIST 800-63B length
//!   policy, the hash, the verify that never raises, and the rehash decision;
//! - [`token`] — the 256-bit opaque session token, its SHA-256 storage form, and
//!   the constant-time compare;
//! - [`email`] — NFKC, then strip, then lower, and the answer is idempotent;
//! - [`session`] — the session-cookie writer, the clearer, the lifetimes of
//!   section 10, and the bearer-then-cookie selector.
//!
//! The one exception to "pure" is the operating-system entropy that
//! [`token::generate_token`] and [`password::hash_password`] draw. Both draw it
//! through `getrandom`, both bound the draw to one fixed size, and both give a
//! `Result` instead of a panic when the kernel refuses.
//!
//! The three ROUTE modules compose those primitives with `cadus_store::auth`, in
//! the section 3.3 call order:
//!
//! - [`rate`] — the four paired rate rules and the client-address key;
//! - [`guard`] — the session guard, the one seam where a request gets a
//!   `user_id`;
//! - [`routes`] — the ten handlers and the router that carries them;
//! - [`oauth`] — the OAuth mechanics of unit U5: the two providers, PKCE S256,
//!   the handshake record, and the transport PORT that keeps the outbound call
//!   out of this crate (R4);
//! - [`oauth_routes`] — the start route, the callback route, and the list the
//!   sign-in page reads.
//!
//! [`store_call`] below is the one place a route reaches the database, so the
//! query bound, the `500` mapping, and the "no store text in a body" rule each
//! have one definition.
//!
//! **Why the cookie writer lives here and the readers live in
//! [`crate::cookie`].** U1 needs the two readers for the CSRF layer, and the
//! layer ships before any route writes a cookie. The writer, the clearer, and
//! the selector are auth surface, so they live beside the token that they carry.
//! The posture — the one `(name, secure)` pair — has one definition, in
//! [`crate::cookie::CookiePosture`], and both sides read it from there.

pub mod email;
pub mod guard;
pub mod oauth;
pub mod oauth_routes;
pub mod password;
pub mod rate;
pub mod routes;
pub mod session;
pub mod token;

use std::future::Future;

use cadus_store::{Db, StoreError, bounded};

use crate::error::ApiError;

/// Run one store call under the client-side bound of `db`.
///
/// Every auth handler goes through this one function, so three rules hold in one
/// place (R4, L1):
///
/// - the call carries the `DB_CLIENT_TIMEOUT_MS` bound, so a database that
///   answers nothing ends the request instead of holding it open;
/// - a store error becomes `500 internal_error`, never a panic;
/// - the error TEXT reaches the log and never the body. A store message can
///   carry a token hash or an address, and `step` names the failing step alone.
///
/// # Errors
///
/// Returns `500 internal_error` when the call fails or passes the bound.
pub async fn store_call<T, F>(db: &Db, step: &'static str, call: F) -> Result<T, ApiError>
where
    F: Future<Output = Result<T, StoreError>>,
{
    // `bounded` takes a future that gives `Result<_, sqlx::Error>`, and a store
    // call gives `Result<_, StoreError>`. The wrapper puts the store answer
    // inside an `Ok`, and the match below takes it out again.
    let wrapped = async { Ok(call.await) };
    match bounded(db, wrapped).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(err)) => {
            tracing::error!(error = %err, step, "auth: a store call failed");
            Err(ApiError::internal(step))
        }
        Err(err) => {
            tracing::error!(error = %err, step, "auth: a store call passed its bound");
            Err(ApiError::internal(step))
        }
    }
}

pub use email::normalize_email;
pub use guard::{Authed, current_user};
pub use oauth::{
    Credentials, GITHUB, GOOGLE, Handshake, Identity, OAuthConfig, OAuthFailure, Provider,
    ProviderRequest, ProviderResponse, ProviderTransport, TransportError,
};
pub use password::{
    ARGON2_PROFILE_VAR, Argon2Profile, MAX_PASSWORD_BYTES, MAX_PASSWORD_LENGTH,
    MIN_PASSWORD_LENGTH, PasswordError, WeakPassword, hash_password, needs_rehash,
    validate_password, verify_password,
};
pub use rate::{RateRule, client_ip, enforce};
pub use routes::router;
pub use session::{
    CookieWriteError, HOST_PREFIX_PATH, LAST_SEEN_TOUCH_SECS, OAUTH_HANDSHAKE_TTL_SECS,
    RESET_TOKEN_TTL_SECS, SESSION_ABSOLUTE_SECS, SESSION_COOKIE_PATH, SESSION_IDLE_SECS,
    VERIFY_TOKEN_TTL_SECS, clear_auth_cookie, clear_session_cookie, read_session_credential,
    set_auth_cookie, set_session_cookie,
};
pub use token::{EntropyError, TOKEN_BYTES, TOKEN_CHARS, generate_token, hash_token, tokens_equal};
