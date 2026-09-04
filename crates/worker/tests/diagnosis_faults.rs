//! The diagnosis worker under a database that refuses one statement (D-O5, T6).
//!
//! Every statement of the pass returns the store's error, and the pass stops at
//! that statement and nowhere else. A role that lacks exactly one privilege
//! (`common::with_grants`), a pool that is closed (`common::closed_handle`), or
//! a deferred constraint that fires at COMMIT makes the one statement fail, so
//! each test names the statement the pass stopped at.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_store::test_support::TestDb;
use cadus_worker::diagnosis::{Job, Outcome, calls_this_session, claim, fail, run_once, sweep};
use cadus_worker::model_log::{CallRecord, PURPOSE_DIAGNOSIS, write};
use serde_json::json;
use sqlx::types::Uuid;

use common::{
    FakeModel, closed_handle, diagnosis_reply, enqueue, enqueue_as, handle, ledger, one_attempt,
    payload, row_of, with_grants,
};

/// The schema, so the role reaches the tables at all.
const SCHEMA: &str = "GRANT USAGE ON SCHEMA public TO {role}";

/// Every privilege the claim needs on the queue.
const CLAIM: &str =
    "GRANT SELECT, UPDATE (status, claimed_at, attempts) ON diagnosis_jobs TO {role}";

/// A constraint trigger that fires at COMMIT on every update of the queue, so
/// the transaction that settles a row fails at its commit and nowhere before.
const FAIL_AT_COMMIT: [&str; 2] = [
    "CREATE FUNCTION refuse_commit() RETURNS trigger LANGUAGE plpgsql AS $$
     BEGIN RAISE EXCEPTION 'the commit is refused'; END $$",
    "CREATE CONSTRAINT TRIGGER refuse_commit AFTER UPDATE ON diagnosis_jobs
     DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION refuse_commit()",
];

/// The store error text of one refused statement.
fn store_error(err: cadus_worker::WorkerError) -> String {
    let text = err.to_string();
    assert!(
        text.starts_with("store error: ") || text.starts_with("database error: "),
        "{text}"
    );
    text
}

/// A claimed row, as the pass holds it, with this attempt count.
fn claimed(id: Uuid, user: Uuid, attempts: i32) -> Job {
    Job {
        id,
        user_id: user,
        attempt_id: "task-1".to_owned(),
        payload: payload(Some("session-1")),
        attempts,
    }
}

/// A closed pool fails every statement of the pass at its first statement: the
/// sweep, the claim, the count, the release, and the settle.
#[tokio::test]
async fn a_closed_pool_fails_every_statement_at_its_first_statement() {
    TestDb::with(|db| async move {
        let user = db.seed_user("closed@example.test").await;
        let id = enqueue(&db.admin, user, "task-1", &payload(Some("session-1"))).await;
        let closed = closed_handle(&db).await;
        let server = FakeModel::start(Vec::new()).await;
        let mut job = server.diagnosis_job(0);

        store_error(sweep(&closed).await.unwrap_err());
        store_error(claim(&closed).await.unwrap_err());
        store_error(
            calls_this_session(&closed, &claimed(id, user, 1), "session-1")
                .await
                .unwrap_err(),
        );
        store_error(
            fail(&closed, &claimed(id, user, 1), "why")
                .await
                .unwrap_err(),
        );
        store_error(
            fail(&closed, &claimed(id, user, 3), "why")
                .await
                .unwrap_err(),
        );
        // The first pass stops at the sweep; the second pass stops at the
        // claim, because the sweep is not due again.
        store_error(run_once(&closed, &mut job).await.unwrap_err());
        store_error(run_once(&closed, &mut job).await.unwrap_err());
        let record = CallRecord {
            purpose: PURPOSE_DIAGNOSIS,
            user_id: Some(user),
            session_id: None,
        };
        store_error(write(&closed, &record, &[one_attempt()]).await.unwrap_err());
        assert!(server.calls().is_empty());
    })
    .await;
}

