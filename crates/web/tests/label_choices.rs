//! The `choices` key of a Label item, from the serve route to the grade route.
//!
//! A Label exemplar serves with `choices`: the first alias of each option, in
//! an order that comes from `problem_id` only. The learner submits the text of
//! one choice with no change, and the grade path of today decides the verdict.
//! Hard Rule 1 holds: the payload shows the options, never the key.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_core::answer::AnswerContract::Label;
use cadus_core::curriculum::{Curriculum, Exemplar};
use common::prelude::*;
use common::{answer_task, app_with_content, seed_learner, seed_open_session, serve_ok};
use serde_json::{Value, json};

const PROOFS: &str = "s_2026-01-01a-lesson-proofs";
const KEY: &str = "Step 3";

/// A curriculum whose `proofs` topic authors one Label exemplar of four steps.
fn proofs() -> Curriculum {
    let options = ["Step 1", "Step 2", KEY, "Step 4"]
        .iter()
        .map(|text| vec![(*text).to_string()])
        .collect();
    let exemplar = Exemplar {
        answer_contract: Some(Label { options }),
        problem: "Which step is the first step that is not valid?".to_string(),
        answer: KEY.to_string(),
        solution_sketch: Some("Step 3 divides by zero.".to_string()),
    };
    common::one_unit_curriculum(vec![common::topic(
        "proofs",
        vec![common::kp("kp1", vec![exemplar])],
    )])
}

/// Serve the Label problem to a new learner, then submit the choice that
/// `pick` selects. The result is the outcome of the grade reply.
async fn outcome_of(db: &TestDb, email: &str, pick: fn(&[Value]) -> &Value) -> String {
    let router = app_with_content(db, proofs());
    let user = seed_learner(db, email).await;
    seed_open_session(db, user).await;

    let served = serve_ok(&router, user, PROOFS).await;
    let choices = served["choices"].as_array().unwrap();
    let mut texts: Vec<&str> = choices.iter().map(|text| text.as_str().unwrap()).collect();
    texts.sort_unstable();
    assert_eq!(texts, ["Step 1", "Step 2", "Step 3", "Step 4"], "{served}");
    // A second serve of the live problem gives the same order.
    let again = serve_ok(&router, user, PROOFS).await;
    assert_eq!(again["problem_id"], served["problem_id"]);
    assert_eq!(again["choices"], served["choices"]);

    let (status, body) = answer_task(
        &router,
        user,
        PROOFS,
        json!({"problem_id": served["problem_id"], "answer": pick(choices)}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["outcome"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn the_choice_that_is_the_key_grades_correct() {
    TestDb::with(|db| async move {
        let outcome = outcome_of(&db, "label-key@example.com", |choices| {
            choices.iter().find(|text| *text == KEY).unwrap()
        })
        .await;
        assert_eq!(outcome, "correct");
    })
    .await;
}

#[tokio::test]
async fn a_different_choice_grades_incorrect() {
    TestDb::with(|db| async move {
        let outcome = outcome_of(&db, "label-other@example.com", |choices| {
            choices.iter().find(|text| *text != KEY).unwrap()
        })
        .await;
        assert_eq!(outcome, "incorrect");
    })
    .await;
}
