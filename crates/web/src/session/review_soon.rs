//! `POST /api/topics/{topic_id}/review-soon`: bring one topic's next review forward.
//!
//! The request is a planning input. It appends no event: the store keeps the instant
//! in the learner settings, and the planner reads the topic as due until the learner
//! practices it again.

use axum::http::StatusCode;
use cadus_core::fire::has_review_history;
use cadus_store::state::request_review_soon;
use serde_json::json;

use super::store::{Ready, Reply, reply_committed};
use crate::error::{ApiError, NOT_FOUND};
use crate::path::ApiPath;

/// The code of a review request for a topic the learner has not practiced.
const NOT_LEARNED: &str = "not_learned";

/// Mark `topic_id` as due at the next plan.
///
/// `404` when the curriculum holds no such topic. `409 not_learned` when the learner
/// has no state on the topic yet.
pub async fn review_soon(req: Ready, ApiPath(topic_id): ApiPath<String>) -> Reply {
    if req.graph().idx_of(&topic_id).is_none() {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            NOT_FOUND,
            format!("No topic {topic_id:?} is in the curriculum."),
        ));
    }
    let input = req.input();
    let (mut tx, projection) = req.locked_projection(&input).await?;
    let learned = projection
        .model
        .topics
        .get(&topic_id)
        .is_some_and(has_review_history);
    if !learned {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            NOT_LEARNED,
            "Learn this topic first.",
        ));
    }
    req.store(request_review_soon(
        &mut tx,
        req.user_id,
        &topic_id,
        req.now.micros(),
    ))
    .await?;
    reply_committed(tx, json!({"topic": topic_id, "review_soon": true})).await
}
