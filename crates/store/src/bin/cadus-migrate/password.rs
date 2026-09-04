//! The role passwords of `--admin-login`: the character rule and the two
//! variable reads (finding #14).

use std::env::VarError;

use cadus_store::StoreError;

/// The environment variable that holds the new password of `cadus_app`.
pub const APP_PASSWORD_VAR: &str = "CADUS_APP_PASSWORD";

/// The environment variable that holds the new password of `cadus_admin`.
pub const ADMIN_PASSWORD_VAR: &str = "CADUS_ADMIN_PASSWORD";

/// The least number of characters of a role password.
///
/// `openssl rand -hex 24` gives 48 characters, so the generator of the message
/// below stays well above this bound.
const PASSWORD_MIN_LEN: usize = 16;

/// The most characters of a role password.
const PASSWORD_MAX_LEN: usize = 128;

/// One raw read of an environment variable.
type VarRead = Result<String, VarError>;

/// Check every password variable of this run against the character rule.
///
/// The function returns the message of the first variable that breaks the rule.
/// An absent variable, an empty variable, and a variable that is not valid
/// Unicode pass this check: `password_from_env` reports those three with its own
/// message, and that message names the defect better than this one.
pub fn check_password_rule() -> Result<(), String> {
    for var in [APP_PASSWORD_VAR, ADMIN_PASSWORD_VAR] {
        check_password_read(var, std::env::var(var))?;
    }
    Ok(())
}

/// Check one password read against the character rule.
fn check_password_read(var: &str, read: VarRead) -> Result<(), String> {
    match read {
        Ok(value) if value.is_empty() => Ok(()),
        Ok(value) if password_follows_rule(&value) => Ok(()),
        Ok(_) => Err(password_rule_message(var)),
        Err(_) => Ok(()),
    }
}

