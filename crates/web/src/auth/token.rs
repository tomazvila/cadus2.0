//! The opaque session token: how it is drawn, how it is stored, how it is
//! compared.
//!
//! Spec `docs/reference/web-service-1.0-spec.md` section 3.1, row "Session
//! token", and section 10, row "Argon2id prod / password / token" (1.0
//! `tokens.py:21-49`).
//!
//! A session credential is 32 random bytes in URL-safe text, never a JWT. The
//! raw token reaches the client once, in the cookie or in the login body. The
//! database holds the SHA-256 hex digest alone, so a dump of `auth_sessions` or
//! a logged query yields no live credential.
//!
//! SHA-256 is the right digest here, and Argon2 is not: the input is already 256
//! bits of uniform entropy, so there is nothing to slow a guesser down, and the
//! lookup runs on every guarded request.

use base64ct::{Base64UrlUnpadded, Encoding};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

/// The number of random bytes behind one token: 256 bits.
pub const TOKEN_BYTES: usize = 32;

/// The length of the encoded token. 32 bytes in URL-safe base64 without padding
/// occupy 43 characters, which is what 1.0's `secrets.token_urlsafe(32)` gives.
pub const TOKEN_CHARS: usize = 43;

/// The kernel refused to give entropy.
///
/// The token draw has no fallback. A predictable session token is a full account
/// takeover, so the caller answers `500` and writes nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntropyError {
    /// The text of the underlying `getrandom` error.
    pub reason: String,
}

impl std::fmt::Display for EntropyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the operating system gave no entropy: {}", self.reason)
    }
}

impl std::error::Error for EntropyError {}

/// Draw a fresh session token.
///
/// This is the raw secret. Store only [`hash_token`] of it, and never write it
/// to a log.
pub fn generate_token() -> Result<String, EntropyError> {
    generate_token_with(getrandom::getrandom)
}

/// Draw a fresh token, but take the entropy source as an argument.
///
/// `generate_token` calls it with `getrandom::getrandom`. A unit test passes a
/// fill that refuses, so the entropy-failure arm is reached without a live
/// kernel that gives no entropy.
fn generate_token_with(
    fill: impl FnOnce(&mut [u8]) -> Result<(), getrandom::Error>,
) -> Result<String, EntropyError> {
    let mut bytes = [0u8; TOKEN_BYTES];
    fill(&mut bytes).map_err(|error| EntropyError {
        reason: error.to_string(),
    })?;
    Ok(Base64UrlUnpadded::encode_string(&bytes))
}

/// The stored form of `token`: its SHA-256 digest in lowercase hex, 64
/// characters.
///
/// The digest runs over the UTF-8 bytes of the token, which is what 1.0 hashes.
pub fn hash_token(token: &str) -> String {
    use std::fmt::Write as _;

    let digest = Sha256::digest(token.as_bytes());
    let mut hex = String::with_capacity(2 * digest.len());
    for byte in digest {
        // `{:02x}` writes a lowercase pair for every byte, the leading zero
        // included, so the answer is 64 characters. A `write!` into a `String`
        // cannot fail, so the answer of the macro is discarded on purpose.
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// Constant-time equality of two tokens or two token digests.
///
/// The compare reads every byte of a same-length pair, so it leaks no
/// information about WHERE two values first differ. It does compare the lengths
/// first, exactly as 1.0's `hmac.compare_digest` does: the length of a token or
/// of a hex digest is a fixed, public number, so it is not a secret.
pub fn tokens_equal(left: &str, right: &str) -> bool {
    let (left, right) = (left.as_bytes(), right.as_bytes());
    if left.len() != right.len() {
        return false;
    }
    left.ct_eq(right).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The entropy error prints the reason of the underlying `getrandom` error.
    #[test]
    fn the_entropy_error_names_the_reason() {
        let err = EntropyError {
            reason: "the source is unavailable".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "the operating system gave no entropy: the source is unavailable"
        );
    }

    /// A fill that refuses gives the entropy-failure arm and no token.
    #[test]
    fn a_refused_entropy_fill_is_an_entropy_error() {
        let outcome = generate_token_with(|_| Err(getrandom::Error::UNSUPPORTED));
        assert!(outcome.is_err(), "the draw must fail when the fill refuses");
    }

    /// A fill that answers gives a token of the documented length.
    #[test]
    fn a_full_fill_gives_a_token_of_the_documented_length() {
        let token = generate_token_with(|buf| {
            buf.fill(0);
            Ok(())
        })
        .expect("a full fill gives a token");
        assert_eq!(token.len(), TOKEN_CHARS);
    }
}
