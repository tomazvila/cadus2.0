//! `GET /api/topics/{topic_id}/teach` (D-TR1): the approved lesson text of one topic,
//! read outside a task.
//!
//! 1. Each point serves its approved page.
//! 2. A point with no approved page answers `page: null`.
//! 3. A topic with no approved page at all is `409 no_instruction`.
//! 4. An unknown topic is `404 not_found`.
//! 5. The call writes no event and no `web_states` row.
//! 6. The reply gives the position, the neighbors and the prerequisite links.
//! 7. No credential is `401`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
mod reading_fixture;

use axum::http::{Method, StatusCode};
use cadus_core::curriculum::{PrereqEdge, Slug};
use cadus_store::test_support::TestDb;
use common::{
    KEY, app_with_content, assert_refused, call, drill_app as app, exemplar, get_ok, kp,
    one_unit_curriculum, parse, seed_content, seed_content_for, seed_learner, seed_open_session,
    topic,
};
use serde_json::{Value, json};
use sqlx::types::Uuid;

/// The approved page body of one knowledge point.
fn page_body(concept: &str) -> Value {
    json!({
        "concept": concept,
        "worked_example": {"problem": "Compute 2 + 3.", "steps": ["2 + 3 = 5."]}
    })
}

/// The number of event rows and the text of the `web_states` rows of `user`.
async fn footprint(db: &TestDb, user: Uuid) -> (i64, String) {
    let mut tx = cadus_store::begin_tenant(&db.app, user).await.unwrap();
    let events = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM events WHERE user_id = $1")
        .bind(user)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let states = sqlx::query_scalar::<_, String>(
        "SELECT coalesce(string_agg(doc::text || updated_at::text, '|'), '') \
         FROM web_states WHERE user_id = $1",
    )
    .bind(user)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    (events, states)
}

#[tokio::test]
async fn topic_teach_serves_the_approved_page_of_each_point() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "topicteach@example.com").await;
        let app = app(&db);
        seed_content(
            &db,
            KEY,
            "teach",
            "digest-teach",
            page_body("Addition combines counts."),
        )
        .await;

        let body = get_ok(&app, user, "/api/topics/addition/teach").await;
        assert_eq!(body["topic"], "addition");
        assert_eq!(body["points"][0]["kp"], "kp1");
        assert_eq!(
            body["points"][0]["page"]["concept"],
            "Addition combines counts."
        );
        assert_eq!(
            body["points"][0]["page"]["worked_example"]["steps"][0],
            "2 + 3 = 5."
        );
    })
    .await;
}

#[tokio::test]
async fn topic_teach_marks_a_point_with_no_page_as_null() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "topicnull@example.com").await;
        let app = app(&db);
        seed_content(
            &db,
            KEY,
            "teach",
            "digest-teach",
            page_body("Addition combines counts."),
        )
        .await;

        let body = get_ok(&app, user, "/api/topics/addition/teach").await;
        assert_eq!(body["points"].as_array().unwrap().len(), 2);
        assert_eq!(body["points"][1]["kp"], "kp2");
        assert!(body["points"][1]["page"].is_null(), "{body}");
    })
    .await;
}

#[tokio::test]
async fn topic_teach_of_a_topic_with_no_approved_page_is_409() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "topicnone@example.com").await;
        let app = app(&db);

        let (status, body) = call(
            &app,
            Method::GET,
            "/api/topics/addition/teach",
            Some(user),
            None,
        )
        .await;
        assert_refused(
            &(status, parse(&body)),
            StatusCode::CONFLICT,
            "no_instruction",
        );
    })
    .await;
}

#[tokio::test]
async fn topic_teach_of_an_unknown_topic_is_404() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "topicmissing@example.com").await;
        let app = app(&db);

        let (status, body) = call(
            &app,
            Method::GET,
            "/api/topics/no-such-topic/teach",
            Some(user),
            None,
        )
        .await;
        assert_refused(&(status, parse(&body)), StatusCode::NOT_FOUND, "not_found");
    })
    .await;
}

#[tokio::test]
async fn topic_teach_writes_no_event_and_no_state() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "topicwrite@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;
        seed_content(
            &db,
            KEY,
            "teach",
            "digest-teach",
            page_body("Addition combines counts."),
        )
        .await;

        let before = footprint(&db, user).await;
        assert!(before.0 > 0, "the fixture must hold an event");
        get_ok(&app, user, "/api/topics/addition/teach").await;
        assert_eq!(footprint(&db, user).await, before);
    })
    .await;
}

