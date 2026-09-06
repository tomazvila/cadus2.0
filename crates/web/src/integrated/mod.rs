//! The integrated-task routes of D-F10: one scenario served as ONE problem, and
//! one submission graded per step and final.
//!
//! # Why this is a module of its own
//!
//! A multi-step task of 1.0 serves one independent question per component
//! topic. D-F10 keeps that task type and its name, and changes what it serves
//! when an integrated item covers the component set: the learner reads one
//! scenario, chooses a method, answers the intermediate steps, and gives one
//! final answer. The old per-component path stays for a component set no item
//! covers, so nothing that works today stops working.
//!
//! # The three routes
//!
//! | Route | What it gives |
//! |---|---|
//! | `POST /api/task/{task_id}/integrated` | the client-safe view of the item |
//! | `POST /api/task/{task_id}/integrated/hint` | ONE rung of one field's ladder |
//! | `POST /api/task/{task_id}/integrated/answer` | the grade of the whole item |
//!
//! Every one of them resolves the item from the TASK, never from a client-named
//! id, so a learner reads only the item their own plan holds.
//!
//! # Hard Rule 1
//!
//! The serve payload is [`cadus_core::integrated::view_of`], which names every
//! field it emits. No authored answer, no alternate form, no `correct` flag of a
//! method option, and no final interpretation is in it. The interpretation
//! arrives with the grade, after the learner has answered.
//!
//! # What the grade route does NOT do
//!
//! It appends no event and writes no progress. The reply carries the whole
//! outcome — per-step verdicts, the credited knowledge points, the assistance
//! flag, and the item digest — and the progression path of the web lane binds
//! that outcome to an attempt. Two owners of one write would double-count.
//!
//! # Reasoning is never graded
//!
//! A submission may carry the learner's own words about the model they built.
//! The reply repeats them under `reasoning` and marks `graded: false`. No rule
//! reads the prose, and no rule corrects it (T1).

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use cadus_core::event::TaskType;
use cadus_core::integrated::{
    FINAL_FIELD_ID, IntegratedItem, Submission, grade, hint, hints_available, view_of,
};
use cadus_core::selector::Task;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::AppState;
use crate::error::ApiError;
use crate::path::ApiPath;
use crate::serve::{Open, find, open};
use crate::session::{content, now_pair};
use crate::state::{Content, INVALID_REQUEST, Tenant};

mod route;

pub use route::{answer, hint_rung, serve};

/// The code of a task that has no integrated item to serve.
pub const NO_INTEGRATED_ITEM: &str = "no_integrated_item";

/// The code of a hint request that names no field of the item.
pub const UNKNOWN_FIELD: &str = "unknown_integrated_field";

/// The item a task serves as ONE problem, or `None` for the old path.
///
/// The rule of D-F10: a multi-step task whose component set an authored item
/// covers serves that item. Every other task type, and a component set no item
/// covers, keeps the per-component serve of `serve::target`.
#[must_use]
pub fn for_task<'content>(
    content: &'content Content,
    task: &Task,
) -> Option<&'content IntegratedItem> {
    if task.task_type != TaskType::MultiStep {
        return None;
    }
    content.integrated.for_components(&task.component_topics)
}

/// `409` for a task with no integrated item.
fn no_item() -> ApiError {
    ApiError::new(
        StatusCode::CONFLICT,
        NO_INTEGRATED_ITEM,
        "This task has no integrated problem; serve it part by part.",
    )
}

/// `400` for a body the route cannot read.
fn invalid(message: &'static str) -> ApiError {
    ApiError::new(StatusCode::BAD_REQUEST, INVALID_REQUEST, message)
}

/// One hint request: which field, and which rung of its ladder.
#[derive(Debug, Deserialize)]
struct HintRequest {
    /// The step id, or `final` for the final answer.
    field: String,
    /// The rung, counted from zero.
    #[serde(default)]
    index: usize,
}

/// The reply of one hint request.
fn hint_payload(item: &IntegratedItem, request: &HintRequest) -> Result<Value, ApiError> {
    let available = hints_available(item, &request.field);
    if available == 0 && item.step(&request.field).is_none() && request.field != FINAL_FIELD_ID {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            UNKNOWN_FIELD,
            "This integrated problem has no field with that id.",
        ));
    }
    let text = hint(item, &request.field, request.index);
    Ok(json!({
        "field": request.field,
        "index": request.index,
        "hint": text,
        "hints_available": available,
        // The count the learner has opened is what marks the field assisted at
        // grade time, so the client reports it back with the submission.
        "hints_used": text.map_or(request.index, |_| request.index + 1),
    }))
}

/// The grade reply, with the correct answers kept apart from the prose.
fn answer_payload(item: &IntegratedItem, submission: &Submission) -> Value {
    let result = grade(item, submission);
    json!({
        "item_id": result.item_id,
        "item_digest": result.item_digest,
        "method": result.method,
        "steps": result.steps,
        "final": result.final_grade,
        "correct_steps": result.correct_steps,
        "total_steps": result.total_steps,
        "solved": result.solved,
        "assisted": result.assisted,
        "ungraded": result.ungraded,
        "skills_credited": result.skills_credited,
        // The interpretation is private until the learner has answered.
        "interpretation": result.interpretation,
        // The learner's own words, kept apart from every verdict above. The
        // service records them and never marks them right or wrong.
        "reasoning": {
            "recorded": result.reasoning_recorded,
            "graded": false,
            "note": submission.reasoning,
        },
    })
}
