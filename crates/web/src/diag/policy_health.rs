//! Health check: approved documents whose policy stamp is not the live one.
//!
//! The server compares the finite-policy fingerprint stored with each approved
//! teach, hint ladder and diagnosis row to the fingerprint of the loaded tree.
//! A row that differs never serves. After the boot restamp the list is empty.

use cadus_store::Db;
use cadus_store::content::{PolicyDrift, policy_drift};

use crate::error::ApiError;
use crate::state::Content;

/// The live policy fingerprint of every knowledge point of the loaded tree.
///
/// The key is the topic-qualified knowledge point key. The value is `None`
/// for a knowledge point without a finite domain.
///
/// # Errors
/// Returns an error when a fingerprint cannot be computed.
pub fn live_policy_map(content: &Content) -> Result<Vec<(String, Option<String>)>, ApiError> {
    let mut map = Vec::new();
    for topic in content.curriculum.topics() {
        for kp in &topic.knowledge_points {
            let key = cadus_core::pool::kp_key(topic.id.as_str(), kp.id.as_str());
            let digest = content.policy_digest(&key)?;
            map.push((key, digest));
        }
    }
    Ok(map)
}

/// List the approved rows whose policy stamp differs from the live value.
///
/// # Errors
/// Returns an error when a fingerprint or the database read fails.
pub async fn stale_policy_rows(db: &Db, content: &Content) -> Result<Vec<PolicyDrift>, ApiError> {
    let live = live_policy_map(content)?;
    policy_drift(db, &live)
        .await
        .map_err(|_| ApiError::internal("The policy stamps of approved content could not be read."))
}
