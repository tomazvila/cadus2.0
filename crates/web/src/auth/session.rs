//! The session-cookie writer, the clearer, the lifetimes, and the
//! bearer-then-cookie selector.
//!
//! Spec `docs/reference/web-service-1.0-spec.md` section 3.1, rows "Cookie",
//! "Guards", "Second channel", and "Idle / absolute window"; section 10, rows
//! "Cookie string" and "Lifetimes" (1.0 `cookies.py:62-84`, `:101-113`,
//! `:153-186`, `oauth.py:68`, `service.py:43`, `:48`, `:375`, `:436`).
//!
//! **One writer for every auth cookie.** [`set_auth_cookie`] and
//! [`clear_auth_cookie`] stamp `HttpOnly` and `SameSite=Lax` as literals and
//! take `Secure` from the one [`CookiePosture`]. The session cookie and, later,
//! the OAuth handshake cookie of unit U5 both go through them, so a new cookie
//! cannot arrive with its own flag literals.
//!
//! **The `__Host-` guard.** The prefix is a contract the browser enforces:
//! `Secure`, `Path=/`, and no `Domain`. A cookie that breaks it is discarded
//! with no error anywhere, so the login appears to work and the session never
//! persists — a failure invisible in every log (trap W9). `CookiePosture`
//! locks the name-against-`Secure` half at boot. This module locks the
//! name-against-path half at the call site, where the path is chosen, and it
//! locks it on the CLEARER as well: a deletion aimed at a path the writer could
//! never have used is a no-op that leaves a live session cookie in the jar.
//!
//! **The clearer matches the writer attribute for attribute.** A browser drops a
//! cookie only when the deletion carries the same name and path. Both strings
//! come out of one private builder below, so the pair cannot drift.

use axum::http::{HeaderMap, HeaderValue};

use crate::cookie::{CookiePosture, HOST_PREFIX, read_bearer_token, read_session_cookie};

/// The only path a `__Host-` cookie may carry, and the path of the session
/// cookie.
pub const HOST_PREFIX_PATH: &str = "/";

/// The path of the session cookie. It is the same string as
/// [`HOST_PREFIX_PATH`], named again where a reader looks for it.
pub const SESSION_COOKIE_PATH: &str = "/";

/// The idle window of a session: 30 days. It is the `Max-Age` of the session
/// cookie and the `expires_at` the session row carries.
pub const SESSION_IDLE_SECS: u64 = 30 * 24 * 60 * 60;

/// The absolute window of a session: 90 days from `created_at`. It never slides.
pub const SESSION_ABSOLUTE_SECS: u64 = 90 * 24 * 60 * 60;

/// The floor between two writes of `last_seen_at`: one hour.
pub const LAST_SEEN_TOUCH_SECS: u64 = 60 * 60;

/// The lifetime of a password-reset token: 30 minutes, single use.
pub const RESET_TOKEN_TTL_SECS: u64 = 30 * 60;

/// The lifetime of an email-verification token: 24 hours, single use.
pub const VERIFY_TOKEN_TTL_SECS: u64 = 24 * 60 * 60;

/// The lifetime of the OAuth handshake cookie: 600 seconds.
pub const OAUTH_HANDSHAKE_TTL_SECS: u64 = 600;

/// The `Expires` of a deletion. The date is in the past, so a browser that
/// ignores `Max-Age` drops the cookie too.
const EXPIRED_DATE: &str = "Thu, 01 Jan 1970 00:00:00 GMT";

/// The reason that the cookie writer refuses to build a header.
///
/// Every variant is a programming error, not a client error. Unit U4 maps them
/// to `500`, and none of them reaches a client body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CookieWriteError {
    /// A `__Host-` name at a path other than `/`.
    HostPrefixAtPath {
        /// The refused cookie name.
        name: String,
        /// The refused path.
        path: String,
    },
    /// The name is not an HTTP token, so it cannot name a cookie.
    BadName {
        /// The refused name.
        name: String,
    },
    /// The value carries a byte that `Set-Cookie` cannot hold — a separator, a
    /// space, or a control character. The variant holds NO value: the value is a
    /// live session token.
    BadValue,
    /// The path carries a byte that `Set-Cookie` cannot hold, or it does not
    /// start with `/`.
    BadPath {
        /// The refused path.
        path: String,
    },
}

impl std::fmt::Display for CookieWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HostPrefixAtPath { name, path } => write!(
                f,
                "refusing to write cookie {name:?} at path {path:?}: a browser discards a \
                 {HOST_PREFIX} cookie at any path but {HOST_PREFIX_PATH:?}; give the cookie a name \
                 without the prefix if it needs a narrower scope"
            ),
            Self::BadName { name } => {
                write!(f, "cookie name {name:?} is not an HTTP token")
            }
            Self::BadValue => write!(f, "the cookie value holds a byte Set-Cookie cannot carry"),
            Self::BadPath { path } => {
                write!(f, "cookie path {path:?} is not a legal Set-Cookie path")
            }
        }
    }
}

impl std::error::Error for CookieWriteError {}

/// Whether `byte` may appear in a cookie NAME (RFC 6265 `token`).
fn is_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
}

/// Whether `byte` may appear in a cookie VALUE (RFC 6265 `cookie-octet`).
///
/// The set excludes the control characters, the space, the double quote, the
/// comma, the semicolon, and the backslash. Those are the bytes that would end
/// the value early and let a caller append an attribute of its own.
fn is_cookie_octet(byte: u8) -> bool {
    matches!(byte, 0x21 | 0x23..=0x2B | 0x2D..=0x3A | 0x3C..=0x5B | 0x5D..=0x7E)
}

