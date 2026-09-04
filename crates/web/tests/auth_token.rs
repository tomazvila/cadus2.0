//! M5 U2 — the opaque session token and the email normalization.
//!
//! Spec `docs/reference/web-service-1.0-spec.md` section 3.1, rows "Session
//! token" and "Email normalization"; section 10, row "Argon2id prod / password /
//! token". Acceptance checks of unit U2: "the section 10 Argon2 and token
//! literals" and "NFKC+strip+lower is idempotent".
//!
//! The digests below are LITERALS produced by `sha256sum`, never by the code
//! under test.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;

use cadus_web::auth::email::normalize_email;
use cadus_web::auth::token::{generate_token, hash_token, tokens_equal};

// --------------------------------------------------------------------------
// The token: 32 bytes, 43 URL-safe characters.
// --------------------------------------------------------------------------

/// A token is 43 characters, which is 32 bytes in URL-safe base64 with no
/// padding — the length 1.0's `secrets.token_urlsafe(32)` gives.
#[test]
fn a_token_is_43_characters() {
    let token = generate_token().unwrap();

    assert_eq!(token.chars().count(), 43, "the token is {token:?}");
    assert_eq!(token.len(), 43, "the token must be all ASCII");
}

/// Every character is URL-safe: no `+`, no `/`, and no `=` padding.
///
/// The token rides in a `Set-Cookie` value and in a URL query of the reset link,
/// so a `+` or a `/` would need escaping somewhere and would arrive changed.
#[test]
fn a_token_holds_only_url_safe_characters() {
    for _ in 0..64 {
        let token = generate_token().unwrap();
        for character in token.chars() {
            assert!(
                character.is_ascii_alphanumeric() || character == '-' || character == '_',
                "the token {token:?} holds {character:?}, which is not URL-safe"
            );
        }
    }
}

/// 256 draws give 256 different tokens.
///
/// A repeat inside one small batch would mean the draw is not random, which is a
/// full account takeover.
#[test]
fn every_token_of_a_batch_differs() {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for _ in 0..256 {
        assert!(
            seen.insert(generate_token().unwrap()),
            "the token draw repeated itself inside 256 draws"
        );
    }
    assert_eq!(seen.len(), 256);
}

// --------------------------------------------------------------------------
// The stored form: SHA-256, lowercase hex, 64 characters.
// --------------------------------------------------------------------------

