//! HTTP adapter: the axum application, its layers, and its request handlers.
//!
//! Requirements: C3 (the boot guard refuses a role that bypasses row-level
//! security), R2 (axum on top of sqlx with compile-time checked queries), R4
//! (handlers do local work and database I/O only), L6 (this crate never links
//! the model-client crate, so a model call on a request path is a compile
//! error, not a review note).
//!
//! Spec: `docs/reference/web-service-1.0-spec.md`, section 11, unit U1. M5 U1
//! delivers the skeleton every later unit hangs its routes on:
//!
//! - [`create_app`], the one place that builds the router and orders the layers;
//! - [`error::ApiError`], the `{"error":{"code","message"}}` envelope of every
//!   4xx and 5xx answer, and both router fallbacks;
//! - [`security::security_headers_layer`], the section 3.1 header literals;
//! - [`origin::csrf_origin_layer`], the CSRF origin check in both polarities;
//! - [`metrics::request_metrics_layer`] and `GET /metrics`, labelled by route
//!   TEMPLATE;
//! - `/api/health` and `/api/ready` (D-M5-6);
//! - the two boot guards: [`boot_check`] (C3) and
//!   [`cookie::CookiePosture::assert_safe`].
//!
//! No handler here calls a model, and no handler here starts generation (R4,
//! T1).
//!
//! **The layer order is part of the contract.** axum wraps the router in each
//! layer as it is added, so the LAST layer added is the OUTERMOST one. Reading
//! [`create_app`] from the outside in:
//!
//! 1. the request-metrics layer, so it counts every answer, the `403` of the
//!    CSRF layer included (1.0 orders it the same way, `cadus_web/app.py:315`);
//! 2. the security-header layer, so the `403` carries the headers too;
//! 3. the CSRF origin layer, which answers before any handler runs;
//! 4. the routes and the two fallbacks.

#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )
)]

pub mod auth;
pub mod cookie;
pub mod error;
pub mod health;
pub mod metrics;
pub mod origin;
pub mod security;
pub mod serve;
pub mod session;
pub mod state;

use std::sync::Arc;

use axum::Router;
use axum::middleware::{from_fn, from_fn_with_state};
use axum::routing::{get, post};
use cadus_store::{Db, RoleInfo, StoreError, bounded};

use crate::auth::oauth::OAuthConfig;
use crate::auth::password::Argon2Profile;
use crate::cookie::CookiePosture;
use crate::metrics::Registry;
use crate::origin::OriginPolicy;
use crate::state::Content;

/// The state that every handler and every layer shares. The process keeps no
/// session data in memory, so the app tier stays stateless (C3).
///
/// The state carries a `Db`, not a bare `PgPool`. A `Db` holds the pool AND the
/// client-side query bound of `DB_CLIENT_TIMEOUT_MS`, so every handler that
/// takes this state applies the bound with `cadus_store::bounded` (R4, L1).
#[derive(Clone)]
pub struct AppState {
    /// The connection pool and its client-side query bound.
    pub db: Db,
    /// The session-cookie posture. The CSRF layer reads the active cookie name
    /// from it, and unit U2 writes the cookie with it.
    pub posture: CookiePosture,
    /// How the CSRF layer names this deployment's own origin.
    pub origin: OriginPolicy,
    /// The request metrics of this process.
    pub metrics: Arc<Registry>,
    /// The curriculum and the scheduler config the M5 routes compose with.
    ///
    /// `None` means the process loaded no curriculum, and every route that
    /// needs one answers `503 curriculum_unavailable`. The binary loads the tree
    /// at boot and exits 2 when it does not load, so `None` is a test-only
    /// state and never a running deployment.
    pub content: Option<Arc<Content>>,
    /// The Argon2id parameter profile of this deployment. The auth routes hash
    /// and rehash with it, and the anti-enumeration dummy hash carries the same
    /// parameters, so the unknown-address path costs what the known one costs.
    pub argon2: Argon2Profile,
    /// The OAuth providers this deployment serves, and the transport that runs
    /// the provider calls (M5 U5). The default serves no provider, so both
    /// OAuth routes answer `404 not_found`.
    pub oauth: OAuthConfig,
}

