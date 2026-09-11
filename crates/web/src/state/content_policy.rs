//! Current trusted policy identity for web approval and serving reads.
use super::{ApiError, Content};

impl Content {
    /// The current trusted policy fingerprint for a topic-qualified knowledge point.
    ///
    /// # Errors
    /// Returns a source serialization error instead of accepting stale policy.
    pub fn policy_digest(&self, kp_id: &str) -> Result<Option<String>, ApiError> {
        let policy = cadus_core::pool::split_kp_key(kp_id).and_then(|(topic, point)| {
            let topic_idx = self.curriculum.idx_of(topic)?;
            let kp_idx = self.curriculum.kp_idx_of(topic_idx, point)?;
            self.curriculum
                .knowledge_point(topic_idx, kp_idx)?
                .finite_objective_domain
                .as_ref()
        });
        policy
            .map(|policy| policy.fingerprint(kp_id))
            .transpose()
            .map_err(|_| ApiError::internal("The current exercise policy could not be read."))
    }
}
