//! The integrated-task routes of D-F10 on the HTTP surface: one scenario served
//! as ONE problem, one hint rung at a time, and one graded submission.
//!
//! Every expected value is a literal: a literal status code, a literal error
//! code, a literal field of the reply. Trap W7 gives the secrecy rule: scan the
//! RAW body text, do not read the parsed fields.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::Router;
use axum::http::{Method, StatusCode};
use cadus_core::curriculum::{Curriculum, Topic};
use cadus_core::event::{Timestamp, TopicStatus};
use cadus_core::integrated::{IntegratedItem, IntegratedSet, parse_item};
use cadus_core::learner::{LearnerModel, TopicState};
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::state::Content;
use cadus_web::{AppState, create_app};
use common::{
    BASE_US, call, exemplar, kp, one_unit_curriculum, parse, plan_body, seed_cached_model,
    seed_learner, seed_open_session, topic,
};
use serde_json::{Value, json};
use sqlx::types::Uuid;

/// The four component topics of the fixture, in plan order.
const COMPONENTS: [&str; 4] = ["counting", "adding", "scaling", "reading"];

/// The task id the composer gives the multi-step task (`{session}-{type}`).
const MULTISTEP: &str = "s_2026-01-01a-multi-step";

/// The authored integrated item of the fixture: a workforce scenario over the
/// four component topics.
const ITEM: &str = r#"
id: integrated-test-window
title: Staff the test window
course: c1
topic: counting
component_topics: [counting, adding, scaling, reading]
domain: workforce_capacity
scenario: >
  A desk opens a 4 hour window and books 48 visits. One clerk needs 20 minutes
  for one visit and takes no break.
given:
  - label: Window
    value: 4 h
  - label: Booked visits
    value: "48"
  - label: Time for one visit
    value: 20 min
    note: A visit of 30 minutes needs more clerks.
method:
  prompt: Which method gives the number of clerks?
  options:
    - id: person-minutes
      label: Divide the person-minutes by the minutes one clerk works.
      correct: true
      why: The window gives every clerk the same minutes.
    - id: visits-per-hour
      label: Divide the visits by the hours.
      correct: false
      why: That answers visits per hour and names no clerk.
steps:
  - id: person-minutes
    ask:
      prompt: How many person-minutes of work does the window hold?
      answer: "960"
      unit: person-minutes
      hints:
        - Every visit needs the same time.
        - Multiply the visits by the minutes of one visit.
    skills: [counting/kp1]
  - id: clerk-minutes
    ask:
      prompt: How many minutes does one clerk work?
      answer: "240"
      unit: min
      hints:
        - The window is stated in hours.
    skills: [adding/kp1]
final:
  ask:
    prompt: How many clerks does the window need?
    answer: "4"
    unit: clerks
    hints:
      - Divide the person-minutes by the minutes of one clerk.
  interpretation: 4 clerks clear 960 person-minutes inside the window.
  skills: [scaling/kp1]
"#;

/// The authored item of the fixture.
fn item() -> IntegratedItem {
    parse_item(ITEM).expect("the fixture parses")
}

/// One fixture topic with one decidable exemplar.
fn component(id: &str, n: i64) -> Topic {
    topic(
        id,
        vec![kp(
            "kp1",
            vec![exemplar(
                &format!("Compute {n} + {n}."),
                &(n * 2).to_string(),
            )],
        )],
    )
}

/// The four-topic curriculum the multi-step cadence needs.
fn arena() -> Curriculum {
    one_unit_curriculum(
        COMPONENTS
            .iter()
            .enumerate()
            .map(|(index, id)| component(id, index as i64 + 1))
            .collect(),
    )
}

/// The router with the authored item loaded, and the readiness rule off.
fn app(db: &TestDb, set: IntegratedSet) -> Router {
    let mut cfg = cadus_core::config::Config::default();
    cfg.readiness.enforce = false;
    create_app(
        AppState::new(Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS)).with_content(Arc::new(
            Content::with_config(arena(), cfg).with_integrated(set),
        )),
    )
}

