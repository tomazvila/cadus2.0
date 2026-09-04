//! M5 U2 — the Argon2id profiles and the password policy.
//!
//! Spec `docs/reference/web-service-1.0-spec.md` section 3.1, rows "Password
//! policy" and "Argon2id"; section 10, row "Argon2id prod / password / token".
//! Acceptance check of unit U2: "the section 10 Argon2 and token literals".
//!
//! Every number below is a LITERAL. No test reads a constant out of the module
//! under test, so a changed parameter fails here instead of passing quietly
//! (HANDOVER section 3).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::ffi::OsString;

use cadus_web::auth::password::{
    Argon2Profile, PasswordError, WeakPassword, hash_password, needs_rehash, validate_password,
    verify_password,
};

/// A password that the policy accepts, used wherever the policy is not the
/// subject.
const GOOD_PASSWORD: &str = "correct horse battery staple";

// --------------------------------------------------------------------------
// The parameter literals of spec section 10.
// --------------------------------------------------------------------------

/// `prod` is `time_cost=3, memory_cost=65536 KiB, parallelism=4`.
#[test]
fn the_prod_profile_carries_the_pinned_parameters() {
    let profile = Argon2Profile::PROD;

    assert_eq!(profile.name, "prod");
    assert_eq!(profile.time_cost, 3);
    assert_eq!(profile.memory_cost_kib, 65536);
    assert_eq!(profile.parallelism, 4);
}

/// `test` is `1 / 8192 / 1`.
#[test]
fn the_test_profile_carries_the_pinned_parameters() {
    let profile = Argon2Profile::TEST;

    assert_eq!(profile.name, "test");
    assert_eq!(profile.time_cost, 1);
    assert_eq!(profile.memory_cost_kib, 8192);
    assert_eq!(profile.parallelism, 1);
}

/// The stored PHC string names Argon2id, version 19, and the prod parameters.
///
/// The prefix is the whole contract of the stored hash: another verifier reads
/// it, and unit U3 stores it in `users.password_hash`. This is the one prod-cost
/// hash in the suite; every other test uses the `test` profile.
#[test]
fn a_prod_hash_names_argon2id_version_19_and_m65536_t3_p4() {
    let hashed = hash_password(Argon2Profile::PROD, GOOD_PASSWORD).unwrap();

    assert!(
        hashed.starts_with("$argon2id$v=19$m=65536,t=3,p=4$"),
        "the prod hash must name argon2id v19 m=65536 t=3 p=4, it is {hashed}"
    );
}

/// The test profile writes `m=8192,t=1,p=1` into the same shape.
#[test]
fn a_test_hash_names_argon2id_version_19_and_m8192_t1_p1() {
    let hashed = hash_password(Argon2Profile::TEST, GOOD_PASSWORD).unwrap();

    assert!(
        hashed.starts_with("$argon2id$v=19$m=8192,t=1,p=1$"),
        "the test hash must name argon2id v19 m=8192 t=1 p=1, it is {hashed}"
    );
}

/// Two hashes of one password differ, because each draws a fresh salt.
#[test]
fn two_hashes_of_one_password_differ() {
    let first = hash_password(Argon2Profile::TEST, GOOD_PASSWORD).unwrap();
    let second = hash_password(Argon2Profile::TEST, GOOD_PASSWORD).unwrap();

    assert_ne!(first, second, "the salt must be fresh on every hash");
    assert!(verify_password(&first, GOOD_PASSWORD));
    assert!(verify_password(&second, GOOD_PASSWORD));
}

// --------------------------------------------------------------------------
// Verify.
// --------------------------------------------------------------------------

/// The right password verifies; a wrong one does not.
#[test]
fn verify_accepts_the_password_and_refuses_another() {
    let hashed = hash_password(Argon2Profile::TEST, GOOD_PASSWORD).unwrap();

    assert!(verify_password(&hashed, GOOD_PASSWORD));
    assert!(!verify_password(&hashed, "correct horse battery stapl"));
    assert!(!verify_password(&hashed, ""));
}

/// A stored string that does not parse gives `false`, never an error.
///
/// The anti-enumeration dummy verify of spec section 3.3 depends on this: an
/// unknown account must cost one real verify and answer the same
/// `401 invalid_credentials`, so a raised exception would be the leak.
#[test]
fn verify_is_false_for_every_unreadable_stored_hash() {
    for stored in [
        "",
        "not-a-hash",
        "$argon2id$",
        "$2b$12$KIXQ2M4hVLcVn0Zg2f0hUuTb6R2yqzq1kk4M0Kk8L1sQ0cGZ6a2fO",
        "$argon2id$v=19$m=8192,t=1,p=1$c2FsdA$",
    ] {
        assert!(
            !verify_password(stored, GOOD_PASSWORD),
            "an unreadable stored hash must verify as false, {stored:?} did not"
        );
    }
}

