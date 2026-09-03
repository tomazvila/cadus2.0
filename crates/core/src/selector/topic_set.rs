//! A set of topics addressed by arena index (D1), the course scope, the
//! mastered set, the frontier, and the encompassing-weight memo.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::curriculum::{Curriculum, TopicIdx};
use crate::learner::TopicState;
use crate::xp::is_mastered;

/// A set of curriculum topics, held as one bit per arena index (D1).
///
/// 1.0 passes `set[str]` between the selector helpers. A set of owned strings
/// costs one allocation per member, and the L1 budget composes a session over
/// 1,090 topics, so the port carries the membership as a dense bit vector and
/// converts to ids only where an id leaves the module.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TopicSet {
    /// One flag per arena topic index.
    members: Vec<bool>,
    /// The number of set flags.
    count: usize,
}

impl TopicSet {
    /// An empty set sized for `graph`.
    #[must_use]
    pub fn empty(graph: &Curriculum) -> Self {
        Self {
            members: vec![false; graph.topic_count()],
            count: 0,
        }
    }

    /// Add one topic. Adding a member twice keeps the count right.
    pub fn insert(&mut self, idx: TopicIdx) {
        if let Some(slot) = self.members.get_mut(idx.index())
            && !*slot
        {
            *slot = true;
            self.count += 1;
        }
    }

    /// Add one topic by id. An id with no topic is ignored.
    pub fn insert_id(&mut self, graph: &Curriculum, id: &str) {
        if let Some(idx) = graph.idx_of(id) {
            self.insert(idx);
        }
    }

    /// Whether the set holds a topic.
    #[must_use]
    pub fn contains(&self, idx: TopicIdx) -> bool {
        self.members.get(idx.index()).copied().unwrap_or(false)
    }

    /// Whether the set holds the topic of an id. An id with no topic is absent.
    #[must_use]
    pub fn contains_id(&self, graph: &Curriculum, id: &str) -> bool {
        graph.idx_of(id).is_some_and(|idx| self.contains(idx))
    }

    /// Whether the set is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Every member, ascending by arena index.
    pub fn indices(&self) -> impl Iterator<Item = TopicIdx> + '_ {
        self.members
            .iter()
            .enumerate()
            .filter(|&(_, &member)| member)
            .filter_map(|(index, _)| u32::try_from(index).ok().map(TopicIdx::from_u32))
    }

    /// Every member id, SORTED by id in byte order (trap T18).
    #[must_use]
    pub fn sorted_ids<'g>(&self, graph: &'g Curriculum) -> Vec<&'g str> {
        let mut out: Vec<&str> = self.indices().map(|idx| graph.id_of(idx)).collect();
        out.sort_unstable();
        out
    }

    /// Every member id as an owned, sorted set.
    #[must_use]
    pub fn to_id_set(&self, graph: &Curriculum) -> BTreeSet<String> {
        self.sorted_ids(graph)
            .into_iter()
            .map(ToOwned::to_owned)
            .collect()
    }

    /// This set restricted to the members of `other` (`self & other`).
    #[must_use]
    pub fn intersect(&self, other: &Self) -> Self {
        let members: Vec<bool> = self
            .members
            .iter()
            .zip(other.members.iter())
            .map(|(&left, &right)| left && right)
            .collect();
        let count = members.iter().filter(|&&member| member).count();
        Self { members, count }
    }

    /// Whether every member of this set is a member of `other` (`self <= other`).
    #[must_use]
    pub fn is_subset(&self, other: &Self) -> bool {
        self.members
            .iter()
            .zip(other.members.iter())
            .all(|(&left, &right)| !left || right)
    }
}

/// The topics of a course, or every topic when the scope is unset
/// (`_course_scope`, `selector.py:163-165`).
#[must_use]
pub fn course_scope(graph: &Curriculum, course_id: Option<&str>) -> TopicSet {
    let mut out = TopicSet::empty(graph);
    match course_id {
        Some(course) => {
            for &idx in graph.topics_in_course(course) {
                out.insert(idx);
            }
        }
        None => {
            for idx in every_index(graph) {
                out.insert(idx);
            }
        }
    }
    out
}

/// Every arena topic index, ascending.
pub(super) fn every_index(graph: &Curriculum) -> impl Iterator<Item = TopicIdx> + '_ {
    (0..graph.topic_count()).filter_map(|index| u32::try_from(index).ok().map(TopicIdx::from_u32))
}

