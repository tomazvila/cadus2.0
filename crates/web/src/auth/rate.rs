//! The four paired rate rules of the credential routes.
//!
//! Spec `docs/reference/web-service-1.0-spec.md` section 3.2 and section 10, row
//! "Rate limit" (1.0 `cadus_web/authn/routes.py:65-104`, `:209-268`).
//!
//! **One rule is four numbers, not two pairs of loose constants.** Every public
//! credential route bounds the normalized address first and the client address
//! second, in ONE shared fixed window. [`RateRule`] binds the four numbers into
//! one value, so a call site cannot wire one key alone, cannot invert the order,
//! and cannot give the two keys different windows. In 1.0 each of those three
//! was a live enumeration or inbox-blast defect while the eight numbers were
//! matched up by hand at four call sites.
//!
//! **Both counters run BEFORE any account lookup.** The keys are the normalized
//! address and the client address, and nothing else. No branch here reads
//! account state, so the `429` of a registered address is byte for byte the
//! `429` of an unregistered one.
//!
//! **The two counters are independent.** They key different scopes, so three
//! sign-ups of one address burn that address's budget and leave the per-address
//! budget of the host with room. The per-address refusal returns at once and
//! does NOT increment the host counter, which is what makes the 1.0 sequence
//! hold: the 4th call for one address and the 6th call from one host are the
//! first two refused.
//!
//! **The `(limit + 1)`-th call of a window is the first refused.**
//! `bump_rate_counter` returns the count AFTER the increment, so the test is
//! `count > limit`.

use axum::http::HeaderMap;
use cadus_store::Db;
use cadus_store::auth::{bump_rate_counter, window_start};
use sqlx::types::chrono::{DateTime, Utc};

use crate::auth::store_call;
use crate::error::ApiError;

/// The rate-limit key of a request with no client address.
///
/// `axum::serve` gives the peer address through `ConnectInfo`, and the
/// deployment of `deploy/Caddyfile` overwrites `X-Forwarded-For` with the real
/// remote address. A request that carries neither still needs a well-defined
/// bucket, so it gets this one (1.0 `routes.py:177-182`).
pub const UNKNOWN_CLIENT_IP: &str = "unknown";

/// The header that the ingress proxy writes with the real client address.
pub const FORWARDED_FOR: &str = "x-forwarded-for";

/// One route's paired per-address and per-host rate rule.
///
/// `prefix` derives both counter scopes and is therefore part of the persisted
/// `auth_rate_counters` key. A change to one of these strings resets that rule's
/// live windows, so treat them as stored data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateRule {
    /// The scope prefix, and the stored key.
    pub prefix: &'static str,
    /// How many calls one normalized address gets in a window.
    pub per_email: i32,
    /// How many calls one client address gets in a window.
    pub per_ip: i32,
    /// The width of the fixed window, in seconds.
    pub window_secs: u32,
}

impl RateRule {
    /// `POST /api/auth/signup`: 3 per address, 5 per host, one hour.
    pub const SIGNUP: Self = Self {
        prefix: "signup",
        per_email: 3,
        per_ip: 5,
        window_secs: 60 * 60,
    };

    /// `POST /api/auth/login`: 10 per address, 30 per host, five minutes.
    pub const LOGIN: Self = Self {
        prefix: "login",
        per_email: 10,
        per_ip: 30,
        window_secs: 5 * 60,
    };

    /// `POST /api/auth/password/forgot`: 3 per address, 10 per host, one hour.
    pub const FORGOT: Self = Self {
        prefix: "forgot",
        per_email: 3,
        per_ip: 10,
        window_secs: 60 * 60,
    };

    /// `POST /api/auth/verify-email/resend`: 3 per address, 10 per host, one
    /// hour.
    pub const VERIFY_RESEND: Self = Self {
        prefix: "verify_resend",
        per_email: 3,
        per_ip: 10,
        window_secs: 60 * 60,
    };

    /// The four rules, in the order of the specification table.
    pub const ALL: [Self; 4] = [Self::SIGNUP, Self::LOGIN, Self::FORGOT, Self::VERIFY_RESEND];

    /// The counter scope of the address key.
    #[must_use]
    pub fn email_scope(self) -> String {
        format!("{}_email", self.prefix)
    }

    /// The counter scope of the host key.
    #[must_use]
    pub fn ip_scope(self) -> String {
        format!("{}_ip", self.prefix)
    }
}

/// The client address of this request, as the rate rules key it.
///
/// `X-Forwarded-For` wins, and the FIRST value of it is the one this deployment
/// trusts: `deploy/Caddyfile` writes `header_up X-Forwarded-For
/// {http.request.remote.host}`, which OVERWRITES whatever the client sent. A
/// deployment that puts another proxy in front, or that lets a client-written
/// header through, hands the caller its own bucket, so the ingress must keep
/// that overwrite.
///
/// With no header, the peer address of the socket serves. With neither, the key
/// is [`UNKNOWN_CLIENT_IP`].
#[must_use]
pub fn client_ip(headers: &HeaderMap, peer: Option<std::net::SocketAddr>) -> String {
    if let Some(raw) = headers
        .get(FORWARDED_FOR)
        .and_then(|value| value.to_str().ok())
    {
        let first = raw.split(',').next().unwrap_or("").trim();
        if !first.is_empty() {
            return first.to_string();
        }
    }
    match peer {
        Some(address) => address.ip().to_string(),
        None => UNKNOWN_CLIENT_IP.to_string(),
    }
}

/// Increment one counter and refuse the call that passes `limit`.
///
/// The answer of `bump_rate_counter` is the count AFTER this call, so the
/// `(limit + 1)`-th call of a window is the first refused.
async fn bump(
    db: &Db,
    scope: &str,
    key: &str,
    start: DateTime<Utc>,
    limit: i32,
) -> Result<(), ApiError> {
    let count = store_call(
        db,
        "rate counter",
        bump_rate_counter(db.pool(), scope, key, start),
    )
    .await?;
    if count > limit {
        return Err(ApiError::rate_limited());
    }
    Ok(())
}

/// Apply one paired rule: the address first, then the host.
///
/// Call this BEFORE any account lookup. `email_key` is the normalized address
/// and `ip` is [`client_ip`]. The address refusal returns at once, so the host
/// counter does not move on a call that the address rule already refused.
///
/// # Errors
///
/// Returns `429 rate_limited` when either counter passes its ceiling, and
/// `500 internal_error` when the counter statement fails.
pub async fn enforce(
    db: &Db,
    rule: RateRule,
    email_key: &str,
    ip: &str,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    let start = window_start(now, rule.window_secs);
    bump(db, &rule.email_scope(), email_key, start, rule.per_email).await?;
    bump(db, &rule.ip_scope(), ip, start, rule.per_ip).await
}