/// Whether `byte` may appear in a cookie PATH (RFC 6265 `path-value`): any
/// visible ASCII except the semicolon.
fn is_path_byte(byte: u8) -> bool {
    (0x20..=0x7E).contains(&byte) && byte != b';'
}

/// Build one `Set-Cookie` value.
///
/// This is the ONE place that writes the attribute list. Both public writers
/// call it, so the deletion cannot drift from the creation. `max_age` and
/// `expires` are the only two things that separate them.
fn build_cookie(
    posture: CookiePosture,
    name: &str,
    value: &str,
    max_age_secs: u64,
    path: &str,
    expires: Option<&str>,
) -> Result<HeaderValue, CookieWriteError> {
    if name.is_empty() || !name.bytes().all(is_token_byte) {
        return Err(CookieWriteError::BadName {
            name: name.to_string(),
        });
    }
    if !value.bytes().all(is_cookie_octet) {
        return Err(CookieWriteError::BadValue);
    }
    if !path.starts_with('/') || !path.bytes().all(is_path_byte) {
        return Err(CookieWriteError::BadPath {
            path: path.to_string(),
        });
    }
    if name.starts_with(HOST_PREFIX) && path != HOST_PREFIX_PATH {
        return Err(CookieWriteError::HostPrefixAtPath {
            name: name.to_string(),
            path: path.to_string(),
        });
    }

    let mut header = format!("{name}={value}; Max-Age={max_age_secs}");
    if let Some(expires) = expires {
        header.push_str("; Expires=");
        header.push_str(expires);
    }
    header.push_str("; Path=");
    header.push_str(path);
    header.push_str("; HttpOnly");
    if posture.secure {
        header.push_str("; Secure");
    }
    header.push_str("; SameSite=Lax");

    // Every byte above is visible ASCII, so the parse cannot fail. The `Result`
    // is mapped anyway: an unwrap here would be the one panic on the auth path.
    HeaderValue::from_str(&header).map_err(|_| CookieWriteError::BadValue)
}

/// The `Set-Cookie` value of an authentication cookie.
///
/// `path` is required and never defaulted. A caller must state the scope on
/// purpose, because the `__Host-` prefix makes the name and the path one
/// decision.
pub fn set_auth_cookie(
    posture: CookiePosture,
    name: &str,
    value: &str,
    max_age_secs: u64,
    path: &str,
) -> Result<HeaderValue, CookieWriteError> {
    build_cookie(posture, name, value, max_age_secs, path, None)
}

/// The `Set-Cookie` value that expires the authentication cookie `name` at
/// `path`.
///
/// The answer carries an empty value, `Max-Age=0`, and an `Expires` in 1970. It
/// matches [`set_auth_cookie`] attribute for attribute in everything else.
pub fn clear_auth_cookie(
    posture: CookiePosture,
    name: &str,
    path: &str,
) -> Result<HeaderValue, CookieWriteError> {
    build_cookie(posture, name, "", 0, path, Some(EXPIRED_DATE))
}

/// The `Set-Cookie` value that carries `token` as the session cookie.
///
/// The name comes from the posture, the path is the literal `/`, and the
/// `Max-Age` is the idle window of spec section 3.1. None of the three is a
/// caller's choice: all three are what the `__Host-` contract and the session
/// contract pin.
pub fn set_session_cookie(
    posture: CookiePosture,
    token: &str,
) -> Result<HeaderValue, CookieWriteError> {
    set_auth_cookie(
        posture,
        posture.name,
        token,
        SESSION_IDLE_SECS,
        SESSION_COOKIE_PATH,
    )
}

/// The `Set-Cookie` value that expires the session cookie (sign-out).
pub fn clear_session_cookie(posture: CookiePosture) -> Result<HeaderValue, CookieWriteError> {
    clear_auth_cookie(posture, posture.name, SESSION_COOKIE_PATH)
}

/// The raw session token this request presents, or `None`.
///
/// The bearer header wins over the cookie, on purpose: an explicit credential
/// must never be shadowed by the ambient one a browser attached by itself. Both
/// channels carry the same opaque token, so this is a SELECTION only. Nothing
/// here hashes, compares, or validates; unit U3 does that.
///
/// `name` is the active cookie name of the posture. A cookie left over under the
/// other name — after the dev flag flipped — is ignored, never trusted.
pub fn read_session_credential<'h>(headers: &'h HeaderMap, name: &str) -> Option<&'h str> {
    read_bearer_token(headers).or_else(|| read_session_cookie(headers, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each cookie-write error prints the offending part and nothing secret.
    #[test]
    fn the_cookie_write_errors_print_their_reason() {
        assert!(
            CookieWriteError::HostPrefixAtPath {
                name: "__Host-x".to_string(),
                path: "/scoped".to_string(),
            }
            .to_string()
            .contains("/scoped")
        );
        assert_eq!(
            CookieWriteError::BadName {
                name: "bad name".to_string(),
            }
            .to_string(),
            "cookie name \"bad name\" is not an HTTP token"
        );
        assert_eq!(
            CookieWriteError::BadValue.to_string(),
            "the cookie value holds a byte Set-Cookie cannot carry"
        );
        assert_eq!(
            CookieWriteError::BadPath {
                path: "no-slash".to_string(),
            }
            .to_string(),
            "cookie path \"no-slash\" is not a legal Set-Cookie path"
        );
    }
}
