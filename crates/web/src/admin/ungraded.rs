//! `/api/admin/ungraded*` — the recovery path of the third outcome (D-F2).
//!
//! An UNGRADED attempt moves no learner state, so a knowledge point that only
//! ever collects ungraded attempts never advances. The two routes here close
//! that hole:
//!
//! | Method and path | What it answers |
//! |---|---|
//! | `GET /api/admin/ungraded` | the last ungraded attempts of the learner, oldest first |
//! | `POST /api/admin/ungraded/{attempt_id}/regrade` | `{attempt_id, outcome, replayed}` |
//!
//! The regrade appends a `regraded` event and never edits the original row (C2).
//! A `regraded` event in the log forces the whole-log replay, so the model the
//! next request reads already holds the corrected grade (D-O6).
//!
//! Both routes read the tenant of the request. The deployment has one learner
//! (owner decision O2), and row-level security binds every read to that tenant,
//! so an admin reads the learner they are signed in as and no other (C3).

use axum::Json;
use cadus_core::event::{
    AttemptOutcome, Event, Regraded, RegradedAttempt, SchemaVersion, Slug, WorkQuality,
};
use cadus_core::learner::UNGRADED_WINDOW;
use cadus_store::state::load_events;
use serde_json::{Value, json};

use super::{AdminUser, unknown_ungraded};
use crate::auth::body::LimitedBody;
use crate::error::ApiError;
use crate::path::ApiPath;
use crate::session::{Ready, Reply, db_failed, reply_read};

/// The `outcome` key the regrade body carries.
pub const OUTCOME_FIELD: &str = "outcome";

/// What a regrade body must hold.
pub const OUTCOME_MESSAGE: &str = "The body needs an `outcome` of \"correct\" or \"incorrect\". A human grades an attempt the \
     checker could not decide, and no other value is a grade.";

/// What an unknown or already decided attempt id gets.
pub const NOT_UNGRADED_MESSAGE: &str = "The log holds no ungraded attempt with that id. A decided attempt keeps the verdict the \
     checker reached (C4).";

/// The note every human regrade of this path stamps.
pub const REGRADE_NOTE: &str = "admin regrade of an ungraded attempt";

/// Why the correction happened, as the log records it.
pub const REGRADE_REASON: &str = "a human graded an attempt the checker could not decide";

/// `GET /api/admin/ungraded` — the ungraded attempts of the learner (D-F2).
///
/// The list holds at most [`UNGRADED_WINDOW`] entries, oldest first, and the fold
/// rebuilds it from the whole log on every read.
///
/// # Errors
///
/// - `401 unauthorized` — the request carries no live session.
/// - `403 forbidden` — the account is not an admin.
/// - `500 internal_error` — the fold or a statement failed.
pub async fn list_ungraded(AdminUser(_authed): AdminUser, req: Ready) -> Reply {
    let input = req.input();
    let mut tx = req.begin().await?;
    let projection = req
        .store(cadus_store::state::project_current(
            &mut tx,
            req.user_id,
            &input,
        ))
        .await?;
    let items: Vec<Value> = projection
        .model
        .ungraded
        .iter()
        .map(|entry| {
            json!({
                "attempt_id": entry.attempt_id,
                "topic": entry.topic,
                "reason": entry.reason,
            })
        })
        .collect();
    let body = json!({ "items": items, "limit": UNGRADED_WINDOW });
    reply_read(tx, body).await
}

/// The outcome one regrade body names.
fn outcome_of(body: &[u8]) -> Result<AttemptOutcome, ApiError> {
    let document: Value = serde_json::from_slice(body)
        .map_err(|_| ApiError::invalid_request(OUTCOME_MESSAGE.to_string()))?;
    match document.get(OUTCOME_FIELD).and_then(Value::as_str) {
        Some("correct") => Ok(AttemptOutcome::Correct),
        Some("incorrect") => Ok(AttemptOutcome::Incorrect),
        _ => Err(ApiError::invalid_request(OUTCOME_MESSAGE.to_string())),
    }
}

/// The tier a human verdict prices, matching the deterministic tiers (D-M5-2).
const fn tier_of(outcome: &AttemptOutcome) -> WorkQuality {
    match *outcome {
        AttemptOutcome::Correct => WorkQuality::NearlyPerfect,
        AttemptOutcome::Incorrect | AttemptOutcome::Ungraded { .. } => WorkQuality::NearlyPassable,
    }
}

/// The task id and the topic of the ungraded attempt `attempt_id` holds.
fn target_of(rows: &[cadus_store::state::EventRow], attempt_id: &str) -> Option<(String, Slug)> {
    rows.iter().find_map(|row| match &row.event {
        Event::Attempt(body) if body.attempt_id == attempt_id && body.outcome.is_ungraded() => {
            Some((body.task_id.clone(), body.topic.clone()))
        }
        _ => None,
    })
}

/// `POST /api/admin/ungraded/{attempt_id}/regrade` — give one ungraded attempt
/// the verdict a human reached (C2, D-F2).
///
/// The route appends a `regraded` event and edits nothing. The append forces the
/// whole-log replay, so the answer reports the model the correction produced.
///
/// # Errors
///
/// - `400 invalid_request` — the body names no `outcome` of `correct` or
///   `incorrect`.
/// - `401 unauthorized` — the request carries no live session.
/// - `403 forbidden` — the account is not an admin.
/// - `404 not_found` — the log holds no ungraded attempt with that id.
/// - `500 internal_error` — the append, the fold, or a statement failed.
pub async fn regrade_ungraded(
    AdminUser(_authed): AdminUser,
    ApiPath(attempt_id): ApiPath<String>,
    req: Ready,
    LimitedBody(body): LimitedBody,
) -> Reply {
    let outcome = outcome_of(&body)?;
    let input = req.input();
    let (mut tx, _projection) = req.locked_projection(&input).await?;
    let rows = req.store(load_events(&mut tx, req.user_id)).await?;
    let Some((task_id, topic)) = target_of(&rows, &attempt_id) else {
        tx.rollback().await.ok();
        return Err(unknown_ungraded());
    };
    let event = Event::Regraded(Regraded {
        ts: req.now,
        session: None,
        v: SchemaVersion::current(),
        task_id,
        topic,
        attempts: vec![RegradedAttempt {
            attempt_id: attempt_id.clone(),
            outcome: Some(outcome.clone()),
            work_quality: tier_of(&outcome),
            error_tags: Vec::new(),
            grader_note: Some(REGRADE_NOTE.to_string()),
        }],
        quality_tier: None,
        xp: None,
        reason: REGRADE_REASON.to_string(),
    });
    let projection = req.append_and_fold(&mut tx, &event, &input).await?;
    let standing = projection.model.ungraded.len();
    tx.commit().await.map_err(db_failed)?;
    Ok(Json(json!({
        "attempt_id": attempt_id,
        "outcome": outcome.as_str(),
        "replayed": true,
        "ungraded": standing,
    })))
}
