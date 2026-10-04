//! The mixed review block: two due reviews serve their questions interleaved,
//! and each review still closes on its own answers with its own result.
#![allow(clippy::unwrap_used)]
mod common;

use std::collections::BTreeMap;

use axum::Router;
use axum::http::StatusCode;
use cadus_core::curriculum::{Curriculum, review_context_digest};
use cadus_core::event::{Timestamp, TopicStatus};
use cadus_core::learner::{LearnerModel, TopicState};
use cadus_core::selector::mixed_review_order;
use cadus_store::test_support::TestDb;
use cadus_web::state::WebState;
use common::sessions::plan_of;
use common::*;
use serde_json::{Value, json};
use sqlx::types::Uuid;

const ADDITION: &str = "s_2026-01-01a-review-addition";
const SUBTRACTION: &str = "s_2026-01-01a-review-subtraction";

fn two_topic_curriculum() -> Curriculum {
    one_unit_curriculum(review_topics())
}

/// The two review topics, and `multiplication` beside them.
fn three_topic_curriculum() -> Curriculum {
    let mut topics = review_topics();
    topics.push(topic(
        "multiplication",
        vec![
            kp("kp1", vec![exemplar("Compute 3 x 2.5.", "7.5")]),
            kp("kp2", vec![exemplar("Compute 4 x 1.5.", "6")]),
        ],
    ));
    one_unit_curriculum(topics)
}

fn review_topics() -> Vec<cadus_core::curriculum::Topic> {
    vec![
        topic(
            "addition",
            vec![
                kp("kp1", vec![exemplar("Compute 8 + 5.5.", "13.5")]),
                kp("kp2", vec![exemplar("Compute 40 + 2.5.", "42.5")]),
            ],
        ),
        topic(
            "subtraction",
            vec![
                kp("kp1", vec![exemplar("Compute 9 - 4.5.", "4.5")]),
                kp("kp2", vec![exemplar("Compute 50 - 2.5.", "47.5")]),
            ],
        ),
    ]
}

fn due() -> TopicState {
    TopicState {
        status: TopicStatus::Learning,
        rep_num: 1.0,
        memory_base: 1.0,
        t0: Some(Timestamp::from_micros(BASE_US - 400 * 86_400_000_000)),
        interval_days: 1.0,
        ability: 0.6,
        ..TopicState::default()
    }
}

/// A learner with an open session and the reviews of `topics` due.
async fn learner(db: &TestDb, email: &str, topics: &[&str]) -> Uuid {
    learner_in(db, email, topics, &two_topic_curriculum()).await
}

/// The same, over `curriculum`.
async fn learner_in(db: &TestDb, email: &str, topics: &[&str], curriculum: &Curriculum) -> Uuid {
    let user = seed_learner(db, email).await;
    seed_open_session(db, user).await;
    let model = LearnerModel {
        topics: topics
            .iter()
            .map(|id| ((*id).to_owned(), due()))
            .collect::<BTreeMap<String, TopicState>>(),
        ..LearnerModel::default()
    };
    seed_cached_model(db, user, &model, 1).await;
    put_state(db, user, &WebState::for_session(SESSION)).await;
    let digest = review_context_digest(curriculum).unwrap();
    for topic in topics {
        for kp in ["kp1", "kp2"] {
            for index in 0..10 {
                seed_pool_row(
                    db,
                    user,
                    &format!("{topic}/{kp}"),
                    &format!("Give {topic} {kp} value {index}."),
                    &index.to_string(),
                    &format!("mixed-{topic}-{kp}-{index}"),
                    (&digest, cadus_core::review_engine::DIGEST),
                )
                .await;
            }
        }
    }
    user
}

