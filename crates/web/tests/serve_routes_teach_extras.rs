//! The optional parts of a teach page (D-TM1, D-TD1): the reply carries each key
//! only when the approved body holds it.
//!
//! The header of `serve_routes.rs` gives the requirements and the rules.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axum::http::StatusCode;
use cadus_store::test_support::TestDb;
use common::{
    KEY, LESSON, drill_app as app, seed_content, seed_learner, seed_open_session, teach_task,
};
use serde_json::json;

#[tokio::test]
async fn teach_on_a_lesson_serves_the_term_list() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "teachterms@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;
        seed_content(
            &db,
            KEY,
            "teach",
            "digest-teach-terms",
            json!({
                "concept": "Addition combines two counts.",
                "worked_example": {"problem": "Compute 2 + 3.", "steps": ["2 + 3 = 5."]},
                "terms": [
                    {"term": "Sum", "meaning": "The count you get when you combine two counts."},
                    {"term": "Addend", "meaning": "A count that you combine with another."}
                ]
            }),
        )
        .await;

        let (status, page) = teach_task(&app, user, LESSON).await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert_eq!(page["terms"].as_array().unwrap().len(), 2);
        assert_eq!(page["terms"][0]["term"], "Sum");
        assert!(page.get("picture").is_none());
    })
    .await;
}

#[tokio::test]
async fn teach_on_a_lesson_serves_the_depth_parts() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "teachdepth@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;
        seed_content(
            &db,
            KEY,
            "teach",
            "digest-teach-depth",
            json!({
                "concept": "Addition combines two counts.",
                "worked_example": {"problem": "Compute 2 + 3.", "steps": ["2 + 3 = 5."]},
                "picture": "Two apples and three apples sit in one bowl.",
                "watch_out": "Counts of different things do not add into one count.",
                "second_example": {
                    "problem": "A bowl holds a total of 9 apples.",
                    "steps": ["The total tells how many apples are in the bowl."]
                }
            }),
        )
        .await;

        let (status, page) = teach_task(&app, user, LESSON).await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert_eq!(
            page["picture"],
            "Two apples and three apples sit in one bowl."
        );
        assert_eq!(
            page["watch_out"],
            "Counts of different things do not add into one count."
        );
        assert_eq!(
            page["second_example"]["steps"][0],
            "The total tells how many apples are in the bowl."
        );
        assert!(page.get("terms").is_none());
    })
    .await;
}