/// Verify reads the parameters out of the hash, not out of any profile.
///
/// A process running `prod` must still admit a password hashed under `test`,
/// and the reverse. The function takes no profile at all, and this pins that.
#[test]
fn verify_reads_the_parameters_out_of_the_stored_hash() {
    let under_test = hash_password(Argon2Profile::TEST, GOOD_PASSWORD).unwrap();
    let under_prod = hash_password(Argon2Profile::PROD, GOOD_PASSWORD).unwrap();

    assert!(verify_password(&under_test, GOOD_PASSWORD));
    assert!(verify_password(&under_prod, GOOD_PASSWORD));
}

// --------------------------------------------------------------------------
// Rehash.
// --------------------------------------------------------------------------

/// A hash under the active profile needs no rewrite; one under the other
/// profile does.
#[test]
fn needs_rehash_follows_the_active_profile() {
    let under_test = hash_password(Argon2Profile::TEST, GOOD_PASSWORD).unwrap();

    assert!(!needs_rehash(Argon2Profile::TEST, &under_test));
    assert!(needs_rehash(Argon2Profile::PROD, &under_test));
}

/// Another algorithm, another version, or an unreadable string all ask for a
/// rewrite.
///
/// The three strings below are hand-written PHC forms with a valid salt and a
/// valid 32-byte digest, so only the named field differs from a `test` hash.
#[test]
fn needs_rehash_is_true_for_another_algorithm_version_or_garbage() {
    let salt_and_hash = "c29tZXNhbHRzb21lc2E$4jvNkjnGY6E5N0YkA0RQuGyC0zqZDbF44MQpVw0YWMQ";

    // The control: the same string with the `test` algorithm, version, and
    // parameters parses and asks for NO rewrite. Without it, every case below
    // would pass on an unparseable string and prove nothing about the fields.
    assert!(!needs_rehash(
        Argon2Profile::TEST,
        &format!("$argon2id$v=19$m=8192,t=1,p=1${salt_and_hash}")
    ));

    for stored in [
        format!("$argon2i$v=19$m=8192,t=1,p=1${salt_and_hash}"),
        format!("$argon2d$v=19$m=8192,t=1,p=1${salt_and_hash}"),
        format!("$argon2id$v=16$m=8192,t=1,p=1${salt_and_hash}"),
        "not-a-hash".to_string(),
        String::new(),
    ] {
        assert!(
            needs_rehash(Argon2Profile::TEST, &stored),
            "{stored:?} must ask for a rewrite"
        );
    }
}

/// A weaker parameter set asks for a rewrite even under the same algorithm.
#[test]
fn needs_rehash_is_true_when_one_parameter_differs() {
    let salt_and_hash = "c29tZXNhbHRzb21lc2E$4jvNkjnGY6E5N0YkA0RQuGyC0zqZDbF44MQpVw0YWMQ";

    for stored in [
        format!("$argon2id$v=19$m=4096,t=1,p=1${salt_and_hash}"),
        format!("$argon2id$v=19$m=8192,t=2,p=1${salt_and_hash}"),
        format!("$argon2id$v=19$m=8192,t=1,p=2${salt_and_hash}"),
    ] {
        assert!(
            needs_rehash(Argon2Profile::TEST, &stored),
            "{stored:?} must ask for a rewrite"
        );
    }
}

// --------------------------------------------------------------------------
// The policy: 8 to 256 characters, at most 1024 UTF-8 bytes.
// --------------------------------------------------------------------------

/// Seven characters is too short; eight is the floor and passes.
#[test]
fn the_floor_is_eight_characters() {
    assert_eq!(validate_password("1234567"), Err(WeakPassword::TooShort));
    assert_eq!(validate_password("12345678"), Ok(()));
    assert_eq!(validate_password(""), Err(WeakPassword::TooShort));
}

/// 256 characters is the ceiling and passes; 257 does not.
#[test]
fn the_ceiling_is_256_characters() {
    assert_eq!(validate_password(&"a".repeat(256)), Ok(()));
    assert_eq!(
        validate_password(&"a".repeat(257)),
        Err(WeakPassword::TooLong)
    );
}

/// The count is characters, not bytes.
///
/// Seven four-byte characters are 28 bytes, and the policy still refuses them:
/// a byte count would have admitted them.
#[test]
fn the_count_is_characters_and_not_bytes() {
    let seven = "\u{1f600}".repeat(7);
    let eight = "\u{1f600}".repeat(8);

    assert_eq!(seven.len(), 28);
    assert_eq!(validate_password(&seven), Err(WeakPassword::TooShort));
    assert_eq!(eight.len(), 32);
    assert_eq!(validate_password(&eight), Ok(()));
}

