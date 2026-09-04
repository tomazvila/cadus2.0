//! Unit tests that cover every branch of [`consume_token_tx`] in one build: the
//! four outcomes of the happy path, and the error edge of every statement.

use sqlx::AssertSqlSafe;
use sqlx::types::chrono::Utc;
use uuid::Uuid;

use super::{consume_token_tx_inner, insert_token};
use crate::auth::{TokenConsumed, TokenEffect};
use crate::begin_tenant;
use crate::test_support::TestDb;

/// Seed one live token of `token_hash` for `user_id`, through a bound
/// transaction of the app role.
async fn seed_token(db: &TestDb, user_id: Uuid, token_hash: &str) {
    let mut tx = begin_tenant(&db.app, user_id).await.unwrap();
    let expires = Utc::now() + std::time::Duration::from_secs(3600);
    insert_token(&mut *tx, user_id, token_hash, "reset", expires)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

/// Run one statement of the test's own text on the admin pool.
async fn admin_exec(db: &TestDb, sql: &str) {
    sqlx::query(AssertSqlSafe(sql.to_string()))
        .execute(&db.admin)
        .await
        .unwrap();
}

/// The order spends a live token and applies each effect, answers
/// already-spent for a token that no row holds, and reports the error of the
/// rollback when the backend ends before it.
#[tokio::test]
async fn the_order_spends_the_token_rolls_back_and_applies_each_effect() {
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

/// A closed pool fails the bind, and a refused token UPDATE fails the spend.
#[tokio::test]
async fn the_bind_and_the_spend_report_their_errors() {
    TestDb::with(|db| async move {
        let user = db.seed_user("err@example.test").await;

        let closed = db.pool_as("cadus_app", 1).await;
        closed.close().await;
        assert!(
            consume_token_tx_inner(&closed, user, "x", TokenEffect::EmailVerify, false)
                .await
                .is_err()
        );

        admin_exec(&db, "REVOKE UPDATE ON auth_tokens FROM cadus_app").await;
        assert!(
            consume_token_tx_inner(&db.app, user, "x", TokenEffect::EmailVerify, false)
                .await
                .is_err()
        );
    })
    .await;
}

/// A refused write of the account fails each effect after the spend.
#[tokio::test]
async fn each_effect_reports_a_refused_account_write() {
    TestDb::with(|db| async move {
        let user = db.seed_user("effect@example.test").await;
        seed_token(&db, user, "reset-hash").await;
        seed_token(&db, user, "verify-hash").await;
        admin_exec(&db, "REVOKE UPDATE ON users FROM cadus_app").await;

        assert!(
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
            .is_err()
        );
        assert!(
            consume_token_tx_inner(
                &db.app,
                user,
                "verify-hash",
                TokenEffect::EmailVerify,
                false
            )
            .await
            .is_err()
        );
    })
    .await;
}

/// A commit that fails after the effect is the error of the commit.
#[tokio::test]
async fn a_failed_commit_after_the_effect_is_an_error() {
    TestDb::with(|db| async move {
        let user = db.seed_user("commit@example.test").await;
        seed_token(&db, user, "reset-hash").await;
        admin_exec(
            &db,
            "CREATE OR REPLACE FUNCTION fault_raise() RETURNS trigger LANGUAGE plpgsql AS $$ \
             BEGIN RAISE EXCEPTION 'injected'; END $$",
        )
        .await;
        admin_exec(
            &db,
            "CREATE CONSTRAINT TRIGGER fault_commit AFTER UPDATE ON users DEFERRABLE \
             INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION fault_raise()",
        )
        .await;

        assert!(
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
            .is_err()
        );
    })
    .await;
}
