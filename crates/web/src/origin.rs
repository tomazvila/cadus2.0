//! The CSRF origin layer, in both polarities.
//!
//! Spec `docs/reference/web-service-1.0-spec.md` section 3.1, rows "CSRF" and
//! "Same-origin test", and section 10, rows "CSRF" and "Login-CSRF". The layer
//! answers `403 cross_origin_rejected` to two shapes and to nothing else:
//!
//! 1. **The authed write.** A non-safe method on `/api/*`, with the session
//!    cookie, with no usable bearer token, that is not same-origin. The cookie
//!    is an ambient credential: a browser attaches it to a form POST from any
//!    page on the internet. A bearer token is not ambient, so a bearer write is
//!    exempt (D-M5-5). A request with neither credential carries nothing to
//!    abuse; it falls through to the auth path, which answers `401`.
//! 2. **The pre-auth write.** An explicitly cross-origin, non-bearer POST to
//!    `/api/auth/login`, `/api/auth/signup`, or `/api/auth/verify-email`. No
//!    session cookie exists yet, so rule 1 cannot see it. This is login-CSRF:
//!    the attacker forces a victim into an account the attacker owns.
//!    `verify-email` belongs on the list because it mints a session.
//!
//! **The two origin tests are deliberately not each other's negation** (trap
//! W11). [`is_same_origin`] fails **closed**: no signal at all is not
//! same-origin, because a live session cookie is at stake.
//! [`is_explicit_cross_origin`] fails **open**: it needs a positive cross-origin
//! signal, so a header-less programmatic client — curl, a test, a native app —
//! is never refused at a pre-auth route, where no ambient credential exists yet.
//! They share the `scheme://host` comparison and never a verdict.
//!
//! **Trap W10 and how 2.0 answers it.** 1.0 rebuilt its own origin from
//! `request.url.scheme` plus the `Host` header, and the scheme was right only
//! while `--proxy-headers` stayed in the compose command. Drop the flag and
//! every https request rebuilds as `http://`, so a genuine same-origin `Origin`
//! stops matching and every browser write starts failing. Nothing in 1.0
//! asserted the flag. In 2.0 the deployment states the origin instead:
//! `PUBLIC_ORIGIN=https://tutor.example` and the comparison depends on no proxy
//! header at all. With `PUBLIC_ORIGIN` unset, the layer falls back to
//! `X-Forwarded-Proto` (first value) or `http`, plus `Host`, which is the 1.0
//! behavior.

use std::ffi::OsString;

use axum::extract::{Request, State};
use axum::http::{HeaderMap, Method};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::AppState;
use crate::cookie::{CookiePosture, read_bearer_token, read_session_cookie};
use crate::error::ApiError;

/// The environment variable that pins the origin the deployment serves.
pub const PUBLIC_ORIGIN_VAR: &str = "PUBLIC_ORIGIN";

/// The methods that change no state, so the layer never reads them.
pub const SAFE_METHODS: [&str; 3] = ["GET", "HEAD", "OPTIONS"];

/// The pre-auth credential routes of rule 2.
pub const PRE_AUTH_CSRF_PATHS: [&str; 3] = [
    "/api/auth/login",
    "/api/auth/signup",
    "/api/auth/verify-email",
];

/// The path prefix that rule 1 covers.
pub const API_PREFIX: &str = "/api/";

/// How the layer names this deployment's own origin.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OriginPolicy {
    /// The origin the operator pinned, as `scheme://host`. `None` falls back to
    /// `X-Forwarded-Proto` and `Host`.
    pub public_origin: Option<String>,
}

/// The reason that [`OriginPolicy::from_env`] refuses a value of
/// `PUBLIC_ORIGIN`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OriginPolicyError {
    /// The value is not valid Unicode.
    NotUnicode,
    /// The value is not exactly `scheme://host`.
    NotAnOrigin {
        /// The value, as the operator wrote it.
        value: String,
    },
}

