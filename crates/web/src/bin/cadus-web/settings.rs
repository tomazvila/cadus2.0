//! The configuration of one process: every environment value the start
//! sequence reads BEFORE the pool opens, so a bad value stops the process at
//! once instead of after the connect timeout.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use cadus_core::curriculum::{CurriculumError, LoadError, load_curriculum};
use cadus_store::{Db, DbConfig};
use cadus_web::auth::oauth::OAuthConfig;
use cadus_web::auth::password::{ARGON2_PROFILE_VAR, Argon2Profile};
use cadus_web::cookie::{CookiePosture, INSECURE_COOKIE_VAR};
use cadus_web::origin::{OriginPolicy, PUBLIC_ORIGIN_VAR};
use cadus_web::state::Content;
use cadus_web::{AppState, BIND_ADDR_VAR};

use super::Fatal;

/// The environment variable that bounds the drain after the stop signal.
const SHUTDOWN_DEADLINE_VAR: &str = "SHUTDOWN_DEADLINE_SECS";

/// The drain deadline in seconds when `SHUTDOWN_DEADLINE_SECS` is absent.
pub(super) const DEFAULT_SHUTDOWN_DEADLINE_SECS: u64 = 10;

/// The environment variable that names the admin connection of the content
/// store (M6 R5).
///
/// `cadus_app` holds SELECT on `content_store` and nothing else, so the approve
/// route and the reject route need a DSN of `cadus_admin`. An absent variable
/// leaves both writes closed with `503 admin_path_unavailable`, and every other
/// route is unchanged.
pub(super) const ADMIN_DSN_VAR: &str = "CADUS_ADMIN_DATABASE_URL";

/// The environment variable that names the curriculum tree (M5 U6).
const CURRICULUM_ENV: &str = "CADUS_CURRICULUM";

/// The curriculum tree when `CADUS_CURRICULUM` is absent.
const DEFAULT_CURRICULUM: &str = "curriculum";

/// Every value the start sequence reads from the environment.
pub(super) struct Settings {
    /// The tenant pool and its two bounds.
    pub cfg: DbConfig,
    /// The address to bind.
    pub addr: String,
    /// The one stop budget of the drain and the pool close.
    pub deadline: Duration,
    /// The session-cookie posture.
    pub posture: CookiePosture,
    /// The Argon2id parameter profile of the auth routes.
    pub argon2: Argon2Profile,
    /// The OAuth providers this deployment serves.
    pub oauth: OAuthConfig,
    /// How the CSRF origin layer names this deployment's own origin.
    pub origin: OriginPolicy,
    /// The curriculum and the scheduler config.
    pub content: Arc<Content>,
}

impl Settings {
    /// Read every value, in the order of the module note.
    pub fn read() -> Result<Self, Fatal> {
        let cfg = DbConfig::from_env().map_err(Fatal::startup)?;
        let addr = bind_addr()?;
        let deadline = shutdown_deadline()?;
        let posture = cookie_posture()?;
        let argon2 = argon2_profile()?;
        let oauth = oauth_config();
        let origin = origin_policy()?;
        // M5 U6. The dashboard, the graph, and the session plan compose against
        // the curriculum, so the tree loads BEFORE the pool opens and a tree that
        // does not load stops the start with exit code 2. `cadus-worker` follows
        // the same rule: a service that serves an empty graph gives the operator
        // one warn line and no other signal.
        //
        // The read comes AFTER every environment parse above. Those parses cost
        // nothing, so a typo in a variable is reported before this file read runs.
        let content = Arc::new(load_content()?);
        Ok(Self {
            cfg,
            addr,
            deadline,
            posture,
            argon2,
            oauth,
            origin,
            content,
        })
    }

    /// The application state of these settings on `db`.
    pub fn state(&self, db: Db) -> AppState {
        AppState::new(db)
            .with_posture(self.posture)
            .with_origin(self.origin.clone())
            .with_content(Arc::clone(&self.content))
            .with_argon2(self.argon2)
            .with_oauth(self.oauth.clone())
    }
}

/// The cookie-posture guard (spec section 3.1, "Guards"; unit U1).
///
/// A `__Host-` cookie without `Secure` is discarded by the browser without a
/// word, so the login appears to work and no session ever persists (trap W9).
/// An insecure posture must be a deliberate choice, never an accident, so both
/// the guard and a bad value of the knob stop the start here.
fn cookie_posture() -> Result<CookiePosture, Fatal> {
    // `from_env` gives one of the two library postures, and both pass
    // `assert_safe`: the insecure one carries no `__Host-` prefix.
    let posture =
        CookiePosture::from_env(std::env::var_os(INSECURE_COOKIE_VAR)).map_err(Fatal::startup)?;
    if !posture.secure {
        tracing::warn!(
            "cadus-web: {INSECURE_COOKIE_VAR}=1, so the session cookie is {} without Secure; use \
             this for local http:// development only",
            posture.name
        );
    }
    Ok(posture)
}

