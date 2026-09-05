//! The handshake record of one sign-in, its cookie form, and the authorize
//! redirect it starts.

use std::fmt::Write as _;

use base64ct::{Base64UrlUnpadded, Encoding};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::{CODE_CHALLENGE_METHOD, Credentials, DEFAULT_NEXT, Provider, SAFE_NEXT_BYTES};
use crate::auth::token::{EntropyError, generate_token};

// ---------------------------------------------------------------------------
// The handshake record
// ---------------------------------------------------------------------------

/// The bytes behind the CSRF `state`: 192 bits.
pub const STATE_BYTES: usize = 24;

/// The bytes behind the PKCE verifier: 384 bits. RFC 7636 wants 43 to 128
/// characters, and 48 bytes give 64.
pub const VERIFIER_BYTES: usize = 48;

/// The sign-in record minted at the start route and spent at the callback.
///
/// The handshake cookie carries exactly this. It is unsigned, and it is safe
/// unsigned: the cookie is `HttpOnly`, so no script reads or writes it; `state`
/// is compared against the callback query, so a value an attacker chose matches
/// nothing they did not also start; and `next_url` is validated again on READ,
/// so a tampered target still cannot leave the site.
#[derive(Clone, PartialEq, Eq)]
pub struct Handshake {
    /// The provider this `state` was minted for.
    pub provider: String,
    /// The CSRF token, compared in constant time at the callback.
    pub state: String,
    /// The PKCE verifier, replayed at the token exchange.
    pub code_verifier: String,
    /// Where to send the browser after sign-in.
    pub next_url: String,
}

impl std::fmt::Debug for Handshake {
    /// Redact both secrets. A failing assertion prints this, and a `state` in a
    /// log line is a live CSRF token.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Handshake")
            .field("provider", &self.provider)
            .field("state", &"<redacted>")
            .field("code_verifier", &"<redacted>")
            .field("next_url", &self.next_url)
            .finish()
    }
}

impl Handshake {
    /// Mint a fresh handshake for `provider`.
    ///
    /// # Errors
    ///
    /// Returns [`EntropyError`] when the operating system gives no entropy. A
    /// predictable `state` or verifier defeats the whole guard, so the caller
    /// answers `500` and starts nothing.
    pub fn fresh(provider: &str, next_url: &str) -> Result<Self, EntropyError> {
        Ok(Self {
            provider: provider.to_string(),
            state: draw(STATE_BYTES)?,
            code_verifier: draw(VERIFIER_BYTES)?,
            next_url: safe_next(Some(next_url)),
        })
    }
}

/// Draw `bytes` of entropy in URL-safe text.
///
/// [`generate_token`] draws a fixed 32 bytes, and the two sizes here are 24 and
/// 48, so this function concatenates whole draws and cuts the text to the length
/// that `bytes` bytes of base64 occupy. Every character still comes from the
/// operating system.
fn draw(bytes: usize) -> Result<String, EntropyError> {
    let chars = bytes.div_ceil(3) * 4;
    let mut text = String::with_capacity(chars);
    while text.len() < chars {
        text.push_str(&generate_token()?);
    }
    text.truncate(chars);
    Ok(text)
}

/// A same-site relative target, or [`DEFAULT_NEXT`].
///
/// The accepted set is one path that holds three properties:
///
/// 1. the first byte is `/`;
/// 2. the second byte is neither `/` nor a backslash;
/// 3. every byte is in [`SAFE_NEXT_BYTES`], and no byte is a backslash.
///
/// Rule 2 closes the plain open redirect: a browser reads `//host` and `/\host`
/// as scheme-relative, so both leave the site. Rule 3 closes the same redirect
/// through one control byte. A browser removes every ASCII tab, LF, and CR from
/// a URL before it parses the URL (WHATWG URL, "remove all ASCII tab or
/// newline"), so a target of `/`, one tab, `/host` reaches the parser as
/// `//host` and leaves the site too. A backslash is a path separator to a
/// browser, so rule 3 refuses it in every position.
///
/// **The upper bound of rule 3 is what the answer must survive.** The answer of
/// this function becomes the `Location` header of a `302`, and
/// `HeaderValue::from_str` refuses `0x7f` (`http` builds a header value from the
/// rule `b >= 32 && b != 127 || b == b'\t'`). A `0x7f` that reached the header
/// therefore failed the build AFTER the callback had already committed the
/// account, the provider link, and the session row, so the learner got a `500`
/// and no `Set-Cookie` for a session that exists. Rule 3 stops at `0x7e`, so
/// every accepted answer builds. A byte at or above `0x80` cannot stand alone in
/// a `&str`, and rule 3 refuses the multi-byte sequences that carry it, so the
/// accepted set is ASCII text.
///
/// The callback runs this function again on the value it read from the cookie,
/// which is what lets the cookie stay unsigned.
#[must_use]
pub fn safe_next(next_url: Option<&str>) -> String {
    let Some(next) = next_url else {
        return DEFAULT_NEXT.to_string();
    };
    let bytes = next.as_bytes();
    let same_site = bytes.first() == Some(&b'/')
        && !matches!(bytes.get(1), Some(b'/' | b'\\'))
        && bytes
            .iter()
            .all(|byte| SAFE_NEXT_BYTES.contains(byte) && *byte != b'\\');
    if same_site {
        next.to_string()
    } else {
        DEFAULT_NEXT.to_string()
    }
}

