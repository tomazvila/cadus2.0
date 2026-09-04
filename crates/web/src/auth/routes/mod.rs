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

use axum::Router;
use axum::routing::{get, post};

use crate::AppState;

mod account;
mod public;
mod recovery;
mod support;

pub use account::{change_password, login, logout, logout_all, me, signup};
pub use public::{ClientAddr, Public};
pub use recovery::{forgot_password, resend_verification, reset_password, verify_email};
pub(crate) use support::{
    bind, commit, cookie_failed, new_session_row, new_token, session_window, user_agent,
};

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
