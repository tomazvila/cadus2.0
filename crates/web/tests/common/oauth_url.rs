//! The fixtures of `tests/auth_oauth.rs` and its parts.

use std::collections::HashMap;
use std::sync::Arc;

use super::*;
use axum::http::HeaderValue;
use cadus_store::test_support::TestDb;
use cadus_web::auth::oauth::{
    Credentials, Handshake, OAuthConfig, code_challenge_s256, decode_handshake, encode_handshake,
    safe_next,
};

/// The base64url of the SHA-256 of `u5-pkce-verifier`, with no padding.
///
/// The value comes from `printf %s u5-pkce-verifier | sha256sum` piped through a
/// base64url encoder, NOT from the function under test.
pub const PINNED_CHALLENGE: &str = "yMX3MGbSS42o98kk95nVakkIY8_y97e5ZshCbIQQQmU";

/// The base64url of `{"next":"/dashboard","p":"google","s":"u5-state-value","v":"u5-pkce-verifier"}`.
pub const PINNED_COOKIE_VALUE: &str = "eyJuZXh0IjoiL2Rhc2hib2FyZCIsInAiOiJnb29nbGUiLCJzIjoidTUtc3RhdGUtdmFsdWUiLCJ2IjoidTUtcGtjZS12ZXJpZmllciJ9";

/// One query parameter of a URL, or `None`.
pub fn param(url: &str, key: &str) -> Option<String> {
    let query = url.split_once('?')?.1;
    for pair in query.split('&') {
        let (name, value) = pair.split_once('=')?;
        if name == key {
            return Some(value.to_string());
        }
    }
    None
}
