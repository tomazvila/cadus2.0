//! Real grade policy, preserved wire evidence, and speed-independent progression.
#![allow(clippy::unwrap_used)]
mod common;
use axum::http::StatusCode;
use cadus_core::event::Event;
use cadus_store::test_support::TestDb;
use common::{
    LESSON, PROBLEM_ID, answer_task, events_of_type, learner_with_kp1, lesson_app, now_secs,
    put_state, stored_state,
};
use serde_json::json;

#[tokio::test]
async fn slow_correct_reasoning_advances_and_unreliable_clocks_are_excluded() {
    TestDb::with(|db| async move {
        let app = lesson_app(&db);
        let user = learner_with_kp1(&db, "slow-reasoning@example.com", 90.0).await;
        for index in 0..2 {
            let mut scratch = stored_state(&db, user).await;
            let live = scratch.served.get_mut(LESSON).unwrap();
            live.started_at = now_secs() - 90.0;
            let id = live.problem_id.clone();
            put_state(&db, user, &scratch).await;
            let (status, reply) =
                answer_task(&app, user, LESSON, json!({"problem_id":id,"answer":"13.5"})).await;
            assert_eq!(status, StatusCode::OK, "{reply}");
            assert_eq!(reply["correct"], true);
            assert_eq!(reply["timing_reliable"], true);
            assert_eq!(reply["timing"]["claim"], "slow_reasoning");
            if index == 1 {
                assert_eq!(reply["task_status"], "kp_advance");
            }
        }
        for (email, age, interrupted) in [
            ("idle-timing@example.com", 1000.0, false),
            ("reload-timing@example.com", 3.0, true),
            ("zero-timing@example.com", -10.0, false),
        ] {
            let user = learner_with_kp1(&db, email, age).await;
            let mut scratch = stored_state(&db, user).await;
            scratch.served.get_mut(LESSON).unwrap().timing_interrupted = interrupted;
            put_state(&db, user, &scratch).await;
            let (status, reply) = answer_task(
                &app,
                user,
                LESSON,
                json!({"problem_id":PROBLEM_ID,"answer":"13.5","timing_reliable":true}),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{reply}");
            assert_eq!(reply["correct"], true);
            assert_eq!(reply["timing_reliable"], false);
            assert_eq!(reply["timing"]["claim"], "not_judged");
            assert!(reply["timing"]["ratio"].is_null());
            let recorded = events_of_type(&db, user, "attempt").await.remove(0);
            let event = Event::from_json(&recorded.to_string()).unwrap();
            let wire = serde_json::to_value(event).unwrap();
            assert_eq!(wire["timing"], reply["timing"]);
            assert_eq!(wire["timing_reliable"], false);
        }
    })
    .await;
}

#[tokio::test]
async fn routine_fluency_requires_a_reliable_unassisted_routine_item() {
    TestDb::with(|db| async move {
        let app = common::drill_app(&db);
        let user = common::seed_learner(&db, "routine-timing@example.com").await;
        common::seed_open_session(&db, user).await;
        common::seed_drill_due(&db, user).await;
        let (status, _) = common::serve_task(&app, user, common::DRILL).await;
        assert_eq!(status, StatusCode::OK);
        let mut scratch = stored_state(&db, user).await;
        let live = scratch.served.get_mut(common::DRILL).unwrap();
        live.started_at = now_secs() - 2.0;
        let id = live.problem_id.clone();
        let answer = live.expected.answer.clone();
        put_state(&db, user, &scratch).await;
        let (status, result) = answer_task(
            &app,
            user,
            common::DRILL,
            json!({"problem_id":id,"answer":answer}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{result}");
        assert_eq!(result["timing"]["claim"], "routine_fluency");
        assert_eq!(result["timing_reliable"], true);
        assert_eq!(
            events_of_type(&db, user, "attempt").await[0]["timing"],
            result["timing"]
        );
    })
    .await;
}