/// A learner whose four topics all came due for review, which is what the
/// multi-step cadence of the composer asks for.
async fn learner_with_due_reviews(db: &TestDb, email: &str) -> Uuid {
    let user = seed_learner(db, email).await;
    seed_open_session(db, user).await;
    let mut topics: BTreeMap<String, TopicState> = BTreeMap::new();
    for id in COMPONENTS {
        topics.insert(
            id.to_owned(),
            TopicState {
                status: TopicStatus::Learning,
                rep_num: 1.0,
                memory_base: 1.0,
                t0: Some(Timestamp::from_micros(BASE_US - 400 * 86_400_000_000)),
                interval_days: 1.0,
                ability: 0.6,
                ..TopicState::default()
            },
        );
    }
    let model = LearnerModel {
        topics,
        ..LearnerModel::default()
    };
    seed_cached_model(db, user, &model, 1).await;
    user
}

/// The task ids of the plan, so a test names the multi-step task the composer
/// built and never a task id of its own invention.
async fn multistep_task_id(app: &Router, user: Uuid) -> String {
    let plan = plan_body(app, user).await;
    let tasks = plan["tasks"].as_array().expect("the plan lists tasks");
    tasks
        .iter()
        .find(|task| task["task_type"] == "multi-step")
        .unwrap_or_else(|| panic!("no multi-step task in {plan}"))["task_id"]
        .as_str()
        .expect("a task id")
        .to_owned()
}

/// `POST` one of the integrated routes as `user`.
async fn post(app: &Router, user: Uuid, uri: &str, body: Option<Value>) -> (StatusCode, String) {
    call(app, Method::POST, uri, Some(user), body).await
}

