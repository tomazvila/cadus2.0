//! A unit test that covers every branch of [`consume_token_tx`] in one build:
//! the already-spent rollback, the failed rollback, and the two effects.

use sqlx::types::chrono::Utc;

use super::{consume_token_tx_inner, insert_token};
use crate::auth::{TokenConsumed, TokenEffect};
use crate::begin_tenant;
use crate::test_support::TestDb;

/// Seed one live token of `token_hash` for `user_id`, through a bound
/// transaction of the app role.
async fn seed_token(db: &TestDb, user_id: uuid::Uuid, token_hash: &str) {
    let mut tx = begin_tenant(&db.app, user_id).await.unwrap();
    let expires = Utc::now() + std::time::Duration::from_secs(3600);
    insert_token(&mut *tx, user_id, token_hash, "reset", expires)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

/// The order spends a live token and applies each effect, answers
/// already-spent for a token that no row holds, and reports the error of the
/// rollback when the backend ends before it.
#[tokio::test]
async fn consume_token_tx_covers_every_branch() {
    TestDb::with(|db| async move {
        let user = db.seed_user("token@example.test").await;

        // No matching row: the UPDATE writes 0 rows, so the order rolls back
        // and answers already-spent.
        assert_eq!(
            consume_token_tx_inner(&db.app, user, "absent", TokenEffect::EmailVerify, false)
                .await
                .unwrap(),
            TokenConsumed::AlreadySpent
        );

        // The backend ends before the rollback, so the rollback is an error.
        assert!(
            consume_token_tx_inner(&db.app, user, "absent", TokenEffect::EmailVerify, true)
                .await
                .is_err()
        );

        // A live token, the password-reset effect: the order commits.
        seed_token(&db, user, "reset-hash").await;
        assert_eq!(
            consume_token_tx_inner(
                &db.app,
                user,
                "reset-hash",
                TokenEffect::PasswordReset {
                    password_hash: "$argon2$new",
                },
                false,
            )
            .await
            .unwrap(),
            TokenConsumed::Consumed
        );

        // A live token, the email-verify effect: the order commits.
        seed_token(&db, user, "verify-hash").await;
        assert_eq!(
            consume_token_tx_inner(
                &db.app,
                user,
                "verify-hash",
                TokenEffect::EmailVerify,
                false
            )
            .await
            .unwrap(),
            TokenConsumed::Consumed
        );
    })
    .await;
}
