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
use cadus_store::{RoleInfo, StoreError};
use serde_json::json;
use sqlx::PgPool;

/// The state that every handler shares. The process keeps no session data in
/// memory, so the app tier stays stateless (C3).
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
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
async fn ready(State(state): State<AppState>) -> Response {
    match sqlx::query_scalar!(r#"SELECT 1 AS "one!""#)
        .fetch_one(&state.pool)
        .await
    {
        Ok(1) => (StatusCode::OK, Json(json!({ "ready": true }))).into_response(),
        Ok(other) => {
            tracing::warn!(value = other, "web: the readiness probe got a wrong value");
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
pub async fn boot_check(pool: &PgPool) -> Result<RoleInfo, StoreError> {
    cadus_store::assert_rls_enforced(pool).await
}
