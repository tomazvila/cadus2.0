//! Whole-item timing is server-owned evidence, independent of reasoning correctness.
use crate::state::WebState;
use cadus_core::{
    curriculum::Curriculum,
    event::{IntegratedAttempt, Secs, Timestamp},
    integrated::IntegratedItem,
    timing::{TimingFacts, read},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Persisted whole-item clock. A re-serve marks the interval interrupted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimingWindow {
    /// The first server hand-off.
    pub started_at: Timestamp,
    /// Whether a subsequent hand-off broke continuous timing.
    #[serde(default)]
    pub interrupted: bool,
}

fn key(item: &IntegratedItem, task_id: &str) -> String {
    format!("{task_id}:{}", item.digest())
}

pub(super) fn start(scratch: &mut WebState, item: &IntegratedItem, task_id: &str, now: Timestamp) {
    scratch
        .integrated_timing
        .entry(key(item, task_id))
        .and_modify(|clock| clock.interrupted = true)
        .or_insert(TimingWindow {
            started_at: now,
            interrupted: false,
        });
}

pub(super) fn record(
    record: &mut IntegratedAttempt,
    item: &IntegratedItem,
    graph: &Curriculum,
    scratch: &WebState,
    task_id: &str,
    now: Timestamp,
) {
    let clock = scratch.integrated_timing.get(&key(item, task_id));
    let elapsed = clock.map_or(0, |clock| {
        now.micros()
            .saturating_sub(clock.started_at.micros())
            .max(0)
            / 1_000_000
    });
    // The item integrates its component topics; their authored budgets add up.
    let expected = item
        .component_topics
        .iter()
        .try_fold(0_i64, |sum, id| {
            let secs = graph
                .idx_of(id.as_str())
                .and_then(|idx| graph.topic(idx))?
                .expected_time_secs;
            (secs > 0).then(|| sum.saturating_add(secs))
        })
        .unwrap_or(0);
    let ungraded = record.final_field.outcome.is_ungraded()
        || record.steps.iter().any(|step| step.outcome.is_ungraded());
    let reading = read(&TimingFacts {
        elapsed_secs: elapsed,
        expected_secs: expected,
        correct: record.solved,
        assisted: record.assisted,
        routine: false,
        interrupted: ungraded || clock.is_none_or(|clock| clock.interrupted),
    });
    record.secs = Secs::new(elapsed).ok();
    record.timing_reliable = Some(reading.ratio.is_some());
    record.timing = Some(reading);
}

pub(super) fn payload(payload: &mut Value, record: &IntegratedAttempt) {
    payload["timing"] = json!(record.timing);
    payload["timing_reliable"] = json!(record.timing_reliable);
}
