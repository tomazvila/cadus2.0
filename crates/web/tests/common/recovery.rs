//! The fixtures of `tests/auth_recovery.rs` and its parts.

use super::*;
use axum::Router;
use cadus_store::test_support::TestDb;
use serde_json::{Value, json};
use sqlx::types::Uuid;
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

/// Fail the test when the paired rate rule of `path` does not refuse the 4th
/// call for one address and the 11th call from one host inside one hour. 3
/// and 10 are the section 3.2 literals.
pub async fn assert_paired_rate_rule(db: &TestDb, path: &str) {
    let app = app_of(db);
    let body = json!({ "email": "one@example.com" });

    let (fourth, eleventh) = in_one_window(db, 3_600, || async {
        for round in 1..=3 {
            let answer = send(&app, post(path, &body)).await;
            assert_eq!(answer.status.as_u16(), 200, "call {round}");
        }
        let fourth = send(&app, post(path, &body)).await;

        // The host tally stands at 3. Seven more addresses fill it to 10.
        for index in 0..7 {
            let answer = send(
                &app,
                post(
                    path,
                    &json!({ "email": format!("host{index}@example.com") }),
                ),
            )
            .await;
            assert_eq!(answer.status.as_u16(), 200, "host call {index}");
        }
        let eleventh = send(&app, post(path, &json!({ "email": "last@example.com" }))).await;
        (fourth, eleventh)
    })
    .await;

    assert_eq!(fourth.status.as_u16(), 429);
    assert_eq!(fourth.code(), "rate_limited");
    assert_eq!(eleventh.status.as_u16(), 429);
    assert_eq!(eleventh.code(), "rate_limited");
}

/// Register `email`, and give the account one token of `purpose` under
/// `token_hash` that lives `ttl_secs`. The answer is the router and the account.
pub async fn learner_with_token(
    db: &TestDb,
    email: &str,
    token_hash: &str,
    purpose: &str,
    ttl_secs: i64,
) -> (Router, Uuid) {
    let app = app_of(db);
    signup(&app, email, GOOD_PASSWORD).await;
    let user = user_id(db, email).await;
    seed_token(db, user, token_hash, purpose, shift(ttl_secs)).await;
    (app, user)
}
