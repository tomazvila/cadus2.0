//! FIX-M5-B acceptance, findings F10 and F16: the serving key of a component
//! review question.
//!
//! The header of `serve_solution.rs` gives the requirements and the rules.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_store::test_support::TestDb;
use common::{
    COMPONENT_ANSWER, COMPONENT_KEY, COMPONENT_TEXT, PARENT_KEY, PROBLEM_TEXT, REVIEW,
    answer_task_ok, component_app as app, events_of_type, hint_ok, learner_at_review_index,
    learner_at_the_component_question, problem_id_of, seed_content, serve_ok,
};
use serde_json::json;

/// The review draws serve index 1 from the COMPONENT skill, so the hint comes
/// from the component's authored ladder. The parent topic's ladder is approved
/// too, and it must not be read (L5, D-O3).
#[tokio::test]
async fn a_component_review_question_reads_the_components_hint_ladder() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_at_the_component_question(&db, "ladder-component@example.com").await;
        seed_content(
            &db,
            COMPONENT_KEY,
            "hint_ladder",
            "digest-component-hints",
            json!({"hints": ["Count the steps one at a time."]}),
        )
        .await;
        seed_content(
            &db,
            PARENT_KEY,
            "hint_ladder",
            "digest-parent-hints",
            json!({"hints": ["Line the decimal points up."]}),
        )
        .await;

        // The statement comes from the component, and the attempt still records
        // against the parent topic.
        let served = serve_ok(&app, user, REVIEW).await;
        assert_eq!(served["text"], COMPONENT_TEXT);
        assert_eq!(served["index"], 2);
        let problem_id = problem_id_of(&served);

        let hint = hint_ok(&app, user, REVIEW, &problem_id).await;
        assert_eq!(
            hint["hint"], "Count the steps one at a time.",
            "the hint came from the wrong knowledge point: {hint}"
        );
        assert_eq!(hint["hint_number"], 1);
    })
    .await;
}

/// The same key rule on the A4 pre-authored path. The distractor is authored on
/// the COMPONENT, so a matching wrong answer is `ready` inline and writes no job
/// row (spec section 6.2). Before the fix the lookup read the parent topic, found
/// no distractor, and enqueued a model call.
#[tokio::test]
async fn a_component_review_question_reads_the_components_diagnosis() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_at_the_component_question(&db, "diagnosis-component@example.com").await;
        seed_content(
            &db,
            COMPONENT_KEY,
            "diagnosis",
            "digest-component-diagnosis",
            json!({
                "v": 1,
                "distractors": [
                    {"answer": "8",
                     "error_tag": "arithmetic-slip",
                     "note": "You counted the starting number as one step."}
                ]
            }),
        )
        .await;

        let served = serve_ok(&app, user, REVIEW).await;
        assert_eq!(served["text"], COMPONENT_TEXT);
        let problem_id = problem_id_of(&served);

        let graded = answer_task_ok(
            &app,
            user,
            REVIEW,
            json!({"problem_id": problem_id, "answer": "8"}),
        )
        .await;
        assert_eq!(graded["correct"], false);
        assert_eq!(
            graded["diagnosis"],
            json!({
                "status": "ready",
                "error_tags": ["arithmetic-slip"],
                "prose": "You counted the starting number as one step.",
            }),
            "the pre-authored lookup read the wrong knowledge point: {graded}"
        );
        let jobs = sqlx::query_scalar!(
            r#"SELECT count(*) AS "n!" FROM diagnosis_jobs WHERE user_id = $1"#,
            user
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(jobs, 0, "a pre-authored hit must write no job row");
    })
    .await;
}

/// The attempt of a component question records against the PARENT topic, and the
/// D-S6 row keeps both topics: the review's FIRe applies to `addition`, and the
/// component gets credit by encompassing propagation (`api.py:352-357`).
#[tokio::test]
async fn a_component_review_question_records_against_the_parent_topic() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_at_the_component_question(&db, "record-parent@example.com").await;

        let served = serve_ok(&app, user, REVIEW).await;
        let problem_id = problem_id_of(&served);
        answer_task_ok(
            &app,
            user,
            REVIEW,
            json!({"problem_id": problem_id, "answer": COMPONENT_ANSWER}),
        )
        .await;

        let attempts = events_of_type(&db, user, "attempt").await;
        let payload = &attempts[0];
        assert_eq!(payload["topic"], "addition");
        assert_eq!(payload["kp"], "kp1");
        assert_eq!(payload["correct"], true);
    })
    .await;
}

/// The first review question is the parent's own: `review_mix("addition")`
/// opens with `kp1`, so serve index 0 draws from `addition` and both topics of
/// the D-S6 row name the parent.
#[tokio::test]
async fn the_first_review_question_draws_from_the_parent_topic() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_at_review_index(&db, "parent-first@example.com", 0).await;

        let served = serve_ok(&app, user, REVIEW).await;
        assert_eq!(served["text"], PROBLEM_TEXT);
        assert_eq!(served["index"], 1);
        assert_eq!(served["kp"], "kp1");

        let stored = common::stored_state(&db, user).await;
        assert_eq!(stored.served[REVIEW].topic.as_deref(), Some("addition"));
        assert_eq!(
            stored.served[REVIEW].serve_topic.as_deref(),
            Some("addition")
        );
    })
    .await;
}
