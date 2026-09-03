//! Selector part 7: the differential check of the compression against a literal transcription of 1.0.
//!
//! The selector of spec section 6, pinned against the ORDER assertions of the
//! 1.0 suite (`tests/test_selector.py`, `tests/test_gap_fill.py`).
//!
//! Every expected value here is a LITERAL taken from the 1.0 test that pins it.
//! Nothing is re-derived from the code under test.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::{BTreeMap, BTreeSet};

use cadus_core::config::Config;
use cadus_core::curriculum::Curriculum;
use cadus_core::learner::TopicState;
use cadus_core::selector::{
    compress, due_reviews, frontier, in_retry_delay, mastered_set, nearly_due,
};
use common::T_US;
use common::selector::{Rng, cfg, pool_of, random_states, real_curriculum};

// --------------------------------------------------------------------------- //
// compress — differential check against a literal transcription of 1.0
// --------------------------------------------------------------------------- //

/// `_covers(candidate, pool)` of `selector.py:500-504`, transcribed literally.
///
/// It asks the arena for `W(candidate -> due)` once per pair, the way 1.0 asks
/// the graph. It is deliberately the slow, obvious form.
fn naive_covers(
    candidate: &str,
    pool: &BTreeSet<String>,
    graph: &Curriculum,
    cfg: &Config,
) -> BTreeSet<String> {
    pool.iter()
        .filter(|due| {
            due.as_str() != candidate
                && graph.encompassing_weight_by_id(candidate, due) >= cfg.fire.knockout_weight
        })
        .cloned()
        .collect()
}

/// `compress` of `selector.py:506-590`, transcribed literally.
fn naive_compress(
    due: &[String],
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    cfg: &Config,
    t_us: i64,
) -> (Vec<String>, BTreeMap<String, Vec<String>>) {
    let due_set: BTreeSet<String> = due
        .iter()
        .filter(|id| graph.idx_of(id).is_some())
        .cloned()
        .collect();
    if due_set.is_empty() {
        return (Vec::new(), BTreeMap::new());
    }
    let mastered = mastered_set(states, graph);
    let mut free: BTreeSet<String> = frontier(graph, &mastered)
        .sorted_ids(graph)
        .into_iter()
        .filter(|id| !in_retry_delay(states.get(*id).unwrap_or(&TopicState::default()), cfg, t_us))
        .map(ToOwned::to_owned)
        .collect();
    free.extend(nearly_due(states, graph, cfg, t_us));
    let free_candidates: Vec<String> = free
        .into_iter()
        .filter(|id| !due_set.contains(id))
        .collect();

    let mut knockouts: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut uncovered = due_set;
    loop {
        if uncovered.is_empty() {
            break;
        }
        let mut best: Option<String> = None;
        let mut best_cov: BTreeSet<String> = BTreeSet::new();
        for candidate in &free_candidates {
            let cov = naive_covers(candidate, &uncovered, graph, cfg);
            if cov.len() > best_cov.len() {
                best = Some(candidate.clone());
                best_cov = cov;
            }
        }
        let Some(winner) = best else { break };
        if best_cov.is_empty() {
            break;
        }
        knockouts.insert(winner, best_cov.iter().cloned().collect());
        uncovered.retain(|id| !best_cov.contains(id));
    }

    let mut surviving: Vec<String> = Vec::new();
    let mut remaining = uncovered;
    while !remaining.is_empty() {
        let mut best: Option<String> = None;
        let mut best_cov: BTreeSet<String> = BTreeSet::new();
        for candidate in &remaining {
            let mut cov = naive_covers(candidate, &remaining, graph, cfg);
            cov.insert(candidate.clone());
            if cov.len() > best_cov.len() {
                best = Some(candidate.clone());
                best_cov = cov;
            }
        }
        let Some(winner) = best else { break };
        let others: Vec<String> = best_cov
            .iter()
            .filter(|id| *id != &winner)
            .cloned()
            .collect();
        if !others.is_empty() {
            knockouts.insert(winner.clone(), others);
        }
        surviving.push(winner);
        remaining.retain(|id| !best_cov.contains(id));
    }
    surviving.sort_unstable();
    (surviving, knockouts)
}

#[test]
fn compress_matches_the_literal_1_0_transcription() {
    let graph = real_curriculum();
    let cfg = cfg();
    let pool = pool_of(&graph);
    let mut rng = Rng::new(4_242_026);
    for example in 0..60 {
        let states = random_states(&pool, &mut rng);
        let due = due_reviews(&states, &graph, &cfg, T_US, &BTreeSet::new());
        let fast = compress(&due, &states, &graph, &cfg, T_US, None);
        let (surviving, knockouts) = naive_compress(&due, &states, &graph, &cfg, T_US);
        assert_eq!(fast.surviving, surviving, "example {example}: surviving");
        assert_eq!(fast.knockouts, knockouts, "example {example}: knockouts");
    }
}
