//! D-RL1: `GET /api/status` gives the reading list of the enrolled course.
//! `GET /api/modules` and `GET /api/graph` do not.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
mod reading_fixture;

use common::parse;
use common::sessions::*;

async fn get(app: &Router, user: Uuid, path: &str) -> Value {
    let (status, _, text) = call(app, Method::GET, path, Some(user), None).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    parse(&text)
}

#[tokio::test]
async fn status_carries_the_reading_list_of_the_enrolled_course() {
    TestDb::with(|db| async move {
        let app =
            common::app_with_content(&db, reading_fixture::curriculum(reading_fixture::entries()));
        let user = common::seed_learner(&db, "reading-status@example.com").await;
        seed_event(&db, user, 1, &enrolled_c1()).await;

        let status = get(&app, user, "/api/status").await;
        let list = status["course"]["reading"].as_array().expect("a list");
        assert_eq!(list.len(), 2);
        for item in list {
            let mut keys: Vec<&String> = item.as_object().unwrap().keys().collect();
            keys.sort();
            assert_eq!(keys, ["access", "author", "note", "title", "where"]);
        }
        assert_eq!(list[0]["where"], "https://example.org/primer");

        let modules = get(&app, user, "/api/modules").await;
        assert!(modules["course"].get("reading").is_none(), "{modules}");
        let graph = get(&app, user, "/api/graph").await;
        assert!(!graph.to_string().contains("\"reading\""), "{graph}");
    })
    .await;
}

#[tokio::test]
async fn status_has_no_reading_key_without_a_list() {
    TestDb::with(|db| async move {
        let app = common::app_with_content(&db, reading_fixture::curriculum(Vec::new()));
        let user = common::seed_learner(&db, "reading-none@example.com").await;
        seed_event(&db, user, 1, &enrolled_c1()).await;

        let status = get(&app, user, "/api/status").await;
        assert!(status["course"].get("reading").is_none(), "{status}");
    })
    .await;
}

#[tokio::test]
async fn status_of_a_learner_in_no_course_has_no_reading_key() {
    TestDb::with(|db| async move {
        let app =
            common::app_with_content(&db, reading_fixture::curriculum(reading_fixture::entries()));
        let user = common::seed_learner(&db, "reading-nocourse@example.com").await;

        let status = get(&app, user, "/api/status").await;
        assert!(status["course"].get("reading").is_none(), "{status}");
    })
    .await;
}
