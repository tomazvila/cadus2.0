//! M5 U6 acceptance, store half: the D-S6 row, the advisory lock, the append-only
//! log, and the fold (C2, C3, D4, D-S6).
//!
//! Every expected value here is a LITERAL: a literal lock key, a literal `seq`,
//! a literal row count, a literal branch flag. Nothing is read back from the
//! code under test.
//!
//! The literals come from:
//!
//! - `docs/reference/web-service-1.0-spec.md` section 4.2 (one lock per
//!   `(user, 'web_state')`) and section 4.3 (the one-transaction grade path,
//!   the `ON CONFLICT DO NOTHING` no-op, the full replay on `regraded`);
//! - `migrations/0003_event_log.sql` (`events_attempt_idem`, the dense `seq`);
//! - `migrations/0006_grants_rls.sql` (`tenant_isolation` on `web_states`).
//!
//! Every test takes its own throwaway database.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_core::event::Event;
use cadus_store::begin_tenant;
use cadus_store::state::{
    WEB_STATE_LOCK_NAMESPACE, append_event, clear_web_state, load_web_state, lock_web_state,
    save_web_state, web_state_lock_key,
};
use cadus_store::test_support::TestDb;
use common::app_db as app;
use common::events::{attempt, end, start};
use common::state::{open_locked, read_log};
use serde_json::json;
use uuid::Uuid;

// --------------------------------------------------------------------------- //
// The advisory lock
// --------------------------------------------------------------------------- //

/// Spec section 4.2: the key is derived in exactly ONE place, and the namespace
/// is `web`. Both halves are literals here, so a second spelling of the key
/// fails this test instead of silently serializing nothing.
#[test]
fn the_lock_key_is_the_web_namespace_and_a_fold_of_the_uuid() {
    assert_eq!(WEB_STATE_LOCK_NAMESPACE, 7_824_738);

    // 00010203-0405-0607-0809-0a0b0c0d0e0f folds as
    // 0x00010203 ^ 0x04050607 ^ 0x08090a0b ^ 0x0c0d0e0f = 0x00000000.
    let zeroish = Uuid::parse_str("00010203-0405-0607-0809-0a0b0c0d0e0f").unwrap();
    assert_eq!(web_state_lock_key(zeroish), (7_824_738, 0));

    // 00000000-0000-0000-0000-000000000001 folds to 1.
    let one = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    assert_eq!(web_state_lock_key(one), (7_824_738, 1));

    // The same uuid always gives the same key.
    let user = Uuid::new_v4();
    assert_eq!(web_state_lock_key(user), web_state_lock_key(user));
}

