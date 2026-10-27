//! The Today card of `GET /api/status` (D-DP1): the local date, the study-day
//! number, the set of the day, one read, the week line, and the frozen set.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::parse;
use common::sessions::*;
use sqlx::types::chrono::Utc;

/// `GET /api/status` as `user`: the `today` member.
async fn today_of(app: &Router, user: Uuid) -> Value {
    let (status, _, text) = call(app, Method::GET, "/api/status", Some(user), None).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    parse(&text)["today"].clone()
}

/// A learner enrolled in `c1` whose `addition` review is due, with no session open.
async fn learner_with_due_review(db: &TestDb, email: &str) -> Uuid {
    let user = common::seed_learner(db, email).await;
    seed_event(db, user, 1, &enrolled_c1()).await;
    seed_due_review(db, user, 1).await;
    user
}

/// The `(task_type, topic id)` pairs of a set, in order.
fn pairs(set: &Value) -> Vec<(String, Option<String>)> {
    set.as_array()
        .unwrap()
        .iter()
        .map(|item| {
            (
                item["task_type"].as_str().unwrap().to_string(),
                item["topic"]["id"].as_str().map(str::to_string),
            )
        })
        .collect()
}

async fn start(app: &Router, user: Uuid) {
    let (status, _, text) = call(app, Method::POST, "/api/session/start", Some(user), None).await;
    assert_eq!(status, StatusCode::OK, "{text}");
}

#[tokio::test]
async fn status_today_has_date_number_set_and_week() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_with_due_review(&db, "today-basic@example.com").await;

        let before = today_of(&app, user).await;
        assert_eq!(before["date"], Utc::now().date_naive().to_string());
        assert_eq!(before["day_number"], 1);
        assert_eq!(before["week"]["study_days"], 0);
        assert_eq!(before["week"]["topics_passed"], 0);
        assert_eq!(before["frozen"], false);

        start(&app, user).await;
        let plan = plan_of(&app, user).await;
        let plan_ids: Vec<&Value> = plan["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|task| &task["task_id"])
            .collect();
        let after = today_of(&app, user).await;
        let set_ids: Vec<&Value> = after["set"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| &item["task_id"])
            .collect();
        assert!(!set_ids.is_empty());
        assert_eq!(set_ids, plan_ids);
        assert_eq!(after["frozen"], true);
    })
    .await;
}

#[tokio::test]
async fn status_today_read_is_the_first_lesson_topic() {
    TestDb::with(|db| async move {
        let app = app(&db);

        // The read is the first lesson topic of the set, else the first review topic.
        let learner = learner_with_due_review(&db, "today-review@example.com").await;
        let card = today_of(&app, learner).await;
        let topic_of = |kind: &str| {
            card["set"]
                .as_array()
                .unwrap()
                .iter()
                .find(|item| item["task_type"] == kind)
                .map(|item| item["topic"]["id"].clone())
        };
        match (topic_of("lesson"), topic_of("review")) {
            (Some(topic), _) => {
                assert_eq!(card["read"]["source"], "lesson");
                assert_eq!(card["read"]["topic"]["id"], topic);
            }
            (None, Some(topic)) => {
                assert_eq!(card["read"]["source"], "review");
                assert_eq!(card["read"]["topic"]["id"], topic);
            }
            (None, None) => panic!("the set holds neither a lesson nor a review"),
        }

        // A learner with no course gets the two lessons of the first course.
        let bare = common::seed_learner(&db, "today-bare@example.com").await;
        let card = today_of(&app, bare).await;
        assert_eq!(card["set"].as_array().unwrap().len(), 2);
        assert_eq!(card["read"]["source"], "lesson");
        assert_eq!(card["read"]["topic"]["id"], card["set"][0]["topic"]["id"]);
    })
    .await;
}

#[tokio::test]
async fn status_today_carries_no_problem_text() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_with_due_review(&db, "today-text@example.com").await;
        start(&app, user).await;
        let card = today_of(&app, user).await;
        for item in card["set"].as_array().unwrap() {
            for key in ["problem", "answer", "solution_sketch", "text"] {
                assert!(item.get(key).is_none(), "{key} in {item}");
            }
        }
    })
    .await;
}

#[tokio::test]
async fn the_set_freezes_at_the_first_session_start_of_the_day() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_with_due_review(&db, "today-freeze@example.com").await;
        start(&app, user).await;
        let first = today_of(&app, user).await;
        assert_eq!(first["frozen"], true);
        let review = first["set"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["task_type"] == "review")
            .expect("a review stands in the set")
            .clone();
        assert_eq!(review["progress"]["done"], false);

        // Close the review through the log, as the grade route does.
        let task_id = review["task_id"].as_str().unwrap();
        let session: String = sqlx::query_scalar(
            "SELECT session_id FROM events WHERE user_id = $1 ORDER BY seq DESC LIMIT 1",
        )
        .bind(user)
        .fetch_one(&db.admin)
        .await
        .unwrap();
        let ts = cadus_core::event::Timestamp::from_micros(Utc::now().timestamp_micros())
            .to_wire_string()
            .unwrap();
        let payload = json!({
            "type": "review_result", "ts": ts, "session": session, "v": 1,
            "topic": "addition", "passed": true, "weighted_score": 1.0, "xp": 0.0,
            "quality_tier": "perfect", "assisted": false, "task_id": task_id,
        });
        sqlx::query(
            "INSERT INTO events (user_id, seq, ts, type, session_id, v, payload) \
             VALUES ($1, (SELECT max(seq) + 1 FROM events WHERE user_id = $1), $2::timestamptz, \
                     'review_result', $3, 1, $4)",
        )
        .bind(user)
        .bind(&ts)
        .bind(&session)
        .bind(payload)
        .execute(&db.admin)
        .await
        .unwrap();

        let second = today_of(&app, user).await;
        assert_eq!(pairs(&second["set"]), pairs(&first["set"]));
        assert_eq!(second["frozen"], true);
        let closed = second["set"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["task_type"] == "review")
            .unwrap();
        assert_eq!(closed["progress"]["done"], true);

        // End the session and start another on the same day: the row stands.
        let held = frozen_row(&db, user).await;
        let (status, _, text) =
            call(&app, Method::POST, "/api/session/end", Some(user), None).await;
        assert_eq!(status, StatusCode::OK, "{text}");
        start(&app, user).await;
        assert_eq!(frozen_row(&db, user).await, held);
    })
    .await;
}

#[tokio::test]
async fn a_new_day_writes_a_new_set() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_with_due_review(&db, "today-newday@example.com").await;
        let yesterday = Utc::now().date_naive().pred_opt().unwrap().to_string();
        sqlx::query("INSERT INTO session_plans (user_id, session_id, plan) VALUES ($1, 'old', $2)")
            .bind(user)
            .bind(json!({"day": yesterday, "set": [], "read": null}))
            .execute(&db.admin)
            .await
            .unwrap();
        let stale = today_of(&app, user).await;
        assert_eq!(stale["frozen"], false);

        start(&app, user).await;
        let row = frozen_row(&db, user).await;
        assert_eq!(row["day"], Utc::now().date_naive().to_string());
        assert_eq!(today_of(&app, user).await["frozen"], true);
    })
    .await;
}

/// The frozen plan document of `user`.
async fn frozen_row(db: &TestDb, user: Uuid) -> Value {
    sqlx::query_scalar("SELECT plan FROM session_plans WHERE user_id = $1")
        .bind(user)
        .fetch_one(&db.admin)
        .await
        .unwrap()
}
