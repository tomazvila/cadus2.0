//! The M5 auth contract, part 3: the rate counters, the session lookup, and
//! the bound profile read (specification sections 3.2 and 3.3).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_store::auth::{self};
use cadus_store::begin_tenant;
use cadus_store::test_support::TestDb;
use sqlx::types::chrono::DateTime;

// ---------------------------------------------------------------------------
// The rate counters.
// ---------------------------------------------------------------------------

/// Specification section 3.2: the counter is keyed by scope, key, and window
/// start. The first call of a window writes 1, and each later call adds 1.
///
/// The email counter and the IP counter are separate rows, so one address does
/// not spend the budget of an address that shares its network.
#[tokio::test]
async fn the_rate_counter_upsert_counts_one_window_at_a_time() {
    TestDb::with(|db| async move {
        let window = DateTime::from_timestamp(1_756_000_000, 0).unwrap();
        let next = DateTime::from_timestamp(1_756_003_600, 0).unwrap();

        for expected in [1, 2, 3] {
            let count =
                auth::bump_rate_counter(&db.app, "signup_email", "learner@example.test", window)
                    .await
                    .unwrap();
            assert_eq!(count, expected);
        }

        // A different scope is a different row.
        assert_eq!(
            auth::bump_rate_counter(&db.app, "signup_ip", "learner@example.test", window)
                .await
                .unwrap(),
            1
        );
        // A different key is a different row.
        assert_eq!(
            auth::bump_rate_counter(&db.app, "signup_email", "other@example.test", window)
                .await
                .unwrap(),
            1
        );
        // A different window is a different row.
        assert_eq!(
            auth::bump_rate_counter(&db.app, "signup_email", "learner@example.test", next)
                .await
                .unwrap(),
            1
        );

        // The table carries no tenant column and no policy, so the upsert works
        // inside a bound transaction too.
        let user = db.seed_user("counter@example.test").await;
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        assert_eq!(
            auth::bump_rate_counter(&mut *tx, "signup_email", "learner@example.test", window)
                .await
                .unwrap(),
            4
        );
        tx.commit().await.unwrap();

        let rows = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM auth_rate_counters"#)
            .fetch_one(&db.admin)
            .await
            .unwrap();
        assert_eq!(rows, 4);
    })
    .await;
}

/// The window start is floored from the 1970 epoch, so every process of a
/// deployment derives the same bucket from the same instant.
#[test]
fn the_window_start_floors_from_the_epoch() {
    let hour = 3_600;
    // 1 756 000 000 = 2025-08-24T01:46:40Z. It is 2 800 s into the hour that
    // starts at 1 755 997 200 = 2025-08-24T01:00:00Z.
    let now = DateTime::from_timestamp(1_756_000_000, 0).unwrap();
    assert_eq!(
        auth::window_start(now, hour),
        DateTime::from_timestamp(1_755_997_200, 0).unwrap()
    );
    // The first instant of a window is its own start.
    assert_eq!(
        auth::window_start(DateTime::from_timestamp(1_755_997_200, 0).unwrap(), hour),
        DateTime::from_timestamp(1_755_997_200, 0).unwrap()
    );
    // The last instant of a window still belongs to it. 1 756 000 799 is
    // 2025-08-24T01:59:59Z.
    assert_eq!(
        auth::window_start(DateTime::from_timestamp(1_756_000_799, 0).unwrap(), hour),
        DateTime::from_timestamp(1_755_997_200, 0).unwrap()
    );
    // The next second opens the next window: 2025-08-24T02:00:00Z.
    assert_eq!(
        auth::window_start(DateTime::from_timestamp(1_756_000_800, 0).unwrap(), hour),
        DateTime::from_timestamp(1_756_000_800, 0).unwrap()
    );
    // The 5-minute window of the login rule: 2025-08-24T01:45:00Z.
    assert_eq!(
        auth::window_start(now, 300),
        DateTime::from_timestamp(1_755_999_900, 0).unwrap()
    );
    // An instant before the epoch floors down, not toward zero.
    assert_eq!(
        auth::window_start(DateTime::from_timestamp(-1, 0).unwrap(), hour),
        DateTime::from_timestamp(-3_600, 0).unwrap()
    );
    // A width of 0 gives the instant unchanged.
    assert_eq!(auth::window_start(now, 0), now);
}

