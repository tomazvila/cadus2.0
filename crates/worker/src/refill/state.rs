//! The verdicts and the backoff map one worker process remembers.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use sqlx::types::Uuid;

/// The gate verdicts one worker process remembers.
///
/// The gate walks up to
/// [`GATE_SAMPLES`](cadus_core::template::GATE_SAMPLES) instances, so it is far
/// too heavy to run once per refill. The verdict belongs to the document, and the
/// document is content-addressed (C6), so one verdict per digest is exact.
#[derive(Debug, Default)]
pub struct RefillState {
    /// Digests the gate accepted.
    accepted: HashSet<String>,
    /// Digests the gate refused, with the reason it wrote.
    refused: HashMap<String, String>,
    /// Pairs with no fillable source, and the instant each one is tried again.
    starved: HashMap<(Uuid, String), Instant>,
    /// Pairs whose last fills inserted no row, and how many in a row.
    ///
    /// A fill that inserts a row removes the pair from this map, so the count is
    /// the CONSECUTIVE count and never a running total (finding #8).
    empty_fills: HashMap<(Uuid, String), u32>,
    /// Pairs whose source ran dry, so `operator_flags` names them (A6).
    exhausted: HashSet<(Uuid, String)>,
}

impl RefillState {
    /// Build an empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The count of digests the cache holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.accepted.len().saturating_add(self.refused.len())
    }

    /// Whether the cache holds no verdict.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.accepted.is_empty() && self.refused.is_empty()
    }

    /// The refusal reason of a digest the gate rejected.
    #[must_use]
    pub fn refusal(&self, digest: &str) -> Option<&str> {
        self.refused.get(digest).map(String::as_str)
    }

    /// The count of pairs the backoff map holds.
    #[must_use]
    pub fn starved_len(&self) -> usize {
        self.starved.len()
    }

    /// Whether one pair is out of the target list at `now`.
    #[must_use]
    pub fn is_starved(&self, user_id: Uuid, kp_id: &str, now: Instant) -> bool {
        self.starved
            .get(&(user_id, kp_id.to_string()))
            .is_some_and(|until| *until > now)
    }

    /// The count of pairs whose source ran dry.
    #[must_use]
    pub fn exhausted_len(&self) -> usize {
        self.exhausted.len()
    }

    /// Whether the source of one pair ran dry (finding #8).
    #[must_use]
    pub fn is_exhausted(&self, user_id: Uuid, kp_id: &str) -> bool {
        self.exhausted.contains(&(user_id, kp_id.to_string()))
    }

    /// The serving keys whose source ran dry, sorted and without a repeat.
    ///
    /// The list is the `exhausted` argument of
    /// `cadus_store::pool::operator_flags_with_exhausted`, whose row is per
    /// knowledge point and not per pair: one exhausted learner is enough to
    /// flag the knowledge point, because the cure is authored content (A6).
    #[must_use]
    pub fn exhausted_kps(&self) -> Vec<String> {
        let mut keys: Vec<String> = self
            .exhausted
            .iter()
            .map(|(_, kp_id)| kp_id.clone())
            .collect();
        keys.sort();
        keys.dedup();
        keys
    }

    /// Whether the gate refused this digest in an earlier pass.
    pub(super) fn is_refused(&self, digest: &str) -> bool {
        self.refused.contains_key(digest)
    }

    /// Whether the gate accepted this digest in an earlier pass.
    pub(super) fn is_accepted(&self, digest: &str) -> bool {
        self.accepted.contains(digest)
    }

    /// Remember that the gate accepted this digest.
    pub(super) fn accept(&mut self, digest: &str) {
        self.accepted.insert(digest.to_string());
    }

    /// Remember that the gate refused this digest, with the reason it wrote.
    pub(super) fn refuse(&mut self, digest: &str, reason: &str) {
        self.refused.insert(digest.to_string(), reason.to_string());
    }

    /// Put one pair out of the target list for `backoff`.
    pub(super) fn starve(&mut self, user_id: Uuid, kp_id: &str, now: Instant, backoff: Duration) {
        let until = now.checked_add(backoff).unwrap_or(now);
        self.starved.insert((user_id, kp_id.to_string()), until);
    }

    /// Count one fill of `pair` that inserted no row, and give the new count.
    pub(super) fn note_empty_fill(&mut self, user_id: Uuid, kp_id: &str) -> u32 {
        let count = self
            .empty_fills
            .entry((user_id, kp_id.to_string()))
            .or_insert(0);
        *count = count.saturating_add(1);
        *count
    }

    /// Mark the source of one pair dry, so `operator_flags` names it (A6).
    pub(super) fn exhaust(&mut self, user_id: Uuid, kp_id: &str) {
        self.exhausted.insert((user_id, kp_id.to_string()));
    }

    /// Clear both empty-fill records of one pair after a fill that wrote a row.
    pub(super) fn note_filled(&mut self, user_id: Uuid, kp_id: &str) {
        let key = (user_id, kp_id.to_string());
        self.empty_fills.remove(&key);
        self.exhausted.remove(&key);
    }

    /// Drop every backoff entry whose period ended, and list the ones that hold.
    pub(super) fn active_starved(&mut self, now: Instant) -> Vec<(Uuid, String)> {
        self.starved.retain(|_, until| *until > now);
        let mut pairs: Vec<(Uuid, String)> = self.starved.keys().cloned().collect();
        pairs.sort();
        pairs
    }
}
