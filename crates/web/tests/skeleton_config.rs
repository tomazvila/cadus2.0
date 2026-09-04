//! Part of `tests/skeleton.rs`: the header of that file gives the
//! requirements and the rules.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axum::http::HeaderMap;
use cadus_web::cookie::{
    CookiePosture, CookiePostureError, read_bearer_token, read_session_cookie,
};
use cadus_web::origin::{OriginPolicy, OriginPolicyError};

// ---------------------------------------------------------------------------
// The cookie-posture guard and the two configuration readers
// ---------------------------------------------------------------------------

/// (18) The boot guard refuses a `__Host-` cookie without `Secure`.
///
/// A browser discards such a cookie without a word, so the login appears to work
/// and no session ever persists (trap W9). `from_flag` makes the state
/// unreachable today; the guard locks the invariant against a later edit.
#[test]
fn the_cookie_posture_guard_refuses_a_host_cookie_without_secure() {
    let broken = CookiePosture {
        name: "__Host-cadus_session",
        secure: false,
    };

    assert_eq!(
        broken.assert_safe(),
        Err(CookiePostureError::HostPrefixWithoutSecure {
            name: "__Host-cadus_session"
        })
    );
    assert!(
        broken
            .assert_safe()
            .unwrap_err()
            .to_string()
            .contains("needs Secure")
    );

    assert_eq!(CookiePosture::SECURE.assert_safe(), Ok(()));
    assert_eq!(CookiePosture::INSECURE.assert_safe(), Ok(()));
}

/// (19) The one knob expands into the correlated pair, and only `0` and `1` are
/// values.
#[test]
fn the_insecure_cookie_knob_reads_only_zero_and_one() {
    use std::ffi::OsString;

    assert_eq!(CookiePosture::from_env(None), Ok(CookiePosture::SECURE));
    assert_eq!(
        CookiePosture::from_env(Some(OsString::from("0"))),
        Ok(CookiePosture::SECURE)
    );
    assert_eq!(
        CookiePosture::from_env(Some(OsString::from("1"))),
        Ok(CookiePosture::INSECURE)
    );

    assert_eq!(
        CookiePosture::from_env(Some(OsString::from("true"))),
        Err(CookiePostureError::BadFlag {
            value: Some("true".to_string())
        })
    );

    // The pair is correlated, so both halves are pinned together.
    assert_eq!(
        CookiePosture::SECURE,
        CookiePosture {
            name: "__Host-cadus_session",
            secure: true
        }
    );
    assert_eq!(
        CookiePosture::INSECURE,
        CookiePosture {
            name: "cadus_session",
            secure: false
        }
    );
}

/// (20) `PUBLIC_ORIGIN` takes an origin and nothing else.
///
/// A path, a query, or a trailing slash never matches an `Origin` header, so a
/// deployment that set one would refuse every browser write. That is a start
/// error, not a silent fallback.
#[test]
fn public_origin_takes_an_origin_and_nothing_else() {
    use std::ffi::OsString;

    assert_eq!(
        OriginPolicy::from_env(Some(OsString::from("https://tutor.example"))),
        Ok(OriginPolicy {
            public_origin: Some("https://tutor.example".to_string())
        })
    );
    assert_eq!(
        OriginPolicy::from_env(Some(OsString::from("http://127.0.0.1:8080"))),
        Ok(OriginPolicy {
            public_origin: Some("http://127.0.0.1:8080".to_string())
        })
    );
    assert_eq!(
        OriginPolicy::from_env(None),
        Ok(OriginPolicy {
            public_origin: None
        })
    );

    for bad in [
        "https://tutor.example/",
        "https://tutor.example/app",
        "tutor.example",
        "https://",
        "https://tutor.example?x=1",
    ] {
        assert_eq!(
            OriginPolicy::from_env(Some(OsString::from(bad))),
            Err(OriginPolicyError::NotAnOrigin {
                value: bad.to_string()
            }),
            "{bad} is not an origin"
        );
    }
}

/// (21) The bearer reader gives back the token, and gives back nothing for every
/// way the header fails to carry one.
#[test]
fn the_bearer_reader_reads_a_usable_token_only() {
    let with = |value: &str| {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", value.parse().unwrap());
        headers
    };

    assert_eq!(read_bearer_token(&with("Bearer abc123")), Some("abc123"));
    assert_eq!(read_bearer_token(&with("bearer abc123")), Some("abc123"));
    assert_eq!(read_bearer_token(&with("BEARER abc123")), Some("abc123"));
    assert_eq!(
        read_bearer_token(&with("Bearer   abc123  ")),
        Some("abc123")
    );

    assert_eq!(read_bearer_token(&HeaderMap::new()), None);
    assert_eq!(read_bearer_token(&with("Bearer")), None);
    assert_eq!(read_bearer_token(&with("Bearer ")), None);
    assert_eq!(read_bearer_token(&with("Bearer    ")), None);
    assert_eq!(read_bearer_token(&with("Basic dXNlcjpwYXNz")), None);
    assert_eq!(read_bearer_token(&with("Bearerabc123")), None);
}

/// (22) The cookie reader finds one pair among many, and reads the active name
/// only.
#[test]
fn the_cookie_reader_reads_the_named_cookie_only() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "cookie",
        "theme=dark; __Host-cadus_session=s3cr3t; locale=en-US"
            .parse()
            .unwrap(),
    );

    assert_eq!(
        read_session_cookie(&headers, "__Host-cadus_session"),
        Some("s3cr3t")
    );
    assert_eq!(read_session_cookie(&headers, "cadus_session"), None);
    assert_eq!(
        read_session_cookie(&HeaderMap::new(), "cadus_session"),
        None
    );

    // HTTP/2 splits the pairs across several `cookie` headers.
    let mut split = HeaderMap::new();
    split.append("cookie", "theme=dark".parse().unwrap());
    split.append("cookie", "__Host-cadus_session=s3cr3t".parse().unwrap());
    assert_eq!(
        read_session_cookie(&split, "__Host-cadus_session"),
        Some("s3cr3t")
    );
}
