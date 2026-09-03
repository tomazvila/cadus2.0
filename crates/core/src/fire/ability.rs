//! The ability model: the neighborhood seed and the EWMA update of an attempt
//! (`fire.py:369-401`).

use std::collections::BTreeMap;

use indexmap::IndexMap;

use crate::config::Config;
use crate::curriculum::Curriculum;
use crate::event::TopicStatus;
use crate::learner::TopicState;
use crate::numeric::neumaier_sum;

use super::NEUTRAL_ABILITY;

/// The difficulty of a topic, or `0.5` when the curriculum has no such topic
/// (`projector.py:585-587`).
#[must_use]
pub fn difficulty(graph: &Curriculum, topic: &str) -> f64 {
    graph
        .idx_of(topic)
        .and_then(|idx| graph.topic(idx))
        .map_or(0.5, |found| found.difficulty)
}

/// The ability seed of an untouched topic: the mean ability of its touched
/// neighbors (`fire.py:379-393`).
///
/// Only a neighbor that is present in `states` AND is not `untouched` counts.
/// With no such neighbor there is no local evidence, so the neutral prior
/// [`NEUTRAL_ABILITY`] comes back.
///
/// 1.0 sums the abilities with `sum()`, so [`neumaier_sum`] is the summation here
/// (trap T1), and the neighborhood arrives sorted (trap T5).
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "the neighbor count is far below 2**53"
)]
pub fn initial_ability(
    topic: &str,
    graph: &Curriculum,
    states: &BTreeMap<String, TopicState>,
    _cfg: &Config,
) -> f64 {
    let abilities: Vec<f64> = graph
        .neighborhood(topic)
        .into_iter()
        .filter_map(|other| states.get(other))
        .filter(|state| state.status != TopicStatus::Untouched)
        .map(|state| state.ability)
        .collect();
    if abilities.is_empty() {
        return NEUTRAL_ABILITY;
    }
    neumaier_sum(&abilities) / abilities.len() as f64
}

/// One exponential-moving-average step, expressed as a delta (`fire.py:396-398`).
fn ewma_delta(ability: f64, target: f64, alpha: f64) -> f64 {
    alpha * (target - ability)
}

/// The per-topic ability DELTAS of an attempt on `topic` (`fire.py:369-401`).
///
/// The attempted topic always takes an `alpha` step toward 1 (correct) or 0
/// (incorrect). A correct answer then propagates DOWN to encompassed topics and
/// an incorrect one propagates UP to dependents, each as a `W`-scaled step. Only
/// a topic already in `states` is adjusted.
///
/// The returned map keeps the 1.0 insertion order (trap T6): the attempted topic
/// first, then the neighbors in sorted-id order. The caller applies it in that
/// order.
#[must_use]
pub fn ability_update(
    states: &BTreeMap<String, TopicState>,
    topic: &str,
    correct: bool,
    graph: &Curriculum,
    cfg: &Config,
) -> IndexMap<String, f64> {
    let alpha = cfg.ability.ewma_alpha;
    let target = if correct { 1.0 } else { 0.0 };

    let base = states.get(topic).map_or(0.0, |state| state.ability);
    let mut deltas: IndexMap<String, f64> = IndexMap::new();
    deltas.insert(topic.to_owned(), ewma_delta(base, target, alpha));

    let weights = if correct {
        graph.reach_weights_by_id(topic)
    } else {
        graph.upward_weights_by_id(topic)
    };
    for (other, weight) in weights {
        if other == topic || weight <= 0.0 {
            continue;
        }
        let Some(state) = states.get(other) else {
            continue;
        };
        deltas.insert(
            other.to_owned(),
            ewma_delta(state.ability, target, alpha * weight),
        );
    }
    deltas
}

#[cfg(test)]
mod tests {
    use super::super::NEUTRAL_ABILITY;
    use super::super::testing::{graph, learned, topic};
    use super::*;

    #[test]
    fn the_ability_moves_the_topic_first_and_the_neighbors_next() {
        let cfg = Config::default();
        let tree = graph(vec![
            topic("a", &[]),
            topic("b", &[]),
            topic("c", &[("a", 0.8, true), ("b", 0.6, false)]),
        ]);
        let mut states: BTreeMap<String, TopicState> = BTreeMap::new();
        for id in ["a", "b"] {
            states.insert(id.to_owned(), learned(0.5));
        }
        assert_eq!(initial_ability("c", &tree, &states, &cfg), 0.5);
        assert_eq!(
            initial_ability("c", &tree, &BTreeMap::new(), &cfg),
            NEUTRAL_ABILITY
        );
        assert_eq!(difficulty(&tree, "c"), 0.3);
        assert_eq!(difficulty(&tree, "ghost"), 0.5);
        states.insert("c".to_owned(), learned(0.5));
        let deltas = ability_update(&states, "c", true, &tree, &cfg);
        let order: Vec<&str> = deltas.keys().map(String::as_str).collect();
        assert_eq!(order, ["c", "a", "b"]);
        let up = ability_update(&states, "a", false, &tree, &cfg);
        assert!(up.contains_key("c") && !up.contains_key("b"));
    }
}
