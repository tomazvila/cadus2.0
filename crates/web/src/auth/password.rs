//! Argon2id hashing, the two parameter profiles, and the length-only password
//! policy.
//!
//! Spec `docs/reference/web-service-1.0-spec.md` section 3.1, rows "Password
//! policy" and "Argon2id", and section 10, row "Argon2id prod / password /
//! token" (1.0 `passwords.py:33-37`, `:55-56`, `:64-69`, `:134-145`).
//!
//! **The policy is length only.** NIST 800-63B drops the composition rules — no
//! demand for a digit, a case mix, or a symbol. What is left is a floor of 8
//! characters, a ceiling of 256 characters, and a ceiling of 1024 UTF-8 bytes.
//! The byte ceiling is not a usability rule; it bounds the work that Argon2 does
//! over the input, so a one-megabyte "password" cannot become a denial of
//! service lever.
//!
//! **Two profiles, one algorithm.** `prod` is the section 3.1 target of about 50
//! to 100 ms per verify on server hardware. `test` runs the same Argon2id at a
//! cost the suite can pay on every login. A profile is a parameter, never a
//! read of the environment inside these functions: the process resolves it once
//! at boot with [`Argon2Profile::from_env`] and passes it down.
//!
//! **A verify never reads the active profile.** It reads the parameters out of
//! the stored PHC string, so a hash written under any profile still verifies.
//! [`needs_rehash`] is the separate question the login path asks afterwards.

use std::ffi::OsString;

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};

/// The floor of the password policy, in characters.
pub const MIN_PASSWORD_LENGTH: usize = 8;

/// The ceiling of the password policy, in characters.
pub const MAX_PASSWORD_LENGTH: usize = 256;

/// The ceiling of the password policy, in UTF-8 bytes.
pub const MAX_PASSWORD_BYTES: usize = 1024;

/// The environment variable that names the active Argon2id profile.
pub const ARGON2_PROFILE_VAR: &str = "CADUS_AUTH_ARGON2_PROFILE";

/// The number of random bytes in an Argon2id salt. It is the RustCrypto and the
/// RFC 9106 recommendation.
const SALT_BYTES: usize = 16;

/// One Argon2id parameter set, under a name an operator can select.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Argon2Profile {
    /// The name that `CADUS_AUTH_ARGON2_PROFILE` selects.
    pub name: &'static str,
    /// The iteration count.
    pub time_cost: u32,
    /// The memory in kibibytes.
    pub memory_cost_kib: u32,
    /// The number of lanes.
    pub parallelism: u32,
}

impl Argon2Profile {
    /// The production profile of spec section 3.1: 3 iterations, 64 MiB, 4
    /// lanes.
    pub const PROD: Self = Self {
        name: "prod",
        time_cost: 3,
        memory_cost_kib: 64 * 1024,
        parallelism: 4,
    };

    /// The test profile of spec section 3.1: 1 iteration, 8 MiB, 1 lane. Same
    /// algorithm, a cost the suite can pay on every login.
    pub const TEST: Self = Self {
        name: "test",
        time_cost: 1,
        memory_cost_kib: 8 * 1024,
        parallelism: 1,
    };

    /// Every profile an operator can name, in the order the error text lists
    /// them.
    pub const ALL: [Self; 2] = [Self::PROD, Self::TEST];

    /// Resolve the profile from the raw value of `CADUS_AUTH_ARGON2_PROFILE`.
    ///
    /// The function is pure: it reads no environment, so a test drives every
    /// branch. The binary calls it with `std::env::var_os`.
    ///
    /// `None` and an empty value give [`Self::PROD`]. Any name that no profile
    /// carries is a start error, never a silent fall back to the default: an
    /// operator who writes `CADUS_AUTH_ARGON2_PROFILE=prd` must hear about it,
    /// and a deployment that quietly ran the `test` parameters would hash every
    /// password at 8 MiB.
    pub fn from_env(raw: Option<OsString>) -> Result<Self, PasswordError> {
        let Some(raw) = raw else {
            return Ok(Self::PROD);
        };
        let value = raw
            .into_string()
            .map_err(|_| PasswordError::UnknownProfile { value: None })?;
        let name = value.trim();
        if name.is_empty() {
            return Ok(Self::PROD);
        }
        Self::ALL
            .into_iter()
            .find(|profile| profile.name == name)
            .ok_or_else(|| PasswordError::UnknownProfile {
                value: Some(name.to_string()),
            })
    }

