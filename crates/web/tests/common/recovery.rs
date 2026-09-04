//! The fixtures of `tests/auth_recovery.rs` and its parts.

use super::*;
use cadus_store::test_support::TestDb;
use serde_json::{Value, json};
use sqlx::types::chrono::{DateTime, Utc};

/// The `expires_at` of the one live token of `purpose`.
pub async fn token_expiry(db: &TestDb, purpose: &str) -> DateTime<Utc> {
    sqlx::query_scalar("SELECT expires_at FROM auth_tokens WHERE purpose = $1")
        .bind(purpose)
        .fetch_one(&db.admin)
        .await
        .unwrap()
}

// ---------------------------------------------------------------------------