/// The digests are the `sha256sum` values of the three inputs.
#[test]
fn hash_token_is_the_sha256_hex_digest() {
    assert_eq!(
        hash_token(""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        hash_token("a"),
        "ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb"
    );
    assert_eq!(
        hash_token("cadus"),
        "26926a4bdb49c16eae0e729992be13c20067db8314920561222bcfc6bd04fb6c"
    );
}

/// The digest is 64 lowercase hex characters for any input, a fresh token
/// included.
#[test]
fn hash_token_is_64_lowercase_hex_characters() {
    let digest = hash_token(&generate_token().unwrap());

    assert_eq!(digest.len(), 64);
    assert!(
        digest
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
        "the digest {digest:?} is not lowercase hex"
    );
}

/// The digest of a token is not the token.
#[test]
fn the_digest_never_carries_the_token() {
    let token = generate_token().unwrap();
    let digest = hash_token(&token);

    assert_ne!(digest, token);
    assert!(!digest.contains(&token));
}

// --------------------------------------------------------------------------
// The compare.
// --------------------------------------------------------------------------

/// Equal strings compare equal; anything else does not.
#[test]
fn tokens_equal_matches_only_an_identical_string() {
    let digest = "26926a4bdb49c16eae0e729992be13c20067db8314920561222bcfc6bd04fb6c";

    assert!(tokens_equal(digest, digest));
    assert!(tokens_equal("", ""));
    // One character differs, at the last position.
    assert!(!tokens_equal(
        digest,
        "26926a4bdb49c16eae0e729992be13c20067db8314920561222bcfc6bd04fb6d"
    ));
    // One character differs, at the first position.
    assert!(!tokens_equal(
        digest,
        "36926a4bdb49c16eae0e729992be13c20067db8314920561222bcfc6bd04fb6c"
    ));
}

/// A prefix is not a match, and neither is a longer string.
#[test]
fn tokens_equal_refuses_a_prefix_and_a_longer_string() {
    assert!(!tokens_equal("abc", "ab"));
    assert!(!tokens_equal("ab", "abc"));
    assert!(!tokens_equal("", "a"));
}

// --------------------------------------------------------------------------
// Email normalization: NFKC, then strip, then lower.
// --------------------------------------------------------------------------

/// The whole address folds to one lowercase form with no edge whitespace.
#[test]
fn normalize_email_strips_and_lowercases() {
    assert_eq!(normalize_email("  Ada@Example.COM \n"), "ada@example.com");
    assert_eq!(normalize_email("\tADA@EXAMPLE.COM\t"), "ada@example.com");
    assert_eq!(normalize_email("ada@example.com"), "ada@example.com");
}

/// NFKC folds the compatibility forms before the strip and the lower run.
///
/// The first case is the whole address in full-width characters. The second is a
/// no-break space at each end: NFKC maps it to a plain space, and only then can
/// the strip see it.
#[test]
fn normalize_email_folds_the_compatibility_forms() {
    assert_eq!(
        normalize_email("ＡＤＡ＠ＥＸＡＭＰＬＥ．ＣＯＭ"),
        "ada@example.com"
    );
    assert_eq!(
        normalize_email("\u{00a0}ada@example.com\u{00a0}"),
        "ada@example.com"
    );
    assert_eq!(
        normalize_email("\u{2004}ada@example.com\u{2005}"),
        "ada@example.com"
    );
}

/// The two hard case-folds of Latin, pinned as literals.
///
/// `İ` (U+0130) lowercases to `i` plus a combining dot above, and `ẞ` (U+1E9E)
/// lowercases to `ß`. Both answers match 1.0's `str.lower()`.
#[test]
fn normalize_email_case_folds_the_hard_latin_letters() {
    assert_eq!(
        normalize_email("\u{0130}nfo@Example.com"),
        "i\u{0307}nfo@example.com"
    );
    assert_eq!(
        normalize_email("\u{1e9e}TRASSE@example.com"),
        "\u{00df}trasse@example.com"
    );
}

/// A space INSIDE the address survives; only the edges are stripped.
#[test]
fn normalize_email_strips_the_edges_only() {
    assert_eq!(
        normalize_email(" ADA\u{2004}@\u{2005}example.com "),
        "ada @ example.com"
    );
}

/// The answer is idempotent: a second pass changes nothing.
///
/// The normalized address is the unique key of `users.email` and the key of the
/// per-email rate counter (spec section 3.2). A key that is not idempotent lets
/// one address own two rows.
#[test]
fn normalize_email_is_idempotent() {
    for raw in [
        "",
        " ",
        "ada@example.com",
        "  Ada@Example.COM \n",
        "ＡＤＡ＠ＥＸＡＭＰＬＥ．ＣＯＭ",
        "\u{00a0}ada@example.com\u{00a0}",
        "\u{0130}nfo@Example.com",
        "\u{1e9e}TRASSE@example.com",
        "\u{fb01}rst@example.com",
        "Ⅻ@example.com",
        "ada+tag@EXAMPLE.co.UK",
        "\u{2004}\u{2005}\u{3000}",
    ] {
        let once = normalize_email(raw);
        let twice = normalize_email(&once);
        assert_eq!(
            once, twice,
            "normalizing {raw:?} twice gave {twice:?}, not {once:?}"
        );
    }
}

/// The ligature `ﬁ` (U+FB01) folds to `fi`, so two spellings of one address give
/// one key.
#[test]
fn normalize_email_folds_a_ligature() {
    assert_eq!(
        normalize_email("\u{fb01}rst@example.com"),
        "first@example.com"
    );
    assert_eq!(
        normalize_email("\u{fb01}RST@example.com"),
        normalize_email("First@example.com")
    );
}