/// The Argon2id parameter profile of the auth routes (spec section 3.1).
///
/// A bad value stops the start: a silent fallback to the fast test parameters
/// would ship a production deployment with a cheap password hash.
fn argon2_profile() -> Result<Argon2Profile, Fatal> {
    let argon2 =
        Argon2Profile::from_env(std::env::var_os(ARGON2_PROFILE_VAR)).map_err(Fatal::startup)?;
    if argon2 != Argon2Profile::PROD {
        tracing::warn!(
            "cadus-web: {ARGON2_PROFILE_VAR}={}, so the password hash uses the {} parameters; use              this for tests only",
            argon2.name,
            argon2.name
        );
    }
    Ok(argon2)
}

/// The OAuth providers this deployment serves (M5 U5).
///
/// A provider needs both credential halves AND an installed transport, and
/// this build installs no transport, so both OAuth routes answer 404 today. The
/// warn line tells the operator that the credentials they set serve nothing
/// yet.
fn oauth_config() -> OAuthConfig {
    let oauth = OAuthConfig::from_env(|name| std::env::var(name).ok());
    if oauth.google.is_some() || oauth.github.is_some() {
        tracing::warn!(
            "cadus-web: OAuth credentials are set, and this build installs no provider transport, \
             so /api/auth/oauth answers 404"
        );
    }
    oauth
}

/// How the CSRF origin layer names this deployment's own origin (trap W10).
fn origin_policy() -> Result<OriginPolicy, Fatal> {
    let origin =
        OriginPolicy::from_env(std::env::var_os(PUBLIC_ORIGIN_VAR)).map_err(Fatal::startup)?;
    if origin.public_origin.is_none() {
        tracing::info!(
            "cadus-web: {PUBLIC_ORIGIN_VAR} is not set, so the CSRF origin check rebuilds the \
             origin from X-Forwarded-Proto and Host; a proxy that drops X-Forwarded-Proto then \
             makes an https deployment rebuild as http://"
        );
    }
    Ok(origin)
}

/// Read the curriculum tree that `CADUS_CURRICULUM` names.
///
/// A tree that does not load is fatal: `/api/status`, `/api/graph`,
/// `/api/modules`, and `/api/session/plan` all compose against it, and an empty
/// graph serves an empty dashboard with no error anywhere.
fn load_content() -> Result<Content, Fatal> {
    let path = PathBuf::from(
        std::env::var(CURRICULUM_ENV).unwrap_or_else(|_| DEFAULT_CURRICULUM.to_string()),
    );
    match load_curriculum(&path) {
        Ok((curriculum, findings)) => {
            // Bind the field values before the event, so the covered start path
            // evaluates them and no field hides in a lazy log expression.
            let shown = path.display().to_string();
            let topics = curriculum.topic_count();
            let findings = findings.len();
            tracing::info!(path = %shown, topics, findings, "cadus-web: curriculum is loaded");
            Ok(Content::new(curriculum))
        }
        Err(err) => {
            let reason = first_reason(&err);
            let shown = path.display().to_string();
            tracing::error!(
                path = %shown,
                error = %reason,
                "cadus-web: the curriculum did not load; set CADUS_CURRICULUM to a tree that does"
            );
            Err(Fatal::Startup(format!(
                "the curriculum at {} did not load: {reason}",
                path.display()
            )))
        }
    }
}

/// The first finding of a load error, or the error itself.
///
/// A fatal parse stage carries every finding, and the joined text of a large
/// tree runs to many lines. The first one names the file the operator must fix.
fn first_reason(err: &LoadError) -> String {
    let LoadError::Curriculum(CurriculumError::FatalFindings { findings }) = err else {
        return err.to_string();
    };
    match findings.first() {
        Some(finding) => format!("[{}] {}", finding.code, finding.message),
        None => err.to_string(),
    }
}

/// The configuration of the admin connection, or `None` when the operator set
/// no `CADUS_ADMIN_DATABASE_URL`.
///
/// The two bounds of the tenant pool `cfg` apply to this pool too: the function
/// keeps them and replaces the connection string alone. A value that is empty
/// or not valid Unicode is a start error, because a silent fallback would leave
/// the review writes closed with no word to the operator.
pub(super) fn admin_dsn(cfg: &DbConfig) -> Result<Option<DbConfig>, Fatal> {
    Ok(
        admin_dsn_from(std::env::var(ADMIN_DSN_VAR))?.map(|url| DbConfig {
            database_url: url,
            ..cfg.clone()
        }),
    )
}

/// The admin connection string from the raw lookup of
/// `CADUS_ADMIN_DATABASE_URL`.
///
/// The function takes the lookup instead of reading the environment, so a unit
/// test drives the empty, the absent, and the not-Unicode branch with no live
/// environment.
fn admin_dsn_from(raw: Result<String, std::env::VarError>) -> Result<Option<String>, Fatal> {
    let raw = match raw {
        Ok(url) if url.is_empty() => {
            return Err(Fatal::Startup(format!("{ADMIN_DSN_VAR} is empty")));
        }
        Ok(url) => url,
        Err(std::env::VarError::NotPresent) => return Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err(Fatal::Startup(format!(
                "{ADMIN_DSN_VAR} is not valid Unicode"
            )));
        }
    };
    Ok(Some(raw))
}