#[tokio::test]
async fn topic_teach_gives_position_and_neighbors_in_load_order() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "topicorder@example.com").await;
        let edge = |id: &str| PrereqEdge {
            id: Slug::new(id).unwrap(),
            weight: 1.0,
            key: false,
        };
        let mut second = topic(
            "second",
            vec![kp("kp1", vec![exemplar("Compute 1 + 1.", "2")])],
        );
        second.prerequisites = vec![edge("first")];
        let mut third = topic("third", vec![kp("kp1", vec![])]);
        third.prerequisites = vec![edge("second")];
        let curriculum =
            one_unit_curriculum(vec![topic("first", vec![kp("kp1", vec![])]), second, third]);
        seed_content_for(
            &db,
            &curriculum,
            "second/kp1",
            "teach",
            "digest-second",
            page_body("Second concept."),
        )
        .await;
        let app = app_with_content(&db, curriculum);

        let body = get_ok(&app, user, "/api/topics/second/teach").await;
        assert_eq!(body["course"], "c1");
        assert_eq!(body["course_name"], "Foundations");
        assert_eq!(body["position"], 2);
        assert_eq!(body["count"], 3);
        assert_eq!(body["previous"]["id"], "first");
        assert_eq!(body["next"]["id"], "third");
        assert_eq!(body["learn_first"][0]["id"], "first");
        assert_eq!(body["opens_next"][0]["id"], "third");
        assert_eq!(body["learn_first"].as_array().unwrap().len(), 1);
    })
    .await;
}

#[tokio::test]
async fn topic_teach_needs_a_session_cookie() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let (status, body) =
            call(&app, Method::GET, "/api/topics/addition/teach", None, None).await;
        assert_refused(
            &(status, parse(&body)),
            StatusCode::UNAUTHORIZED,
            "unauthorized",
        );
    })
    .await;
}

/// D-RL1: the read route serves the reading list of the course as a plain list,
/// and no key for a course with no list.
#[tokio::test]
async fn topic_teach_serves_the_reading_list_of_the_course() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "topicreading@example.com").await;
        seed_content(
            &db,
            KEY,
            "teach",
            "digest-teach",
            page_body("Addition combines counts."),
        )
        .await;

        let with = app_with_content(&db, reading_fixture::curriculum(reading_fixture::entries()));
        let body = get_ok(&with, user, "/api/topics/addition/teach").await;
        assert_eq!(body["reading"], json!(reading_fixture::entries()));
        assert_eq!(body["course_name"], "Foundations");

        let without = app_with_content(&db, reading_fixture::curriculum(Vec::new()));
        let plain = get_ok(&without, user, "/api/topics/addition/teach").await;
        assert!(plain.get("reading").is_none(), "{plain}");
    })
    .await;
}

/// D-VBE1: the read route serves the Lithuanian term when the topic has one,
/// and no key when it has none.
#[tokio::test]
async fn topic_teach_serves_the_lithuanian_term() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "topicltterm@example.com").await;
        seed_content(
            &db,
            KEY,
            "teach",
            "digest-teach",
            page_body("Addition combines counts."),
        )
        .await;

        let with = app_with_content(
            &db,
            reading_fixture::curriculum_with_term(Vec::new(), Some("Sudėtis")),
        );
        let body = get_ok(&with, user, "/api/topics/addition/teach").await;
        assert_eq!(body["lt_term"], "Sudėtis");

        let without = app_with_content(&db, reading_fixture::curriculum(Vec::new()));
        let plain = get_ok(&without, user, "/api/topics/addition/teach").await;
        assert!(plain.get("lt_term").is_none(), "{plain}");
    })
    .await;
}

#[tokio::test]
async fn topic_teach_serves_the_optional_parts_of_a_point() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "topicextras@example.com").await;
        let app = app(&db);
        let mut body = page_body("Addition combines counts.");
        body["terms"] = json!([{"term": "Sum", "meaning": "The count of two counts combined."}]);
        body["picture"] = json!("Two apples and three apples sit in one bowl.");
        body["watch_out"] = json!("Counts of different things do not add into one count.");
        body["second_example"] = json!({
            "problem": "A bowl holds a total of 9 apples.",
            "steps": ["The total tells how many apples are in the bowl."]
        });
        seed_content(&db, KEY, "teach", "digest-teach-extras", body).await;

        let reply = get_ok(&app, user, "/api/topics/addition/teach").await;
        let page = &reply["points"][0]["page"];
        assert_eq!(page["terms"][0]["term"], "Sum");
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
    })
    .await;
}

#[tokio::test]
async fn topic_teach_of_an_older_page_carries_none_of_the_optional_parts() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "topicnoextras@example.com").await;
        let app = app(&db);
        seed_content(
            &db,
            KEY,
            "teach",
            "digest-teach-plain",
            page_body("Addition combines counts."),
        )
        .await;
        let reply = get_ok(&app, user, "/api/topics/addition/teach").await;
        let page = reply["points"][0]["page"].as_object().unwrap();
        for key in ["terms", "picture", "watch_out", "second_example"] {
            assert!(!page.contains_key(key), "{key}");
        }
    })
    .await;
}