/// The byte rule fires BEFORE the character rule.
///
/// 257 four-byte characters are 1028 bytes. Both rules apply, and the answer
/// must be the byte one: it is the rule that bounds the work Argon2 does, and
/// 1.0 orders the two the same way (`passwords.py:97-103`).
#[test]
fn the_byte_ceiling_is_checked_before_the_character_ceiling() {
    let bomb = "\u{1f600}".repeat(257);

    assert_eq!(bomb.chars().count(), 257);
    assert_eq!(bomb.len(), 1028);
    assert_eq!(validate_password(&bomb), Err(WeakPassword::TooManyBytes));
}

/// 256 four-byte characters are exactly 1024 bytes, and both ceilings admit
/// them.
#[test]
fn the_two_ceilings_meet_at_1024_bytes() {
    let edge = "\u{1f600}".repeat(256);

    assert_eq!(edge.chars().count(), 256);
    assert_eq!(edge.len(), 1024);
    assert_eq!(validate_password(&edge), Ok(()));
}

/// An over-long ASCII password is refused for its character count, because 257
/// ASCII bytes are well under the byte ceiling.
#[test]
fn an_over_long_ascii_password_is_refused_for_its_length() {
    let long = "a".repeat(257);

    assert_eq!(long.len(), 257);
    assert_eq!(validate_password(&long), Err(WeakPassword::TooLong));
}

/// The policy has no composition rule. A long run of one character passes.
#[test]
fn the_policy_asks_for_no_digit_case_or_symbol() {
    assert_eq!(validate_password("aaaaaaaa"), Ok(()));
    assert_eq!(validate_password("        "), Ok(()));
}

/// The three refusal messages, character for character.
#[test]
fn the_refusal_messages_are_the_pinned_text() {
    assert_eq!(
        WeakPassword::TooShort.to_string(),
        "Password must be at least 8 characters."
    );
    assert_eq!(
        WeakPassword::TooManyBytes.to_string(),
        "Password must be at most 1024 UTF-8 bytes."
    );
    assert_eq!(
        WeakPassword::TooLong.to_string(),
        "Password must be at most 256 characters."
    );
}

// --------------------------------------------------------------------------
// Profile selection.
// --------------------------------------------------------------------------

/// An absent or empty variable gives `prod`; the two names give their profiles.
#[test]
fn the_profile_variable_selects_the_profile() {
    assert_eq!(Argon2Profile::from_env(None), Ok(Argon2Profile::PROD));
    assert_eq!(
        Argon2Profile::from_env(Some(OsString::from(""))),
        Ok(Argon2Profile::PROD)
    );
    assert_eq!(
        Argon2Profile::from_env(Some(OsString::from("prod"))),
        Ok(Argon2Profile::PROD)
    );
    assert_eq!(
        Argon2Profile::from_env(Some(OsString::from("test"))),
        Ok(Argon2Profile::TEST)
    );
    assert_eq!(
        Argon2Profile::from_env(Some(OsString::from("  test  "))),
        Ok(Argon2Profile::TEST)
    );
}

/// An unknown name is a start error, never a silent fall back to `prod`.
///
/// A deployment that quietly ran the `test` parameters would hash every password
/// at 8 MiB, and nothing anywhere would say so.
#[test]
fn an_unknown_profile_name_is_a_start_error() {
    assert_eq!(
        Argon2Profile::from_env(Some(OsString::from("prd"))),
        Err(PasswordError::UnknownProfile {
            value: Some("prd".to_string())
        })
    );
    assert_eq!(
        Argon2Profile::from_env(Some(OsString::from("PROD"))),
        Err(PasswordError::UnknownProfile {
            value: Some("PROD".to_string())
        })
    );
}

/// A value that is not valid Unicode is a start error too.
#[cfg(unix)]
#[test]
fn a_non_unicode_profile_value_is_a_start_error() {
    use std::os::unix::ffi::OsStringExt;

    let raw = OsString::from_vec(vec![0x70, 0x72, 0xff]);

    assert_eq!(
        Argon2Profile::from_env(Some(raw)),
        Err(PasswordError::UnknownProfile { value: None })
    );
}

/// The start-error text names the variable, the value, and both profiles.
#[test]
fn the_profile_error_text_names_the_variable_and_the_choices() {
    assert_eq!(
        PasswordError::UnknownProfile {
            value: Some("prd".to_string())
        }
        .to_string(),
        "CADUS_AUTH_ARGON2_PROFILE=\"prd\" is not a known Argon2 profile (expected one of [\"prod\", \"test\"])"
    );
    assert_eq!(
        PasswordError::UnknownProfile { value: None }.to_string(),
        "CADUS_AUTH_ARGON2_PROFILE is not valid Unicode"
    );
}
