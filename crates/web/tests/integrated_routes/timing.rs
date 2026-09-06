//! Integrated reasoning clocks are persisted and idempotent.
use super::*;

#[tokio::test]
async fn slow_integrated_reasoning_is_correct_and_replay_freezes_its_clock() {
    TestDb::with(|db| async move {
        let router = app(&db, IntegratedSet::from_items(vec![item()]));
        let user = learner_with_due_reviews(&db, "integrated-clock@example.com").await;
        let serve = format!("/api/task/{MULTISTEP}/integrated");
        let (status, body) = post(&router, user, &serve, None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let mut scratch = common::stored_state(&db, user).await;
        let clock = scratch.integrated_timing.values_mut().next().unwrap();
        clock.started_at = Timestamp::from_micros((common::now_secs() * 1_000_000.0) as i64 - 360_000_000);
        common::put_state(&db, user, &scratch).await;
        let uri = format!("/api/task/{MULTISTEP}/integrated/answer");
        let submitted = json!({"method":"person-minutes", "steps":[{"id":"person-minutes","answer":"960"},{"id":"clerk-minutes","answer":"240"}], "final_answer":{"id":"final","answer":"4"}});
        let (status, body) = post(&router, user, &uri, Some(submitted.clone())).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let result = parse(&body);
        assert_eq!(result["solved"], true);
        assert_eq!(result["timing_reliable"], true);
        assert_eq!(result["timing"]["claim"], "slow_reasoning");
        let (_, replay) = post(&router, user, &uri, Some(submitted)).await;
        assert_eq!(parse(&replay)["timing"], result["timing"]);
        let rows = events_of(&db, user).await;
        let record = &rows.iter().find(|(kind,_)| kind == "integrated_attempt").unwrap().1;
        assert_eq!(record["timing"], result["timing"]);
        assert_eq!(record["timing_reliable"], true);
        cadus_core::event::Event::from_json(&record.to_string()).unwrap();
        assert!(common::stored_state(&db, user).await.tasks[MULTISTEP].done);
    }).await;
}

#[tokio::test]
async fn integrated_reload_excludes_speed_without_changing_correctness() {
    TestDb::with(|db| async move {
        let router = app(&db, IntegratedSet::from_items(vec![item()]));
        let user = learner_with_due_reviews(&db, "integrated-interrupted@example.com").await;
        let serve = format!("/api/task/{MULTISTEP}/integrated");
        for _ in 0..2 {
            assert_eq!(post(&router, user, &serve, None).await.0, StatusCode::OK);
        }
        let uri = format!("/api/task/{MULTISTEP}/integrated/answer");
        let (status, body) = post(
            &router,
            user,
            &uri,
            Some(json!({"final_answer":{"id":"final","answer":"4"}})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(parse(&body)["solved"], true);
        assert_eq!(parse(&body)["timing_reliable"], false);
        assert_eq!(parse(&body)["timing"]["claim"], "not_judged");
    })
    .await;
}
