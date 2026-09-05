//! The session guard: the one seam where a request acquires an identity.
//!
//! Spec `docs/reference/web-service-1.0-spec.md` section 3.3, "Cookie check",
//! and section 10, row "Lifetimes" (1.0 `cadus_web/authn/deps.py:72-128`).
//!
//! `user_id` starts HERE and nowhere else. No handler reads it from a body, a
//! path, or a query, so a missed tenant bind downstream is a compile error and
//! never a silent cross-tenant read (C3).
//!
//! The order is the binding one, and the first three steps are UNBOUND:
//!
//! 1. select the credential, bearer before cookie, and hash it;
//! 2. `auth_session_by_token_hash` — refuse no row, `expires_at <= now`, and an
//!    age of 90 days or more (migration 0008 added `created_at` to the answer
//!    for this test);
//! 3. `auth_user_by_id` — refuse no row and a non-`None` `disabled_at`;
//! 4. bind the tenant and touch `last_seen_at`, at most once per hour.
//!
//! **Every refusal is the same `401 unauthorized`.** A caller must not learn
//! whether a session is absent, stale, past its ceiling, or attached to a
//! disabled account.

use axum::http::HeaderMap;
use cadus_store::auth::{AuthUser, SessionRow, session_by_token_hash, touch_last_seen, user_by_id};
use cadus_store::{Db, begin_tenant};
use sqlx::types::chrono::{DateTime, Utc};

use crate::AppState;
use crate::auth::session::{LAST_SEEN_TOUCH_SECS, SESSION_ABSOLUTE_SECS, read_session_credential};
use crate::auth::store_call;
use crate::auth::token::hash_token;
use crate::error::ApiError;

/// The message of every session refusal.
///
/// One string for four causes. A caller that can tell "no such session" from
/// "expired" learns which stolen cookies are still live.
pub const NO_SESSION_MESSAGE: &str = "Invalid or expired session.";

/// The message of a request that carries no credential at all.
pub const NO_CREDENTIAL_MESSAGE: &str = "Missing session cookie or bearer token.";

/// The identity of one request. It never carries the raw token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Authed {
    /// The account behind the presented session.
    pub user: AuthUser,
    /// SHA-256 of the presented token. The sign-out and the "keep this session"
    /// sweep both key on it.
    pub token_hash: String,
}

/// Add `secs` to `now`, or `None` when the sum leaves the calendar.
///
/// The arithmetic runs on whole seconds and keeps the sub-second part of `now`.
/// `sqlx::types::chrono` re-exports no duration type, so the sum goes through
/// the epoch second and not through a `TimeDelta`.
#[must_use]
pub fn plus_secs(now: DateTime<Utc>, secs: u64) -> Option<DateTime<Utc>> {
    let secs = i64::try_from(secs).ok()?;
    let stamp = now.timestamp().checked_add(secs)?;
    DateTime::from_timestamp(stamp, now.timestamp_subsec_nanos())
}

/// Whether `earlier` is `secs` or more behind `now`.
///
/// Both windows this serves are hours and days wide, so a resolution of one
/// second is enough.
fn at_least_old(now: DateTime<Utc>, earlier: DateTime<Utc>, secs: u64) -> bool {
    let Ok(secs) = i64::try_from(secs) else {
        return false;
    };
    now.timestamp().saturating_sub(earlier.timestamp()) >= secs
}

/// Whether the session that `session` describes is still usable at `now`.
///
/// The two windows are the section 10 literals: an idle window that ends at
/// `expires_at`, and an absolute ceiling of 90 days from `created_at` that never
/// slides. `now == expires_at` counts as expired, which is the 1.0 rule.
#[must_use]
pub fn session_is_live(session: &SessionRow, now: DateTime<Utc>) -> bool {
    now < session.expires_at && !at_least_old(now, session.created_at, SESSION_ABSOLUTE_SECS)
}

/// Whether `last_seen_at` is due for its next write.
///
/// The floor is one hour, and the test is strictly greater, so a busy hour costs
/// one write and not one per request.
#[must_use]
pub fn touch_is_due(session: &SessionRow, now: DateTime<Utc>) -> bool {
    now > session.last_seen_at + std::time::Duration::from_secs(LAST_SEEN_TOUCH_SECS)
}

/// Resolve the identity of this request, or answer `401 unauthorized`.
///
/// # Errors
///
/// Returns `401 unauthorized` for every refusal of the four steps above, and
/// `500 internal_error` when a statement fails.
pub async fn current_user(state: &AppState, headers: &HeaderMap) -> Result<Authed, ApiError> {
    let db: &Db = &state.db;
    let token = read_session_credential(headers, state.posture.name)
        .ok_or_else(|| ApiError::unauthorized(NO_CREDENTIAL_MESSAGE))?;
    let token_hash = hash_token(token);

    let session = store_call(
        db,
        "session lookup",
        session_by_token_hash(db.pool(), &token_hash),
    )
    .await?
    .ok_or_else(|| ApiError::unauthorized(NO_SESSION_MESSAGE))?;

    let now = Utc::now();
    if !session_is_live(&session, now) {
        return Err(ApiError::unauthorized(NO_SESSION_MESSAGE));
    }

    let user = store_call(db, "account lookup", user_by_id(db.pool(), session.user_id))
        .await?
        .ok_or_else(|| ApiError::unauthorized(NO_SESSION_MESSAGE))?;
    if user.disabled_at.is_some() {
        return Err(ApiError::unauthorized(NO_SESSION_MESSAGE));
    }

    if touch_is_due(&session, now) {
        let mut tx = store_call(db, "tenant bind", begin_tenant(db.pool(), user.id)).await?;
        store_call(db, "session touch", touch_last_seen(&mut *tx, &token_hash)).await?;
        store_call(db, "session touch", async { Ok(tx.commit().await?) }).await?;
    }

    Ok(Authed { user, token_hash })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `plus_secs` refuses an offset that does not fit an `i64`, and one that
    /// overflows the epoch second of `now`.
    #[test]
    fn plus_secs_refuses_an_offset_that_overflows() {
        let now = DateTime::from_timestamp(1_700_000_000, 0).expect("a valid instant");
        assert_eq!(plus_secs(now, u64::MAX), None);
        assert_eq!(plus_secs(now, i64::MAX as u64), None);
        assert!(plus_secs(now, 60).is_some());
    }

    /// `at_least_old` answers false for an offset that does not fit an `i64`,
    /// and reads a real gap otherwise.
    #[test]
    fn at_least_old_refuses_an_offset_that_does_not_fit() {
        let now = DateTime::from_timestamp(1_700_000_000, 0).expect("a valid instant");
        let earlier = DateTime::from_timestamp(1_699_000_000, 0).expect("a valid instant");
        assert!(!at_least_old(now, earlier, u64::MAX));
        assert!(at_least_old(now, earlier, 60));
        assert!(!at_least_old(now, earlier, 2_000_000));
    }
}
