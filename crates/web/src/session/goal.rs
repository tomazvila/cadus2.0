//! `POST /api/goal/xp`: the learner sets the daily XP goal (D-XP1).
//!
//! The route appends one `goal_set` event and folds it in ONE transaction under the
//! tenant's advisory lock. The fold applies the latest goal to the whole log, so the
//! streak counts the days against the new goal at once. The route never updates or
//! deletes an event row. No model call stands on this path.

use axum::Json;
use axum::http::StatusCode;
use cadus_core::event::{Event, GoalSet, SchemaVersion};
use cadus_core::projector::{MAX_XP_GOAL, MIN_XP_GOAL};
use serde_json::{Value, json};

use super::store::{Ready, Reply, json_of, reply_committed};
use crate::error::ApiError;
use crate::state::INVALID_REQUEST;

/// The `goal` field of the body, when it is a whole number from 10 to 100.
fn asked_goal(body: Option<&Json<Value>>) -> Option<i64> {
    body.and_then(|Json(value)| value.get("goal"))
        .and_then(Value::as_i64)
        .filter(|goal| (MIN_XP_GOAL..=MAX_XP_GOAL).contains(goal))
}

/// Set the daily XP goal. `422 invalid_request` when the goal is absent, not a whole
/// number, below 10, or above 100.
pub async fn set_xp_goal(req: Ready, body: Option<Json<Value>>) -> Reply {
    let Some(goal) = asked_goal(body.as_ref()) else {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            INVALID_REQUEST,
            "Type a whole number from 10 to 100.",
        ));
    };
    let input = req.input();
    let (mut tx, before) = req.locked_projection(&input).await?;
    let event = Event::GoalSet(GoalSet {
        ts: req.now,
        session: before.view.current_session.clone(),
        v: SchemaVersion::current(),
        goal,
    });
    let projection = req.append_and_fold(&mut tx, &event, &input).await?;
    let body = json!({"goal": goal, "xp": json_of(&projection.model.xp)});
    reply_committed(tx, body).await
}