/// Spec section 4.2: the lock is transaction-scoped, so a second tab of the SAME
/// learner cannot take it while the first transaction is open.
#[tokio::test]
async fn a_second_tab_cannot_take_the_lock_while_the_first_holds_it() {
    TestDb::with(|db| async move {
        let user = db.seed_user("lock@example.com").await;
        let handle = app(&db);
        let (class, key) = web_state_lock_key(user);

        let mut first = begin_tenant(handle.pool(), user).await.unwrap();
        lock_web_state(&mut first, user).await.unwrap();

        // A second session tries the very same key. `pg_try_advisory_xact_lock`
        // never waits, so the answer is the literal contention verdict.
        let second = db.pool_as("cadus_app", 1).await;
        let taken: bool = sqlx::query_scalar!(
            r#"SELECT pg_try_advisory_xact_lock($1, $2) AS "taken!""#,
            class,
            key
        )
        .fetch_one(&second)
        .await
        .unwrap();
        assert!(!taken, "the second tab took a lock the first one holds");

        first.rollback().await.unwrap();

        // The lock went away with the transaction.
        let after: bool = sqlx::query_scalar!(
            r#"SELECT pg_try_advisory_xact_lock($1, $2) AS "taken!""#,
            class,
            key
        )
        .fetch_one(&second)
        .await
        .unwrap();
        assert!(after, "the lock outlived its transaction");
        second.close().await;
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The D-S6 row and C3
// --------------------------------------------------------------------------- //

/// The row round-trips, and RLS hides it: tenant A never reads tenant B's state
/// row (`migrations/0006_grants_rls.sql`, `tenant_isolation` on `web_states`).
#[tokio::test]
async fn tenant_a_never_reads_tenant_b_state_row() {
    TestDb::with(|db| async move {
        let alice = db.seed_user("alice@example.com").await;
        let bob = db.seed_user("bob@example.com").await;
        let handle = app(&db);

        let doc = json!({"session": "s_2026-01-01a", "active_secs": 42.0});
        let mut tx = begin_tenant(handle.pool(), alice).await.unwrap();
        save_web_state(&mut tx, alice, &doc).await.unwrap();
        tx.commit().await.unwrap();

        let mut tx = begin_tenant(handle.pool(), alice).await.unwrap();
        let read = load_web_state(&mut tx, alice).await.unwrap();
        tx.rollback().await.unwrap();
        assert_eq!(read, Some(doc));

        // Bob's transaction asks for Alice's row by primary key. The policy
        // answers zero rows, so the read is None and not a leak.
        let mut tx = begin_tenant(handle.pool(), bob).await.unwrap();
        let leaked = load_web_state(&mut tx, alice).await.unwrap();
        assert_eq!(leaked, None, "the policy let another tenant read the row");
        let own = load_web_state(&mut tx, bob).await.unwrap();
        assert_eq!(own, None);

        // A raw SELECT with no WHERE reads exactly zero of Alice's rows.
        let visible: i64 = sqlx::query_scalar!(r#"SELECT count(*) AS "n!" FROM web_states"#)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(visible, 0);
        tx.rollback().await.unwrap();

        // The row is still there for its owner: Bob's read wrote nothing.
        let total: i64 = sqlx::query_scalar!(r#"SELECT count(*) AS "n!" FROM web_states"#)
            .fetch_one(&db.admin)
            .await
            .unwrap();
        assert_eq!(total, 1);
    })
    .await;
}

/// `clear_web_state` drops the row. An enroll and a session end both call it.
#[tokio::test]
async fn clearing_the_state_row_removes_it() {
    TestDb::with(|db| async move {
        let user = db.seed_user("clear@example.com").await;
        let handle = app(&db);

        let mut tx = begin_tenant(handle.pool(), user).await.unwrap();
        save_web_state(&mut tx, user, &json!({"active_secs": 1.0}))
            .await
            .unwrap();
        clear_web_state(&mut tx, user).await.unwrap();
        let read = load_web_state(&mut tx, user).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(read, None);

        let total: i64 = sqlx::query_scalar!(r#"SELECT count(*) AS "n!" FROM web_states"#)
            .fetch_one(&db.admin)
            .await
            .unwrap();
        assert_eq!(total, 0);
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The append-only log
// --------------------------------------------------------------------------- //

/// The `seq` is dense and per user, and a repeated `attempt_id` appends nothing
/// (`migrations/0003_event_log.sql`, spec section 4.3 step 6).
#[tokio::test]
async fn the_log_numbers_densely_and_dedups_a_repeated_attempt_id() {
    TestDb::with(|db| async move {
        let user = db.seed_user("log@example.com").await;
        let other = db.seed_user("other@example.com").await;
        let handle = app(&db);

        let mut tx = open_locked(&handle, user).await;
        assert_eq!(
            append_event(&mut tx, user, &start("s_2026-01-01a"), None)
                .await
                .unwrap(),
            Some(1)
        );
        assert_eq!(
            append_event(&mut tx, user, &attempt("t-1"), Some("t-1"))
                .await
                .unwrap(),
            Some(2)
        );
        // The same attempt id again. The partial unique index makes it a no-op.
        assert_eq!(
            append_event(&mut tx, user, &attempt("t-1"), Some("t-1"))
                .await
                .unwrap(),
            None
        );
        assert_eq!(
            append_event(&mut tx, user, &end("s_2026-01-01a"), None)
                .await
                .unwrap(),
            Some(3)
        );
        tx.commit().await.unwrap();

        // A second tenant starts at 1 again: `seq` is per user.
        let mut tx = begin_tenant(handle.pool(), other).await.unwrap();
        lock_web_state(&mut tx, other).await.unwrap();
        assert_eq!(
            append_event(&mut tx, other, &start("s_2026-01-01a"), None)
                .await
                .unwrap(),
            Some(1)
        );
        tx.commit().await.unwrap();

        let rows = read_log(&handle, user).await;
        assert_eq!(rows.len(), 3);
        assert_eq!(
            rows.iter().map(|row| row.seq).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert_eq!(
            rows.iter()
                .map(|row| row.event.type_name())
                .collect::<Vec<_>>(),
            vec!["session_start", "attempt", "session_end"]
        );
    })
    .await;
}

/// The export contract: every stored payload reads back through the event reader
/// unchanged (spec section 11 row U6, `api.py:2245-2269`).
#[tokio::test]
async fn every_stored_event_round_trips_through_the_event_reader() {
    TestDb::with(|db| async move {
        let user = db.seed_user("export@example.com").await;
        let handle = app(&db);
        let written = vec![start("s_2026-01-01a"), attempt("t-1"), end("s_2026-01-01a")];

        let mut tx = open_locked(&handle, user).await;
        for event in &written {
            let attempt_id = match event {
                Event::Attempt(body) => Some(body.attempt_id.as_str()),
                _ => None,
            };
            append_event(&mut tx, user, event, attempt_id)
                .await
                .unwrap();
        }
        tx.commit().await.unwrap();

        let rows = read_log(&handle, user).await;

        for (row, original) in rows.iter().zip(&written) {
            let line = row.event.to_canonical_json().unwrap();
            let parsed = Event::from_json(&line).unwrap();
            assert_eq!(&parsed, original, "the export line did not round-trip");
        }
    })
    .await;
}
