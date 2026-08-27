//! The auth primitives of `cadus_web`: password hashing, the password policy,
//! opaque tokens, email normalization, and the session-cookie writer.
//!
//! Spec: `docs/reference/web-service-1.0-spec.md`, section 11, unit U2, and the
//! literals of sections 3.1 and 10. Every item here is a pure function over its
//! arguments. Nothing reads the environment, nothing opens a connection, and
//! nothing serves a route. Units U3 to U5 compose these into the auth store and
//! the `/api/auth/*` handlers, in the section 3.3 call order.
//!
//! The one exception to "pure" is the operating-system entropy that
//! [`token::generate_token`] and [`password::hash_password`] draw. Both draw it
//! through `getrandom`, both bound the draw to one fixed size, and both give a
//! `Result` instead of a panic when the kernel refuses.
//!
//! The four groups:
//!
//! - [`password`] — the two Argon2id parameter profiles, the NIST 800-63B length
//!   policy, the hash, the verify that never raises, and the rehash decision;
//! - [`token`] — the 256-bit opaque session token, its SHA-256 storage form, and
//!   the constant-time compare;
//! - [`email`] — NFKC, then strip, then lower, and the answer is idempotent;
//! - [`session`] — the session-cookie writer, the clearer, the lifetimes of
//!   section 10, and the bearer-then-cookie selector.
//!
//! **Why the cookie writer lives here and the readers live in
//! [`crate::cookie`].** U1 needs the two readers for the CSRF layer, and the
//! layer ships before any route writes a cookie. The writer, the clearer, and
//! the selector are auth surface, so they live beside the token that they carry.
//! The posture — the one `(name, secure)` pair — has one definition, in
//! [`crate::cookie::CookiePosture`], and both sides read it from there.

pub mod email;
pub mod password;
pub mod session;
pub mod token;

pub use email::normalize_email;
pub use password::{
    ARGON2_PROFILE_VAR, Argon2Profile, MAX_PASSWORD_BYTES, MAX_PASSWORD_LENGTH,
    MIN_PASSWORD_LENGTH, PasswordError, WeakPassword, hash_password, needs_rehash,
    validate_password, verify_password,
};
pub use session::{
    CookieWriteError, HOST_PREFIX_PATH, LAST_SEEN_TOUCH_SECS, OAUTH_HANDSHAKE_TTL_SECS,
    RESET_TOKEN_TTL_SECS, SESSION_ABSOLUTE_SECS, SESSION_COOKIE_PATH, SESSION_IDLE_SECS,
    VERIFY_TOKEN_TTL_SECS, clear_auth_cookie, clear_session_cookie, read_session_credential,
    set_auth_cookie, set_session_cookie,
};
pub use token::{EntropyError, TOKEN_BYTES, TOKEN_CHARS, generate_token, hash_token, tokens_equal};
