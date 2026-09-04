//! The session-cookie posture, the boot guard on it, and the two credential
//! readers.
//!
//! Spec `docs/reference/web-service-1.0-spec.md` section 3.1 pins the cookie:
//! `__Host-cadus_session`, `HttpOnly`, `Secure`, `SameSite=Lax`, `Path=/`. The
//! `__Host-` prefix is a contract that the browser enforces. A `__Host-` cookie
//! without `Secure`, or at any path but `/`, is discarded without a word, so the
//! login appears to work and the session never persists (trap W9).
//!
//! Local `http://` development cannot carry `Secure`, so there is one dev
//! fallback: the plain name `cadus_session`, behind the explicit opt-in
//! `CADUS_WEB_INSECURE_COOKIE=1`.
//!
//! The name and the `Secure` flag are not two independent choices. A `__Host-`
//! name demands `Secure`, and the plain name exists because local `http://`
//! cannot give it. [`CookiePosture`] therefore expands the one knob into the
//! correlated pair in one place, and [`CookiePosture::assert_safe`] is the boot
//! guard that refuses the incoherent combination (spec section 3.1, row
//! "Guards"; unit U1 of section 11).
//!
//! This module also owns the two readers that decide which credential a request
//! carries: [`read_bearer_token`] and [`read_session_cookie`]. The CSRF origin
//! layer reads both, and unit U2 builds the cookie writer, the clearer, and the
//! bearer-then-cookie selector on the same two functions. One definition, so the
//! name can never drift between the writer and the reader.

use std::ffi::OsString;

use axum::http::HeaderMap;

/// The production session cookie (spec section 3.1).
pub const SESSION_COOKIE: &str = "__Host-cadus_session";

/// The dev fallback name. Plain, and never `Secure`.
pub const INSECURE_SESSION_COOKIE: &str = "cadus_session";

/// The cookie-name prefix that the browser enforces.
pub const HOST_PREFIX: &str = "__Host-";

/// The environment variable that selects the dev fallback.
pub const INSECURE_COOKIE_VAR: &str = "CADUS_WEB_INSECURE_COOKIE";

/// The `Authorization` scheme that carries a raw session token, with its one
/// space. The scheme itself is case-insensitive (RFC 9110), so the compare
/// lowercases first.
const BEARER_PREFIX: &str = "bearer ";

/// The two correlated consequences of the one insecure-cookie knob.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CookiePosture {
    /// The name of the session cookie under this posture.
    pub name: &'static str,
    /// Whether the writer stamps `Secure`.
    pub secure: bool,
}

/// The reason that [`CookiePosture`] refuses a value or a posture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CookiePostureError {
    /// `CADUS_WEB_INSECURE_COOKIE` holds something other than `0` or `1`.
    BadFlag {
        /// The value, as the operator wrote it. `None` means it is not valid
        /// Unicode.
        value: Option<String>,
    },
    /// A `__Host-` cookie name without `Secure`. The browser discards such a
    /// cookie, so the process refuses to start.
    HostPrefixWithoutSecure {
        /// The cookie name of the refused posture.
        name: &'static str,
    },
}

impl std::fmt::Display for CookiePostureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadFlag { value: Some(value) } => {
                write!(f, "{INSECURE_COOKIE_VAR} must be 0 or 1, not {value:?}")
            }
            Self::BadFlag { value: None } => {
                write!(f, "{INSECURE_COOKIE_VAR} is not valid Unicode")
            }
            Self::HostPrefixWithoutSecure { name } => write!(
                f,
                "refusing to start: the {name} cookie needs Secure, because a browser discards a \
                 {HOST_PREFIX} cookie without it; set {INSECURE_COOKIE_VAR}=1 to serve the plain \
                 {INSECURE_SESSION_COOKIE} cookie over http://"
            ),
        }
    }
}

impl std::error::Error for CookiePostureError {}

impl CookiePosture {
    /// The production posture: the `__Host-` name and `Secure`.
    pub const SECURE: Self = Self {
        name: SESSION_COOKIE,
        secure: true,
    };

    /// The dev posture: the plain name and no `Secure`.
    pub const INSECURE: Self = Self {
        name: INSECURE_SESSION_COOKIE,
        secure: false,
    };

    /// Expand the one knob into the correlated pair. Nothing else derives it.
    pub fn from_flag(insecure: bool) -> Self {
        if insecure {
            Self::INSECURE
        } else {
            Self::SECURE
        }
    }

    /// Read the posture from the raw value of `CADUS_WEB_INSECURE_COOKIE`.
    ///
    /// The function is pure: it reads no environment, so a test drives every
    /// branch. The binary calls it with `std::env::var_os`.
    ///
    /// `None` and `0` give [`Self::SECURE`]. `1` gives [`Self::INSECURE`]. Any
    /// other value is a start error. A silent fallback on a typo such as
    /// `CADUS_WEB_INSECURE_COOKIE=true` would ship the production posture to a
    /// developer who asked for the dev one, and the login would fail with no
    /// message anywhere.
    pub fn from_env(raw: Option<OsString>) -> Result<Self, CookiePostureError> {
        let Some(raw) = raw else {
            return Ok(Self::SECURE);
        };
        let value = raw
            .into_string()
            .map_err(|_| CookiePostureError::BadFlag { value: None })?;
        match value.trim() {
            "" | "0" => Ok(Self::SECURE),
            "1" => Ok(Self::INSECURE),
            other => Err(CookiePostureError::BadFlag {
                value: Some(other.to_string()),
            }),
        }
    }