/// Reveal the first person-minutes hint and require the successful receipt.
async fn reveal_first_hint(app: &Router, user: Uuid) {
    let (status, body) = post(
        app,
        user,
        &format!("/api/task/{MULTISTEP}/integrated/hint"),
        Some(json!({"field": "person-minutes", "index": 0})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[tokio::test]
async fn a_multi_step_task_serves_the_integrated_item_as_one_problem() {
    TestDb::with(|db| async move {
        let app = app(&db, IntegratedSet::from_items(vec![item()]));
        let user = learner_with_due_reviews(&db, "integrated-serve@example.com").await;
        let task_id = multistep_task_id(&app, user).await;
        assert_eq!(task_id, MULTISTEP);

        let (status, body) =
            post(&app, user, &format!("/api/task/{task_id}/integrated"), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let payload = parse(&body);
        assert_eq!(payload["item_id"], "integrated-test-window");
        assert_eq!(payload["domain"], "workforce_capacity");
        assert_eq!(payload["steps"].as_array().unwrap().len(), 2);
        assert_eq!(payload["steps"][0]["ask"]["hints_available"], 2);
        assert_eq!(payload["method"]["options"].as_array().unwrap().len(), 2);
        assert_eq!(payload["final_ask"]["unit"], "clerks");
        assert_eq!(payload["skills"][0], "counting/kp1");

        // Hard Rule 1, on the RAW body: no authored answer and no method flag.
        for secret in [
            "\"960\"",
            "\"240\"",
            "\"4\"",
            "\"correct\"",
            "\"why\"",
            "clear 960 person-minutes",
        ] {
            assert!(!body.contains(secret), "the serve leaked {secret}: {body}");
        }
    })
    .await;
}

#[tokio::test]
async fn a_hint_gives_one_rung_and_never_the_answer() {
    TestDb::with(|db| async move {
        let app = app(&db, IntegratedSet::from_items(vec![item()]));
        let user = learner_with_due_reviews(&db, "integrated-hint@example.com").await;
        let uri = format!("/api/task/{MULTISTEP}/integrated/hint");

        let (status, body) = post(
            &app,
            user,
            &uri,
            Some(json!({"field": "person-minutes", "index": 0})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let first = parse(&body);
        assert_eq!(first["hint"], "Every visit needs the same time.");
        assert_eq!(first["hints_available"], 2);
        assert_eq!(first["hints_used"], 1);
        assert!(!body.contains("960"), "a hint leaked the answer: {body}");

        // The ladder ends: a rung past the last one is null, not the answer.
        let (status, body) = post(
            &app,
            user,
            &uri,
            Some(json!({"field": "person-minutes", "index": 5})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(parse(&body)["hint"], Value::Null);

        let (status, body) = post(&app, user, &uri, Some(json!({"field": "no-such-step"}))).await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(parse(&body)["error"]["code"], "unknown_integrated_field");
    })
    .await;
}

#[tokio::test]
async fn one_submission_grades_every_step_and_the_final_answer() {
    TestDb::with(|db| async move {
        let app = app(&db, IntegratedSet::from_items(vec![item()]));
        let user = learner_with_due_reviews(&db, "integrated-answer@example.com").await;
        reveal_first_hint(&app, user).await;
        let uri = format!("/api/task/{MULTISTEP}/integrated/answer");

        let (status, body) = post(
            &app,
            user,
            &uri,
            Some(json!({
                "method": "person-minutes",
                "steps": [
                    {"id": "person-minutes", "answer": "960", "hints_used": 1},
                    {"id": "clerk-minutes", "answer": "300"}
                ],
                "final_answer": {"id": "final", "answer": "4"},
                "reasoning": "I counted the work first, then the minutes of one clerk."
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let reply = parse(&body);
        assert_eq!(reply["method"]["correct"], true);
        assert_eq!(reply["steps"][0]["correct"], true);
        assert_eq!(reply["steps"][0]["assisted"], true);
        assert_eq!(reply["steps"][1]["correct"], false);
        assert_eq!(reply["correct_steps"], 1);
        assert_eq!(reply["total_steps"], 2);
        assert_eq!(reply["solved"], true);
        assert_eq!(reply["assisted"], true);
        assert_eq!(reply["ungraded"], false);
        assert_eq!(
            reply["skills_credited"],
            json!(["counting/kp1", "scaling/kp1"])
        );
        // The interpretation is released with the grade, and never before it.
        assert_eq!(
            reply["interpretation"],
            "4 clerks clear 960 person-minutes inside the window."
        );
        // The learner's words stand apart from every verdict above, and no rule
        // marks them right or wrong.
        assert_eq!(reply["reasoning"]["graded"], false);
        assert_eq!(reply["reasoning"]["recorded"], true);
    })
    .await;
}

#[tokio::test]
async fn a_task_with_no_authored_item_keeps_the_per_component_path() {
    TestDb::with(|db| async move {
        let app = app(&db, IntegratedSet::empty());
        let user = learner_with_due_reviews(&db, "integrated-absent@example.com").await;

        let (status, body) = post(
            &app,
            user,
            &format!("/api/task/{MULTISTEP}/integrated"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(parse(&body)["error"]["code"], "no_integrated_item");
    })
    .await;
}

#[tokio::test]
async fn a_task_of_another_learner_is_never_served() {
    TestDb::with(|db| async move {
        let app = app(&db, IntegratedSet::from_items(vec![item()]));
        let owner = learner_with_due_reviews(&db, "integrated-owner@example.com").await;
        let other = seed_learner(&db, "integrated-other@example.com").await;
        seed_open_session(&db, other).await;

        let task_id = multistep_task_id(&app, owner).await;
        let (status, body) = post(
            &app,
            other,
            &format!("/api/task/{task_id}/integrated"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
        assert_eq!(parse(&body)["error"]["code"], "unknown_task");
    })
    .await;
}

/// The event rows of one learner, newest last: `(type, payload)`.
async fn events_of(db: &TestDb, user: Uuid) -> Vec<(String, Value)> {
    // A runtime query, not the checked macro: this file reads the log as an
    // oracle, and the oracle must not depend on the offline query cache.
    sqlx::query_as::<_, (String, Value)>(
        "SELECT type, payload FROM events WHERE user_id = $1 ORDER BY seq",
    )
    .bind(user)
    .fetch_all(&db.admin)
    .await
    .unwrap()
}

#[tokio::test]
async fn the_serve_records_the_exposure_once_however_often_it_reloads() {
    TestDb::with(|db| async move {
        let app = app(&db, IntegratedSet::from_items(vec![item()]));
        let user = learner_with_due_reviews(&db, "integrated-exposure@example.com").await;
        let uri = format!("/api/task/{MULTISTEP}/integrated");

        for _ in 0..3 {
            let (status, body) = post(&app, user, &uri, None).await;
            assert_eq!(status, StatusCode::OK, "{body}");
        }
        let served: Vec<(String, Value)> = events_of(&db, user)
            .await
            .into_iter()
            .filter(|(kind, _)| kind == "integrated_served")
            .collect();
        assert_eq!(served.len(), 1, "a reload is one exposure");
        let payload = &served[0].1;
        assert_eq!(payload["item_id"], "integrated-test-window");
        assert_eq!(payload["task_id"], MULTISTEP);
        assert_eq!(payload["topic"], "counting");
        assert_eq!(
            payload["skills"],
            json!(["counting/kp1", "adding/kp1", "scaling/kp1"])
        );
        // Hard Rule 1 holds in the LOG too: the exposure row names no answer.
        let raw = payload.to_string();
        for secret in ["\"960\"", "\"240\"", "\"answer\""] {
            assert!(
                !raw.contains(secret),
                "the exposure row leaked {secret}: {raw}"
            );
        }
    })
    .await;
}

#[tokio::test]
async fn the_submission_is_recorded_with_every_answer_and_its_contract() {
    TestDb::with(|db| async move {
        let app = app(&db, IntegratedSet::from_items(vec![item()]));
        let user = learner_with_due_reviews(&db, "integrated-record@example.com").await;
        reveal_first_hint(&app, user).await;
        let body = json!({
            "method": "person-minutes",
            "steps": [
                {"id": "person-minutes", "answer": "960", "hints_used": 2},
                {"id": "clerk-minutes", "answer": "about four hours"}
            ],
            "final_answer": {"id": "final", "answer": "4"},
            "reasoning": "I counted the work first."
        });
        let uri = format!("/api/task/{MULTISTEP}/integrated/answer");
        let (status, first) = post(&app, user, &uri, Some(body.clone())).await;
        assert_eq!(status, StatusCode::OK, "{first}");
        assert_eq!(parse(&first)["recorded"], true);

        // A second submission of the same item writes NO second row (double-credit).
        let (status, again) = post(&app, user, &uri, Some(body)).await;
        assert_eq!(status, StatusCode::OK, "{again}");
        assert_eq!(parse(&again)["recorded"], false);

        let rows: Vec<Value> = events_of(&db, user)
            .await
            .into_iter()
            .filter(|(kind, _)| kind == "integrated_attempt")
            .map(|(_, payload)| payload)
            .collect();
        assert_eq!(rows.len(), 1, "a repeat submission writes one row");
        let row = &rows[0];
        assert_eq!(row["item_digest"], parse(&first)["item_digest"]);
        assert_eq!(row["method"], "person-minutes");
        assert_eq!(row["method_correct"], true);
        assert_eq!(row["solved"], true);
        assert_eq!(row["assisted"], true);
        assert_eq!(row["steps"][0]["answer"], "960");
        assert_eq!(row["steps"][0]["outcome"], "correct");
        assert_eq!(row["steps"][0]["contract"], json!({"kind": "exact"}));
        assert_eq!(row["steps"][0]["assisted"], true);
        // An answer the checker cannot read is UNGRADED, and it credits nothing.
        assert_eq!(row["steps"][1]["answer"], "about four hours");
        assert!(row["steps"][1]["outcome"]["ungraded"]["reason"].is_string());
        assert_eq!(
            row["skills_credited"],
            json!(["counting/kp1", "scaling/kp1"])
        );
        // The prose is stored under a name that says it was never graded.
        assert_eq!(row["reasoning_ungraded"], "I counted the work first.");
    })
    .await;
}

#[path = "integrated_routes/hint_authority.rs"]
mod hint_authority;

#[path = "integrated_routes/timing.rs"]
mod timing;

#[path = "integrated_routes/journey.rs"]
mod journey;
