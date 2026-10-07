//! A session never dead-ends on a knowledge point whose approved content is
//! gone: the authored exemplars serve, and a topic with nothing at all gives
//! the learner a sentence and no topic id.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod common;
use cadus_core::curriculum::{
    Curriculum, Exemplar, FiniteCaseRole, FiniteCaseVariant, FiniteObjectiveCase,
    FiniteObjectiveDomain, Slug,
};
use common::*;

fn policy() -> FiniteObjectiveDomain {
    FiniteObjectiveDomain {
        schema_version: 1,
        review_ref: "sha256:test-reviewed-two-cases".to_owned(),
        cases: (1..=2)
            .map(|n| FiniteObjectiveCase {
                id: Slug::new(format!("case-{n}")).unwrap(),
                role: FiniteCaseRole::PracticeFresh,
                variants: vec![FiniteCaseVariant {
                    problem: format!("Return the index ${n}$."),
                    answer: n.to_string(),
                    answer_contract: None,
                }],
            })
            .collect(),
    }
}

fn exemplar(n: u8) -> Exemplar {
    Exemplar {
        answer_contract: None,
        problem: format!("Return the index ${n}$."),
        answer: n.to_string(),
        solution_sketch: Some("Read the index.".to_owned()),
        visual: None,
    }
}

/// A finite point with authored exemplars and no approved template anywhere.
fn finite_without_template() -> Curriculum {
    let mut point = kp("kp1", vec![exemplar(1), exemplar(2)]);
    point.finite_objective_domain = Some(policy());
    one_unit_curriculum(vec![topic("addition", vec![point])])
}

/// The owner report of 2026-10-07: the next problem of a finite point whose
/// approved template no longer matches failed with `pool_unavailable` and the
/// topic id. The authored exemplars are cases of the policy, so they serve.
#[tokio::test]
async fn a_finite_point_with_no_current_template_serves_its_exemplars() {
    TestDb::with(|db| async move {
        let app = app_with_content(&db, finite_without_template());
        let user = seed_learner(&db, "empty-finite@example.test").await;
        seed_open_session(&db, user).await;

        let (status, reply) = serve_task(&app, user, LESSON).await;
        assert_eq!(status, StatusCode::OK, "{reply}");
        assert!(
            reply["text"]
                .as_str()
                .unwrap()
                .starts_with("Return the index"),
            "{reply}"
        );
        let scratch = stored_state(&db, user).await;
        let handoff = scratch.served[LESSON].handoff.as_ref().unwrap();
        assert_eq!(handoff.item_source, cadus_core::event::ItemSource::Exemplar);
        assert!(handoff.finite_case_id.is_some());
    })
    .await;
}

/// A topic that can serve nothing refuses with a sentence for the learner. The
/// sentence names no topic id, and a second try answers the same way.
#[tokio::test]
async fn a_topic_with_nothing_to_serve_refuses_without_a_topic_id() {
    TestDb::with(|db| async move {
        let curriculum = one_unit_curriculum(vec![topic("addition", vec![kp("kp1", Vec::new())])]);
        let app = app_with_content(&db, curriculum);
        let user = seed_learner(&db, "empty-topic@example.test").await;
        seed_open_session(&db, user).await;

        for _ in 0..2 {
            let (status, reply) = serve_task(&app, user, LESSON).await;
            assert_eq!(status, StatusCode::CONFLICT, "{reply}");
            assert_eq!(reply["error"]["code"], "pool_unavailable");
            let message = reply["error"]["message"].as_str().unwrap();
            assert!(!message.contains("addition"), "{message}");
            assert!(message.contains("Go back to the lesson"), "{message}");
        }
    })
    .await;
}