impl AppState {
    /// The state of a production deployment with the default posture and the
    /// fallback origin policy.
    pub fn new(db: Db) -> Self {
        Self {
            db,
            posture: CookiePosture::SECURE,
            origin: OriginPolicy::default(),
            metrics: Arc::new(Registry::new()),
            content: None,
            argon2: Argon2Profile::PROD,
            oauth: OAuthConfig::default(),
        }
    }

    /// The same state with a loaded curriculum.
    #[must_use]
    pub fn with_content(mut self, content: Arc<Content>) -> Self {
        self.content = Some(content);
        self
    }

    /// The same state with another cookie posture.
    #[must_use]
    pub fn with_posture(mut self, posture: CookiePosture) -> Self {
        self.posture = posture;
        self
    }

    /// The same state with another origin policy.
    #[must_use]
    pub fn with_origin(mut self, origin: OriginPolicy) -> Self {
        self.origin = origin;
        self
    }

    /// The same state with another Argon2id profile.
    #[must_use]
    pub fn with_argon2(mut self, argon2: Argon2Profile) -> Self {
        self.argon2 = argon2;
        self
    }

    /// The same state with an OAuth configuration.
    #[must_use]
    pub fn with_oauth(mut self, oauth: OAuthConfig) -> Self {
        self.oauth = oauth;
        self
    }
}

/// Build the axum application.
///
/// The module header gives the layer order and why it is the contract.
pub fn create_app(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health::health))
        .route("/api/ready", get(health::ready))
        .route("/metrics", get(metrics::scrape))
        // Unit U6, spec section 11. Every route below is added BEFORE the three
        // `.layer(...)` calls, or it escapes all three layers.
        .route("/api/status", get(session::status))
        .route("/api/graph", get(session::graph))
        .route("/api/modules", get(session::modules))
        .route("/api/export", get(session::export))
        .route("/api/enroll", post(session::enroll))
        .route("/api/session/start", post(session::session_start))
        .route("/api/session/end", post(session::session_end))
        .route("/api/session/plan", get(session::session_plan))
        // Unit U7, spec section 11. The same rule: before the three layers.
        .route("/api/task/{task_id}/serve", post(serve::serve))
        .route("/api/task/{task_id}/teach", post(serve::teach))
        .route("/api/task/{task_id}/hint", post(serve::hint))
        // M5 U4: the `/api/auth/*` routes. They sit INSIDE every layer
        // below, so a cross-origin login is refused before the handler runs.
        .merge(auth::routes::router())
        // M5 U5: the OAuth start and callback. Both are GET, so the CSRF layer
        // never reads them and the provider's callback navigation is never
        // refused.
        .merge(auth::oauth_routes::router())
        // axum's own fallbacks answer with an empty body, so both of them
        // return the envelope instead (spec section 2).
        .fallback(error::not_found)
        .method_not_allowed_fallback(error::method_not_allowed)
        .layer(from_fn_with_state(state.clone(), origin::csrf_origin_layer))
        .layer(from_fn(security::security_headers_layer))
        .layer(from_fn_with_state(
            state.clone(),
            metrics::request_metrics_layer,
        ))
        .with_state(state)
}

/// C3 boot guard. Return `Ok` only when row-level security applies to the
/// connected role.
///
/// The check itself lives in `cadus_store::assert_rls_enforced`. The web crate
/// only wires it into the start sequence, so one implementation serves every
/// binary.
///
/// The guard query runs inside `cadus_store::bounded` too, so a database that
/// answers nothing gives `StoreError::Timeout` instead of a start that never
/// ends (L1).
///
/// `bounded` takes a future that gives `Result<T, sqlx::Error>`, and the guard
/// gives `Result<RoleInfo, StoreError>`. The future below therefore wraps the
/// answer of the guard in `Ok`, and the `?` takes it out again. `bounded` adds
/// the bound and nothing else.
pub async fn boot_check(db: &Db) -> Result<RoleInfo, StoreError> {
    let guard = async { Ok(cadus_store::assert_rls_enforced(db.pool()).await) };
    bounded(db, guard).await?
}

/// The environment variable that holds the listen address.
pub const BIND_ADDR_VAR: &str = "BIND_ADDR";

/// The address to bind when `BIND_ADDR` is absent.
pub const DEFAULT_BIND_ADDR: &str = "0.0.0.0:8080";

