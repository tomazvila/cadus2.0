//! The pool refill under a source that does not serve and a database that
//! refuses one statement (D-O4, A6, C6).
//!
//! A pair whose fill or insert fails is counted and logged, and the pass goes
//! on; a failure of the two statements that frame the pass stops it. A role
//! that lacks exactly one privilege (`common::with_grants`) or a pool that is
//! closed (`common::closed_handle`) makes the one statement fail.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::time::Instant;

use cadus_core::curriculum::load_curriculum;
use cadus_store::Db;
use cadus_store::pool::unclaimed_depth;
use cadus_store::test_support::TestDb;
use cadus_worker::{RefillReport, WorkerError};

use common::{
    ADDING, Refill, SQUARES, SQUARES_BODY, closed_handle, pool_rows, seed_approved_template,
    seed_drained_pair, seed_fixed_user, seed_squares_pair, with_grants,
};

/// A serving key the fixture curriculum does not name.
const UNKNOWN: &str = "unknown-topic/kp9";

/// The schema, so the role reaches the tables at all.
const SCHEMA: &str = "GRANT USAGE ON SCHEMA public TO {role}";

/// A template whose answer expression does not parse, so it never compiles.
const UNPARSED_BODY: &str = r#"{
    "v": 1,
    "topic_id": "unknown-topic",
    "answer_kind": "numeric",
    "statement": "Compute ${a} + 1$.",
    "params": {"a": {"kind": "int", "low": 1, "high": 20}},
    "answer_expr": "a +",
    "hints": ["Add one."],
    "samples": [{"params": {"a": 1}, "expected": "2"}]
}"#;

/// A template whose constraint no tuple satisfies, so no batch draws.
const UNSATISFIABLE_BODY: &str = r#"{
    "v": 1,
    "topic_id": "unknown-topic",
    "answer_kind": "numeric",
    "statement": "Compute ${a} + 1$.",
    "params": {"a": {"kind": "int", "low": 1, "high": 20}},
    "constraints": [{"op": "gt", "left": "a", "right": "a"}],
    "answer_expr": "a + 1",
    "hints": ["Add one."],
    "samples": [{"params": {"a": 1}, "expected": "2"}]
}"#;

/// One pass at depth 12 over the fixture tree with a fresh state, as the
/// superuser.
async fn one_pass(db: &TestDb) -> RefillReport {
    Refill::new(12, 32, 0).pass(db, 1, Instant::now()).await
}

/// One pass at depth 12 over the fixture tree with a fresh state, through this
/// handle.
async fn one_pass_through(handle: &Db) -> Result<RefillReport, WorkerError> {
    Refill::new(12, 32, 0).pass_through(handle, 1).await
}

/// A serving key the curriculum does not name serves its approved template on
/// the C6 approval alone: the gate has no exemplars and no answer kind to run
/// with, so it does not run again.
#[tokio::test]
async fn an_approved_template_of_an_unknown_knowledge_point_serves_on_its_approval() {
    TestDb::with(|db| async move {
        let user = seed_fixed_user(&db.admin).await;
        seed_approved_template(&db.admin, "template-unknown", UNKNOWN, SQUARES_BODY).await;
        seed_drained_pair(&db.admin, user, UNKNOWN).await;

        let report = one_pass(&db).await;

        assert_eq!(report.targets, 1);
        assert_eq!(report.inserted, 12, "the template fills the pair");
        assert_eq!(report.from_template, 12);
        assert_eq!(report.without_source, 0);
        assert_eq!(report.failed, 0);
        assert_eq!(pool_rows(&db.admin, user, UNKNOWN).await.len(), 12);
    })
    .await;
}

