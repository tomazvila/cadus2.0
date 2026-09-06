//! Common framework imports for task-scoped HTTP handlers.

pub(crate) use axum::{Json, extract::State, http::StatusCode};
pub(crate) use serde_json::{Value, json};
pub(crate) use sqlx::{Postgres, Transaction, types::Uuid};

pub(crate) use crate::AppState;
pub(crate) use crate::error::ApiError;
pub(crate) use crate::grade::{db_failed, route_input, store};
pub(crate) use crate::path::{ApiPath, TaskContext, TaskWithBody};

/// Unpack a task request and stamp the request instant once.
pub(crate) fn task_request(
    ((State(state), Tenant(user_id), ApiPath(task_id)), raw): TaskWithBody,
) -> (AppState, Uuid, String, Option<Json<Value>>, Timestamp) {
    let (_, now) = crate::session::now_pair();
    (state, user_id, task_id, raw, now)
}

use crate::state::Tenant;
use cadus_core::event::Timestamp;
