//! HTTP adapter: the axum application, routes, and request handlers.
//!
//! Requirements: C3 (the boot guard refuses a role that bypasses row-level
//! security), R2 (axum on top of sqlx with compile-time checked queries), R4
//! (handlers do local work and database I/O only).
//!
//! M0 serves two probes and nothing else. `/api/health` reports that the
//! process is alive. `/api/ready` reports that the database answers. Milestone
//! M5 owns the product API. No handler here calls a model, and no handler here
//! starts generation (R4).

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

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use cadus_store::{Db, RoleInfo, StoreError, bounded};
use serde_json::json;

/// The state that every handler shares. The process keeps no session data in
/// memory, so the app tier stays stateless (C3).
///
/// The state carries a `Db`, not a bare `PgPool`. A `Db` holds the pool AND the
/// client-side query bound of `DB_CLIENT_TIMEOUT_MS`, so every handler that
/// takes this state applies the bound with `cadus_store::bounded` (R4, L1).
#[derive(Clone)]
pub struct AppState {
    pub db: Db,
}

/// Build the axum application.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/ready", get(ready))
        .with_state(state)
}

/// Liveness probe. The answer depends on nothing outside the process.
///
/// The body is exactly `{"ok":true}` and the content type is
/// `application/json`.
async fn health() -> Response {
    (StatusCode::OK, Json(json!({ "ok": true }))).into_response()
}

/// Readiness probe. The handler sends `SELECT 1` through the pool.
///
/// The answer is `200` with `{"ready":true}` when the database replies, and
/// `503` with `{"ready":false}` when it does not. A load balancer reads the
/// status code, so the code carries the verdict and the body repeats it.
///
/// The query runs inside `cadus_store::bounded`, so `DB_CLIENT_TIMEOUT_MS`
/// bounds it (L1). A database that accepts the socket and then answers nothing
/// held this handler open without end, because `statement_timeout` needs a live
/// server and sqlx 0.9 sets no TCP keepalive. The bound turns that stall into
/// the same `503` that every other database fault gives.
async fn ready(State(state): State<AppState>) -> Response {
    let query = sqlx::query_scalar!(r#"SELECT 1 AS "one!""#).fetch_one(state.db.pool());
    match bounded(&state.db, query).await {
        Ok(1) => (StatusCode::OK, Json(json!({ "ready": true }))).into_response(),
        Ok(other) => {
            tracing::warn!(value = other, "web: the readiness probe got a wrong value");
            unready()
        }
        Err(StoreError::Timeout { after_ms }) => {
            tracing::warn!("web: the readiness probe timed out after {after_ms} ms");
            unready()
        }
        Err(err) => {
            tracing::warn!(error = %err, "web: the readiness probe failed");
            unready()
        }
    }
}

/// The single negative answer of the readiness probe.
fn unready() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({ "ready": false })),
    )
        .into_response()
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