    /// The boot guard of spec section 3.1: refuse a `__Host-` name without
    /// `Secure`.
    ///
    /// [`Self::from_flag`] makes that state unreachable today. The guard locks
    /// the invariant against a later edit of the pair, which is the only way it
    /// can come back.
    pub fn assert_safe(self) -> Result<(), CookiePostureError> {
        if self.name.starts_with(HOST_PREFIX) && !self.secure {
            return Err(CookiePostureError::HostPrefixWithoutSecure { name: self.name });
        }
        Ok(())
    }
}

/// The raw session token of `Authorization: Bearer <token>`, or `None`.
///
/// `None` covers every way the header fails to carry a usable credential:
/// absent, another scheme, a value that is not visible ASCII, or the scheme with
/// an empty value. That last case matters. A bare `Authorization: Bearer`
/// authenticates nothing, so it must never read as "this request is
/// bearer-authed": in 1.0 it once earned the CSRF exemption and then went on to
/// authenticate by cookie, which is the exact request the layer exists to refuse.
pub fn read_bearer_token(headers: &HeaderMap) -> Option<&str> {
    let raw = headers.get("authorization")?.to_str().ok()?;
    if raw.len() < BEARER_PREFIX.len() {
        return None;
    }
    let (scheme, token) = raw.split_at(BEARER_PREFIX.len());
    if !scheme.eq_ignore_ascii_case(BEARER_PREFIX) {
        return None;
    }
    let token = token.trim();
    if token.is_empty() { None } else { Some(token) }
}

/// The raw session token of the cookie named `name`, or `None`.
///
/// The reader takes the active name from the posture, so a cookie left over
/// under the other name — after the dev flag flipped — is ignored, never
/// trusted.
///
/// HTTP/2 sends the cookie pairs in several `cookie` headers, so the walk reads
/// every one of them.
pub fn read_session_cookie<'h>(headers: &'h HeaderMap, name: &str) -> Option<&'h str> {
    for header in headers.get_all("cookie") {
        let Ok(raw) = header.to_str() else {
            continue;
        };
        for pair in raw.split(';') {
            let Some((key, value)) = pair.split_once('=') else {
                continue;
            };
            if key.trim() == name {
                return Some(value.trim());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bad-flag error prints the value, and prints the not-Unicode note
    /// when it has none.
    #[test]
    fn the_bad_flag_error_prints_the_value_or_the_unicode_note() {
        assert_eq!(
            CookiePostureError::BadFlag {
                value: Some("true".to_string()),
            }
            .to_string(),
            format!("{INSECURE_COOKIE_VAR} must be 0 or 1, not \"true\"")
        );
        assert_eq!(
            CookiePostureError::BadFlag { value: None }.to_string(),
            format!("{INSECURE_COOKIE_VAR} is not valid Unicode")
        );
    }

    /// The one knob expands into the correlated posture pair.
    #[test]
    fn the_flag_expands_into_the_posture_pair() {
        assert_eq!(CookiePosture::from_flag(false), CookiePosture::SECURE);
        assert_eq!(CookiePosture::from_flag(true), CookiePosture::INSECURE);
    }

    /// A value that is not valid Unicode is a bad-flag error with no value.
    #[cfg(unix)]
    #[test]
    fn a_non_unicode_flag_is_a_bad_flag_with_no_value() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let raw = OsString::from_vec(vec![0x31, 0xff]);
        assert_eq!(
            CookiePosture::from_env(Some(raw)),
            Err(CookiePostureError::BadFlag { value: None })
        );
    }

    /// The bearer reader answers `None` with no `Authorization` header, and a
    /// value that is not visible ASCII.
    #[test]
    fn the_bearer_reader_needs_a_usable_header() {
        use axum::http::{HeaderMap, HeaderValue};

        assert_eq!(read_bearer_token(&HeaderMap::new()), None);
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_bytes(b"Bearer \xff").unwrap(),
        );
        assert_eq!(read_bearer_token(&headers), None);
    }

    /// The cookie reader skips a header that is not text and a pair with no
    /// `=`, and reads the named cookie from the rest.
    #[test]
    fn the_cookie_reader_skips_unusable_headers_and_pairs() {
        use axum::http::{HeaderMap, HeaderValue};

        let mut headers = HeaderMap::new();
        headers.append("cookie", HeaderValue::from_bytes(b"\xff").unwrap());
        headers.append(
            "cookie",
            HeaderValue::from_static("flag; __Host-cadus_session=token-value"),
        );
        assert_eq!(
            read_session_cookie(&headers, SESSION_COOKIE),
            Some("token-value")
        );
        assert_eq!(read_session_cookie(&headers, "absent"), None);
    }
}