    /// The Argon2id hasher of this profile.
    fn hasher(self) -> Result<Argon2<'static>, PasswordError> {
        let params = Params::new(self.memory_cost_kib, self.time_cost, self.parallelism, None)
            .map_err(|error| PasswordError::Parameters {
                profile: self.name,
                reason: error.to_string(),
            })?;
        Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
    }
}

/// A password that the policy refuses.
///
/// Unit U4 maps every variant to one answer: `422 weak_password` (spec section
/// 10, row "Weak password / wrong current password").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WeakPassword {
    /// Fewer than [`MIN_PASSWORD_LENGTH`] characters.
    TooShort,
    /// More than [`MAX_PASSWORD_BYTES`] UTF-8 bytes.
    TooManyBytes,
    /// More than [`MAX_PASSWORD_LENGTH`] characters.
    TooLong,
}

impl std::fmt::Display for WeakPassword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort => write!(
                f,
                "Password must be at least {MIN_PASSWORD_LENGTH} characters."
            ),
            Self::TooManyBytes => write!(
                f,
                "Password must be at most {MAX_PASSWORD_BYTES} UTF-8 bytes."
            ),
            Self::TooLong => write!(
                f,
                "Password must be at most {MAX_PASSWORD_LENGTH} characters."
            ),
        }
    }
}

impl std::error::Error for WeakPassword {}

/// A failure of the hashing machinery itself, never a wrong password.
///
/// Unit U4 maps every variant to `500`. None of them tells the client anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasswordError {
    /// `CADUS_AUTH_ARGON2_PROFILE` names no profile. `None` means the value is
    /// not valid Unicode.
    UnknownProfile {
        /// The value, as the operator wrote it.
        value: Option<String>,
    },
    /// The profile's numbers are not a legal Argon2 parameter set.
    Parameters {
        /// The name of the refused profile.
        profile: &'static str,
        /// The text of the underlying error.
        reason: String,
    },
    /// The kernel gave no entropy for the salt.
    Entropy {
        /// The text of the underlying `getrandom` error.
        reason: String,
    },
    /// Argon2 refused to hash. The text never holds the password.
    Hashing {
        /// The text of the underlying error.
        reason: String,
    },
}

impl std::fmt::Display for PasswordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownProfile { value: Some(value) } => {
                let names: Vec<&str> = Argon2Profile::ALL.iter().map(|p| p.name).collect();
                write!(
                    f,
                    "{ARGON2_PROFILE_VAR}={value:?} is not a known Argon2 profile (expected one \
                     of {names:?})"
                )
            }
            Self::UnknownProfile { value: None } => {
                write!(f, "{ARGON2_PROFILE_VAR} is not valid Unicode")
            }
            Self::Parameters { profile, reason } => {
                write!(f, "the {profile} Argon2 profile is not legal: {reason}")
            }
            Self::Entropy { reason } => {
                write!(f, "the operating system gave no entropy: {reason}")
            }
            Self::Hashing { reason } => write!(f, "Argon2 refused to hash: {reason}"),
        }
    }
}

impl std::error::Error for PasswordError {}

/// Apply the length-only policy of spec section 3.1.
///
/// The byte ceiling runs BEFORE the character ceiling, and the order is part of
/// the contract: a multi-byte password can pass 256 characters and still carry
/// more than 1024 bytes, so the byte rule must be the one that fires. 1.0 orders
/// the two the same way (`passwords.py:97-103`).
///
/// The character count is a count of Unicode scalar values, which is what
/// Python's `len` counts over a `str`.
///
/// This function does not hash. Callers run it first, then
/// [`hash_password`].
pub fn validate_password(password: &str) -> Result<(), WeakPassword> {
    if password.chars().count() < MIN_PASSWORD_LENGTH {
        return Err(WeakPassword::TooShort);
    }
    if password.len() > MAX_PASSWORD_BYTES {
        return Err(WeakPassword::TooManyBytes);
    }
    if password.chars().count() > MAX_PASSWORD_LENGTH {
        return Err(WeakPassword::TooLong);
    }
    Ok(())
}

