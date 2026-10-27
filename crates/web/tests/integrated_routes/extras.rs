//! The integrated-item teach route sends the optional parts of the page
//! (D-TM1, D-TD1), as the lesson route does.
use super::*;

#[tokio::test]
async fn the_integrated_teach_page_carries_the_terms_and_the_picture() {
    TestDb::with(|db| async move {
        let graph = one_unit_curriculum(
            COMPONENTS
                .iter()
                .map(|id| {
                    let point = kp(
                        "kp1",
                        (1..=4)
                            .map(|n| exemplar(&format!("Compute {n} + {n}."), &(n * 2).to_string()))
                            .collect(),
                    );
                    topic(id, vec![point])
                })
                .collect(),
        );
        common::seed_content_for(
            &db,
            &graph,
            "counting/kp1",
            "teach",
            "teach-counting-extras",
            json!({
                "concept": "Combine workload with capacity.",
                "worked_example": {"problem": "Two visits need six minutes each.", "steps": ["The total is 12 minutes."]},
                "terms": [{"term": "Workload", "meaning": "The time that all visits need."}],
                "picture": "A clock with two blocks of six minutes.",
            }),
        )
        .await;
        let router = create_app(
            AppState::new(Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS)).with_content(
                Arc::new(Content::new(graph).with_integrated(IntegratedSet::from_items(vec![item()]))),
            ),
        );
        let user = learner_with_due_reviews(&db, "integrated-extras@example.com").await;
        plan_body(&router, user).await;
        let (status, page) = common::teach_task(&router, user, MULTISTEP).await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert_eq!(page["terms"][0]["term"], "Workload", "{page}");
        assert_eq!(page["picture"], "A clock with two blocks of six minutes.");
        assert!(page.get("watch_out").is_none());
    })
    .await;
}
