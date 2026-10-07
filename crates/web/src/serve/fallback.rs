//! The draw of one problem when the chosen knowledge point has nothing to give.
//!
//! The knowledge point choice names a point that authors practice, but its pool
//! can still come up empty at the draw: the recency filter hides every row, or
//! the point's approved content is gone. A session never stops there. The order
//! is: draw as the plan chose; draw again with the recency filter relaxed; draw
//! from a sibling knowledge point of the same topic. Each step past the first
//! logs the point and the filter counts. Only a topic where nothing at all can
//! be served gives the learner a refusal.

use super::route::{FreshPolicy, draw_fresh};
use super::*;
use cadus_core::pool::{Ring, TaskMemory};

/// Draw a problem for `target`, moving `target` to a sibling point when the
/// chosen point serves nothing.
///
/// A retention probe, a pending practice and a feedback re-draw keep their own
/// rule (an unseen item, or an explicit block), so they take no fallback.
pub(super) async fn draw_servable(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    content: &Content,
    target: &mut Target,
    windows: (Ring, TaskMemory),
    policy: FreshPolicy<'_>,
) -> Result<(PoolRow, bool), ApiError> {
    let (ring_hashes, memory_hashes) = (windows.0.hashes().len(), windows.1.hashes().len());
    let refusal = match draw_fresh(state, tx, user_id, content, target, windows, policy).await {
        Err(error)
            if error.code == POOL_UNAVAILABLE
                && policy.feedback.is_none()
                && policy.probe_seen.is_none() =>
        {
            error
        }
        settled => return settled,
    };
    let relaxed = || (Ring::default(), TaskMemory::default());
    if let Ok(drawn) = draw_fresh(state, tx, user_id, content, target, relaxed(), policy).await {
        tracing::warn!(
            user_id = %user_id,
            kp_id = %target.key,
            ring_hashes,
            memory_hashes,
            "serve: the recency filter hid every item; it was relaxed for this draw"
        );
        return Ok(drawn);
    }
    for kp in sibling_kps(&content.curriculum, target) {
        let sibling = Target::new(target.record.clone(), target.serve.clone(), kp);
        if let Ok(drawn) =
            draw_fresh(state, tx, user_id, content, &sibling, relaxed(), policy).await
        {
            tracing::warn!(
                user_id = %user_id,
                kp_id = %target.key,
                served_kp = %sibling.key,
                ring_hashes,
                memory_hashes,
                "serve: the chosen point serves nothing; a sibling point of the topic serves instead"
            );
            *target = sibling;
            return Ok(drawn);
        }
    }
    Err(refusal)
}

/// The other knowledge points of the target's topic: the ones after the chosen
/// point first, then the ones before it.
fn sibling_kps(graph: &Curriculum, target: &Target) -> Vec<String> {
    let Some(idx) = graph.idx_of(&target.serve) else {
        return Vec::new();
    };
    let ids: Vec<&str> = graph
        .knowledge_points(idx)
        .iter()
        .map(|kp| kp.id.as_str())
        .collect();
    let at = ids.iter().position(|id| *id == target.kp).unwrap_or(0);
    ids[at.saturating_add(1).min(ids.len())..]
        .iter()
        .chain(ids[..at].iter())
        .map(|id| (*id).to_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::fixture::{arena, topic_doc};
    use super::*;

    /// The retry order: the points after the chosen one, then the ones before
    /// it, and never the chosen point itself.
    #[test]
    fn the_siblings_follow_the_chosen_point_and_exclude_it() {
        let graph = arena(&[topic_doc(
            "counting",
            &[("kp1", &["1"]), ("kp2", &["2"]), ("kp3", &["3"])],
        )]);
        let at = |kp: &str| Target::new("counting".into(), "counting".into(), kp.into());
        assert_eq!(sibling_kps(&graph, &at("kp2")), ["kp3", "kp1"]);
        assert_eq!(sibling_kps(&graph, &at("kp3")), ["kp1", "kp2"]);
        assert!(sibling_kps(&graph, &Target::new("x".into(), "x".into(), "kp1".into())).is_empty());
    }
}