impl std::fmt::Display for OriginPolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotUnicode => write!(f, "{PUBLIC_ORIGIN_VAR} is not valid Unicode"),
            Self::NotAnOrigin { value } => write!(
                f,
                "{PUBLIC_ORIGIN_VAR} {value:?} is not an origin; write scheme://host with no path \
                 and no trailing slash, such as https://tutor.example"
            ),
        }
    }
}

impl std::error::Error for OriginPolicyError {}

impl OriginPolicy {
    /// Read the policy from the raw value of `PUBLIC_ORIGIN`.
    ///
    /// The function is pure. The binary calls it with `std::env::var_os`, and a
    /// test drives every branch with no environment.
    ///
    /// An absent or empty value gives the fallback policy. Any other value must
    /// be exactly `scheme://host`: a path, a query, or a trailing slash never
    /// matches an `Origin` header, so the layer would refuse every browser
    /// write. That is a start error, never a silent fallback.
    pub fn from_env(raw: Option<OsString>) -> Result<Self, OriginPolicyError> {
        let Some(raw) = raw else {
            return Ok(Self::default());
        };
        let value = raw
            .into_string()
            .map_err(|_| OriginPolicyError::NotUnicode)?;
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Ok(Self::default());
        }
        if !is_origin(trimmed) {
            return Err(OriginPolicyError::NotAnOrigin {
                value: trimmed.to_string(),
            });
        }
        Ok(Self {
            public_origin: Some(trimmed.to_string()),
        })
    }
}

/// Whether `value` is exactly `scheme://host`.
///
/// The scheme is one or more ASCII letters. The host is non-empty and carries
/// no `/`, so a path, a query, and a trailing slash are all refused.
fn is_origin(value: &str) -> bool {
    let Some((scheme, host)) = value.split_once("://") else {
        return false;
    };
    !scheme.is_empty()
        && scheme.chars().all(|c| c.is_ascii_alphabetic())
        && !host.is_empty()
        && !host.contains('/')
        && !host.contains('?')
        && !host.contains('#')
}

/// The origin this deployment serves, as the layer compares it.
///
/// With `PUBLIC_ORIGIN` set, that value. Without it, the scheme of
/// `X-Forwarded-Proto` (or `http`) and the `Host` header. `None` means the
/// request carries no `Host` header, and then no `Origin` can match.
pub fn own_origin(policy: &OriginPolicy, headers: &HeaderMap) -> Option<String> {
    if let Some(pinned) = &policy.public_origin {
        return Some(pinned.clone());
    }
    let host = headers.get("host")?.to_str().ok()?.trim();
    if host.is_empty() {
        return None;
    }
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("http");
    Some(format!("{scheme}://{host}"))
}

/// Whether `Origin` is exactly this deployment's own origin.
///
/// The one comparison. Both predicates below read it, in opposite polarity, so
/// a change to how the origin is built can never make the two policies disagree.
fn origin_is_own(policy: &OriginPolicy, headers: &HeaderMap, origin: &str) -> bool {
    own_origin(policy, headers).is_some_and(|own| own == origin)
}

/// Whether the request says it is same-origin. **Fails closed.**
///
/// `Sec-Fetch-Site: same-origin` or `none` is the browser's own signal. An
/// `Origin` that matches this deployment's origin is the fallback. Neither
/// signal means "not same-origin": a cross-site forgery carries neither.
pub fn is_same_origin(policy: &OriginPolicy, headers: &HeaderMap) -> bool {
    if let Some(site) = headers.get("sec-fetch-site").and_then(|v| v.to_str().ok())
        && (site == "same-origin" || site == "none")
    {
        return true;
    }
    match headers.get("origin").and_then(|v| v.to_str().ok()) {
        Some(origin) => origin_is_own(policy, headers, origin),
        None => false,
    }
}

/// Whether the request carries a positive cross-origin signal. **Fails open.**
///
/// `Sec-Fetch-Site: cross-site` or `same-site`, or an `Origin` that does not
/// match. A request with neither signal is not flagged, so a header-less curl
/// POST to a pre-auth route goes through (spec section 11, unit U1).
pub fn is_explicit_cross_origin(policy: &OriginPolicy, headers: &HeaderMap) -> bool {
    if let Some(site) = headers.get("sec-fetch-site").and_then(|v| v.to_str().ok())
        && (site == "cross-site" || site == "same-site")
    {
        return true;
    }
    match headers.get("origin").and_then(|v| v.to_str().ok()) {
        Some(origin) => !origin_is_own(policy, headers, origin),
        None => false,
    }
}

