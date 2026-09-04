//! The authoring loop under a database that refuses one statement (C6, T3).
//!
//! Every read and every write of the loop returns the store's error, and the
//! loop stops at that statement and nowhere else. A role that lacks exactly one
//! privilege (`common::with_grants`) or a pool that is closed
//! (`common::closed_handle`) makes the one statement fail, so each test names
//! the statement the loop stopped at.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::Arc;

use cadus_store::Db;
use cadus_store::test_support::TestDb;
use cadus_worker::authoring::job::{
    Outcome, author_one, run_batch, served_instances, slots_taken, stale_rows, stale_slots,
    store_pending,
};
use cadus_worker::authoring::prompt::Kind;
use cadus_worker::model_log::{CallRecord, PURPOSE_AUTHORING, write};

use common::{
    FakeModel, SQUARES_KEY as KP_KEY, STORED_TEACH_BODY, STORED_TEACH_DIGEST, Seed, closed_handle,
    content_rows, good_arguments, ledger_shape, named_reply, one_attempt, squares_spec,
    teach_arguments, tool_reply, with_grants,
};

/// The schema, and every column of `content_store` but the one a test names.
const SCHEMA: &str = "GRANT USAGE ON SCHEMA public TO {role}";

/// The store error text of one refused statement.
fn store_error(err: cadus_worker::WorkerError) -> String {
    let text = err.to_string();
    assert!(text.starts_with("store error: "), "{text}");
    text
}

/// A closed pool fails every read and every write of the loop at its first
/// statement.
#[tokio::test]
async fn a_closed_pool_fails_every_read_and_write_at_its_first_statement() {
    TestDb::with(|db| async move {
        let closed = closed_handle(&db).await;
        let fake = FakeModel::start(vec![tool_reply(&good_arguments())]).await;

        store_error(
            slots_taken(&closed, KP_KEY, Kind::Template)
                .await
                .unwrap_err(),
        );
        store_error(
            stale_slots(&closed, KP_KEY, Kind::Template)
                .await
                .unwrap_err(),
        );
        store_error(served_instances(&closed, KP_KEY).await.unwrap_err());
        store_error(stale_rows(&closed, &[Kind::Teach]).await.unwrap_err());
        store_error(
            store_pending(&closed, KP_KEY, Kind::Template, "{}", 1, &[])
                .await
                .unwrap_err(),
        );
        store_error(
            author_one(&closed, &fake.job(), Kind::Template, &squares_spec())
                .await
                .unwrap_err(),
        );
        store_error(
            run_batch(&closed, &fake.job(), Kind::Template, &[squares_spec()])
                .await
                .unwrap_err(),
        );
        let record = CallRecord {
            purpose: PURPOSE_AUTHORING,
            user_id: None,
            session_id: None,
        };
        let ledger = write(&closed, &record, &[one_attempt()])
            .await
            .unwrap_err()
            .to_string();
        assert!(ledger.starts_with("database error: "), "{ledger}");
        assert_eq!(fake.call_count(), 0, "no read reached the model");
    })
    .await;
}

/// Run `author_one` on the teach kind as a role that holds these grants, and
/// give the store error it stopped at.
async fn teach_fails_with(db: &Arc<TestDb>, grants: &'static [&'static str]) -> String {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    with_grants(db, grants, |_, handle| async move {
        let fake = FakeModel::start(vec![named_reply("emit_teach", &teach_arguments())]).await;
        let err = author_one(&handle, &fake.job(), Kind::Teach, &squares_spec())
            .await
            .unwrap_err();
        sender.send(store_error(err)).unwrap();
    })
    .await;
    receiver.await.unwrap()
}

/// The second count of the bank fails when the role cannot read the prompt
/// stamp, and the served-instance read fails when it cannot read the approval
/// time: each statement is the one the loop stops at.
#[tokio::test]
async fn the_bank_reads_stop_at_the_statement_the_role_cannot_run() {
    TestDb::with(|db| async move {
        let no_stamp = teach_fails_with(
            &db,
            &[
                SCHEMA,
                "GRANT SELECT (digest, kp_id, kind, status, body) ON content_store TO {role}",
            ],
        )
        .await;
        assert!(no_stamp.contains("permission denied"), "{no_stamp}");

        let no_approval_time = teach_fails_with(
            &db,
            &[
                SCHEMA,
                "GRANT SELECT (digest, kp_id, kind, status, body, prompt_digest, created_at) \
                 ON content_store TO {role}",
            ],
        )
        .await;
        assert!(
            no_approval_time.contains("permission denied"),
            "{no_approval_time}"
        );
    })
    .await;
}

