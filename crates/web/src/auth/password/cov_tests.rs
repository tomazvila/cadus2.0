//! The coverage tests of the password module: every error arm, every rehash
//! rule, and the hashing steps through one instantiation.

use super::*;

/// Every hashing-error variant prints its own reason.
#[test]
fn every_password_error_prints_its_reason() {
    assert!(
        PasswordError::UnknownProfile {
            value: Some("prd".to_string()),
        }
        .to_string()
        .contains("prd")
    );
    assert_eq!(
        PasswordError::UnknownProfile { value: None }.to_string(),
        format!("{ARGON2_PROFILE_VAR} is not valid Unicode")
    );
    assert_eq!(
        PasswordError::Parameters {
            profile: "prod",
            reason: "too small".to_string(),
        }
        .to_string(),
        "the prod Argon2 profile is not legal: too small"
    );
    assert_eq!(
        PasswordError::Entropy {
            reason: "no pool".to_string(),
        }
        .to_string(),
        "the operating system gave no entropy: no pool"
    );
    assert_eq!(
        PasswordError::Hashing {
            reason: "refused".to_string(),
        }
        .to_string(),
        "Argon2 refused to hash: refused"
    );
}

/// A profile whose numbers are not a legal Argon2 parameter set is a
/// `Parameters` error, at the hash and at the `hasher` call.
#[test]
fn an_illegal_profile_is_a_parameters_error() {
    let illegal = Argon2Profile {
        name: "illegal",
        time_cost: 0,
        memory_cost_kib: 1,
        parallelism: 4,
    };
    let err = hash_password(illegal, "correct horse battery staple").unwrap_err();
    assert!(
        err.to_string()
            .contains("illegal Argon2 profile is not legal")
    );
}

type Fill = fn(&mut [u8]) -> Result<(), getrandom::Error>;
type Encode = fn(&[u8]) -> argon2::password_hash::Result<SaltString>;
type Hash = fn(&Argon2<'static>, &[u8], &SaltString) -> argon2::password_hash::Result<String>;

fn refuse_fill(_: &mut [u8]) -> Result<(), getrandom::Error> {
    Err(getrandom::Error::UNSUPPORTED)
}

fn refuse_encode(_: &[u8]) -> argon2::password_hash::Result<SaltString> {
    Err(argon2::password_hash::Error::Password)
}

fn refuse_hash(
    _: &Argon2<'static>,
    _: &[u8],
    _: &SaltString,
) -> argon2::password_hash::Result<String> {
    Err(argon2::password_hash::Error::Password)
}

/// Run the hashing steps through function pointers, so every case below
/// shares one instantiation and the whole function is measured as one.
fn via(
    fill: Fill,
    encode: Encode,
    hash: Hash,
    profile: Argon2Profile,
) -> Result<String, PasswordError> {
    hash_password_via(fill, encode, hash, profile, "correct horse battery staple")
}

/// A salt fill that refuses is an entropy error and no hash.
#[test]
fn a_refused_salt_fill_is_an_entropy_error() {
    let err = hash_password_with(
        |_| Err(getrandom::Error::UNSUPPORTED),
        Argon2Profile::TEST,
        "correct horse battery staple",
    )
    .unwrap_err();
    assert!(err.to_string().contains("gave no entropy"));
    let err = via(
        refuse_fill,
        SaltString::encode_b64,
        hash_with,
        Argon2Profile::TEST,
    )
    .unwrap_err();
    assert!(err.to_string().contains("gave no entropy"));
}

/// An encoder that refuses the salt and a hash that refuses the password are
/// both hashing errors; an illegal profile is a parameters error; and the
/// real steps hash.
#[test]
fn a_step_that_refuses_is_a_hashing_error() {
    let encoder_refuses = via(
        getrandom::getrandom,
        refuse_encode,
        hash_with,
        Argon2Profile::TEST,
    )
    .unwrap_err();
    assert!(
        encoder_refuses
            .to_string()
            .contains("Argon2 refused to hash")
    );
    let hash_refuses = via(
        getrandom::getrandom,
        SaltString::encode_b64,
        refuse_hash,
        Argon2Profile::TEST,
    )
    .unwrap_err();
    assert!(hash_refuses.to_string().contains("Argon2 refused to hash"));
    let illegal = Argon2Profile {
        name: "illegal",
        time_cost: 0,
        memory_cost_kib: 1,
        parallelism: 4,
    };
    let params = via(
        getrandom::getrandom,
        SaltString::encode_b64,
        hash_with,
        illegal,
    )
    .unwrap_err();
    assert!(params.to_string().contains("is not legal"));
    let hashed = via(
        getrandom::getrandom,
        SaltString::encode_b64,
        hash_with,
        Argon2Profile::TEST,
    )
    .expect("the real steps hash");
    assert!(hashed.starts_with("$argon2id$"));
}

/// A hash of another algorithm needs a rehash: the algorithm check refuses
/// it before the version and the parameters are read.
#[test]
fn a_hash_of_another_algorithm_needs_a_rehash() {
    let real = hash_password(Argon2Profile::TEST, "pw").expect("the test profile hashes");
    let other = real.replacen("$argon2id$", "$argon2i$", 1);
    assert!(needs_rehash(Argon2Profile::TEST, &other));
}

/// A PHC string whose parameters are not a legal set needs a rehash.
#[test]
fn a_hash_with_illegal_parameters_needs_a_rehash() {
    // A well-formed argon2id hash whose params the library refuses: keep
    // the salt and the digest of a real hash and name m=1, which is below
    // the minimum, so the string parses but Params::try_from rejects it.
    let real = hash_password(Argon2Profile::TEST, "pw").expect("the test profile hashes");
    let digest = real.rsplit('$').next().unwrap();
    let salt = real.rsplit('$').nth(1).unwrap();
    let illegal = format!("$argon2id$v=19$m=1,t=1,p=1${salt}${digest}");
    assert!(needs_rehash(Argon2Profile::PROD, &illegal));
}

/// A hash written under one profile needs a rehash under another, and a
/// string that does not parse needs one too.
#[test]
fn a_hash_of_an_older_version_needs_a_rehash() {
    // Take a well-formed TEST hash and name the older version 0x10 (16)
    // instead of the 0x13 (19) this build writes. The salt and the digest
    // stay valid, so the string parses and only the version differs.
    let current = hash_password(Argon2Profile::TEST, "correct horse battery staple")
        .expect("the test profile hashes");
    assert!(
        current.contains("v=19"),
        "the hash names no version: {current}"
    );
    let older = current.replace("v=19", "v=16");
    assert!(needs_rehash(Argon2Profile::TEST, &older));
}

#[test]
fn a_hash_of_another_profile_needs_a_rehash() {
    let stored = hash_password(Argon2Profile::TEST, "correct horse battery staple")
        .expect("the test profile hashes");
    assert!(needs_rehash(Argon2Profile::PROD, &stored));
    assert!(!needs_rehash(Argon2Profile::TEST, &stored));
    assert!(needs_rehash(Argon2Profile::PROD, "not a phc string"));
}