/// Read `BIND_ADDR`, or use the default.
///
/// The rules live in `cadus_web::bind_addr`, a pure function. This wrapper only
/// reads the environment and maps the error to an exit code, so the unit tests
/// of the library cover every rule without a bind (item FIX10b/a).
fn bind_addr() -> Result<String, Fatal> {
    cadus_web::bind_addr(std::env::var_os(BIND_ADDR_VAR)).map_err(Fatal::startup)
}

/// Read `SHUTDOWN_DEADLINE_SECS`, or use the default of 10 seconds.
///
/// A present value that is not a positive whole number of seconds is a start
/// error, because a silent fallback hides an operator mistake.
fn shutdown_deadline() -> Result<Duration, Fatal> {
    let raw = match std::env::var(SHUTDOWN_DEADLINE_VAR) {
        Ok(raw) => raw,
        Err(std::env::VarError::NotPresent) => {
            return Ok(Duration::from_secs(DEFAULT_SHUTDOWN_DEADLINE_SECS));
        }
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err(Fatal::Startup(format!(
                "{SHUTDOWN_DEADLINE_VAR} is not valid Unicode"
            )));
        }
    };
    deadline_secs(raw.trim()).map(Duration::from_secs)
}

/// The whole number of seconds `trimmed` names, at least 1.
fn deadline_secs(trimmed: &str) -> Result<u64, Fatal> {
    if trimmed.is_empty() {
        return Err(Fatal::Startup(format!(
            "{SHUTDOWN_DEADLINE_VAR} is empty; give a whole number of seconds or remove the \
             variable"
        )));
    }
    let secs: u64 = trimmed.parse().map_err(|_| {
        Fatal::Startup(format!(
            "{SHUTDOWN_DEADLINE_VAR} must be a whole number of seconds, not {trimmed:?}"
        ))
    })?;
    if secs == 0 {
        return Err(Fatal::Startup(format!(
            "{SHUTDOWN_DEADLINE_VAR} must be 1 or more"
        )));
    }
    Ok(secs)
}

#[cfg(test)]
mod tests {
    use cadus_core::curriculum::{CurriculumError, Finding, LoadError};

    use super::{Fatal, admin_dsn_from, deadline_secs, first_reason};

    impl Fatal {
        /// The text a fatal carries, for the assertions below. Both arms run.
        fn text(&self) -> &str {
            match self {
                Fatal::Startup(message) => message,
                Fatal::RlsBypass { role } => role,
            }
        }
    }

    /// The text reader visits both fatal variants.
    #[test]
    fn the_fatal_text_reads_both_variants() {
        assert_eq!(Fatal::Startup("boom".to_string()).text(), "boom");
        assert_eq!(
            Fatal::RlsBypass {
                role: "super".to_string(),
            }
            .text(),
            "super"
        );
    }

    /// An absent admin DSN is no admin path; an empty one and a not-Unicode one
    /// are start errors.
    #[test]
    fn the_admin_dsn_reads_the_present_value() {
        assert_eq!(
            admin_dsn_from(Ok("postgresql://admin@db/cadus".to_string()))
                .ok()
                .unwrap()
                .as_deref(),
            Some("postgresql://admin@db/cadus")
        );
    }

    #[test]
    fn the_admin_dsn_reads_the_three_no_database_branches() {
        assert!(
            admin_dsn_from(Err(std::env::VarError::NotPresent))
                .ok()
                .unwrap()
                .is_none()
        );
        assert!(
            admin_dsn_from(Ok(String::new()))
                .unwrap_err()
                .text()
                .contains("is empty")
        );
        assert!(
            admin_dsn_from(Err(std::env::VarError::NotUnicode(
                std::ffi::OsString::from("x"),
            )))
            .unwrap_err()
            .text()
            .contains("not valid Unicode")
        );
    }

    /// The deadline reader takes a positive whole number and refuses an empty,
    /// a non-number, and a zero value.
    #[test]
    fn the_deadline_reader_takes_a_positive_whole_number() {
        assert_eq!(deadline_secs("7").ok().unwrap(), 7);
        assert!(deadline_secs("").unwrap_err().text().contains("is empty"));
        assert!(
            deadline_secs("many")
                .unwrap_err()
                .text()
                .contains("whole number of seconds")
        );
        assert!(deadline_secs("0").unwrap_err().text().contains("1 or more"));
    }

    /// The first reason names the first finding of a fatal-findings error, and
    /// prints an empty-findings error and any other load error whole.
    #[test]
    fn the_first_reason_names_the_first_finding_or_the_error() {
        let named = LoadError::Curriculum(CurriculumError::FatalFindings {
            findings: vec![Finding::new("schema", "the field is missing")],
        });
        assert_eq!(first_reason(&named), "[schema] the field is missing");

        let empty = LoadError::Curriculum(CurriculumError::FatalFindings {
            findings: Vec::new(),
        });
        assert_eq!(first_reason(&empty), empty.to_string());

        let other = LoadError::Curriculum(CurriculumError::DuplicateTopicId {
            id: "addition".into(),
        });
        assert_eq!(first_reason(&other), other.to_string());
    }
}