/// The batch stops at the pass that fails, after the stale-first read ran.
#[tokio::test]
async fn the_batch_stops_at_the_pass_that_fails() {
    TestDb::with(|db| async move {
        with_grants(
            &db,
            &[
                SCHEMA,
                "GRANT SELECT (digest, kp_id, kind, status, body, prompt_digest, created_at) \
                 ON content_store TO {role}",
            ],
            |_, handle| async move {
                let fake = FakeModel::start(Vec::new()).await;
                let err = run_batch(&handle, &fake.job(), Kind::Teach, &[squares_spec()])
                    .await
                    .unwrap_err();
                assert!(store_error(err).contains("permission denied"));
                assert_eq!(fake.call_count(), 0);
            },
        )
        .await;
    })
    .await;
}

/// The store of a verified body stops at the insert when the role cannot
/// write, and the paid call stays in the ledger.
#[tokio::test]
async fn the_store_stops_at_an_insert_the_role_cannot_run() {
    TestDb::with(|db| async move {
        with_grants(
            &db,
            &[
                SCHEMA,
                "GRANT SELECT ON content_store TO {role}",
                "GRANT INSERT ON model_call_log TO {role}",
                "GRANT USAGE ON SEQUENCE model_call_log_id_seq TO {role}",
            ],
            |db, handle| async move {
                let fake = FakeModel::start(vec![tool_reply(&good_arguments())]).await;
                let err = author_one(&handle, &fake.job(), Kind::Template, &squares_spec())
                    .await
                    .unwrap_err();
                assert!(store_error(err).contains("permission denied"));
                assert_eq!(fake.call_count(), 1);
                assert_eq!(ledger_shape(&db.admin, "authoring").await, (1, 0));
                assert!(content_rows(&db.admin, KP_KEY).await.is_empty());
            },
        )
        .await;
    })
    .await;
}

/// A collision reads the verdict of the held row and stamps the current prompt
/// on it, and each of those two statements is the one the store stops at when
/// the role cannot run it.
#[tokio::test]
async fn a_collision_stops_at_the_verdict_read_or_the_prompt_stamp() {
    TestDb::with(|db| async move {
        // The digest of the empty body `{}` under the knowledge point and the
        // template kind, so the store below collides with this row.
        Seed::new("sha256:deb2349817ba6f6d", KP_KEY, "template", "pending")
            .prompt(common::OLD_PROMPT)
            .insert(&db.admin)
            .await;

        for grants in [
            &[
                SCHEMA,
                "GRANT SELECT (digest, kp_id, kind, status, body, prompt_digest, created_at) \
                 ON content_store TO {role}",
                "GRANT INSERT ON content_store TO {role}",
            ][..],
            &[
                SCHEMA,
                "GRANT SELECT ON content_store TO {role}",
                "GRANT INSERT ON content_store TO {role}",
            ][..],
        ] {
            with_grants(&db, grants, |_, handle| async move {
                let err = store_pending(&handle, KP_KEY, Kind::Template, "{}", 1, &[])
                    .await
                    .unwrap_err();
                assert!(store_error(err).contains("permission denied"));
            })
            .await;
        }
    })
    .await;
}

/// A ledger write that fails stops nothing: the pass logs it and stores the
/// document, so the learner-facing content never waits on the bill (T6).
#[tokio::test]
async fn a_ledger_write_that_fails_does_not_stop_the_pass() {
    TestDb::with(|db| async move {
        with_grants(
            &db,
            &[
                SCHEMA,
                "GRANT SELECT, INSERT, UPDATE ON content_store TO {role}",
            ],
            |db, handle| async move {
                let fake =
                    FakeModel::start(vec![named_reply("emit_teach", &teach_arguments())]).await;
                let report = author_one(&handle, &fake.job(), Kind::Teach, &squares_spec())
                    .await
                    .unwrap();
                assert_eq!(report.outcome, Outcome::Stored);
                assert_eq!(report.digest.as_deref(), Some(STORED_TEACH_DIGEST));
                assert_eq!(ledger_shape(&db.admin, "authoring").await, (0, 0));
                let rows = content_rows(&db.admin, KP_KEY).await;
                assert_eq!(rows.len(), 1);
                assert_eq!(
                    rows[0].body,
                    serde_json::from_str::<serde_json::Value>(STORED_TEACH_BODY).unwrap()
                );
            },
        )
        .await;
    })
    .await;
}

/// An empty attempt list writes no ledger row and opens no transaction.
#[tokio::test]
async fn an_empty_attempt_list_writes_no_ledger_row() {
    TestDb::with(|db| async move {
        let handle: Db = common::handle(&db);
        let record = CallRecord {
            purpose: PURPOSE_AUTHORING,
            user_id: None,
            session_id: None,
        };
        assert_eq!(write(&handle, &record, &[]).await.unwrap(), 0);
        assert_eq!(ledger_shape(&db.admin, "authoring").await, (0, 0));
    })
    .await;
}
