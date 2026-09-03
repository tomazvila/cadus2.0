//! The probe set: a greedy set cover over the two demands of every topic.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::curriculum::{Curriculum, TopicIdx};

use super::scope;

/// The topics within `radius` hops of `start`, `start` itself excluded.
///
/// `forward` picks the edge direction: `true` walks dependents (downstream),
/// `false` walks prerequisites (upstream).
fn within(graph: &Curriculum, start: TopicIdx, radius: i64, forward: bool) -> BTreeSet<TopicIdx> {
    let mut seen: BTreeSet<TopicIdx> = BTreeSet::new();
    if radius <= 0 {
        return seen;
    }
    let mut layer: VecDeque<TopicIdx> = VecDeque::from(vec![start]);
    for _ in 0..radius {
        let mut next: VecDeque<TopicIdx> = VecDeque::new();
        for node in layer.drain(..) {
            let neighbors: Vec<TopicIdx> = if forward {
                graph.dependents(node).collect()
            } else {
                graph.prerequisites(node).collect()
            };
            for neighbor in neighbors {
                if neighbor != start && seen.insert(neighbor) {
                    next.push_back(neighbor);
                }
            }
        }
        if next.is_empty() {
            break;
        }
        layer = next;
    }
    seen
}

/// A bit per demand. Demand `2i` is the ancestor demand of universe member `i`,
/// and demand `2i + 1` is its descendant demand.
type DemandMask = Vec<u64>;

/// An all-zero mask over `demands` bits.
fn empty_mask(demands: usize) -> DemandMask {
    vec![0_u64; demands.div_ceil(64)]
}

/// Set one bit of the mask.
fn set_bit(mask: &mut DemandMask, bit: usize) {
    mask[bit / 64] |= 1_u64 << (bit % 64);
}

/// The number of bits set in `left & right`.
fn overlap(left: &DemandMask, right: &DemandMask) -> u32 {
    left.iter()
        .zip(right.iter())
        .map(|(a, b)| (a & b).count_ones())
        .sum()
}

/// Clear every bit of `target` that `covered` holds.
fn clear_all(target: &mut DemandMask, covered: &DemandMask) {
    for (slot, bits) in target.iter_mut().zip(covered.iter()) {
        *slot &= !bits;
    }
}

/// The demands one probe answers: the ancestor demand of every topic within
/// the radius downstream (and of itself), and the descendant demand of every
/// topic within the radius upstream (and of itself).
fn cover_of(
    graph: &Curriculum,
    probe: TopicIdx,
    radius: i64,
    demand_index: &BTreeMap<TopicIdx, usize>,
) -> DemandMask {
    let mut mask = empty_mask(demand_index.len() * 2);
    for (forward, offset) in [(true, 0), (false, 1)] {
        let mut side = within(graph, probe, radius, forward);
        side.insert(probe);
        for topic in side {
            if let Some(&position) = demand_index.get(&topic) {
                set_bit(&mut mask, position * 2 + offset);
            }
        }
    }
    mask
}

/// The candidate with the most open demands, or `None` when no candidate
/// covers an open demand. A STRICT maximum, so the first candidate of a tie
/// wins.
fn best_candidate<'c>(
    covers: &'c [(String, DemandMask)],
    remaining: &DemandMask,
) -> Option<&'c (String, DemandMask)> {
    let mut best: Option<(&'c (String, DemandMask), u32)> = None;
    for candidate in covers {
        let gain = overlap(&candidate.1, remaining);
        if gain > best.map_or(0, |(_, best_gain)| best_gain) {
            best = Some((candidate, gain));
        }
    }
    best.map(|(candidate, _)| candidate)
}

/// A probe set that covers the course and its foundations at `radius`.
///
/// Every in-scope topic raises two demands: one probe among its ancestors within
/// the radius (or itself), and one among its descendants within the radius (or
/// itself). Choosing probe `p` answers the ancestor demand of every topic in the
/// descendants of `p` within the radius, and the descendant demand of every topic
/// in its ancestors within the radius.
///
/// The solution is the standard greedy set cover: take the probe that covers the
/// most demands that are still open, and break a tie by the lowest id. It is an
/// approximation and not a minimum, which is what 1.0 does. A root and a leaf are
/// always taken, because only they answer their own demand, so the diagnostic
/// always probes the extremes.
///
/// The cover of each candidate is computed ONCE as a bit mask over the demand
/// list, and each greedy round intersects those bits. That is the same choice the
/// set arithmetic of 1.0 makes, and it is the shape `selector::compress` already
/// uses for the review compression.
#[must_use]
pub fn probe_set(graph: &Curriculum, course: Option<&str>, radius: i64) -> BTreeSet<String> {
    let universe = scope(graph, course);
    if universe.is_empty() {
        return BTreeSet::new();
    }
    let members: Vec<TopicIdx> = universe.iter().filter_map(|id| graph.idx_of(id)).collect();
    let demand_index: BTreeMap<TopicIdx, usize> = members
        .iter()
        .enumerate()
        .map(|(position, &idx)| (idx, position))
        .collect();
    let demands = members.len() * 2;

    let mut covers: Vec<(String, DemandMask)> = members
        .iter()
        .map(|&probe| {
            (
                graph.id_of(probe).to_owned(),
                cover_of(graph, probe, radius, &demand_index),
            )
        })
        .collect();
    // The candidate order is the sorted id order, so a tie takes the lowest id.
    covers.sort_by(|left, right| left.0.cmp(&right.0));

    let mut remaining = empty_mask(demands);
    for position in 0..demands {
        set_bit(&mut remaining, position);
    }
    let mut chosen: BTreeSet<String> = BTreeSet::new();
    // Every open demand is covered by its own topic, so the loop ends exactly
    // when no demand is open.
    while let Some((id, mask)) = best_candidate(&covers, &remaining) {
        clear_all(&mut remaining, mask);
        chosen.insert(id.clone());
    }
    chosen
}

#[cfg(test)]
mod tests {
    use super::super::scope;
    use super::*;
    use crate::fire::testing::{ladder, topic};

    /// Course `c`: a chain `a -> b -> c -> d`, a second dependent `e` of `a`,
    /// and a diamond `f` under `b` and `e`. Course `d`: `g` under `d`.
    fn tree() -> Curriculum {
        ladder(&[
            (
                "c",
                &[],
                vec![
                    topic("a", &[]),
                    topic("b", &[("a", 1.0, true)]),
                    topic("c", &[("b", 1.0, true)]),
                    topic("d", &[("c", 1.0, true)]),
                    topic("e", &[("a", 1.0, true)]),
                    topic("f", &[("b", 1.0, true), ("e", 1.0, true)]),
                ],
            ),
            ("d", &[], vec![topic("g", &[("d", 1.0, true)])]),
        ])
    }

    #[test]
    fn the_cover_takes_the_extremes_and_a_zero_radius_takes_every_topic() {
        let tree = tree();
        let probes = probe_set(&tree, Some("c"), 2);
        assert!(probes.contains("a") && probes.contains("d") && probes.contains("f"));
        assert!(!probes.contains("g"));
        let all: BTreeSet<String> = scope(&tree, None);
        assert_eq!(probe_set(&tree, None, 0), all);
        assert!(probe_set(&tree, Some("nope"), 2).is_empty());
        let start = tree.idx_of("a").expect("a is a topic");
        assert_eq!(within(&tree, start, 2, true).len(), 4);
        assert!(within(&tree, start, 1, false).is_empty());
    }
}
