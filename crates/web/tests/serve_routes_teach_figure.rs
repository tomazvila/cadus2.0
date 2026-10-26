//! Brief 09 part A: the teach route sends the checked figure of a teach page.
//!
//! The page names one figure by index into the `visuals` of its knowledge point.
//! The route draws it with `render_all` and sends it as `worked_example.figure`.
//! A page with no figure, a stale index, or a figure that fails `validate` gives
//! the reply of before. The integrated route is in `integrated_routes/figure.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axum::http::StatusCode;
use cadus_core::curriculum::{Curriculum, KnowledgePoint};
use cadus_core::visual::VisualSpec;
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use common::admin::{Seed, admin_get, seed_admin, seed_row};
use common::{
    DRILL_TEXT, EXEMPLAR_TEXT_2, EXPECTED_ANSWER, KEY, LESSON, PROBLEM_TEXT, app_with_content,
    drill_topic, exemplar, kp, one_unit_curriculum, seed_content_for, seed_learner,
    seed_open_session, state_with_content, teach_task, topic,
};
use serde_json::{Value, json};

/// A figure that holds: one filled point on a number line.
fn good_figure() -> VisualSpec {
    serde_json::from_value(json!({
        "kind": "number_line", "min": "-10", "max": "10", "tick": "1",
        "points": [{"at": "6", "filled": true}],
        "caption": "A number line with a filled point at 6.",
    }))
    .unwrap()
}

/// A figure that fails `validate`: the range does not ascend.
fn broken_figure() -> VisualSpec {
    serde_json::from_value(json!({
        "kind": "number_line", "min": "5", "max": "-5", "tick": "1",
        "points": [{"at": "1", "filled": true}],
    }))
    .unwrap()
}

/// The drill fixture of the serve tests, with two figures on `addition/kp1`.
fn curriculum() -> Curriculum {
    let mut point: KnowledgePoint = kp(
        "kp1",
        vec![
            exemplar(PROBLEM_TEXT, EXPECTED_ANSWER),
            exemplar(EXEMPLAR_TEXT_2, "13.25"),
        ],
    );
    point.visuals = vec![good_figure(), broken_figure()];
    one_unit_curriculum(vec![
        topic(
            "addition",
            vec![
                point,
                kp("kp2", vec![exemplar("Compute 40 + 2.5.", "42.5")]),
            ],
        ),
        topic("subtraction", vec![kp("kp1", vec![])]),
        drill_topic(
            "tables",
            vec![kp(
                "kp1",
                vec![
                    exemplar(DRILL_TEXT, "56.5"),
                    exemplar("Compute 6 x 7 + 0.25.", "42.25"),
                ],
            )],
        ),
    ])
}

fn page(figure: Option<Value>) -> Value {
    let mut body = json!({
        "concept": "Addition combines two counts.",
        "worked_example": {"problem": "Compute 2 + 3.", "steps": ["2 + 3 = 5."]}
    });
    if let Some(figure) = figure {
        body["figure"] = figure;
    }
    body
}

/// Serve the lesson page of `body` and return the reply.
async fn served(email: &str, body: Value) -> (StatusCode, Value) {
    let email = email.to_owned();
    let result = std::sync::Arc::new(std::sync::Mutex::new(None));
    let slot = result.clone();
    TestDb::with(|db| async move {
        let graph = curriculum();
        let user = seed_learner(&db, &email).await;
        let app = app_with_content(&db, curriculum());
        seed_open_session(&db, user).await;
        seed_content_for(&db, &graph, KEY, "teach", "digest-teach", body).await;
        *slot.lock().unwrap() = Some(teach_task(&app, user, LESSON).await);
    })
    .await;
    result.lock().unwrap().take().unwrap()
}

/// Every key at every depth of `value`.
fn keys(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, inner) in map {
                out.push(key.clone());
                keys(inner, out);
            }
        }
        Value::Array(items) => items.iter().for_each(|item| keys(item, out)),
        _ => {}
    }
}

#[tokio::test]
async fn a_page_with_a_figure_serves_the_drawn_figure_and_no_answer() {
    let (status, reply) = served("figure@example.com", page(Some(json!(0)))).await;
    assert_eq!(status, StatusCode::OK, "{reply}");
    let figure = &reply["worked_example"]["figure"];
    assert_eq!(figure["kind"], "number_line");
    assert!(figure["svg"].as_str().unwrap().starts_with("<svg"));
    assert!(figure["text"].as_str().unwrap().contains('6'));
    let mut all = Vec::new();
    keys(&reply, &mut all);
    for banned in ["answer", "expected", "reveal"] {
        assert!(!all.iter().any(|key| key == banned), "{banned} in {reply}");
    }
}

#[tokio::test]
async fn a_page_without_a_figure_keeps_the_key_set_of_today() {
    let (status, reply) = served("nofigure@example.com", page(None)).await;
    assert_eq!(status, StatusCode::OK, "{reply}");
    let mut top: Vec<&String> = reply.as_object().unwrap().keys().collect();
    top.sort();
    assert_eq!(top, ["concept", "kp", "worked_example"]);
    let mut inner: Vec<&String> = reply["worked_example"]
        .as_object()
        .unwrap()
        .keys()
        .collect();
    inner.sort();
    assert_eq!(inner, ["problem", "steps"]);
}

#[tokio::test]
async fn a_stored_figure_out_of_range_at_serve_time_gives_no_figure_key() {
    let (status, reply) = served("range@example.com", page(Some(json!(7)))).await;
    assert_eq!(status, StatusCode::OK, "{reply}");
    assert!(reply["worked_example"].get("figure").is_none(), "{reply}");
}

#[tokio::test]
async fn a_stored_figure_that_fails_validate_gives_no_figure_key() {
    let (status, reply) = served("broken@example.com", page(Some(json!(1)))).await;
    assert_eq!(status, StatusCode::OK, "{reply}");
    assert!(reply["worked_example"].get("figure").is_none(), "{reply}");
}

/// The reviewer who approves "figure 0" reads what figure 0 shows.
#[tokio::test]
async fn the_admin_show_reply_of_a_teach_row_with_a_figure_carries_figure_text() {
    TestDb::with(|db| async move {
        let point = {
            let mut point = kp("kp1", vec![exemplar("Compute $7 - 2$.", "5")]);
            point.visuals = vec![good_figure()];
            point
        };
        let graph = one_unit_curriculum(vec![topic("band", vec![point])]);
        let app = cadus_web::create_app(
            state_with_content(&db, graph)
                .with_admin(Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS)),
        );
        seed_admin(&db).await;
        let mut with_figure = Seed::template("teach-with-figure", "band/kp1", "pending");
        with_figure.kind = "teach";
        with_figure.body = page(Some(json!(0)));
        seed_row(&db, &with_figure).await;
        let mut plain = Seed::template("teach-plain", "band/kp1", "pending");
        plain.kind = "teach";
        plain.body = page(None);
        seed_row(&db, &plain).await;

        let shown = admin_get(&app, "/api/admin/content/teach-with-figure").await;
        assert_eq!(shown.status.as_u16(), 200, "{}", shown.body);
        assert_eq!(
            shown.body["figure_text"],
            json!(good_figure().text_equivalent())
        );
        let shown = admin_get(&app, "/api/admin/content/teach-plain").await;
        assert_eq!(shown.status.as_u16(), 200, "{}", shown.body);
        assert!(shown.body.get("figure_text").is_none());
    })
    .await;
}
