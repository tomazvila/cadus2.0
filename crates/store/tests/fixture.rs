//! The cleanup contract of `TestDb::with_role` on its failure paths: a role
//! that cannot log in, an option list that does not parse, and a drop that
//! fails after the body.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

mod common;

use cadus_store::test_support::TestDb;
use common::cluster_count;
use sqlx::AssertSqlSafe;

/// The message of a panic payload.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_string()))
        .unwrap_or_else(|| "a panic with no message".to_string())
}

/// The count of cluster roles with `name`.
async fn roles_named(name: &str) -> i64 {
    cluster_count("SELECT count(*) FROM pg_roles WHERE rolname = $1", name).await
}

/// A role without LOGIN opens no pool. The fixture drops the role and raises
/// the panic of the pool open.
#[tokio::test]
async fn a_role_that_cannot_log_in_is_dropped_before_the_panic() {
    TestDb::with(|db| async move {
        let seen = Arc::new(std::sync::Mutex::new(String::new()));
        let seen_in = Arc::clone(&seen);
        let outcome = tokio::spawn({
            let db = Arc::clone(&db);
            async move {
                TestDb::with_role(&db, "cadus2_nologin", "NOLOGIN", |_, role, _| async move {
                    *seen_in.lock().unwrap() = role;
                })
                .await
            }
        })
        .await;
        let err = outcome.unwrap_err();
        assert!(err.is_panic());
        let message = panic_message(err.into_panic().as_ref());
        assert!(message.starts_with("pool as cadus2_nologin_"), "{message}");
        assert!(seen.lock().unwrap().is_empty(), "the body must not run");
        let leaked: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_roles WHERE rolname LIKE 'cadus2_nologin_%'",
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(leaked, 0);
    })
    .await;
}

/// An option list that `CREATE ROLE` refuses stops the fixture before any
/// role exists.
#[tokio::test]
async fn an_option_list_that_does_not_parse_creates_no_role() {
    TestDb::with(|db| async move {
        let outcome = tokio::spawn({
            let db = Arc::clone(&db);
            async move {
                TestDb::with_role(&db, "cadus2_bogus", "BOGUS OPTION", |_, _, _| async {}).await
            }
        })
        .await;
        let message = panic_message(outcome.unwrap_err().into_panic().as_ref());
        assert!(
            message.starts_with("CREATE ROLE cadus2_bogus_"),
            "{message}"
        );
    })
    .await;
}

/// A role that owns a grant cannot be dropped. The body's verdict stands, and
/// the failed drop stops the test with the DROP ROLE line.
#[tokio::test]
async fn a_drop_that_fails_after_the_body_stops_the_test() {
    TestDb::with(|db| async move {
        let outcome = tokio::spawn({
            let db = Arc::clone(&db);
            async move {
                TestDb::with_role(&db, "cadus2_owner", "LOGIN", |db, role, _| async move {
                    sqlx::query(AssertSqlSafe(format!(
                        "GRANT SELECT ON users TO \"{role}\""
                    )))
                    .execute(&db.admin)
                    .await
                    .unwrap();
                    role
                })
                .await
            }
        })
        .await;
        let message = panic_message(outcome.unwrap_err().into_panic().as_ref());
        assert!(message.starts_with("DROP ROLE cadus2_owner_"), "{message}");
        let role = message["DROP ROLE ".len()..]
            .split(' ')
            .next()
            .unwrap()
            .to_string();
        assert_eq!(roles_named(&role).await, 1);
        sqlx::query(AssertSqlSafe(format!(
            "REVOKE SELECT ON users FROM \"{role}\""
        )))
        .execute(&db.admin)
        .await
        .unwrap();
        sqlx::query(AssertSqlSafe(format!("DROP ROLE \"{role}\"")))
            .execute(&db.admin)
            .await
            .unwrap();
        assert_eq!(roles_named(&role).await, 0);
    })
    .await;
}
