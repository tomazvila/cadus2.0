//! The admin-pool reads and writes of the auth tables, and the rate-window guard.

use std::future::Future;

use cadus_store::test_support::TestDb;
use sqlx::types::Uuid;
use sqlx::types::chrono::{DateTime, Utc};

pub async fn mark_verified(db: &TestDb, email: &str) {
    sqlx::query("UPDATE users SET email_verified_at = now() WHERE email = $1::text::citext")
        .bind(email)
        .execute(&db.admin)
        .await
        .unwrap();
}

/// Stamp `disabled_at` with the admin pool.
pub async fn disable(db: &TestDb, email: &str) {
    sqlx::query("UPDATE users SET disabled_at = now() WHERE email = $1::text::citext")
        .bind(email)
        .execute(&db.admin)
        .await
        .unwrap();
}

/// The id of the account that `email` names.
pub async fn user_id(db: &TestDb, email: &str) -> Uuid {
    sqlx::query_scalar("SELECT id FROM users WHERE email = $1::text::citext")
        .bind(email)
        .fetch_one(&db.admin)
        .await
        .unwrap()
}

/// Write one `auth_tokens` row with the admin pool.
pub async fn seed_token(
    db: &TestDb,
    user: Uuid,
    token_hash: &str,
    purpose: &str,
    expires_at: DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO auth_tokens (token_hash, user_id, purpose, expires_at) VALUES ($1, $2, $3, $4)",
    )
    .bind(token_hash)
    .bind(user)
    .bind(purpose)
    .bind(expires_at)
    .execute(&db.admin)
    .await
    .unwrap();
}

/// Write one `auth_sessions` row with the admin pool.
pub async fn seed_session(
    db: &TestDb,
    user: Uuid,
    token_hash: &str,
    created_at: DateTime<Utc>,
    last_seen_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO auth_sessions (token_hash, user_id, created_at, last_seen_at, expires_at)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(token_hash)
    .bind(user)
    .bind(created_at)
    .bind(last_seen_at)
    .bind(expires_at)
    .execute(&db.admin)
    .await
    .unwrap();
}

/// How many session rows one account has.
pub async fn session_count(db: &TestDb, user: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM auth_sessions WHERE user_id = $1")
        .bind(user)
        .fetch_one(&db.admin)
        .await
        .unwrap()
}

/// How many live tokens of `purpose` one account has.
pub async fn token_count(db: &TestDb, user: Uuid, purpose: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM auth_tokens WHERE user_id = $1 AND purpose = $2")
        .bind(user)
        .bind(purpose)
        .fetch_one(&db.admin)
        .await
        .unwrap()
}

/// The `last_seen_at` of one session row.
pub async fn last_seen_at(db: &TestDb, token_hash: &str) -> DateTime<Utc> {
    sqlx::query_scalar("SELECT last_seen_at FROM auth_sessions WHERE token_hash = $1")
        .bind(token_hash)
        .fetch_one(&db.admin)
        .await
        .unwrap()
}

/// The stored password hash of one account.
pub async fn password_hash(db: &TestDb, email: &str) -> Option<String> {
    sqlx::query_scalar("SELECT password_hash FROM users WHERE email = $1::text::citext")
        .bind(email)
        .fetch_one(&db.admin)
        .await
        .unwrap()
}

/// Drop the password hash of `email` with the admin pool.
///
/// The row that stays behind has the shape of an OAuth-only account: an address
/// and no password. The M5 call order refuses a password login against it
/// (specification section 3.3, "Login").
pub async fn clear_password_hash(db: &TestDb, email: &str) {
    sqlx::query("UPDATE users SET password_hash = NULL WHERE email = $1::text::citext")
        .bind(email)
        .execute(&db.admin)
        .await
        .unwrap();
}

/// Whether `email` is a verified address.
pub async fn is_verified(db: &TestDb, email: &str) -> bool {
    let stamp: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT email_verified_at FROM users WHERE email = $1::text::citext")
            .bind(email)
            .fetch_one(&db.admin)
            .await
            .unwrap();
    stamp.is_some()
}

/// `now` plus `secs`, for a seeded window.
pub fn shift(secs: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(Utc::now().timestamp() + secs, 0).unwrap()
}

/// The fixed-window index of the cluster clock, for a window of `window_secs`.
///
/// A rate rule floors `now` to a window that starts at the 1970 epoch
/// (specification section 3.2). The expression below writes that formula in SQL,
/// so the guard never calls the function it guards.
pub async fn window_index(db: &TestDb, window_secs: i64) -> i64 {
    sqlx::query_scalar("SELECT floor(extract(epoch FROM now()) / $1)::bigint")
        .bind(window_secs)
        .fetch_one(&db.admin)
        .await
        .unwrap()
}

/// Delete every rate counter of this throwaway database.
pub async fn clear_rate_counters(db: &TestDb) {
    sqlx::query("DELETE FROM auth_rate_counters")
        .execute(&db.admin)
        .await
        .unwrap();
}

/// Run `burst` inside ONE fixed window, and give the answer of that run.
///
/// **Why the guard exists.** The windows are fixed, not sliding: a burst that
/// crosses a boundary starts a second counter, the tally falls back to 1, and
/// the refusal the test asks for never comes. The shortest window is 300
/// seconds and a burst of 31 logins takes seconds, so the crossing is rare and
/// it is not impossible. A test that fails once a month is a test nobody trusts.
///
/// The guard reads the window index before and after the burst. Two equal
/// indices mean one window held the whole burst, and the answer stands. Two
/// different indices mean the window rolled: the guard clears the counters and
/// runs the burst again. A second crossing fails the test instead of looping.
pub async fn in_one_window<F, Fut, T>(db: &TestDb, window_secs: i64, mut burst: F) -> T
where
    F: FnMut() -> Fut,
    Fut: Future<Output = T>,
{
    let opened = window_index(db, window_secs).await;
    let answer = burst().await;
    if window_index(db, window_secs).await == opened {
        return answer;
    }

    clear_rate_counters(db).await;
    let opened = window_index(db, window_secs).await;
    let answer = burst().await;
    assert_eq!(
        window_index(db, window_secs).await,
        opened,
        "the burst crossed a window boundary twice"
    );
    answer
}
