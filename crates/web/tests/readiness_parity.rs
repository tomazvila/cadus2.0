//! H-4 (ISSUES.md): `cadus-worker readiness` and the web operator surface are
//! ONE audit, read over the SAME fixture.
//!
//! The grind's finding was an operator report that said every knowledge point
//! was blocked while the selector served lessons for the same knowledge points.
//! Both halves of this test build their readiness from one fixture — one
//! curriculum, one `content_store` — so any read either side does differently
//! fails here instead of on the operator's screen.
//!
//! The worker half is `cadus_worker::readiness_run`, the exact function the
//! `readiness` subcommand prints. The web half is `GET /api/operator/flags`,
//! the exact route the operator screen renders. Both go through
//! `approved_index_current` (D-F5), so the store half is shared; the test pins
//! that the digests each side computes from the loaded curriculum agree, which
//! is the edge a stale worker image (ISSUE-8) broke.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_core::curriculum::{Curriculum, review_context_digest};
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::state::Content;
use common::admin::fixture_curriculum_digest;
use common::{kp, one_unit_curriculum, seed_content_for, topic};
use serde_json::{Value, json};

/// The readiness of the operator surface, read as the admin.
async fn operator_readiness(
    router: &axum::Router,
) -> Vec<(String, usize, usize, Value)> {
    let answer = common::admin::admin_get(router, "/api/operator/flags").await;
    assert_eq!(answer.status.as_u16(), 200, "{}", answer.body);
    let courses = answer.body["readiness"]["courses"]
        .as_array()
        .expect("the operator answer carries the readiness courses")
        .clone();
    courses
        .iter()
        .map(|course| {
            (
                course["course_id"].as_str().expect("course_id").to_string(),
                course["ready"].as_u64().expect("ready") as usize,
                course["blocked"].as_u64().expect("blocked") as usize,
                course["blockers"].clone(),
            )
        })
        .collect()
}

/// The readiness of the worker audit, by course.
fn worker_readiness(run: &cadus_worker::ReadinessRun) -> Vec<(String, usize, usize, Value)> {
    run.report
        .courses
        .iter()
        .map(|course| {
            (
                course.course_id.clone(),
                course.ready,
                course.blocked,
                json!(course
                    .blockers
                    .iter()
                    .map(|(blocker, count)| (blocker.as_str().to_owned(), json!(count)))
                    .collect::<serde_json::Map<String, Value>>()),
            )
        })
        .collect()
}

/// Four decidable exemplars `Compute n + n.`, so the practice and assessment
/// conditions hold and the store decides `teachable` and `hints` alone.
fn four_exemplars(first: i64) -> Vec<cadus_core::curriculum::Exemplar> {
    (first..first + 4)
        .map(|n| common::exemplar(&format!("Compute {n} + {n}."), &(n * 2).to_string()))
        .collect()
}

fn parity_curriculum() -> Curriculum {
    one_unit_curriculum(vec![topic(
        "addition",
        vec![kp("kp1", four_exemplars(1))],
    )])
}

/// Assert the two audits name the SAME courses with the SAME counts.
fn assert_parity(
    worker: &[(String, usize, usize, Value)],
    operator: &[(String, usize, usize, Value)],
) {
    assert_eq!(worker.len(), operator.len(), "course sets diverge");
    for ((w_course, w_ready, w_blocked, w_blockers), (o_course, o_ready, o_blocked, o_blockers)) in
        worker.iter().zip(operator.iter())
    {
        assert_eq!(w_course, o_course, "the course lists diverge");
        assert_eq!(
            (w_ready, w_blocked),
            (o_ready, o_blocked),
            "course {w_course}: the worker report and the operator surface disagree"
        );
        assert_eq!(
            w_blockers, o_blockers,
            "course {w_course}: the blocker histograms disagree"
        );
    }
}

#[tokio::test]
async fn the_worker_report_and_the_operator_surface_agree_on_one_fixture() {
    TestDb::with(|db| async move {
        let curriculum = parity_curriculum();
        let router = common::app_with_content(&db, curriculum.clone());
        common::admin::seed_admin(&db).await;

        // An empty store: every knowledge point is blocked on the teach page
        // (audit findings h and j), and BOTH surfaces say so.
        let worker = worker_readiness(
            &cadus_worker::readiness_run(
                &Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS),
                &curriculum,
                None,
            )
            .await
            .unwrap(),
        );
        let operator = operator_readiness(&router).await;
        assert_parity(&worker, &operator);
        assert!(
            worker.iter().all(|(_, _, blocked, _)| *blocked > 0),
            "an empty store must block every knowledge point: {worker:?}"
        );

        // One approved teach page clears `teachable` on BOTH surfaces.
        seed_content_for(
            &db,
            &curriculum,
            "addition/kp1",
            "teach",
            "digest-parity-teach",
            json!({
                "concept": "Addition combines two counts.",
                "worked_example": {"problem": "Compute 2 + 3.", "steps": ["2 + 3 = 5."]}
            }),
        )
        .await;
        let worker = worker_readiness(
            &cadus_worker::readiness_run(
                &Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS),
                &curriculum,
                None,
            )
            .await
            .unwrap(),
        );
        let operator = operator_readiness(&router).await;
        assert_parity(&worker, &operator);

        // One approved hint ladder clears `hints` on BOTH surfaces, and the
        // knowledge point is READY on both: the SPA's hint affordance gate
        // (H-3) reads the same store the worker report prints.
        seed_content_for(
            &db,
            &curriculum,
            "addition/kp1",
            "hint_ladder",
            "digest-parity-hints",
            json!({"hints": ["Add the ones first."]}),
        )
        .await;
        let worker = worker_readiness(
            &cadus_worker::readiness_run(
                &Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS),
                &curriculum,
                None,
            )
            .await
            .unwrap(),
        );
        let operator = operator_readiness(&router).await;
        assert_parity(&worker, &operator);
        assert!(
            worker.iter().all(|(_, ready, _, _)| *ready > 0),
            "the taught knowledge point must be ready on both sides: {worker:?}"
        );
    })
    .await;
}

/// The digest the worker computes from a loaded curriculum is the digest the
/// web boot computes from the SAME curriculum. This is the edge the stale
/// worker image broke: the approval stamps bind to it, so a divergence reads as
/// "every knowledge point blocked".
#[test]
fn the_two_context_digests_are_one_digest() {
    let curriculum = parity_curriculum();
    let worker_digest = review_context_digest(&curriculum).unwrap();
    let web_digest = Content::new(curriculum)
        .curriculum_context_digest()
        .unwrap()
        .to_owned();
    assert_eq!(worker_digest, web_digest);
    // And the digest binds to the curriculum, not to the loader: the admin
    // fixture computes its own, which the rows it seeds carry.
    assert_ne!(
        worker_digest,
        fixture_curriculum_digest(),
        "two different curricula must not share one context digest"
    );
    }