/// The reason that [`bind_addr`] refuses a value of `BIND_ADDR`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindAddrError {
    /// The value is not valid Unicode.
    NotUnicode,
    /// The value is valid Unicode, but it is not `host:port`.
    NotSocketAddr {
        /// The value, as the operator wrote it.
        value: String,
        /// The text of the parse error.
        reason: String,
    },
}

impl std::fmt::Display for BindAddrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotUnicode => write!(f, "{BIND_ADDR_VAR} is not valid Unicode"),
            Self::NotSocketAddr { value, reason } => {
                write!(
                    f,
                    "{BIND_ADDR_VAR} {value} is not a socket address: {reason}"
                )
            }
        }
    }
}

impl std::error::Error for BindAddrError {}

/// Resolve the listen address from the raw value of `BIND_ADDR`.
///
/// The function is pure. It reads no environment and it opens no socket, so a
/// test drives every branch without a bind. The binary calls it with
/// `std::env::var_os(BIND_ADDR_VAR)` and binds the answer.
///
/// The rules are:
///
/// - `None` gives the default `0.0.0.0:8080`.
/// - A value that parses as `host:port` comes back unchanged.
/// - A value that is not valid Unicode gives [`BindAddrError::NotUnicode`]. The
///   old code sent that value to the default and bound every interface without
///   a word (round-3 finding #31).
/// - Any other value gives [`BindAddrError::NotSocketAddr`].
pub fn bind_addr(raw: Option<std::ffi::OsString>) -> Result<String, BindAddrError> {
    let Some(raw) = raw else {
        return Ok(DEFAULT_BIND_ADDR.to_string());
    };
    let value = raw.into_string().map_err(|_| BindAddrError::NotUnicode)?;
    match value.parse::<std::net::SocketAddr>() {
        Ok(_) => Ok(value),
        Err(err) => Err(BindAddrError::NotSocketAddr {
            reason: err.to_string(),
            value,
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::{BindAddrError, bind_addr};

    /// An absent `BIND_ADDR` binds every interface on port 8080.
    ///
    /// The Dockerfile, docker-compose.yml, and `docs/SELF_HOST.md` expose 8080
    /// and set no `BIND_ADDR`, so this literal is the contract of the image. A
    /// bind test cannot hold that contract: port 8080 collides on the build
    /// machine.
    #[test]
    fn absent_bind_addr_gives_the_default() {
        assert_eq!(bind_addr(None), Ok("0.0.0.0:8080".to_string()));
    }

    /// A valid value comes back unchanged, character for character.
    #[test]
    fn a_socket_address_comes_back_unchanged() {
        assert_eq!(
            bind_addr(Some(OsString::from("127.0.0.1:0"))),
            Ok("127.0.0.1:0".to_string())
        );
        assert_eq!(
            bind_addr(Some(OsString::from("[::1]:8443"))),
            Ok("[::1]:8443".to_string())
        );
    }

    /// A value that is not valid Unicode is a start error, not the default.
    ///
    /// The byte 0xff is not valid UTF-8. On unix an environment variable holds
    /// any byte string, so a typo reaches this branch.
    #[cfg(unix)]
    #[test]
    fn a_non_unicode_value_is_an_error() {
        use std::os::unix::ffi::OsStringExt;

        let raw = OsString::from_vec(vec![0x31, 0x32, 0x37, 0xff]);

        assert_eq!(bind_addr(Some(raw)), Err(BindAddrError::NotUnicode));
    }

    /// A value that is not `host:port` is a start error too.
    #[test]
    fn a_value_that_is_not_a_socket_address_is_an_error() {
        let answer = bind_addr(Some(OsString::from("not-an-address")));

        assert!(
            matches!(
                &answer,
                Err(BindAddrError::NotSocketAddr { value, .. }) if value == "not-an-address"
            ),
            "BIND_ADDR=not-an-address must fail, it gave {answer:?}"
        );
    }

    /// The error text names the variable and the value.
    #[test]
    fn the_error_text_names_the_variable() {
        assert_eq!(
            BindAddrError::NotUnicode.to_string(),
            "BIND_ADDR is not valid Unicode"
        );
        assert_eq!(
            BindAddrError::NotSocketAddr {
                value: "1.2.3.4".to_string(),
                reason: "invalid socket address syntax".to_string(),
            }
            .to_string(),
            "BIND_ADDR 1.2.3.4 is not a socket address: invalid socket address syntax"
        );
    }
}
