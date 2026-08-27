//! The two probes: `/api/health` and `/api/ready`.
//!
//! Spec `docs/reference/web-service-1.0-spec.md` section 2, rows `GET
//! /api/health` and `GET /api/ready`, and ruling D-M5-6 of `docs/plans/M5.md`.
//!
//! `/api/health` is liveness. It depends on nothing outside the process, so it
//! answers while the database is down. That is the point: a load balancer must
//! be able to tell "this process is gone" from "this process cannot reach its
//! database".
//!
//! `/api/ready` is readiness. Ruling D-M5-6 drops the 1.0 `redis` field (2.0 has
//! no Redis, D8) and takes worker liveness from the `diagnosis_jobs` claim age
//! instead of the 1.0 Redis heartbeat key.
//!
//! The two facts are not equal in weight, and 1.0 fixed that split for a reason
//! (spec section 10, row "Ready"): a **configured** dependency that reports
//! `down` fails readiness with `503`, and the worker reading is best effort. A
//! stale worker is a `warnings` entry and never a `503`, because the learner can
//! still study: the whole grade verdict is local CPU work (A3), and only the
//! diagnosis prose waits (A4).

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Json, http};
use cadus_store::{Db, StoreError, bounded};
use serde_json::{Value, json};

use crate::AppState;

/// A pending diagnosis job older than this reports the worker stale.
///
/// 60 seconds, the 1.0 value (`cadus_web/health.py:44`).
pub const WORKER_STALE_AFTER_SECS: f64 = 60.0;

/// The `warnings` entry of a stale worker.
pub const WORKER_STALE_WARNING: &str = "worker_claim_stale";

/// The `db` value of a database that answers.
pub const DEP_OK: &str = "ok";

/// The `db` value of a database that does not answer.
pub const DEP_DOWN: &str = "down";

/// Liveness. The answer depends on nothing outside the process.
///
/// The body is exactly `{"ok":true}` and the content type is
/// `application/json`.
pub async fn health() -> Response {
    (StatusCode::OK, Json(json!({ "ok": true }))).into_response()
}

/// Readiness (D-M5-6).
///
/// `200` with `ok:true` when the database answers, `503` with `ok:false` when it
/// does not. A load balancer reads the status code, so the code carries the
/// verdict and the body says which dependency decided it.
pub async fn ready(State(state): State<AppState>) -> Response {
    let db = probe_db(&state.db).await;
    let claim_age = probe_claim_age(&state.db).await;
    let stale = claim_age.is_some_and(|age| age > WORKER_STALE_AFTER_SECS);
    let ok = db == DEP_OK;

    let mut body = serde_json::Map::new();
    body.insert("ok".to_string(), json!(ok));
    body.insert("db".to_string(), json!(db));
    body.insert(
        "worker".to_string(),
        json!({ "claim_age_secs": claim_age, "stale": stale }),
    );
    // A stale worker never decides the status code. It only names itself.
    if stale {
        body.insert("warnings".to_string(), json!([WORKER_STALE_WARNING]));
    }

    let status = if ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    // A readiness answer is a live reading, so no cache may hold it. The
    // security-header layer sets `no-cache` only when the answer carries none,
    // and `no-store` is the stronger rule this one answer needs.
    (
        status,
        [(http::header::CACHE_CONTROL, "no-store")],
        Json(Value::Object(body)),
    )
        .into_response()
}

/// Ask the database whether it answers.
///
/// The query runs inside `cadus_store::bounded`, so `DB_CLIENT_TIMEOUT_MS`
/// bounds it (L1). A database that accepts the socket and then answers nothing
/// held this handler open without end, because `statement_timeout` needs a live
/// server and sqlx 0.9 sets no TCP keepalive. The bound turns that stall into
/// the same `down` that every other database fault gives.
async fn probe_db(db: &Db) -> &'static str {
    let query = sqlx::query_scalar!(r#"SELECT 1 AS "one!""#).fetch_one(db.pool());
    match bounded(db, query).await {
        Ok(1) => DEP_OK,
        Ok(other) => {
            tracing::warn!(value = other, "web: the readiness probe got a wrong value");
            DEP_DOWN
        }
        Err(StoreError::Timeout { after_ms }) => {
            tracing::warn!("web: the readiness probe timed out after {after_ms} ms");
            DEP_DOWN
        }
        Err(err) => {
            tracing::warn!(error = %err, "web: the readiness probe failed");
            DEP_DOWN
        }
    }
}

/// The age in seconds of the oldest diagnosis job that waits for a claim.
///
/// `None` means "unknown": no job waits, or the read failed. The reading is best
/// effort by design, so a failure here reports an unknown age and never a `503`
/// (spec section 10, row "Ready").
///
/// `diagnosis_jobs` carries a FORCEd tenant policy and this connection is
/// unbound, so a plain SELECT would read zero rows on every deployment. The
/// SECURITY DEFINER function `diagnosis_claim_age_secs` of migration 0007 is the
/// one read that crosses the policy, and it gives back one aggregate number and
/// no tenant row.
async fn probe_claim_age(db: &Db) -> Option<f64> {
    let query =
        sqlx::query_scalar!(r#"SELECT diagnosis_claim_age_secs() AS "age?""#).fetch_one(db.pool());
    match bounded(db, query).await {
        Ok(age) => age,
        Err(err) => {
            tracing::warn!(error = %err, "web: the worker claim-age read failed");
            None
        }
    }
}
