//! Approved instruction, fresh delayed application, and durable crash/retry receipt.
use super::*;
use cadus_core::event::Event;

fn fresh() -> IntegratedItem {
    parse_item(
        &ITEM
            .replace("integrated-test-window", "integrated-fresh-window")
            .replace("48", "72")
            .replace("960", "1440")
            .replace("answer: \"4\"", "answer: \"6\"")
            .replace("4 clerks clear", "6 clerks clear"),
    )
    .unwrap()
}
fn journey_curriculum() -> Curriculum {
    one_unit_curriculum(
        COMPONENTS
            .iter()
            .map(|id| {
                topic(
                    id,
                    vec![kp(
                        "kp1",
                        (1..=4)
                            .map(|n| exemplar(&format!("Compute {n} + {n}."), &(n * 2).to_string()))
                            .collect(),
                    )],
                )
            })
            .collect(),
    )
}
fn journey_app(db: &TestDb) -> Router {
    let graph = journey_curriculum();
    create_app(
        AppState::new(Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS)).with_content(Arc::new(
            Content::new(graph).with_integrated(IntegratedSet::from_items(vec![item(), fresh()])),
        )),
    )
}
async fn teach_pages(db: &TestDb) {
    let curriculum = journey_curriculum();
    for id in COMPONENTS {
        common::seed_content_for(db,&curriculum,&format!("{id}/kp1"),"teach",&format!("teach-{id}"), json!({
            "concept":"Combine workload with capacity.","worked_example":{"problem":"Two visits need ten minutes each.","steps":["The total workload is twenty minutes."]}
        })).await;
    }
}
async fn lose_rebuildable_state(db: &TestDb, user: Uuid) {
    sqlx::query("DELETE FROM learner_models WHERE user_id = $1")
        .bind(user)
        .execute(&db.admin)
        .await
        .unwrap();
    sqlx::query("DELETE FROM web_states WHERE user_id = $1")
        .bind(user)
        .execute(&db.admin)
        .await
        .unwrap();
}
fn answer(final_answer: &str) -> Value {
    json!({"method":"person-minutes", "steps":[{"id":"person-minutes","answer":"960"},{"id":"clerk-minutes","answer":"240"}],"final_answer":{"id":"final","answer":final_answer}})
}

#[tokio::test]
async fn instruction_application_delayed_assessment_survives_crash_and_receipt_retry() {
    TestDb::with(|db| async move {
        teach_pages(&db).await;
        let router=journey_app(&db);
        let user=learner_with_due_reviews(&db,"whole-journey@example.com").await;
        let plan=plan_body(&router,user).await;
        assert!(plan["tasks"].as_array().unwrap().iter().any(|task| task["task_id"]==MULTISTEP),"{plan}");
        let serve=format!("/api/task/{MULTISTEP}/integrated");
        let (status,body)=post(&router,user,&serve,None).await;
        assert_eq!(status,StatusCode::CONFLICT,"{body}");
        assert_eq!(parse(&body)["error"]["code"],"instruction_required");
        let (status,page)=common::teach_task(&router,user,MULTISTEP).await;
        assert_eq!(status,StatusCode::OK,"{page}");
        assert_eq!(page["concept"],"Combine workload with capacity.");
        assert!(common::events_of_type(&db,user,"integrated_served").await.is_empty());
        let router=journey_app(&db); // New application instance reads persisted instruction state.
        assert_eq!(post(&router,user,&serve,None).await.0,StatusCode::OK);
        let (_,receipt)=post(&router,user,&format!("{serve}/answer"),Some(answer("4"))).await;
        assert_eq!(parse(&receipt)["solved"],true,"{receipt}");
        let source=common::events_of_type(&db,user,"integrated_attempt").await.remove(0);
        assert_eq!(source["instruction_kp"],"counting/kp1");
        assert!(!plan_body(&router,user).await["tasks"].as_array().unwrap().iter().any(|task| task["integrated_assessment"]==true));
        // Test-clock advance: backdate only this fixture's source application, then rebuild.
        let old=Timestamp::from_micros((common::now_secs()*1_000_000.0) as i64 - 8*86_400_000_000);
        sqlx::query("UPDATE events SET payload = jsonb_set(payload, '{ts}', $2) WHERE user_id = $1 AND type = 'integrated_attempt'")
            .bind(user).bind(serde_json::to_value(old).unwrap()).execute(&db.admin).await.unwrap();
        lose_rebuildable_state(&db,user).await;
        let plan=plan_body(&router,user).await;
        let delayed=plan["tasks"].as_array().unwrap().iter().find(|task| task["integrated_assessment"]==true).unwrap_or_else(||panic!("{plan}"));
        assert_eq!(delayed["integrated_instruction_required"],false);
        let id=delayed["task_id"].as_str().unwrap().to_owned();
        let component_route=format!("/api/task/{id}/serve");
        assert_eq!(post(&router,user,&component_route,None).await.0,StatusCode::CONFLICT);
        let route=format!("/api/task/{id}/integrated");
        let (status,body)=post(&router,user,&route,None).await;
        assert_eq!(status,StatusCode::OK,"{body}");
        assert_eq!(parse(&body)["item_id"],"integrated-fresh-window");
        lose_rebuildable_state(&db,user).await; // Crash after serve; event replay pins the item.
        let recovered=journey_app(&db);
        assert_eq!(post(&recovered,user,&route,None).await.0,StatusCode::OK);
        let mut response=answer("6");response["steps"][0]["answer"]=json!("1440");
        let (status,body)=post(&recovered,user,&format!("{route}/answer"),Some(response)).await;
        assert_eq!(status,StatusCode::OK,"{body}");let receipt=parse(&body);
        assert_eq!(receipt["solved"],true);
        lose_rebuildable_state(&db,user).await; // Crash after commit, before client receipt.
        let (status,retry)=post(&journey_app(&db),user,&format!("{route}/answer"),Some(answer("0"))).await;
        assert_eq!(status,StatusCode::OK,"{retry}");let mut retry=parse(&retry);
        assert_eq!(retry["recorded"],false);retry["recorded"]=json!(true);assert_eq!(retry,receipt);
        let rows=common::events_of_type(&db,user,"integrated_attempt").await;
        assert_eq!(rows.len(),2);
        assert_eq!(rows[1]["assessment_of"],"integrated-test-window");
        assert_eq!(rows[1]["assessment_delay_days"],7);
        assert!(rows[1].get("instruction_kp").is_none());
        Event::from_json(&rows[1].to_string()).unwrap();
        let plan=plan_body(&recovered,user).await;
        assert!(plan["tasks"].as_array().unwrap().iter().any(|task|task["task_id"]==id && task["progress"]["done"]==true),"{plan}");
    }).await;
}
