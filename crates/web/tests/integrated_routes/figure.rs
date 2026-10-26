//! The integrated-item teach route sends the figure of the page, as the lesson route does.
use super::*;
use cadus_core::visual::VisualSpec;

#[tokio::test]
async fn the_integrated_teach_page_carries_the_checked_figure() {
    TestDb::with(|db| async move {
        let figure: VisualSpec = serde_json::from_value(json!({
            "kind": "number_line", "min": "-10", "max": "10", "tick": "1",
            "points": [{"at": "6", "filled": true}],
            "caption": "A number line with a filled point at 6.",
        }))
        .unwrap();
        let graph = one_unit_curriculum(
            COMPONENTS
                .iter()
                .map(|id| {
                    let mut point = kp(
                        "kp1",
                        (1..=4)
                            .map(|n| exemplar(&format!("Compute {n} + {n}."), &(n * 2).to_string()))
                            .collect(),
                    );
                    if *id == "counting" {
                        point.visuals = vec![figure.clone()];
                    }
                    topic(id, vec![point])
                })
                .collect(),
        );
        common::seed_content_for(
            &db,
            &graph,
            "counting/kp1",
            "teach",
            "teach-counting",
            json!({
                "concept": "Combine workload with capacity.",
                "worked_example": {"problem": "Two visits need six minutes each.", "steps": ["The total is 12 minutes."]},
                "figure": 0,
            }),
        )
        .await;
        let router = create_app(
            AppState::new(Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS)).with_content(
                Arc::new(Content::new(graph).with_integrated(IntegratedSet::from_items(vec![item()]))),
            ),
        );
        let user = learner_with_due_reviews(&db, "integrated-figure@example.com").await;
        plan_body(&router, user).await;
        let (status, page) = common::teach_task(&router, user, MULTISTEP).await;
        assert_eq!(status, StatusCode::OK, "{page}");
        let figure = &page["worked_example"]["figure"];
        assert_eq!(figure["kind"], "number_line", "{page}");
        assert!(figure["svg"].as_str().unwrap().starts_with("<svg"));
        assert!(figure["text"].as_str().unwrap().contains('6'));
    })
    .await;
}
