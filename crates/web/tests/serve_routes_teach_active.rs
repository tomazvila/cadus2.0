//! Step 5a, active worked examples: the teach payload withholds every graded
//! field, and `POST /api/task/{task_id}/teach/check` grades the act and writes
//! nothing.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axum::http::{Method, StatusCode};
use cadus_core::answer::AnswerContract;
use cadus_core::curriculum::{Curriculum, StepCheck, StepRef, TryFirst};
use cadus_store::test_support::TestDb;
use common::{
    DRILL_TEXT, EXEMPLAR_TEXT_2, EXPECTED_ANSWER, KEY, LESSON, PROBLEM_TEXT, app_with_content,
    assert_refused, call, drill_topic, exemplar, kp, one_unit_curriculum, parse, seed_content_for,
    seed_learner, seed_open_session, teach_task, topic,
};
use serde_json::{Value, json};
use sqlx::types::Uuid;

const WHY: &str = "Line the points up first, so tenths meet tenths.";
const REVEAL: &str = "Writing 6 as 6.00 is what made the columns meet.";
const RIGHT: &str = "Both numbers then carry two decimal places";

/// The drill fixture, with both active-example blocks on `addition/kp1` when
/// `with_blocks` holds.
fn active_curriculum(with_blocks: bool) -> Curriculum {
    let mut first = kp(
        "kp1",
        vec![
            exemplar(PROBLEM_TEXT, EXPECTED_ANSWER),
            exemplar(EXEMPLAR_TEXT_2, "13.25"),
        ],
    );
    if with_blocks {
        first.step_check = Some(StepCheck {
            step: StepRef::Text("Write 6 as 6.00".to_owned()),
            question: "Why is 6 written as 6.00 in step 1?".to_owned(),
            options: vec![
                RIGHT.to_owned(),
                "It makes 6 bigger".to_owned(),
                "Decimals must have three digits".to_owned(),
            ],
            answer: RIGHT.to_owned(),
            why: WHY.to_owned(),
        });
        first.try_first = Some(TryFirst {
            problem: "Compute 3 + 1.75.".to_owned(),
            answer_contract: AnswerContract::Exact,
            answer: "4.75".to_owned(),
            reveal: REVEAL.to_owned(),
        });
    }
    one_unit_curriculum(vec![
        topic(
            "addition",
            vec![
                first,
                kp("kp2", vec![exemplar("Compute 40 + 2.5.", "42.5")]),
            ],
        ),
        topic("subtraction", vec![kp("kp1", vec![])]),
        drill_topic(
            "tables",
            vec![kp(
                "kp1",
                vec![
                    exemplar(DRILL_TEXT, "56.5"),
                    exemplar("Compute 6 x 7 + 0.25.", "42.25"),
                ],
            )],
        ),
    ])
}

/// The router over [`active_curriculum`], with the teach page of `KEY`
/// approved against that exact curriculum.
async fn active_app(db: &TestDb, with_blocks: bool) -> axum::Router {
    seed_content_for(
        db,
        &active_curriculum(with_blocks),
        KEY,
        "teach",
        "digest-teach-active",
        json!({
            "concept": "To add a whole number and a decimal, line up the decimal points.",
            "worked_example": {
                "problem": "Compute 6 + 2.25.",
                "steps": [
                    "Write 6 as 6.00, so both numbers carry two decimal places.",
                    "Add column by column: $6.00 + 2.25 = 8.25$."
                ]
            }
        }),
    )
    .await;
    app_with_content(db, active_curriculum(with_blocks))
}

async fn check(app: &axum::Router, user: Uuid, body: Value) -> (StatusCode, Value) {
    let (status, raw) = call(
        app,
        Method::POST,
        &format!("/api/task/{LESSON}/teach/check"),
        Some(user),
        Some(body),
    )
    .await;
    (status, parse(&raw))
}

/// Rows of the learner's log and D-S6 state, which the check must not move.
async fn footprint(db: &TestDb, user: Uuid) -> (i64, i64) {
    let events = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM events WHERE user_id = $1")
        .bind(user)
        .fetch_one(&db.admin)
        .await
        .unwrap();
    let states = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM web_states WHERE user_id = $1")
        .bind(user)
        .fetch_one(&db.admin)
        .await
        .unwrap();
    (events, states)
}

