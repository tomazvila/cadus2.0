//! The overturn report of the background equivalence check.
//!
//! A job exists only for an answer the first pass refused. A settled job that
//! accepted the answer overturned that refusal. The report lists the items by
//! overturn rate, so the items whose first pass refuses natural spellings come
//! first. `GET /api/admin/equivalence/overturns` serves it to an admin.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use cadus_store::equivalence::{self, Overturn};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::AppState;
use crate::admin::AdminUser;
use crate::error::ApiError;
use crate::session::store;

/// The fewest settled checks an item needs to appear in the report.
const DEFAULT_MIN_CHECKS: i64 = 2;

/// The most items one report lists.
const LIMIT: i64 = 100;

/// The query of the report.
#[derive(Debug, Deserialize)]
pub struct ReportQuery {
    /// The fewest settled checks an item needs.
    pub min_checks: Option<i64>,
}

/// One report row as the admin reads it.
fn row_view(row: &Overturn) -> Value {
    #[expect(
        clippy::cast_precision_loss,
        reason = "a check count is far below 2**53"
    )]
    let rate = row.overturned as f64 / row.checks.max(1) as f64;
    json!({
        "item_digest": row.item_digest,
        "topic": row.topic,
        "checks": row.checks,
        "overturned": row.overturned,
        "rate": rate,
    })
}

/// `GET /api/admin/equivalence/overturns`: the items by overturn rate.
///
/// # Errors
///
/// `401`/`403` for a non-admin, `503` without the admin connection, `500` when
/// the statement fails.
pub async fn overturns(
    AdminUser(_authed): AdminUser,
    State(state): State<AppState>,
    Query(query): Query<ReportQuery>,
) -> Result<Json<Value>, ApiError> {
    let Some(admin) = state.admin.as_ref() else {
        return Err(ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "admin_path_unavailable",
            "The admin connection is not configured.",
        ));
    };
    let min = query.min_checks.unwrap_or(DEFAULT_MIN_CHECKS).max(1);
    let rows = store(
        &state,
        equivalence::overturn_rates(admin.pool(), min, LIMIT),
    )
    .await?;
    Ok(Json(json!({
        "items": rows.iter().map(row_view).collect::<Vec<_>>(),
    })))
}
