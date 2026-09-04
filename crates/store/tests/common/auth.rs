//! The auth fixtures: a session row with a 30-day window, and the admin-pool
//! reads of the session count and the account row.

use cadus_store::auth::NewSession;
use cadus_store::test_support::TestDb;
use sqlx::types::chrono::{DateTime, Utc};
use uuid::Uuid;

/// Add `secs` to an instant. The test cluster never reaches the range bound of
/// a timestamp, so an out-of-range sum is a defect of the test, not a case.
pub fn plus_secs(now: DateTime<Utc>, secs: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(now.timestamp() + secs, 0).expect("the instant is in range")
}

/// Build a session row for `token_hash` with a 30-day window.
pub fn new_session(token_hash: &str) -> NewSession<'_> {
    let now = Utc::now();
    NewSession {
        token_hash,
        created_at: now,
        last_seen_at: now,
        expires_at: plus_secs(now, 2_592_000),
        ip: Some("203.0.113.7"),
        user_agent: Some("cadus-test/1.0"),
    }
}

/// Count the session rows of one account, with the admin pool.
pub async fn session_count(db: &TestDb, user_id: Uuid) -> i64 {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM auth_sessions WHERE user_id = $1"#,
        user_id
    )
    .fetch_one(&db.admin)
    .await
    .unwrap()
}

/// Read one account row with the admin pool.
pub async fn read_account(db: &TestDb, user_id: Uuid) -> (Option<String>, Option<DateTime<Utc>>) {
    let row = sqlx::query!(
        r#"SELECT password_hash, email_verified_at FROM users WHERE id = $1"#,
        user_id
    )
    .fetch_one(&db.admin)
    .await
    .unwrap();
    (row.password_hash, row.email_verified_at)
}

/// Assert the session counts of two accounts, with the admin pool.
pub async fn assert_session_counts(
    db: &TestDb,
    user_a: Uuid,
    count_a: i64,
    user_b: Uuid,
    count_b: i64,
) {
    assert_eq!(session_count(db, user_a).await, count_a);
    assert_eq!(session_count(db, user_b).await, count_b);
}