/// Hard Rule 1: the payload names the question, the options, the step and the
/// try-first problem, and none of `answer`, `why`, `reveal`.
#[tokio::test]
async fn the_teach_payload_carries_no_answer_before_the_act() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "activeteach@example.com").await;
        let app = active_app(&db, true).await;
        seed_open_session(&db, user).await;

        let (status, page) = teach_task(&app, user, LESSON).await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert_eq!(page["step_check"]["step"], 1);
        assert_eq!(
            page["step_check"]["question"],
            "Why is 6 written as 6.00 in step 1?"
        );
        let mut options: Vec<String> = page["step_check"]["options"]
            .as_array()
            .unwrap()
            .iter()
            .map(|option| option.as_str().unwrap().to_owned())
            .collect();
        options.sort();
        assert_eq!(
            options,
            [
                RIGHT,
                "Decimals must have three digits",
                "It makes 6 bigger"
            ]
        );
        assert_eq!(page["try_first"], json!({"problem": "Compute 3 + 1.75."}));
        let text = page.to_string();
        for secret in [WHY, REVEAL, "4.75", "\"answer\"", "\"why\"", "\"reveal\""] {
            assert!(!text.contains(secret), "the payload leaks {secret}: {text}");
        }
    })
    .await;
}

/// Without the blocks the payload is the plain page, and the check refuses.
#[tokio::test]
async fn a_point_without_blocks_serves_the_plain_page() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "plainteach@example.com").await;
        let app = active_app(&db, false).await;
        seed_open_session(&db, user).await;

        let (status, page) = teach_task(&app, user, LESSON).await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert!(page.get("step_check").is_none(), "{page}");
        assert!(page.get("try_first").is_none(), "{page}");

        assert_refused(
            &check(&app, user, json!({"part": "step_check", "choice": RIGHT})).await,
            StatusCode::CONFLICT,
            "no_active_example",
        );
    })
    .await;
}

/// The check grades the pick and the attempt, returns the withheld material,
/// and leaves the log and the state row exactly as they were: no event, so
/// the fold, the lesson pass rule and the schedule never see the act.
#[tokio::test]
async fn the_check_grades_and_writes_nothing() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "activecheck@example.com").await;
        let app = active_app(&db, true).await;
        seed_open_session(&db, user).await;
        let before = footprint(&db, user).await;

        let (status, wrong) = check(
            &app,
            user,
            json!({"part": "step_check", "choice": "It makes 6 bigger"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{wrong}");
        assert_eq!(
            wrong,
            json!({"part": "step_check", "correct": false, "answer": RIGHT, "why": WHY})
        );
        let (_, right) = check(&app, user, json!({"part": "step_check", "choice": RIGHT})).await;
        assert_eq!(right["correct"], true);

        let (status, miss) = check(&app, user, json!({"part": "try_first", "answer": "4.5"})).await;
        assert_eq!(status, StatusCode::OK, "{miss}");
        assert_eq!(
            miss,
            json!({
                "part": "try_first", "outcome": "incorrect", "correct": false,
                "answer": "4.75", "reveal": REVEAL
            })
        );
        let (_, hit) = check(&app, user, json!({"part": "try_first", "answer": "4.75"})).await;
        assert_eq!(hit["outcome"], "correct");

        assert_eq!(footprint(&db, user).await, before, "the check wrote a row");
    })
    .await;
}

/// A pick outside the options and a malformed body are refused by name.
#[tokio::test]
async fn a_bad_act_is_refused() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "activebad@example.com").await;
        let app = active_app(&db, true).await;
        seed_open_session(&db, user).await;

        for body in [
            json!({"part": "step_check", "choice": "a made-up option"}),
            json!({"part": "step_check"}),
            json!({"part": "guess", "answer": "1"}),
        ] {
            assert_refused(
                &check(&app, user, body).await,
                StatusCode::UNPROCESSABLE_ENTITY,
                "invalid_request",
            );
        }
    })
    .await;
}
