//! The connection configuration of the store: the URL and the two query
//! bounds, read from the environment or given by the caller.

use std::env::VarError;

use crate::StoreError;

/// The environment variable that holds the statement timeout, in milliseconds.
pub const STATEMENT_TIMEOUT_VAR: &str = "DB_STATEMENT_TIMEOUT_MS";

/// The statement timeout that applies when `DB_STATEMENT_TIMEOUT_MS` is absent.
///
/// `ACQUIRE_TIMEOUT` bounds the checkout of a connection and nothing after it.
/// A database that accepts the socket and answers no query therefore holds the
/// readiness probe of `cadus-web` and the tick of `cadus-worker` open without a
/// bound. `statement_timeout` adds the server-side bound: the backend cancels
/// the statement and reports SQLSTATE 57014. 5000 ms is longer than every M0
/// query and shorter than every scrape interval in `deploy/Caddyfile`.
pub const DEFAULT_STATEMENT_TIMEOUT_MS: u64 = 5000;

/// The environment variable that holds the client-side query bound, in
/// milliseconds.
pub const CLIENT_TIMEOUT_VAR: &str = "DB_CLIENT_TIMEOUT_MS";

/// The client-side query bound that applies when `DB_CLIENT_TIMEOUT_MS` is
/// absent.
///
/// `statement_timeout` is a server-side bound: the server cancels the statement
/// and reports SQLSTATE 57014, so that bound needs a live server. sqlx 0.9 sets
/// no TCP keepalive, so a server that disappears in the middle of a query
/// leaves the caller in a read that the kernel never ends. `bounded` adds the
/// client-side bound for that case. 10000 ms is longer than
/// `DEFAULT_STATEMENT_TIMEOUT_MS`, so a live server answers 57014 first and the
/// client-side bound stays the last resort.
pub const DEFAULT_CLIENT_TIMEOUT_MS: u64 = 10_000;

/// The environment variable that holds the connection string.
const DATABASE_URL_VAR: &str = "DATABASE_URL";

/// One raw read of an environment variable.
type VarRead = Result<String, VarError>;

/// The connection configuration of the store.
#[derive(Clone)]
pub struct DbConfig {
    pub database_url: String,
    /// The `statement_timeout` of every connection of the pool, in
    /// milliseconds. 0 turns the timeout off.
    pub statement_timeout_ms: u64,
    /// The client-side bound of one query, in milliseconds. 0 turns the bound
    /// off. `bounded` applies it.
    pub client_timeout_ms: u64,
}

impl DbConfig {
    /// Build a configuration from a connection string with the default
    /// statement timeout.
    pub fn new(database_url: impl Into<String>) -> Self {
        Self {
            database_url: database_url.into(),
            statement_timeout_ms: DEFAULT_STATEMENT_TIMEOUT_MS,
            client_timeout_ms: DEFAULT_CLIENT_TIMEOUT_MS,
        }
    }

    /// Read `DATABASE_URL`, `DB_STATEMENT_TIMEOUT_MS`, and
    /// `DB_CLIENT_TIMEOUT_MS` from the environment.
    ///
    /// The function returns `StoreError::Config` when `DATABASE_URL` is absent,
    /// empty, or not valid Unicode, and when `DB_STATEMENT_TIMEOUT_MS` or
    /// `DB_CLIENT_TIMEOUT_MS` holds anything other than a whole number of
    /// milliseconds. An absent bound variable gives the default of that
    /// bound.
    pub fn from_env() -> Result<Self, StoreError> {
        Self::from_reads(
            std::env::var(DATABASE_URL_VAR),
            std::env::var(STATEMENT_TIMEOUT_VAR),
            std::env::var(CLIENT_TIMEOUT_VAR),
        )
    }

    /// Build the configuration from the three raw variable reads of `from_env`.
    fn from_reads(url: VarRead, statement: VarRead, client: VarRead) -> Result<Self, StoreError> {
        let database_url = match url {
            Ok(url) if url.is_empty() => {
                return Err(StoreError::Config(format!("{DATABASE_URL_VAR} is empty")));
            }
            Ok(url) => url,
            Err(VarError::NotPresent) => {
                return Err(StoreError::Config(format!("{DATABASE_URL_VAR} is not set")));
            }
            Err(VarError::NotUnicode(_)) => return Err(not_unicode(DATABASE_URL_VAR)),
        };
        let raw = optional_var(STATEMENT_TIMEOUT_VAR, statement)?;
        let raw_client = optional_var(CLIENT_TIMEOUT_VAR, client)?;
        Ok(Self {
            database_url,
            statement_timeout_ms: parse_bound(STATEMENT_TIMEOUT_VAR, raw.as_deref())?,
            client_timeout_ms: parse_bound(CLIENT_TIMEOUT_VAR, raw_client.as_deref())?,
        })
    }
}

/// The error of a variable whose value is not valid Unicode.
fn not_unicode(name: &str) -> StoreError {
    StoreError::Config(format!("{name} is not valid Unicode"))
}