// ---------------------------------------------------------------------------
// Migration 0008 and the bound profile read
// ---------------------------------------------------------------------------

/// The session lookup gives `created_at`, so the 90-day absolute window has a
/// value to test BEFORE the tenant bind.
///
/// Specification section 3.3, "Cookie check", step 1 puts both refusals in the
/// unbound step. Migration 0006 returned three columns and left the second one
/// with nothing to read; migration 0008 adds the fourth.
#[tokio::test]
async fn the_session_lookup_gives_the_created_at_of_the_row() {
    TestDb::with(|db| async move {
        let user = db.seed_user("created-at@example.test").await;
        // 2025-08-24T00:00:00Z, written as an epoch second.
        let minted = DateTime::from_timestamp(1_755_993_600, 0).unwrap();
        // 30 days later: 1 755 993 600 + 2 592 000.
        let ends = DateTime::from_timestamp(1_758_585_600, 0).unwrap();
        sqlx::query!(
            "INSERT INTO auth_sessions
                 (token_hash, user_id, created_at, last_seen_at, expires_at)
             VALUES ('created-at-hash', $1, $2, $3, $4)",
            user,
            minted,
            minted,
            ends
        )
        .execute(&db.admin)
        .await
        .unwrap();

        let session = auth::session_by_token_hash(&db.app, "created-at-hash")
            .await
            .unwrap()
            .expect("the unbound session lookup must read the row");

        assert_eq!(session.user_id, user);
        assert_eq!(session.created_at.timestamp(), 1_755_993_600);
        assert_eq!(session.last_seen_at.timestamp(), 1_755_993_600);
        assert_eq!(session.expires_at.timestamp(), 1_758_585_600);
    })
    .await;
}

/// The bound profile read answers the CALLER's row and no other tenant's.
///
/// `docs/SCHEMA.md`: "after the bind a plain SELECT on `users` reads the
/// caller's own rows". The `users_read_self` policy is what holds it, so the
/// statement carries no `WHERE` clause of its own.
#[tokio::test]
async fn the_bound_profile_read_answers_the_callers_row_alone() {
    TestDb::with(|db| async move {
        let user_a = db.seed_user("profile-a@example.test").await;
        let user_b = db.seed_user("profile-b@example.test").await;
        sqlx::query!(
            "UPDATE users SET email_verified_at = now() WHERE id = $1",
            user_a
        )
        .execute(&db.admin)
        .await
        .unwrap();

        let mut tx = begin_tenant(&db.app, user_a).await.unwrap();
        let mine = auth::account_profile(&mut *tx)
            .await
            .unwrap()
            .expect("the bound read must answer the caller's row");
        let _ = tx.rollback().await;

        assert_eq!(mine.id, user_a);
        assert_eq!(mine.email, "profile-a@example.test");
        assert!(mine.email_verified_at.is_some());
        assert_ne!(mine.id, user_b);

        // Bound to B the same statement answers B's row, never A's.
        let mut tx = begin_tenant(&db.app, user_b).await.unwrap();
        let yours = auth::account_profile(&mut *tx).await.unwrap().unwrap();
        let _ = tx.rollback().await;

        assert_eq!(yours.id, user_b);
        assert_eq!(yours.email, "profile-b@example.test");
        assert_eq!(yours.email_verified_at, None);

        // Unbound the same statement reads nothing at all.
        assert_eq!(auth::account_profile(&db.app).await.unwrap(), None);
    })
    .await;
}