/// The sweep runs two statements, and the second one is the one it stops at
/// when the role cannot clear a lease.
#[tokio::test]
async fn the_sweep_stops_at_the_lease_reset_the_role_cannot_run() {
    TestDb::with(|db| async move {
        with_grants(
            &db,
            &[
                SCHEMA,
                "GRANT SELECT, UPDATE (status, finished_at) ON diagnosis_jobs TO {role}",
            ],
            |_, handle| async move {
                assert!(
                    store_error(sweep(&handle).await.unwrap_err()).contains("permission denied")
                );
            },
        )
        .await;
    })
    .await;
}

/// Every end state of a claimed row is one UPDATE, and each one is the
/// statement the pass stops at when the role cannot write the result: the
/// dead letter of an unreadable payload, the cap, the diagnosis, and the dead
/// letter of a third failure.
#[tokio::test]
async fn every_end_state_stops_at_the_settle_the_role_cannot_run() {
    TestDb::with(|db| async move {
        let user = db.seed_user("settle@example.test").await;
        enqueue(
            &db.admin,
            user,
            "task-1",
            &json!({"v": 1, "nonsense": true}),
        )
        .await;
        enqueue_as(&db.admin, user, "task-0", "done", 1).await;
        let capped = enqueue(&db.admin, user, "task-2", &payload(Some("session-1"))).await;
        let done = enqueue(&db.admin, user, "task-3", &payload(None)).await;
        let dead = enqueue_as(&db.admin, user, "task-4", "pending", 2).await;

        with_grants(
            &db,
            &[
                SCHEMA,
                CLAIM,
                "GRANT INSERT ON model_call_log TO {role}",
                "GRANT USAGE ON SEQUENCE model_call_log_id_seq TO {role}",
            ],
            move |db, handle| async move {
                let server = FakeModel::start(vec![
                    diagnosis_reply("{\"error_tags\":[],\"prose\":\"Try again.\"}"),
                    (500, String::new()),
                    (500, String::new()),
                ])
                .await;
                let mut job = server.diagnosis_job(1);

                for (row, calls) in [
                    (None, 0),
                    (Some(capped), 0),
                    (Some(done), 1),
                    (Some(dead), 3),
                ] {
                    let err = run_once(&handle, &mut job).await.unwrap_err();
                    assert!(store_error(err).contains("permission denied"));
                    assert_eq!(server.call_count(), calls);
                    if let Some(id) = row {
                        assert_eq!(
                            row_of(&db.admin, id).await.0,
                            "running",
                            "the row stays claimed"
                        );
                    }
                }
                assert_eq!(
                    ledger(&db.admin).await.len(),
                    3,
                    "the paid calls are in the ledger"
                );
            },
        )
        .await;
    })
    .await;
}

/// The session count reads the payload with `->>`, so a role that cannot run
/// that operator stops the pass at the count and calls no model.
#[tokio::test]
async fn the_cap_stops_at_the_count_the_role_cannot_run() {
    TestDb::with(|db| async move {
        let user = db.seed_user("count@example.test").await;
        enqueue(&db.admin, user, "task-1", &payload(Some("session-1"))).await;

        with_grants(
            &db,
            &[
                SCHEMA,
                "GRANT SELECT, UPDATE ON diagnosis_jobs TO {role}",
                "REVOKE EXECUTE ON FUNCTION pg_catalog.jsonb_object_field_text(jsonb, text) FROM PUBLIC",
            ],
            |_, handle| async move {
                let server = FakeModel::start(Vec::new()).await;
                let mut job = server.diagnosis_job(1);
                let err = run_once(&handle, &mut job).await.unwrap_err();
                assert!(store_error(err).contains("permission denied"));
                assert!(server.calls().is_empty());
            },
        )
        .await;
    })
    .await;
}

