//! Server reveal persistence, replay and forged assistance counters.
use super::*;

#[tokio::test]
async fn committed_reveals_survive_refresh_and_cannot_be_erased_or_fabricated() {
    TestDb::with(|db| async move {
        let router = app(&db, IntegratedSet::from_items(vec![item()]));
        let user = learner_with_due_reviews(&db, "hint-authority@example.com").await;
        let hint_uri = format!("/api/task/{MULTISTEP}/integrated/hint");
        for _ in 0..2 {
            let (status, body) = post(&router, user, &hint_uri, Some(json!({"field":"person-minutes","index":0}))).await;
            assert_eq!(status, StatusCode::OK, "{body}");
            assert_eq!(parse(&body)["hints_used"], 1);
        }
        let (status, body) = post(&router, user, &hint_uri, Some(json!({"field":"person-minutes","index":999}))).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(parse(&body)["hints_used"], 1);
        assert_eq!(parse(&body)["hint"], Value::Null);
        let rows = events_of(&db, user).await;
        assert_eq!(rows.iter().filter(|(kind, _)| kind == "integrated_hint_revealed").count(), 1);
        for (_, payload) in &rows {
            cadus_core::event::Event::from_json(&payload.to_string()).unwrap();
        }
        // A fresh router has no client or in-process hint state.
        let refreshed = app(&db, IntegratedSet::from_items(vec![item()]));
        let (status, body) = post(&refreshed, user, &format!("/api/task/{MULTISTEP}/integrated"), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(parse(&body)["hints_used"]["person-minutes"], 1);
        // Refresh also traverses the normal projector with the new event in its log.
        let (status, body) = post(&refreshed, user, &format!("/api/task/{MULTISTEP}/integrated/answer"), Some(json!({
            "steps": [{"id":"person-minutes","answer":"960","hints_used":0}, {"id":"clerk-minutes","answer":"240","hints_used":999}],
            "final_answer":{"id":"person-minutes","answer":"4","hints_used":999}
        }))).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let result = parse(&body);
        assert_eq!(result["steps"][0]["assisted"], true);
        assert_eq!(result["steps"][1]["assisted"], false);
        assert_eq!(result["final"]["assisted"], false);
        assert_eq!(result["assisted"], true);
        let plan = plan_body(&refreshed, user).await;
        let tasks = plan["tasks"].as_array().unwrap();
        let completed = tasks.iter().find(|task| task["task_id"] == MULTISTEP).unwrap();
        assert_eq!(completed["progress"]["done"], true);
        assert_eq!(completed["progress"]["answered"], 1);
        assert!(tasks.iter().find(|task| task["progress"]["done"] != true).is_none_or(|task| task["task_id"] != MULTISTEP));

        let rows = events_of(&db, user).await;
        let attempt = &rows.iter().find(|(kind,_)| kind == "integrated_attempt").unwrap().1;
        assert_eq!(attempt["steps"][0]["assisted"], true);
        assert_eq!(attempt["steps"][1]["assisted"], false);
        assert_eq!(attempt["final_field"]["assisted"], false);
        let mut legacy = attempt.clone();
        legacy.as_object_mut().unwrap().remove("grade");
        let old = cadus_core::event::Event::from_json(&legacy.to_string()).unwrap();
        assert!(matches!(old, cadus_core::event::Event::IntegratedAttempt(record) if record.grade.is_none()));
        let mut tx = db.admin.begin().await.unwrap();
        for (session, task, digest) in [
            ("another-session", MULTISTEP, item().digest()),
            ("s_2026-01-01a", "another-task", item().digest()),
            ("s_2026-01-01a", MULTISTEP, "edited-digest".to_owned())
        ] {
            assert!(cadus_store::integrated::hints_used(&mut tx, user, session, task, &digest).await.unwrap().is_empty());
        }
        tx.rollback().await.unwrap();

        // New hints and altered answers after grading cannot change the committed verdict.
        let (status, body) = post(&refreshed, user, &hint_uri, Some(json!({"field":"final","index":0}))).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let (status, body) = post(&refreshed, user, &format!("/api/task/{MULTISTEP}/integrated/answer"), Some(json!({"final_answer":{"id":"final","answer":"999"}}))).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let mut replay = parse(&body);
        assert_eq!(replay["recorded"], false);
        replay["recorded"] = json!(true);
        assert_eq!(replay, result);
        // Simulate an event produced by the previous schema without a frozen view.
        sqlx::query("UPDATE events SET payload = payload - 'grade' WHERE user_id = $1 AND type = 'integrated_attempt'").bind(user).execute(&db.admin).await.unwrap();
        let (status, body) = post(&refreshed, user, &format!("/api/task/{MULTISTEP}/integrated/answer"), Some(json!({"final_answer":{"id":"final","answer":"999"}}))).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let legacy_reply = parse(&body);
        assert_eq!(legacy_reply["recorded"], false);
        assert_eq!(legacy_reply["solved"], true);
        assert_eq!(legacy_reply["steps"][0]["assisted"], true);
        assert_eq!(legacy_reply["final"]["assisted"], false);
        assert_eq!(events_of(&db, user).await.iter().filter(|(kind,_)| kind == "integrated_attempt").count(), 1);


    }).await;
}

#[tokio::test]
async fn an_omitted_answer_keeps_its_recorded_hint_assistance() {
    TestDb::with(|db| async move {
        let router = app(&db, IntegratedSet::from_items(vec![item()]));
        let user = learner_with_due_reviews(&db, "hint-omitted@example.com").await;
        let (status, body) = post(
            &router,
            user,
            &format!("/api/task/{MULTISTEP}/integrated/hint"),
            Some(json!({"field":"person-minutes"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let (status, body) = post(
            &router,
            user,
            &format!("/api/task/{MULTISTEP}/integrated/answer"),
            Some(json!({"final_answer":{"id":"final","answer":"4"}})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let result = parse(&body);
        assert_eq!(result["steps"][0]["answered"], false);
        assert_eq!(result["steps"][0]["assisted"], true);
        assert_eq!(result["assisted"], true);
    })
    .await;
}

#[tokio::test]
async fn the_log_replays_after_an_integrated_attempt() {
    TestDb::with(|db| async move {
        let app = app(&db, IntegratedSet::from_items(vec![item()]));
        let user = learner_with_due_reviews(&db, "integrated-replay@example.com").await;
        post(
            &app,
            user,
            &format!("/api/task/{MULTISTEP}/integrated"),
            None,
        )
        .await;
        let (status, _) = post(
            &app,
            user,
            &format!("/api/task/{MULTISTEP}/integrated/answer"),
            Some(json!({"steps": [], "final_answer": {"id": "final", "answer": "4"}})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        // The fold reads the two new rows and the plan still composes: an event
        // kind the projector ignores must never stop a later request.
        let task_id = multistep_task_id(&app, user).await;
        assert_eq!(task_id, MULTISTEP);
        let (status, body) = post(
            &app,
            user,
            &format!("/api/task/{MULTISTEP}/integrated"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
    })
    .await;
}

#[tokio::test]
async fn another_learner_writes_no_row_of_this_learner() {
    TestDb::with(|db| async move {
        let app = app(&db, IntegratedSet::from_items(vec![item()]));
        let owner = learner_with_due_reviews(&db, "integrated-own@example.com").await;
        let other = seed_learner(&db, "integrated-stranger@example.com").await;
        seed_open_session(&db, other).await;
        let uri = format!("/api/task/{MULTISTEP}/integrated/answer");
        let body = json!({"steps": [], "final_answer": {"id": "final", "answer": "4"}});

        let (status, _) = post(&app, other, &uri, Some(body.clone())).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(
            events_of(&db, owner)
                .await
                .iter()
                .all(|(kind, _)| kind != "integrated_attempt"),
            "a stranger wrote a row of the owner"
        );
        let (status, _) = post(&app, owner, &uri, Some(body)).await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            events_of(&db, other)
                .await
                .iter()
                .all(|(kind, _)| kind != "integrated_attempt"),
            "the owner wrote a row of the stranger"
        );
    })
    .await;
}