/// Answer the live problem of `task`, right or wrong.
async fn answer_live(app: &Router, db: &TestDb, user: Uuid, task: &str, correct: bool) -> Value {
    let live = stored_state(db, user).await.served[task].clone();
    let (status, reply) = answer_task(
        app,
        user,
        task,
        json!({
            "problem_id": live.problem_id,
            "answer": if correct { live.expected.answer } else { "99999".to_owned() }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{reply}");
    reply
}

/// The per-task answer script: the n-th fresh question of a task is answered
/// right unless the script says wrong; corrective practice is always right.
fn script(task: &str, question: usize) -> bool {
    match task {
        ADDITION => question != 1,
        _ => question != 2,
    }
}

/// Walk a session from `first` until no next problem is handed back. Gives the
/// task of every FRESH question served, in order.
async fn walk(app: &Router, db: &TestDb, user: Uuid, first: &str) -> Vec<String> {
    let served = serve_ok(app, user, first).await;
    let mut current = served["task_id"].as_str().unwrap_or(first).to_owned();
    let mut order = Vec::new();
    let mut asked: BTreeMap<String, usize> = BTreeMap::new();
    let mut practice = false;
    loop {
        let correct = if practice {
            true
        } else {
            order.push(current.clone());
            let n = asked.entry(current.clone()).or_default();
            *n += 1;
            script(&current, *n - 1)
        };
        let reply = answer_live(app, db, user, &current, correct).await;
        practice = reply["feedback_practice"] == true;
        let next = &reply["next"];
        if next.is_null() {
            assert!(
                !practice && reply.get("next_unavailable").is_none(),
                "{reply}"
            );
            return order;
        }
        let shown = next["task_id"].as_str().unwrap_or(&current).to_owned();
        if practice {
            assert_eq!(
                shown, current,
                "corrective practice stays on the missed review"
            );
        }
        current = shown;
    }
}

/// The fields of a review result that the order must not change.
fn result_core(result: &Value) -> Value {
    json!({
        "task_id": result["task_id"], "topic": result["topic"], "passed": result["passed"],
        "weighted_score": result["weighted_score"], "xp": result["xp"],
        "quality_tier": result["quality_tier"], "assisted": result["assisted"],
        "inconclusive": result["inconclusive"],
        "confirmation_skills": result["confirmation_skills"],
    })
}

/// The per-task attempt trail, with the clock and the ids left out.
async fn trail(db: &TestDb, user: Uuid) -> BTreeMap<String, Vec<Value>> {
    let mut by_task: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for attempt in events_of_type(db, user, "attempt").await {
        by_task
            .entry(attempt["task_id"].as_str().unwrap().to_owned())
            .or_default()
            .push(json!({
                "attempt_id": attempt["attempt_id"], "kp": attempt["kp"],
                "correct": attempt["correct"], "outcome": attempt["outcome"],
                "feedback_practice": attempt["feedback_practice"],
                "work_quality": attempt["work_quality"], "assisted": attempt["assisted"],
            }));
    }
    by_task
}

#[tokio::test]
async fn two_due_reviews_interleave_and_each_closes_with_its_own_result() {
    TestDb::with(|db| async move {
        let app = app_with_content(&db, two_topic_curriculum());
        let user = learner(&db, "mixed@example.com", &["addition", "subtraction"]).await;
        let plan = plan_of(&app, user).await;
        let reviews: Vec<&str> = plan["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|task| task["task_type"] == "review")
            .map(|task| task["task_id"].as_str().unwrap())
            .collect();
        assert_eq!(reviews.len(), 2, "{plan}");

        let order = walk(&app, &db, user, reviews[0]).await;
        assert_eq!(order.len(), 8);
        assert!(
            order.windows(2).all(|pair| pair[0] != pair[1]),
            "no topic twice in a row: {order:?}"
        );
        let expected = mixed_review_order(SESSION, &[(reviews[0], 4), (reviews[1], 4)]);
        assert_eq!(order, expected, "the served order is the seeded rule's");

        let results = events_of_type(&db, user, "review_result").await;
        assert_eq!(results.len(), 2);
        let state = stored_state(&db, user).await;
        for task in [ADDITION, SUBTRACTION] {
            assert!(state.tasks[task].done);
            assert_eq!(state.tasks[task].answered, 4);
        }
        for task in [ADDITION, SUBTRACTION] {
            assert_refused(
                &serve_task(&app, user, task).await,
                StatusCode::CONFLICT,
                "task_complete",
            );
        }
    })
    .await;
}

#[tokio::test]
async fn interleaving_writes_the_same_per_review_events_as_blocked_serving() {
    TestDb::with(|db| async move {
        let app = app_with_content(&db, two_topic_curriculum());
        let mixed = learner(
            &db,
            "mixed-events@example.com",
            &["addition", "subtraction"],
        )
        .await;
        walk(&app, &db, mixed, ADDITION).await;

        // Blocked: each review alone in its own learner, the same answers.
        let mut blocked_results = Vec::new();
        let mut blocked_trail = BTreeMap::new();
        for (email, topic, task) in [
            ("blocked-a@example.com", "addition", ADDITION),
            ("blocked-s@example.com", "subtraction", SUBTRACTION),
        ] {
            let user = learner(&db, email, &[topic]).await;
            let order = walk(&app, &db, user, task).await;
            assert_eq!(
                order,
                vec![task.to_owned(); 4],
                "a lone review serves in a row"
            );
            for result in events_of_type(&db, user, "review_result").await {
                blocked_results.push(result_core(&result));
            }
            blocked_trail.extend(trail(&db, user).await);
        }

        let mut mixed_results: Vec<Value> = events_of_type(&db, mixed, "review_result")
            .await
            .iter()
            .map(result_core)
            .collect();
        mixed_results.sort_by_key(|result| result["task_id"].as_str().unwrap().to_owned());
        blocked_results.sort_by_key(|result| result["task_id"].as_str().unwrap().to_owned());
        assert_eq!(mixed_results, blocked_results);
        assert_eq!(trail(&db, mixed).await, blocked_trail);
    })
    .await;
}

#[tokio::test]
async fn a_reload_mid_block_serves_the_live_problem_and_keeps_the_order() {
    TestDb::with(|db| async move {
        let app = app_with_content(&db, two_topic_curriculum());
        let user = learner(
            &db,
            "mixed-resume@example.com",
            &["addition", "subtraction"],
        )
        .await;
        let expected = mixed_review_order(SESSION, &[(ADDITION, 4), (SUBTRACTION, 4)]);

        let first = serve_ok(&app, user, ADDITION).await;
        assert_eq!(first["task_id"], expected[0].as_str());
        assert_eq!(first["mixed_review"], json!({"position": 1, "total": 8}));
        // Idempotent: either task id hands back the same live problem.
        for task in [ADDITION, SUBTRACTION] {
            let again = serve_ok(&app, user, task).await;
            assert_eq!(again["problem_id"], first["problem_id"]);
            assert_eq!(again["task_id"], expected[0].as_str());
        }
        let mut served_order = Vec::new();
        for (step, task) in expected.iter().enumerate().take(5) {
            // A reload: the client asks for the FIRST review of its plan.
            let reloaded = serve_ok(&app, user, ADDITION).await;
            assert_eq!(reloaded["task_id"], task.as_str(), "step {step}");
            assert_eq!(reloaded["mixed_review"]["position"], step as i64 + 1);
            served_order.push(task.clone());
            answer_live(&app, &db, user, task, true).await;
        }
        assert_eq!(served_order, expected[..5]);
        // The rest of the block continues the same order after the reload.
        let attempts_before = events_of_type(&db, user, "attempt").await.len();
        assert_eq!(attempts_before, 5);
        let rest = {
            let mut rest = Vec::new();
            let mut current = serve_ok(&app, user, SUBTRACTION).await["task_id"]
                .as_str()
                .unwrap()
                .to_owned();
            loop {
                rest.push(current.clone());
                let reply = answer_live(&app, &db, user, &current, true).await;
                if reply["next"].is_null() {
                    break rest;
                }
                current = reply["next"]["task_id"].as_str().unwrap().to_owned();
            }
        };
        assert_eq!(rest, expected[5..]);
        assert_eq!(events_of_type(&db, user, "review_result").await.len(), 2);
    })
    .await;
}

#[tokio::test]
async fn a_lone_review_keeps_its_payload() {
    TestDb::with(|db| async move {
        let app = lesson_app(&db);
        let user = seed_learner(&db, "mixed-lone@example.com").await;
        seed_open_session(&db, user).await;
        seed_due_review(&db, user).await;
        put_state(&db, user, &WebState::for_session(SESSION)).await;
        let curriculum = addition_curriculum(Vec::new());
        let digest = review_context_digest(&curriculum).unwrap();
        for kp in ["kp1", "kp2"] {
            for index in 0..6 {
                seed_pool_row(
                    &db,
                    user,
                    &format!("addition/{kp}"),
                    &format!("Give {kp} value {index}."),
                    &index.to_string(),
                    &format!("lone-{kp}-{index}"),
                    (&digest, cadus_core::review_engine::DIGEST),
                )
                .await;
            }
        }
        let served = serve_ok(&app, user, REVIEW).await;
        assert!(served.get("task_id").is_none() && served.get("mixed_review").is_none());
        let reply = answer_live(&app, &db, user, REVIEW, true).await;
        assert!(reply["next"].get("task_id").is_none());
    })
    .await;
}

#[tokio::test]
async fn every_review_of_a_block_records_exactly_one_task_served() {
    TestDb::with(|db| async move {
        let curriculum = three_topic_curriculum();
        let app = app_with_content(&db, curriculum.clone());
        let topics = ["addition", "multiplication", "subtraction"];
        let user = learner_in(&db, "mixed-served@example.com", &topics, &curriculum).await;
        let reviews: Vec<String> = plan_of(&app, user).await["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|task| task["task_type"] == "review")
            .map(|task| task["task_id"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(reviews.len(), 3);

        // Walk the block by hand-off alone: one serve, then only answers.
        let mut current = serve_ok(&app, user, &reviews[0]).await["task_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let mut order = Vec::new();
        loop {
            order.push(current.clone());
            let reply = answer_live(&app, &db, user, &current, true).await;
            if reply["next"].is_null() {
                break;
            }
            current = reply["next"]["task_id"].as_str().unwrap().to_owned();
        }
        assert_eq!(order.len(), 12);
        assert!(order.windows(2).all(|pair| pair[0] != pair[1]), "{order:?}");

        let served = events_of_type(&db, user, "task_served").await;
        for review in &reviews {
            let count = served
                .iter()
                .filter(|event| event["task_id"] == review.as_str())
                .count();
            assert_eq!(count, 1, "{review}: {served:?}");
            assert!(
                served
                    .iter()
                    .any(|event| event["task_id"] == review.as_str()
                        && event["task_type"] == "review"
                        && event["session"] == SESSION),
            );
        }
        assert_eq!(served.len(), 3, "{served:?}");
        assert_eq!(events_of_type(&db, user, "review_result").await.len(), 3);
        // The fold cursor stands at the head of the log after the hand-offs.
        assert_eq!(fold_cursor(&db, user).await, log_head(&db, user).await);
    })
    .await;
}

#[tokio::test]
async fn a_reload_serves_the_corrective_practice_a_sibling_review_owes() {
    TestDb::with(|db| async move {
        let app = app_with_content(&db, two_topic_curriculum());
        let user = learner(
            &db,
            "mixed-practice@example.com",
            &["addition", "subtraction"],
        )
        .await;
        let first = serve_ok(&app, user, ADDITION).await;
        let missed = first["task_id"].as_str().unwrap().to_owned();
        let sibling = if missed == ADDITION {
            SUBTRACTION
        } else {
            ADDITION
        };

        let reply = answer_live(&app, &db, user, &missed, false).await;
        assert_eq!(reply["feedback_practice"], true, "{reply}");
        // A reload that lost the live practice problem: the practice is still owed.
        let mut state = stored_state(&db, user).await;
        assert!(state.feedback_practice.contains_key(&missed));
        state.served.clear();
        put_state(&db, user, &state).await;

        let served = serve_ok(&app, user, sibling).await;
        assert_eq!(served["task_id"], missed.as_str(), "{served}");
        assert_eq!(served["feedback_practice"], true, "{served}");
        let state = stored_state(&db, user).await;
        assert!(state.served.contains_key(&missed));
        assert!(!state.served.contains_key(sibling));
    })
    .await;
}