/// The topics currently mastered for frontier purposes
/// (`mastered_set`, `selector.py:157-160`).
///
/// A topic absent from `states` is untouched, so it is not mastered.
#[must_use]
pub fn mastered_set(states: &BTreeMap<String, TopicState>, graph: &Curriculum) -> TopicSet {
    let mut out = TopicSet::empty(graph);
    for (id, state) in states {
        if is_mastered(state) {
            out.insert_id(graph, id);
        }
    }
    out
}

/// The learnable topics: not mastered, every prerequisite mastered
/// (`Graph.frontier`, `graph.py:375-389`).
#[must_use]
pub fn frontier(graph: &Curriculum, mastered: &TopicSet) -> TopicSet {
    let mut out = TopicSet::empty(graph);
    for idx in every_index(graph) {
        if mastered.contains(idx) {
            continue;
        }
        if graph
            .prerequisites(idx)
            .all(|prereq| mastered.contains(prereq))
        {
            out.insert(idx);
        }
    }
    out
}

/// A memo of `W(src -> *)` per source topic.
///
/// 1.0 caches the relaxation on the graph (`graph.py:298-299`). The 2.0 arena
/// recomputes it per call, and [`compress`] asks for the same source many times,
/// so the selector holds the memo for the length of one composition. The floats
/// are the arena floats, so the memo changes no value.
pub(super) struct ReachCache<'g> {
    /// The curriculum the weights come from.
    graph: &'g Curriculum,
    /// `src -> [(target, weight)]`, each list sorted by target id.
    entries: HashMap<&'g str, Vec<(&'g str, f64)>>,
}

impl<'g> ReachCache<'g> {
    /// An empty memo over `graph`.
    pub(super) fn new(graph: &'g Curriculum) -> Self {
        Self {
            graph,
            entries: HashMap::new(),
        }
    }

    /// `W(src -> *)`, sorted by target id. An id with no topic gives an empty list.
    pub(super) fn weights(&mut self, src: &str) -> &[(&'g str, f64)] {
        let graph = self.graph;
        let key: &'g str = match graph.idx_of(src) {
            Some(idx) => graph.id_of(idx),
            None => return &[],
        };
        self.entries
            .entry(key)
            .or_insert_with(|| graph.reach_weights_by_id(key))
    }

    /// `W(a -> b)`, the way `Curriculum::encompassing_weight_by_id` computes it.
    pub(super) fn weight(&mut self, a: &str, b: &str) -> f64 {
        if a == b {
            return 1.0;
        }
        let weights = self.weights(a);
        weights
            .binary_search_by(|probe| probe.0.cmp(b))
            .ok()
            .and_then(|position| weights.get(position))
            .map_or(0.0, |&(_, weight)| weight)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fire::testing::{graph, learned, topic};

    #[test]
    fn a_set_counts_its_members_once_and_ignores_a_foreign_index() {
        let tree = graph(vec![topic("a", &[]), topic("b", &[("a", 0.9, true)])]);
        let mut set = TopicSet::empty(&tree);
        let a = tree.idx_of("a").expect("a is a topic");
        set.insert(a);
        set.insert(a);
        set.insert(TopicIdx::from_u32(99));
        set.insert_id(&tree, "ghost");
        assert!(set.contains(a) && set.contains_id(&tree, "a"));
        assert!(!set.is_empty() && !set.contains_id(&tree, "ghost"));
        assert_eq!(set.sorted_ids(&tree), ["a"]);
        assert_eq!(set.to_id_set(&tree).len(), 1);
        let scope = course_scope(&tree, Some("c"));
        assert!(set.is_subset(&scope) && !scope.is_subset(&set));
        assert_eq!(scope.intersect(&set).sorted_ids(&tree), ["a"]);
        assert_eq!(course_scope(&tree, None).sorted_ids(&tree).len(), 2);
        let states = [("a".to_owned(), learned(0.5))].into_iter().collect();
        let mastered = mastered_set(&states, &tree);
        assert_eq!(frontier(&tree, &mastered).sorted_ids(&tree), ["b"]);
        let mut cache = ReachCache::new(&tree);
        assert_eq!(cache.weight("b", "a"), 0.9);
        assert_eq!(cache.weight("b", "b"), 1.0);
        assert!(cache.weights("ghost").is_empty());
    }
}