/// The settle writes the row and pushes the notice in ONE transaction, so a
/// notice the role cannot push fails the settle, and a commit a deferred
/// constraint refuses fails it too.
#[tokio::test]
async fn the_settle_stops_at_the_notice_or_at_the_commit() {
    TestDb::with(|db| async move {
        let user = db.seed_user("notice@example.test").await;
        let id = enqueue(&db.admin, user, "task-1", &payload(None)).await;

        with_grants(
            &db,
            &[
                SCHEMA,
                "GRANT SELECT, UPDATE ON diagnosis_jobs TO {role}",
                "REVOKE EXECUTE ON FUNCTION pg_catalog.pg_notify(text, text) FROM PUBLIC",
            ],
            move |_, handle| async move {
                let err = fail(&handle, &claimed(id, user, 3), "why")
                    .await
                    .unwrap_err();
                assert!(store_error(err).contains("permission denied"));
            },
        )
        .await;
        assert_eq!(
            row_of(&db.admin, id).await.0,
            "pending",
            "the transaction rolled back"
        );

        for statement in FAIL_AT_COMMIT {
            sqlx::query(sqlx::AssertSqlSafe(statement.to_owned()))
                .execute(&db.admin)
                .await
                .unwrap();
        }
        let err = fail(&handle(&db), &claimed(id, user, 3), "why")
            .await
            .unwrap_err();
        assert!(store_error(err).contains("the commit is refused"));
        assert_eq!(
            row_of(&db.admin, id).await.0,
            "pending",
            "the commit rolled back"
        );
    })
    .await;
}

/// A release the role cannot run stops a failed attempt at the release, and the
/// pass reports the store's error.
#[tokio::test]
async fn a_failed_attempt_stops_at_the_release_the_role_cannot_run() {
    TestDb::with(|db| async move {
        let user = db.seed_user("release@example.test").await;
        let id = enqueue(&db.admin, user, "task-1", &payload(None)).await;

        with_grants(
            &db,
            &[
                SCHEMA,
                "GRANT SELECT, UPDATE (status, finished_at, result) ON diagnosis_jobs TO {role}",
            ],
            move |_, handle| async move {
                let err = fail(&handle, &claimed(id, user, 1), "why")
                    .await
                    .unwrap_err();
                assert!(store_error(err).contains("permission denied"));
            },
        )
        .await;
        assert_eq!(row_of(&db.admin, id).await.0, "pending");
    })
    .await;
}

/// A ledger write that fails stops nothing: the pass logs it and settles the
/// row, so the learner keeps the diagnosis that is already paid for (T6).
#[tokio::test]
async fn a_ledger_write_that_fails_does_not_stop_the_pass() {
    TestDb::with(|db| async move {
        let user = db.seed_user("ledger@example.test").await;
        let id = enqueue(&db.admin, user, "task-1", &payload(Some("session-1"))).await;

        with_grants(
            &db,
            &[SCHEMA, "GRANT SELECT, UPDATE ON diagnosis_jobs TO {role}"],
            move |db, handle| async move {
                let server = FakeModel::start(vec![diagnosis_reply(
                    "{\"error_tags\":[\"units\"],\"prose\":\"Name the unit.\"}",
                )])
                .await;
                let mut job = server.diagnosis_job(0);
                let report = run_once(&handle, &mut job).await.unwrap();
                assert_eq!(report.outcome, Outcome::Done);
                assert_eq!(report.attempts.len(), 1);
                assert_eq!(row_of(&db.admin, id).await.0, "done");
                assert!(ledger(&db.admin).await.is_empty(), "the bill did not land");
            },
        )
        .await;
    })
    .await;
}

/// The ledger write is one transaction: the rows land together or not at all,
/// so a commit a deferred constraint refuses leaves no row.
#[tokio::test]
async fn a_ledger_commit_that_fails_leaves_no_row() {
    TestDb::with(|db| async move {
        for statement in [
            "CREATE FUNCTION refuse_commit() RETURNS trigger LANGUAGE plpgsql AS $$
             BEGIN RAISE EXCEPTION 'the commit is refused'; END $$",
            "CREATE CONSTRAINT TRIGGER refuse_commit AFTER INSERT ON model_call_log
             DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION refuse_commit()",
        ] {
            sqlx::query(sqlx::AssertSqlSafe(statement.to_owned()))
                .execute(&db.admin)
                .await
                .unwrap();
        }
        let record = CallRecord {
            purpose: PURPOSE_DIAGNOSIS,
            user_id: None,
            session_id: None,
        };

        let err = write(&handle(&db), &record, &[one_attempt()])
            .await
            .unwrap_err()
            .to_string();

        assert!(err.contains("the commit is refused"), "{err}");
        assert!(ledger(&db.admin).await.is_empty());
    })
    .await;
}