/// An approved body that does not read as a template is refused, remembered,
/// and the pair falls back to its exemplars (A6).
#[tokio::test]
async fn an_approved_body_that_does_not_read_falls_back_to_exemplars() {
    TestDb::with(|db| async move {
        let user = seed_fixed_user(&db.admin).await;
        seed_approved_template(&db.admin, "template-unreadable", SQUARES, r#"{"v": 1}"#).await;
        seed_drained_pair(&db.admin, user, SQUARES).await;

        let mut refill = Refill::new(12, 32, 0);
        let report = refill.pass(&db, 1, Instant::now()).await;

        assert_eq!(report.inserted, 2, "the two exemplars fill in");
        assert_eq!(report.from_exemplar, 2);
        assert_eq!(report.from_template, 0);
        assert_eq!(report.failed, 0);
        let refusal = refill
            .state
            .refusal("template-unreadable")
            .expect("the digest carries its refusal");
        assert!(
            refusal.starts_with("the body did not read: "),
            "the refusal names the parse, it read {refusal}"
        );
    })
    .await;
}

/// A template that reads but does not compile, and one whose constraint no
/// tuple satisfies, fail the pair: the pass counts it and goes on.
#[tokio::test]
async fn a_template_that_does_not_compile_or_draw_fails_the_pair() {
    TestDb::with(|db| async move {
        let user = seed_fixed_user(&db.admin).await;
        seed_approved_template(&db.admin, "template-unparsed", UNKNOWN, UNPARSED_BODY).await;
        seed_drained_pair(&db.admin, user, UNKNOWN).await;
        seed_approved_template(
            &db.admin,
            "template-unsatisfiable",
            "unknown-topic/kp8",
            UNSATISFIABLE_BODY,
        )
        .await;
        seed_drained_pair(&db.admin, user, "unknown-topic/kp8").await;

        let report = one_pass(&db).await;

        assert_eq!(report.targets, 2);
        assert_eq!(report.failed, 2, "neither template gives an instance");
        assert_eq!(report.inserted, 0);
        assert_eq!(report.without_source, 0);
    })
    .await;
}

/// A knowledge point with as many exemplars as the anti-repeat ring holds
/// fills without the short-rotation note, up to the target depth.
#[tokio::test]
async fn a_full_exemplar_rotation_fills_to_the_target_depth() {
    TestDb::with(|db| async move {
        let user = seed_fixed_user(&db.admin).await;
        seed_drained_pair(&db.admin, user, "counting/kp1").await;
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ring");
        let (curriculum, _) = load_curriculum(&root).expect("the ring fixture loads");

        let report = Refill::over(curriculum, 12, 32, 0)
            .pass(&db, 1, Instant::now())
            .await;

        assert_eq!(
            report.inserted, 12,
            "twenty exemplars fill a depth of twelve"
        );
        assert_eq!(report.from_exemplar, 12);
        assert_eq!(report.failed, 0);
        assert_eq!(
            unclaimed_depth(&db.admin, user, "counting/kp1")
                .await
                .unwrap(),
            12
        );
    })
    .await;
}

/// An insert the role cannot run fails the pair, on the template path and on
/// the exemplar path alike, and the pass counts both.
#[tokio::test]
async fn an_insert_the_role_cannot_run_fails_the_pair() {
    TestDb::with(|db| async move {
        let user = seed_squares_pair(&db).await;
        seed_drained_pair(&db.admin, user, ADDING).await;

        with_grants(
            &db,
            &[
                SCHEMA,
                "GRANT SELECT ON content_store TO {role}",
                "GRANT SELECT, UPDATE ON serving_pool TO {role}",
            ],
            move |db, handle| async move {
                let report = one_pass_through(&handle).await.unwrap();

                assert_eq!(report.targets, 2);
                assert_eq!(report.failed, 2, "the template pair and the exemplar pair");
                assert_eq!(report.inserted, 0);
                assert_eq!(unclaimed_depth(&db.admin, user, SQUARES).await.unwrap(), 0);
            },
        )
        .await;
    })
    .await;
}

/// The read of the approved template fails the pair when the role cannot read
/// the body, and the pass goes on.
#[tokio::test]
async fn an_approved_template_the_role_cannot_read_fails_the_pair() {
    TestDb::with(|db| async move {
        seed_squares_pair(&db).await;

        with_grants(
            &db,
            &[
                SCHEMA,
                "GRANT SELECT (digest, kp_id, kind, status, approved_at, created_at) ON content_store TO {role}",
                "GRANT SELECT, UPDATE ON serving_pool TO {role}",
            ],
            |_, handle| async move {
                let report = one_pass_through(&handle).await.unwrap();

                assert_eq!(report.targets, 1);
                assert_eq!(report.failed, 1);
                assert_eq!(report.inserted, 0);
            },
        )
        .await;
    })
    .await;
}

/// The two statements that frame the pass stop it: the retire on a closed pool,
/// and the target query when the role cannot count.
#[tokio::test]
async fn the_retire_and_the_target_query_stop_the_pass() {
    TestDb::with(|db| async move {
        let closed = closed_handle(&db).await;
        let err = one_pass_through(&closed).await.unwrap_err().to_string();
        assert!(err.starts_with("store error: "), "{err}");

        with_grants(
            &db,
            &[
                SCHEMA,
                "GRANT SELECT ON content_store TO {role}",
                "GRANT SELECT, UPDATE ON serving_pool TO {role}",
                "REVOKE EXECUTE ON FUNCTION pg_catalog.count() FROM PUBLIC",
            ],
            |_, handle| async move {
                let err = one_pass_through(&handle).await.unwrap_err().to_string();
                assert!(err.contains("permission denied"), "{err}");
            },
        )
        .await;
    })
    .await;
}