/// The raw value of an optional variable. An absent variable gives `None`.
fn optional_var(name: &str, read: VarRead) -> Result<Option<String>, StoreError> {
    match read {
        Ok(value) => Ok(Some(value)),
        Err(VarError::NotPresent) => Ok(None),
        Err(VarError::NotUnicode(_)) => Err(not_unicode(name)),
    }
}

/// Read one query bound from the raw value of its variable.
///
/// `None` means the variable is absent, so the default of that variable
/// applies. Every other value must be a whole number of milliseconds, and 0
/// turns the bound off. A value that is not a whole number is a configuration
/// error: the store never guesses a bound that an operator wrote by hand.
fn parse_bound(name: &str, raw: Option<&str>) -> Result<u64, StoreError> {
    let Some(value) = raw else {
        return Ok(default_of(name));
    };
    value.parse::<u64>().map_err(|_| {
        StoreError::Config(format!(
            "{name} must be a whole number of milliseconds, not {value:?}"
        ))
    })
}

/// The default of one bound variable.
fn default_of(name: &str) -> u64 {
    if name == CLIENT_TIMEOUT_VAR {
        DEFAULT_CLIENT_TIMEOUT_MS
    } else {
        DEFAULT_STATEMENT_TIMEOUT_MS
    }
}

/// The connection string holds the database password. Keep it out of every log
/// line and every panic message.
impl std::fmt::Debug for DbConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DbConfig")
            .field("database_url", &"<redacted>")
            .field("statement_timeout_ms", &self.statement_timeout_ms)
            .field("client_timeout_ms", &self.client_timeout_ms)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::env::VarError;
    use std::ffi::OsString;

    use super::{
        CLIENT_TIMEOUT_VAR, DATABASE_URL_VAR, DEFAULT_CLIENT_TIMEOUT_MS,
        DEFAULT_STATEMENT_TIMEOUT_MS, DbConfig, STATEMENT_TIMEOUT_VAR, parse_bound,
    };
    use crate::StoreError;

    /// The message of a configuration error, or the Display of any other error.
    fn message(err: StoreError) -> String {
        err.to_string()
    }

    /// A variable read of a value that is not valid Unicode.
    fn not_unicode() -> Result<String, VarError> {
        Err(VarError::NotUnicode(OsString::from("x")))
    }

    /// The URL read that every bound test uses.
    fn url() -> Result<String, VarError> {
        Ok("postgresql://h/d".to_string())
    }

    /// R4: an absent variable gives the documented default of 5000 ms.
    #[test]
    fn an_absent_statement_timeout_gives_the_default() {
        assert_eq!(parse_bound(STATEMENT_TIMEOUT_VAR, None).unwrap(), 5000);
        assert_eq!(DEFAULT_STATEMENT_TIMEOUT_MS, 5000);
        assert_eq!(DbConfig::new("postgresql://h/d").statement_timeout_ms, 5000);
    }

    /// The client-side bound follows the same three rules, with a default of
    /// 10000 ms. `DB_CLIENT_TIMEOUT_MS=300` gives the 300 ms bound that
    /// `tests/client_timeout.rs` applies.
    #[test]
    fn the_client_timeout_reads_the_same_three_rules() {
        assert_eq!(parse_bound(CLIENT_TIMEOUT_VAR, None).unwrap(), 10000);
        assert_eq!(DEFAULT_CLIENT_TIMEOUT_MS, 10000);
        assert_eq!(DbConfig::new("postgresql://h/d").client_timeout_ms, 10000);
        assert_eq!(parse_bound(CLIENT_TIMEOUT_VAR, Some("300")).unwrap(), 300);
        assert_eq!(parse_bound(CLIENT_TIMEOUT_VAR, Some("0")).unwrap(), 0);

        for raw in ["", "5s", "-1", "2.5", "10000ms"] {
            let err = parse_bound(CLIENT_TIMEOUT_VAR, Some(raw))
                .expect_err("a value that is not a whole number must be an error");
            assert_eq!(
                message(err),
                format!(
                    "configuration error: DB_CLIENT_TIMEOUT_MS must be a whole number of \
                     milliseconds, not {raw:?}"
                )
            );
        }
    }

    /// A whole number passes through unchanged. 0 turns the timeout off.
    #[test]
    fn a_whole_number_passes_through() {
        assert_eq!(
            parse_bound(STATEMENT_TIMEOUT_VAR, Some("200")).unwrap(),
            200
        );
        assert_eq!(parse_bound(STATEMENT_TIMEOUT_VAR, Some("0")).unwrap(), 0);
    }

    /// A value that is not a whole number is a configuration error. The store
    /// stops instead of a silent fall back to the default.
    #[test]
    fn a_value_that_is_not_a_whole_number_is_a_configuration_error() {
        for raw in ["", "5s", "-1", "2.5", "5000ms"] {
            let err = parse_bound(STATEMENT_TIMEOUT_VAR, Some(raw))
                .expect_err("a value that is not a whole number must be an error");
            assert_eq!(
                message(err),
                format!(
                    "configuration error: DB_STATEMENT_TIMEOUT_MS must be a whole number of \
                     milliseconds, not {raw:?}"
                )
            );
        }
    }

    /// The three reads of `from_env` map to the documented messages: an empty,
    /// an absent, and a non-Unicode `DATABASE_URL`, and a non-Unicode bound.
    #[test]
    fn every_bad_read_names_its_variable() {
        let cases: [(Result<String, VarError>, &str); 3] = [
            (Ok(String::new()), "DATABASE_URL is empty"),
            (Err(VarError::NotPresent), "DATABASE_URL is not set"),
            (not_unicode(), "DATABASE_URL is not valid Unicode"),
        ];
        for (read, expected) in cases {
            let err = DbConfig::from_reads(read, Err(VarError::NotPresent), Ok("1".to_string()))
                .expect_err("a bad URL read must be an error");
            assert_eq!(message(err), format!("configuration error: {expected}"));
        }
        let err = DbConfig::from_reads(url(), not_unicode(), Err(VarError::NotPresent))
            .expect_err("a non-Unicode bound must be an error");
        assert_eq!(
            message(err),
            "configuration error: DB_STATEMENT_TIMEOUT_MS is not valid Unicode"
        );
        let err = DbConfig::from_reads(url(), Err(VarError::NotPresent), not_unicode())
            .expect_err("a non-Unicode bound must be an error");
        assert_eq!(
            message(err),
            "configuration error: DB_CLIENT_TIMEOUT_MS is not valid Unicode"
        );
        // A non-numeric bound reaches `from_reads` through `parse_bound`.
        let err = DbConfig::from_reads(url(), Ok("nan".to_string()), Err(VarError::NotPresent))
            .expect_err("a non-numeric statement bound must be an error");
        assert_eq!(
            message(err),
            "configuration error: DB_STATEMENT_TIMEOUT_MS must be a whole number of \
             milliseconds, not \"nan\""
        );
        let err = DbConfig::from_reads(url(), Err(VarError::NotPresent), Ok("nan".to_string()))
            .expect_err("a non-numeric client bound must be an error");
        assert_eq!(
            message(err),
            "configuration error: DB_CLIENT_TIMEOUT_MS must be a whole number of \
             milliseconds, not \"nan\""
        );
    }

    /// Three good reads give the URL and both bounds; two absent bounds give
    /// the two defaults.
    #[test]
    fn good_reads_give_the_url_and_the_bounds() {
        let cfg = DbConfig::from_reads(url(), Ok("250".to_string()), Ok("0".to_string())).unwrap();
        assert_eq!(cfg.database_url, "postgresql://h/d");
        assert_eq!(cfg.statement_timeout_ms, 250);
        assert_eq!(cfg.client_timeout_ms, 0);

        let cfg = DbConfig::from_reads(url(), Err(VarError::NotPresent), Err(VarError::NotPresent))
            .unwrap();
        assert_eq!(cfg.statement_timeout_ms, DEFAULT_STATEMENT_TIMEOUT_MS);
        assert_eq!(cfg.client_timeout_ms, DEFAULT_CLIENT_TIMEOUT_MS);
    }

    /// `from_env` reads the three variables of this process and nothing else,
    /// so it gives the answer of `from_reads` over those three reads.
    #[test]
    fn from_env_is_from_reads_over_the_process_environment() {
        // The full text of one outcome: the redacted Debug form and the URL, or
        // the message. It runs on a known-good config here, and on the two
        // process-environment reads below.
        fn shown(outcome: Result<DbConfig, StoreError>) -> Result<String, String> {
            outcome
                .map(|cfg| format!("{cfg:?}{}", cfg.database_url))
                .map_err(message)
        }
        let good = DbConfig::new("postgresql://h/d");
        assert_eq!(
            shown(Ok(good.clone())),
            Ok(format!("{good:?}postgresql://h/d"))
        );
        // `from_env` reads exactly the three variables, so it gives the answer
        // of `from_reads` over the reads of those three.
        assert_eq!(
            shown(DbConfig::from_env()),
            shown(DbConfig::from_reads(
                std::env::var(DATABASE_URL_VAR),
                std::env::var(STATEMENT_TIMEOUT_VAR),
                std::env::var(CLIENT_TIMEOUT_VAR),
            )),
        );
    }

    /// The Debug form redacts the connection string and shows both bounds.
    #[test]
    fn debug_redacts_the_url() {
        let cfg = DbConfig::new("postgresql://u:secret@h/d");
        assert_eq!(
            format!("{cfg:?}"),
            "DbConfig { database_url: \"<redacted>\", statement_timeout_ms: 5000, \
             client_timeout_ms: 10000 }"
        );
    }
}