/// Report whether a password holds allowed characters only and a length inside
/// the bounds.
///
/// The allowed set is `A-Z a-z 0-9 - _`. Every character of that set goes
/// through a `postgresql://user:password@host/db` URL unchanged, so the value
/// that reaches the role is the value that the runtime DSN carries. `@` ends the
/// user information, `#` starts a fragment, and `%` opens a percent escape, so
/// each of those three makes the DSN name another host or another password with
/// no error at all (finding #14).
///
/// The length bound is the second half of the rule: a short password is weak,
/// and a long one is a paste mistake.
fn password_follows_rule(password: &str) -> bool {
    let length = password.chars().count();
    if !(PASSWORD_MIN_LEN..=PASSWORD_MAX_LEN).contains(&length) {
        return false;
    }
    password
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Build the message that a password outside the rule prints.
///
/// The message names the variable, the allowed set, the bounds, and one command
/// that gives a value which passes.
fn password_rule_message(var: &str) -> String {
    format!(
        "{var} holds a character outside [A-Za-z0-9_-] or a length outside \
         {PASSWORD_MIN_LEN}..={PASSWORD_MAX_LEN}; generate one with: openssl rand -hex 24"
    )
}

/// Read a password variable.
///
/// The function returns `None` when the variable is absent, and
/// `StoreError::Config` when the variable is empty or not valid Unicode. An
/// empty password is a configuration mistake, not a request to clear the
/// password, so the program stops instead of guessing.
pub fn password_from_env(var: &str) -> Result<Option<String>, StoreError> {
    password_from_read(var, std::env::var(var))
}

/// The password of one variable read. `var` names the variable in the error.
fn password_from_read(var: &str, read: VarRead) -> Result<Option<String>, StoreError> {
    match read {
        Ok(value) if value.is_empty() => Err(StoreError::Config(format!("{var} is empty"))),
        Ok(value) => Ok(Some(value)),
        Err(VarError::NotPresent) => Ok(None),
        Err(VarError::NotUnicode(_)) => {
            Err(StoreError::Config(format!("{var} is not valid Unicode")))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::env::VarError;
    use std::ffi::OsString;

    use super::{
        APP_PASSWORD_VAR, check_password_read, check_password_rule, password_follows_rule,
        password_from_env, password_from_read, password_rule_message,
    };

    /// Finding #14: a character outside `[A-Za-z0-9_-]` breaks the rule. Each
    /// character below changes the meaning of a DSN.
    #[test]
    fn a_character_outside_the_set_breaks_the_rule() {
        for password in [
            "corr@horse#battery1",
            "0123456789abcdef@",
            "0123456789abcdef#",
            "0123456789abcdef%",
            "0123456789abcdef/",
            "0123456789abcdef:",
            "0123456789abcdef?",
            "0123456789 abcdef",
            "0123456789abcdef'",
            "0123456789abcdéf",
        ] {
            assert!(
                !password_follows_rule(password),
                "{password:?} must break the rule"
            );
        }
    }

    /// Finding #14: the length bound is 16..=128, and both ends are inclusive.
    #[test]
    fn the_length_bounds_are_sixteen_and_one_hundred_twenty_eight() {
        assert!(!password_follows_rule(&"a".repeat(15)));
        assert!(password_follows_rule(&"a".repeat(16)));
        assert!(password_follows_rule(&"a".repeat(128)));
        assert!(!password_follows_rule(&"a".repeat(129)));
        assert!(!password_follows_rule(""));
    }

    /// Finding #14: the output of the generator that the message names passes
    /// the rule, and so does every character of the allowed set.
    #[test]
    fn the_generated_password_follows_the_rule() {
        // 48 characters, the length of `openssl rand -hex 24`.
        assert!(password_follows_rule(
            "9f2c1d4b7a6e0358cf91d24e7b60a5c38d1f4e29b70c6a55"
        ));
        assert!(password_follows_rule(
            "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_"
        ));
    }

    /// Finding #14: the message names the variable, the set, the bounds, and
    /// the generator.
    #[test]
    fn the_rule_message_is_the_literal_line() {
        assert_eq!(
            password_rule_message("CADUS_APP_PASSWORD"),
            "CADUS_APP_PASSWORD holds a character outside [A-Za-z0-9_-] or a length outside \
             16..=128; generate one with: openssl rand -hex 24"
        );
    }

    /// The rule check passes an absent, an empty, and a non-Unicode read, and
    /// refuses a value outside the rule with the message of its variable.
    #[test]
    fn the_rule_check_refuses_a_present_value_outside_the_rule_only() {
        let ok = |read| check_password_read(APP_PASSWORD_VAR, read).is_ok();
        assert!(ok(Err(VarError::NotPresent)));
        assert!(ok(Err(VarError::NotUnicode(OsString::from("x")))));
        assert!(ok(Ok(String::new())));
        assert!(ok(Ok("0123456789abcdef".to_string())));
        assert_eq!(
            check_password_read(APP_PASSWORD_VAR, Ok("short".to_string())),
            Err(password_rule_message(APP_PASSWORD_VAR))
        );
        // The process environment of a test run holds no password variable, so
        // the whole-environment check passes.
        assert_eq!(check_password_rule(), Ok(()));
    }

    /// A password read gives the value, `None` for an absent variable, and a
    /// configuration error for an empty or non-Unicode one.
    #[test]
    fn a_password_read_maps_its_four_shapes() {
        assert_eq!(
            password_from_read("V", Ok("pw".to_string())).unwrap(),
            Some("pw".to_string())
        );
        assert_eq!(
            password_from_read("V", Err(VarError::NotPresent)).unwrap(),
            None
        );
        assert_eq!(
            password_from_read("V", Ok(String::new()))
                .unwrap_err()
                .to_string(),
            "configuration error: V is empty"
        );
        assert_eq!(
            password_from_read("V", Err(VarError::NotUnicode(OsString::from("x"))))
                .unwrap_err()
                .to_string(),
            "configuration error: V is not valid Unicode"
        );
        assert_eq!(password_from_env("CADUS_NO_SUCH_PASSWORD").unwrap(), None);
    }
}
