//! `GET /api/topics/{topic_id}/teach`: the approved lesson text of one topic.
//!
//! A read of the approved lesson text by topic. It writes nothing. D-O3 covers the
//! content read, and D-TR1 allows the read outside a task. The route appends no
//! event, writes no `web_states` row, and calls no model. It reads one approved
//! row per knowledge point and the curriculum in memory. It uses the same approval
//! read as the task teach route, so an unapproved page never leaves the service.

use axum::http::StatusCode;
use cadus_core::curriculum::{Curriculum, TopicIdx};
use cadus_core::pool::kp_key;
use serde_json::{Value, json};

use super::store::{Ready, Reply, reply_read};
use crate::error::{ApiError, NOT_FOUND};
use crate::path::ApiPath;
use crate::serve::{NO_INSTRUCTION, teach_figure, teach_page};

/// The id and the name of one topic, as a link.
fn link(graph: &Curriculum, idx: TopicIdx) -> Value {
    json!({
        "id": graph.id_of(idx),
        "name": graph.topic(idx).map(|topic| topic.name.as_str()),
    })
}

/// The link to the topic at `place` in `order`, or `null` past either end.
fn neighbor(graph: &Curriculum, order: &[TopicIdx], place: Option<usize>) -> Value {
    place
        .and_then(|at| order.get(at))
        .map_or(Value::Null, |idx| link(graph, *idx))
}

/// Answer the approved teach page of every knowledge point of `topic_id`.
///
/// `404 not_found` when the curriculum holds no such topic. `409 no_instruction`
/// when no knowledge point of the topic has an approved teach page. A point with no
/// approved page answers `page: null` when a sibling point has one.
pub async fn topic_teach(req: Ready, ApiPath(topic_id): ApiPath<String>) -> Reply {
    let graph = req.graph();
    let Some(idx) = graph.idx_of(&topic_id) else {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            NOT_FOUND,
            format!("No topic {topic_id:?} is in the curriculum."),
        ));
    };

    // A read transaction. It takes no advisory lock and folds no projection,
    // because the route reads no learner state.
    let mut tx = req.begin().await?;
    let mut points = Vec::new();
    let mut approved = 0_usize;
    for point in graph.knowledge_points(idx) {
        let key = kp_key(&topic_id, point.id.as_str());
        let page = match teach_page(&req.state, &req.content, &mut tx, &key).await {
            Ok(page) => {
                approved += 1;
                let mut example = json!({
                    "problem": page.worked_example.problem,
                    "steps": page.worked_example.steps,
                });
                if let Some(figure) = teach_figure(graph, &key, &page) {
                    example["figure"] = json!(figure);
                }
                json!({"concept": page.concept, "worked_example": example})
            }
            Err(error) if error.code == NO_INSTRUCTION => Value::Null,
            Err(error) => return Err(error),
        };
        points.push(json!({"kp": point.id.as_str(), "name": point.name, "page": page}));
    }
    if approved == 0 {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            NO_INSTRUCTION,
            "This topic has no approved lesson text yet.",
        ));
    }

    // The position in the course, by load index.
    let course = graph.course_of(idx);
    let order = graph.topics_in_course(course);
    let place = order.iter().position(|other| *other == idx);
    let links = |ids: &mut dyn Iterator<Item = TopicIdx>| -> Vec<Value> {
        ids.map(|other| link(graph, other)).collect()
    };
    let body = json!({
        "topic": topic_id,
        "name": graph.topic(idx).map(|topic| topic.name.as_str()),
        "module": graph.module_of(idx),
        "course": course,
        "course_name": graph.course(course).map(|found| found.name.as_str()),
        "position": place.map_or(0, |at| at + 1),
        "count": order.len(),
        "previous": neighbor(graph, order, place.and_then(|at| at.checked_sub(1))),
        "next": neighbor(graph, order, place.map(|at| at + 1)),
        "learn_first": links(&mut graph.prerequisites(idx)),
        "opens_next": links(&mut graph.dependents(idx)),
        "points": points,
    });
    reply_read(tx, body).await
}