/// Hash `password` under `profile` and give the PHC string to store.
///
/// The answer names the algorithm, the version, the parameters, and the salt, so
/// [`verify_password`] and [`needs_rehash`] need nothing else.
///
/// The salt is 16 fresh bytes from the operating system. This function does NOT
/// apply the policy; call [`validate_password`] first.
pub fn hash_password(profile: Argon2Profile, password: &str) -> Result<String, PasswordError> {
    hash_password_with(getrandom::getrandom, profile, password)
}

/// Hash `password` under `profile`, but take the salt source as an argument.
///
/// [`hash_password`] calls it with `getrandom::getrandom`. A unit test passes a
/// fill that refuses, so the entropy-failure arm is reached without a live
/// kernel that gives no entropy.
fn hash_password_with(
    fill: impl FnOnce(&mut [u8]) -> Result<(), getrandom::Error>,
    profile: Argon2Profile,
    password: &str,
) -> Result<String, PasswordError> {
    let mut salt_bytes = [0u8; SALT_BYTES];
    fill(&mut salt_bytes).map_err(|error| PasswordError::Entropy {
        reason: error.to_string(),
    })?;
    let salt = SaltString::encode_b64(&salt_bytes).map_err(|error| PasswordError::Hashing {
        reason: error.to_string(),
    })?;
    let hashed = profile
        .hasher()?
        .hash_password(password.as_bytes(), &salt)
        .map_err(|error| PasswordError::Hashing {
            reason: error.to_string(),
        })?;
    Ok(hashed.to_string())
}

/// Whether `password` matches the stored PHC string `hashed`.
///
/// The answer is `false` for a wrong password AND for a stored string that does
/// not parse. The function never gives an error, so a caller cannot tell the two
/// apart, and neither can a client watching the answers. The anti-enumeration
/// dummy verify of spec section 3.3 relies on that: it verifies against a fixed
/// hash and always gets `false`, at the same cost as a real verify.
///
/// The verify reads the parameters out of `hashed`, not out of any profile, so a
/// hash written under `prod` still verifies in a process running `test`.
pub fn verify_password(hashed: &str, password: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(hashed) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

/// Whether `hashed` should be written again under `profile`.
///
/// The answer is `true` when the stored string carries another algorithm,
/// another version, or any parameter that differs from the profile, and `true`
/// when it does not parse at all — an unreadable hash cannot be trusted.
///
/// The login path asks this AFTER a successful verify and, on a `true`, writes a
/// fresh hash. That upgrades a stored credential without asking the account
/// holder for anything.
pub fn needs_rehash(profile: Argon2Profile, hashed: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(hashed) else {
        return true;
    };
    if !matches!(
        Algorithm::try_from(parsed.algorithm),
        Ok(Algorithm::Argon2id)
    ) {
        return true;
    }
    if parsed.version != Some(Version::V0x13 as u32) {
        return true;
    }
    let Ok(params) = Params::try_from(&parsed) else {
        return true;
    };
    params.m_cost() != profile.memory_cost_kib
        || params.t_cost() != profile.time_cost
        || params.p_cost() != profile.parallelism
}

#[cfg(test)]
mod cov_tests {
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
        assert!(matches!(
            hash_password(illegal, "correct horse battery staple"),
            Err(PasswordError::Parameters {
                profile: "illegal",
                ..
            })
        ));
    }

    /// A salt fill that refuses is an entropy error and no hash.
    #[test]
    fn a_refused_salt_fill_is_an_entropy_error() {
        let outcome = hash_password_with(
            |_| Err(getrandom::Error::UNSUPPORTED),
            Argon2Profile::TEST,
            "correct horse battery staple",
        );
        assert!(matches!(outcome, Err(PasswordError::Entropy { .. })));
    }

    /// A hash written under one profile needs a rehash under another, and a
    /// string that does not parse needs one too.
    #[test]
    fn a_hash_of_another_profile_needs_a_rehash() {
        let stored = hash_password(Argon2Profile::TEST, "correct horse battery staple")
            .expect("the test profile hashes");
        assert!(needs_rehash(Argon2Profile::PROD, &stored));
        assert!(!needs_rehash(Argon2Profile::TEST, &stored));
        assert!(needs_rehash(Argon2Profile::PROD, "not a phc string"));
    }
}
