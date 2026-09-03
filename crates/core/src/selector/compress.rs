//! Compression: the greedy weighted knockout set cover of the due reviews
//! (`compress`, `selector.py:478-590`).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::config::Config;
use crate::curriculum::Curriculum;
use crate::learner::TopicState;

use super::review::{in_retry_delay, nearly_due};
use super::topic_set::{ReachCache, frontier, mastered_set};

/// The result of [`compress`] (`Compression`, `selector.py:478-497`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Compression {
    /// The due topics that must be served as explicit reviews, sorted.
    pub surviving: Vec<String>,
    /// Per covering task, the sorted due topics it knocks out. It never holds
    /// the covering task itself.
    pub knockouts: BTreeMap<String, Vec<String>>,
}

/// A subset of the due list, one bit per due topic.
///
/// 1.0 rebuilds `_covers(c, uncovered)` as a `set[str]` on every greedy round
/// (`selector.py:500-504`), which is quadratic in the due count and allocates a
/// string set per candidate. The port computes each candidate's cover ONCE over
/// the whole due list and intersects the bits per round. The greedy choice is
/// the same choice: `_covers(c, pool)` is exactly `cover(c) & pool`, and the bit
/// order is the sorted due order, so the FIRST strict maximum is the same
/// candidate and every recorded knockout list is already sorted.
type DueMask = Vec<u64>;

/// An all-zero mask over `n` due topics.
fn empty_mask(n: usize) -> DueMask {
    vec![0_u64; n.div_ceil(64)]
}

/// Set the bit of one due topic.
fn set_bit(mask: &mut DueMask, index: usize) {
    if let Some(word) = mask.get_mut(index / 64) {
        *word |= 1_u64 << (index % 64);
    }
}

/// Whether the bit of one due topic is set.
fn get_bit(mask: &DueMask, index: usize) -> bool {
    mask.get(index / 64)
        .is_some_and(|word| word & (1_u64 << (index % 64)) != 0)
}

/// The number of due topics in `left & right`.
fn and_count(left: &DueMask, right: &DueMask) -> usize {
    left.iter()
        .zip(right.iter())
        .map(|(a, b)| (a & b).count_ones() as usize)
        .sum()
}

/// `left & right`.
fn and_mask(left: &DueMask, right: &DueMask) -> DueMask {
    left.iter().zip(right.iter()).map(|(a, b)| a & b).collect()
}

/// Clear every bit of `mask` that `cover` holds.
fn clear_mask(mask: &mut DueMask, cover: &DueMask) {
    for (slot, bits) in mask.iter_mut().zip(cover.iter()) {
        *slot &= !bits;
    }
}

/// The ids of the due topics a mask holds, in the sorted due order.
fn ids_of_mask(mask: &DueMask, due_list: &[String], skip: Option<usize>) -> Vec<String> {
    due_list
        .iter()
        .enumerate()
        .filter(|&(index, _)| Some(index) != skip && get_bit(mask, index))
        .map(|(_, id)| id.clone())
        .collect()
}

/// The due topics `candidate` knocks out, excluding itself
/// (`_covers`, `selector.py:500-504`), as a mask over the sorted due list.
fn cover_mask(
    candidate: &str,
    due_list: &[String],
    due_index: &HashMap<&str, usize>,
    cfg: &Config,
    cache: &mut ReachCache<'_>,
) -> DueMask {
    let mut mask = empty_mask(due_list.len());
    if cfg.fire.knockout_weight <= 0.0 {
        // Every weight, present or absent, clears a non-positive threshold.
        for (index, id) in due_list.iter().enumerate() {
            if id.as_str() != candidate {
                set_bit(&mut mask, index);
            }
        }
        return mask;
    }
    for &(target, weight) in cache.weights(candidate) {
        if target != candidate
            && weight >= cfg.fire.knockout_weight
            && let Some(&index) = due_index.get(target)
        {
            set_bit(&mut mask, index);
        }
    }
    mask
}

/// The greedy weighted set cover of the due reviews (`compress`, `selector.py:506-590`).
///
/// Phase 1 spends the free candidates — frontier lessons and nearly-due topics —
/// then phase 2 serves the rest explicitly, each surviving review dominoing the
/// others it covers. Both phases keep the FIRST strict maximum over the sorted
/// candidates, so a tie breaks to the lowest id.
///
/// `frontier_candidates` is the exact set of frontier lessons the caller serves.
/// `None` falls back to the global retry-filtered frontier, for a caller that
/// compresses alone.
#[must_use]
pub fn compress(
    due: &[String],
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    cfg: &Config,
    t_us: i64,
    frontier_candidates: Option<&BTreeSet<String>>,
) -> Compression {
    let mut cache = ReachCache::new(graph);
    compress_with(
        due,
        states,
        graph,
        cfg,
        t_us,
        frontier_candidates,
        &mut cache,
    )
}

