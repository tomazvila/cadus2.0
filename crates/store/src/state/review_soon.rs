//! The learner's "review this topic soon" requests, and the latest graded answer.
//!
//! A request lives in `user_settings.settings.review_soon`, a map from topic id to the
//! request instant in microseconds. It is a planning input only: the saved learner
//! model never carries it. [`apply_review_soon`] marks the topic as due in the model
//! that the planner reads. The mark ends by itself when a later practice of the topic
//! moves `t0` past the request instant.

use cadus_core::event::Timestamp;
use cadus_core::fire::has_review_history;
use cadus_core::learner::LearnerModel;
use serde_json::Value;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::StoreError;

/// Record that the learner wants `topic` reviewed at the next plan.
///
/// A second request for the same topic moves the instant forward.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn request_review_soon(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    topic: &str,
    now_us: i64,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO user_settings (user_id, settings) \
         VALUES ($1, jsonb_build_object('review_soon', jsonb_build_object($2::text, $3::bigint))) \
         ON CONFLICT (user_id) DO UPDATE SET \
             settings = jsonb_set(user_settings.settings, '{review_soon}', \
                 COALESCE(user_settings.settings->'review_soon', '{}'::jsonb) \
                 || jsonb_build_object($2::text, $3::bigint), true), \
             updated_at = now()",
    )
    .bind(user_id)
    .bind(topic)
    .bind(now_us)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Mark every requested topic as due in `model`, for the planner that reads it.
///
/// A topic is marked when it has review history and its last practice is older than the
/// request. The mark zeroes `memory_base`, which puts the topic under the due threshold.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub(super) async fn apply_review_soon(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    model: &mut LearnerModel,
) -> Result<(), StoreError> {
    let row: Option<Option<Value>> =
        sqlx::query_scalar("SELECT settings->'review_soon' FROM user_settings WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(&mut **tx)
            .await?;
    let Some(Some(Value::Object(requests))) = row else {
        return Ok(());
    };
    for (topic, at) in requests {
        let Some(requested_us) = at.as_i64() else {
            continue;
        };
        let Some(state) = model.topics.get_mut(&topic) else {
            continue;
        };
        let practiced_us = state.t0.map_or(i64::MIN, Timestamp::micros);
        if has_review_history(state) && practiced_us < requested_us {
            state.memory_base = 0.0;
        }
    }
    Ok(())
}

/// The topic and instant of the learner's latest graded answer, if any.
///
/// An ungraded attempt does not count. The instant is the event time in microseconds since the Unix epoch.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn latest_graded_topic(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> Result<Option<(String, i64)>, StoreError> {
    let row: Option<(Option<String>, i64)> = sqlx::query_as(
        "SELECT payload->>'topic', (extract(epoch FROM ts) * 1000000)::bigint FROM events \
         WHERE user_id = $1 AND type = 'attempt' \
           AND payload->>'topic' IS NOT NULL \
           AND NOT COALESCE(payload->'outcome' ? 'ungraded', false) \
         ORDER BY seq DESC LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.and_then(|(topic, ts)| topic.map(|name| (name, ts))))
}