/// Serialize a handshake into the cookie value.
///
/// The one-letter keys are a wire format. A cookie a running process wrote must
/// still decode after a restart, so a key is added or ignored, never renamed.
#[must_use]
pub fn encode_handshake(handshake: &Handshake) -> String {
    let payload = serde_json::json!({
        "p": handshake.provider,
        "s": handshake.state,
        "v": handshake.code_verifier,
        "next": handshake.next_url,
    });
    Base64UrlUnpadded::encode_string(payload.to_string().as_bytes())
}

/// Read a handshake back, or `None` for every unusable value.
///
/// `None` covers an absent cookie, a value that is not base64, a payload that is
/// not JSON, a payload that is not an object, and a payload missing any of the
/// three secrets. The callback answers all of them with one `400 oauth_error`.
#[must_use]
pub fn decode_handshake(value: Option<&str>) -> Option<Handshake> {
    let raw = Base64UrlUnpadded::decode_vec(value?.trim()).ok()?;
    let payload: Value = serde_json::from_slice(&raw).ok()?;
    let text = |key: &str| -> Option<String> {
        payload
            .get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    Some(Handshake {
        provider: text("p")?,
        state: text("s")?,
        code_verifier: text("v")?,
        next_url: safe_next(payload.get("next").and_then(Value::as_str)),
    })
}

// ---------------------------------------------------------------------------
// The authorize redirect
// ---------------------------------------------------------------------------

/// Percent-encode one query or form component (RFC 3986 unreserved set).
fn encode_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(byte) {
            out.push(char::from(*byte));
        } else {
            // The format writes two uppercase hex digits, so the answer stays
            // ASCII. A write into a `String` cannot fail.
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// Join `pairs` into a query string or a form body.
pub(super) fn encode_pairs(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(key, value)| format!("{}={}", encode_component(key), encode_component(value)))
        .collect::<Vec<_>>()
        .join("&")
}

/// The PKCE S256 challenge of `verifier`: base64url, no padding, of its SHA-256.
#[must_use]
pub fn code_challenge_s256(verifier: &str) -> String {
    Base64UrlUnpadded::encode_string(&Sha256::digest(verifier.as_bytes()))
}

/// The provider URL the browser is sent to.
#[must_use]
pub fn authorize_url(
    provider: Provider,
    credentials: &Credentials,
    redirect_uri: &str,
    handshake: &Handshake,
) -> String {
    let challenge = code_challenge_s256(&handshake.code_verifier);
    let query = encode_pairs(&[
        ("response_type", "code"),
        ("client_id", &credentials.client_id),
        ("redirect_uri", redirect_uri),
        ("scope", provider.scope),
        ("state", &handshake.state),
        ("code_challenge", &challenge),
        ("code_challenge_method", CODE_CHALLENGE_METHOD),
    ]);
    format!("{}?{query}", provider.authorize_url)
}

#[cfg(test)]
mod cov_tests {
    use super::*;
    use crate::auth::token::refuse_entropy_after;

    /// A refused first draw fails the state of the handshake, and a refused
    /// second draw fails the verifier: both give the entropy error.
    #[test]
    fn a_handshake_needs_entropy_for_both_the_state_and_the_verifier() {
        // The state draws once (24 bytes), so a refusal after zero draws fails
        // the state, and a refusal after one draw fails the verifier.
        for draws in [0, 1] {
            refuse_entropy_after(Some(draws));
            let result = Handshake::fresh("google", "/next");
            refuse_entropy_after(None);
            assert!(result.is_err(), "draw {draws} did not refuse");
        }
    }
}