/// [`compress`] over a shared weight memo.
pub(super) fn compress_with(
    due: &[String],
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    cfg: &Config,
    t_us: i64,
    frontier_candidates: Option<&BTreeSet<String>>,
    cache: &mut ReachCache<'_>,
) -> Compression {
    let due_set: BTreeSet<String> = due
        .iter()
        .filter(|id| graph.idx_of(id).is_some())
        .cloned()
        .collect();
    if due_set.is_empty() {
        return Compression::default();
    }
    let free_candidates = free_candidates(states, graph, cfg, t_us, frontier_candidates, &due_set);

    // The sorted due list is the bit order of every mask below.
    let due_list: Vec<String> = due_set.into_iter().collect();
    let due_index: HashMap<&str, usize> = due_list
        .iter()
        .enumerate()
        .map(|(index, id)| (id.as_str(), index))
        .collect();
    let mut knockouts: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut uncovered = full_mask(due_list.len());

    // Phase 1: the free candidates, served for progression anyway.
    let free_covers: Vec<DueMask> = free_candidates
        .iter()
        .map(|candidate| cover_mask(candidate, &due_list, &due_index, cfg, cache))
        .collect();
    while let Some((winner, cover)) = best_free(&free_candidates, &free_covers, &uncovered) {
        let taken = and_mask(cover, &uncovered);
        knockouts.insert(winner.clone(), ids_of_mask(&taken, &due_list, None));
        clear_mask(&mut uncovered, &taken);
    }

    // Phase 2: the remaining due topics, served explicitly and dominoing.
    let due_covers: Vec<DueMask> = due_list
        .iter()
        .map(|candidate| cover_mask(candidate, &due_list, &due_index, cfg, cache))
        .collect();
    let mut surviving: Vec<String> = Vec::new();
    let mut remaining = uncovered;
    while let Some((index, winner, cover)) = best_due(&due_list, &due_covers, &remaining) {
        let mut taken = and_mask(cover, &remaining);
        set_bit(&mut taken, index);
        let others = ids_of_mask(&taken, &due_list, Some(index));
        if !others.is_empty() {
            knockouts.insert(winner.clone(), others);
        }
        surviving.push(winner.clone());
        clear_mask(&mut remaining, &taken);
    }

    surviving.sort_unstable();
    Compression {
        surviving,
        knockouts,
    }
}

/// The free candidates of phase 1: the frontier lessons the caller serves (or
/// the retry-filtered global frontier) plus the nearly-due topics, less every
/// due topic, in sorted order.
fn free_candidates(
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    cfg: &Config,
    t_us: i64,
    frontier_candidates: Option<&BTreeSet<String>>,
    due_set: &BTreeSet<String>,
) -> Vec<String> {
    let default = TopicState::default();
    let mut free: BTreeSet<String> = match frontier_candidates {
        Some(given) => given.clone(),
        None => frontier(graph, &mastered_set(states, graph))
            .sorted_ids(graph)
            .into_iter()
            .filter(|id| !in_retry_delay(states.get(*id).unwrap_or(&default), cfg, t_us))
            .map(ToOwned::to_owned)
            .collect(),
    };
    free.extend(nearly_due(states, graph, cfg, t_us));
    free.into_iter()
        .filter(|id| !due_set.contains(id))
        .collect()
}

/// A mask with every one of `n` bits set.
fn full_mask(n: usize) -> DueMask {
    let mut mask = empty_mask(n);
    for index in 0..n {
        set_bit(&mut mask, index);
    }
    mask
}

/// The free candidate that covers the most uncovered due topics, with its
/// cover, or `None` when no free candidate covers an uncovered topic. A STRICT
/// maximum, so a tie keeps the first candidate in sorted order.
fn best_free<'c>(
    candidates: &'c [String],
    covers: &'c [DueMask],
    uncovered: &DueMask,
) -> Option<(&'c String, &'c DueMask)> {
    let mut best: Option<(&'c String, &'c DueMask)> = None;
    let mut best_count = 0_usize;
    for (candidate, cover) in candidates.iter().zip(covers) {
        let count = and_count(cover, uncovered);
        if count > best_count {
            best = Some((candidate, cover));
            best_count = count;
        }
    }
    best
}

/// The remaining due topic that covers the most of the remaining ones, itself
/// included, or `None` once no due topic remains.
///
/// A due topic always covers itself, and `cover` never holds it, so the size of
/// `{c} | covers(c, remaining)` is one more than the intersection count. Only
/// the winner's mask is materialized.
fn best_due<'c>(
    due_list: &'c [String],
    covers: &'c [DueMask],
    remaining: &DueMask,
) -> Option<(usize, &'c String, &'c DueMask)> {
    let mut best: Option<(usize, &'c String, &'c DueMask)> = None;
    let mut best_count = 0_usize;
    for (index, (id, cover)) in due_list.iter().zip(covers).enumerate() {
        if !get_bit(remaining, index) {
            continue;
        }
        let count = and_count(cover, remaining) + 1;
        if count > best_count {
            best_count = count;
            best = Some((index, id, cover));
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fire::testing::{T_US, graph, learned, topic};

    #[test]
    fn a_free_lesson_and_a_due_topic_both_knock_out_a_due_review() {
        let cfg = Config::default();
        let tree = graph(vec![
            topic("child", &[]),
            topic("parent", &[("child", 0.9, true)]),
            topic("add", &[]),
            topic("sub", &[("add", 0.8, true)]),
            topic("far", &[]),
        ]);
        let states: BTreeMap<String, TopicState> = [
            ("child".to_owned(), learned(0.5)),
            ("add".to_owned(), learned(0.5)),
            ("sub".to_owned(), learned(0.5)),
            ("far".to_owned(), learned(0.5)),
        ]
        .into_iter()
        .collect();
        let due = ["child", "add", "sub", "far", "ghost"].map(str::to_owned);
        let comp = compress(&due, &states, &tree, &cfg, T_US, None);
        assert_eq!(comp.surviving, ["far", "sub"]);
        assert_eq!(comp.knockouts["parent"], ["child"]);
        assert_eq!(comp.knockouts["sub"], ["add"]);
        assert_eq!(
            compress(&[], &states, &tree, &cfg, T_US, None),
            Compression::default()
        );

        let given: BTreeSet<String> = BTreeSet::new();
        let mut loose = Config::default();
        loose.fire.knockout_weight = 0.0;
        let all = compress(&due, &states, &tree, &loose, T_US, Some(&given));
        assert_eq!(all.surviving, ["add"]);
        assert_eq!(all.knockouts["add"], ["child", "far", "sub"]);
    }
}