/// What the CSRF origin layer does with one request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CsrfVerdict {
    /// Hand the request to the next layer.
    Allow,
    /// Answer `403 cross_origin_rejected` and call no handler.
    Reject,
}

/// The whole decision of the CSRF origin layer, as a pure function.
///
/// The layer itself only calls this and builds the answer, so every rule above
/// is testable with no socket, no router, and no database.
pub fn csrf_verdict(
    policy: &OriginPolicy,
    posture: CookiePosture,
    method: &Method,
    path: &str,
    headers: &HeaderMap,
) -> CsrfVerdict {
    if SAFE_METHODS.contains(&method.as_str()) {
        return CsrfVerdict::Allow;
    }
    // A bearer write cannot be forged cross-site, so it is exempt under both
    // rules. `read_bearer_token` is the same reader that selects the credential,
    // so "exempt" means exactly "will authenticate by bearer".
    if read_bearer_token(headers).is_some() {
        return CsrfVerdict::Allow;
    }
    // Rule 1: the authed write. Fails closed on no origin signal.
    if path.starts_with(API_PREFIX)
        && read_session_cookie(headers, posture.name).is_some()
        && !is_same_origin(policy, headers)
    {
        return CsrfVerdict::Reject;
    }
    // Rule 2: the pre-auth write. Fails open on no origin signal.
    if PRE_AUTH_CSRF_PATHS.contains(&path) && is_explicit_cross_origin(policy, headers) {
        return CsrfVerdict::Reject;
    }
    CsrfVerdict::Allow
}

/// The CSRF origin layer.
///
/// It reads [`csrf_verdict`] and builds the answer. Every rule lives in that
/// pure function, so a test drives the whole policy with no socket and no
/// database, and this wrapper adds no rule of its own.
///
/// The layer sits INSIDE the security-header layer, so the `403` it answers
/// carries the section 3.1 headers, and INSIDE the request-metrics layer, so
/// that `403` is counted.
pub async fn csrf_origin_layer(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let verdict = csrf_verdict(
        &state.origin,
        state.posture,
        request.method(),
        request.uri().path(),
        request.headers(),
    );
    match verdict {
        CsrfVerdict::Reject => ApiError::cross_origin_rejected().into_response(),
        CsrfVerdict::Allow => next.run(request).await,
    }
}

#[cfg(test)]
mod origin_tests {
    use super::*;

    /// The not-Unicode error names the variable.
    #[test]
    fn the_not_unicode_error_names_the_variable() {
        assert_eq!(
            OriginPolicyError::NotUnicode.to_string(),
            format!("{PUBLIC_ORIGIN_VAR} is not valid Unicode")
        );
    }

    /// An empty value gives the fallback policy, and a value that is not valid
    /// Unicode is a start error.
    #[test]
    fn from_env_handles_the_empty_and_the_non_unicode_value() {
        use std::ffi::OsString;

        assert_eq!(
            OriginPolicy::from_env(Some(OsString::from("   "))),
            Ok(OriginPolicy::default())
        );

        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            let raw = OsString::from_vec(vec![0x68, 0xff]);
            assert_eq!(
                OriginPolicy::from_env(Some(raw)),
                Err(OriginPolicyError::NotUnicode)
            );
        }
    }

    /// The own origin needs a `Host` header, and a blank one gives none.
    #[test]
    fn the_own_origin_needs_a_host_header() {
        use axum::http::{HeaderMap, HeaderValue};

        let policy = OriginPolicy::default();
        assert_eq!(own_origin(&policy, &HeaderMap::new()), None);

        let mut blank = HeaderMap::new();
        blank.insert("host", HeaderValue::from_static("   "));
        assert_eq!(own_origin(&policy, &blank), None);
    }
}
