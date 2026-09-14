//! `/api/admin/outbox*` — the operator drain of the mail queue.
//!
//! Sign-up mints a verification token and stores the SHA-256 digest alone, so
//! the raw token reaches `email_outbox` at mint time and nowhere else. A
//! deployment with no mailer has no other reader: the two routes here are the
//! path a real learner's verification takes today.
//!
//! | Method and path | What it answers |
//! |---|---|
//! | `GET /api/admin/outbox` | the pending messages, oldest first, with their tokens and links |
//! | `POST /api/admin/outbox/{id}/sent` | `{id, status}`, marking one message delivered |
//!
//! The drain crosses every tenant: the row of a freshly signed-up learner
//! belongs to that learner, and the operator who delivers it is another account.
//! `email_outbox` is under row-level security, so both routes read and write
//! through [`AppState::admin`], the `cadus_admin` handle that holds BYPASSRLS,
//! exactly as the M6 review writes do. A deployment that configured no admin
//! connection answers `503 admin_path_unavailable` on both.
//!
//! Both routes refuse every account whose `users.is_admin` is false, through the
//! same [`AdminUser`] guard that `/api/admin/ungraded` uses.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use cadus_store::{Db, StoreError};
use serde_json::{Value, json};
use sqlx::types::Uuid;
use sqlx::types::chrono::{DateTime, Utc};

use crate::AppState;
use crate::admin::AdminUser;
use crate::auth::store_call;
use crate::error::ApiError;
use crate::path::ApiPath;

/// The route that lists the pending messages.
pub const OUTBOX_PATH: &str = "/api/admin/outbox";

/// The route that marks one message sent.
pub const OUTBOX_SENT_PATH: &str = "/api/admin/outbox/{id}/sent";

/// The `email_outbox.status` of a message no mailer has sent.
pub const PENDING: &str = "pending";

/// The `email_outbox.status` of a message delivered by hand.
pub const SENT: &str = "sent";

/// The code of a deployment that configured no admin connection.
pub const ADMIN_PATH_UNAVAILABLE: &str = "admin_path_unavailable";

/// The message of that refusal. It mirrors the M6 review writes.
pub const ADMIN_PATH_MESSAGE: &str =
    "This deployment configured no admin database connection, so the outbox drain is closed.";

/// The message of an id that names no pending message.
pub const UNKNOWN_MESSAGE: &str = "No pending outbox message carries that id.";

/// One pending `email_outbox` row, as the drain reads it.
#[derive(Debug, sqlx::FromRow)]
struct OutboxRow {
    id: Uuid,
    user_id: Option<Uuid>,
    to_addr: String,
    kind: String,
    payload: Value,
    status: String,
    attempts: i32,
    created_at: DateTime<Utc>,
}

impl OutboxRow {
    /// The wire shape of one row. The payload carries the raw token and the
    /// link, so an operator can deliver it without a second read.
    fn json(&self) -> Value {
        json!({
            "id": self.id.to_string(),
            "user_id": self.user_id.map(|id| id.to_string()),
            "to_addr": self.to_addr,
            "kind": self.kind,
            "status": self.status,
            "attempts": self.attempts,
            "created_at": self.created_at.to_rfc3339(),
            "payload": self.payload,
        })
    }
}

/// The admin connection of this deployment, or `503`.
fn admin_path(state: &AppState) -> Result<&Db, ApiError> {
    state.admin.as_ref().ok_or_else(|| {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            ADMIN_PATH_UNAVAILABLE,
            ADMIN_PATH_MESSAGE,
        )
    })
}

/// `GET /api/admin/outbox` — the pending messages across every tenant (C3).
///
/// The list is oldest first. `pending` is the count of `items`, so an operator
/// reads the size of the queue from one field.
///
/// # Errors
///
/// - `401 unauthorized` — the request carries no live session.
/// - `403 forbidden` — the account is not an admin.
/// - `503 admin_path_unavailable` — the deployment configured no admin path.
/// - `500 internal_error` — the read failed.
pub async fn list_pending(
    AdminUser(_authed): AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let admin = admin_path(&state)?;
    let rows: Vec<OutboxRow> = store_call(admin, "outbox list", async {
        sqlx::query_as::<_, OutboxRow>(
            "SELECT id, user_id, to_addr, kind, payload, status, attempts, created_at
             FROM email_outbox
             WHERE status = $1
             ORDER BY created_at, id",
        )
        .bind(PENDING)
        .fetch_all(admin.pool())
        .await
        .map_err(StoreError::from)
    })
    .await?;
    let items: Vec<Value> = rows.iter().map(OutboxRow::json).collect();
    let pending = items.len();
    Ok(Json(json!({ "items": items, "pending": pending })))
}

/// `POST /api/admin/outbox/{id}/sent` — mark one pending message delivered.
///
/// The `status = 'pending'` guard makes the mark single use: a second call on a
/// sent row is `404`, so a double delivery is visible instead of silent.
///
/// # Errors
///
/// - `401 unauthorized` — the request carries no live session.
/// - `403 forbidden` — the account is not an admin.
/// - `404 not_found` — no pending message carries the id.
/// - `503 admin_path_unavailable` — the deployment configured no admin path.
/// - `500 internal_error` — the write failed.
pub async fn mark_sent(
    AdminUser(_authed): AdminUser,
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let admin = admin_path(&state)?;
    let written = store_call(admin, "outbox mark sent", async {
        Ok(sqlx::query(
            "UPDATE email_outbox
             SET status = $1, sent_at = now(), attempts = attempts + 1
             WHERE id = $2 AND status = $3",
        )
        .bind(SENT)
        .bind(id)
        .bind(PENDING)
        .execute(admin.pool())
        .await?
        .rows_affected())
    })
    .await?;
    if written == 0 {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            crate::error::NOT_FOUND,
            UNKNOWN_MESSAGE,
        ));
    }
    Ok(Json(json!({ "id": id.to_string(), "status": SENT })))
}

#[cfg(test)]
mod tests {
    use super::{ADMIN_PATH_MESSAGE, PENDING, SENT, UNKNOWN_MESSAGE};

    /// The status literals are the `email_outbox` spellings, and every refusal
    /// message names the thing it refuses.
    #[test]
    fn the_status_literals_are_the_outbox_spellings() {
        assert_eq!(PENDING, "pending");
        assert_eq!(SENT, "sent");
        assert!(UNKNOWN_MESSAGE.contains("pending"));
        assert!(ADMIN_PATH_MESSAGE.contains("admin"));
    }
}
