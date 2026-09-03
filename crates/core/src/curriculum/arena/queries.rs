//! The graph queries of the arena: the two prerequisite directions, the two
//! encompassing maps, and the 1.0 neighborhood.

use std::collections::BTreeSet;

use super::super::graph;
use super::{Curriculum, EncLink, EncNode, TopicIdx};

impl Curriculum {
    // -- prerequisite adjacency ------------------------------------------- //

    /// The direct prerequisites of a topic, ascending by load index. A
    /// prerequisite with no topic was dropped at build (parity trap 7).
    pub fn prerequisites(&self, idx: TopicIdx) -> impl ExactSizeIterator<Item = TopicIdx> + '_ {
        self.prereqs
            .neighbors(idx.index())
            .iter()
            .copied()
            .map(TopicIdx)
    }

    /// The direct dependents of a topic, ascending by load index.
    pub fn dependents(&self, idx: TopicIdx) -> impl ExactSizeIterator<Item = TopicIdx> + '_ {
        self.dependents
            .neighbors(idx.index())
            .iter()
            .copied()
            .map(TopicIdx)
    }

    /// The number of prerequisite edges over existing targets.
    pub fn prereq_edge_count(&self) -> usize {
        self.prereqs.edge_count()
    }

    /// The number of dependent edges. It equals [`Self::prereq_edge_count`].
    pub fn dependent_edge_count(&self) -> usize {
        self.dependents.edge_count()
    }

    /// Every transitive prerequisite of a topic, ascending by load index.
    pub fn ancestors(&self, idx: TopicIdx) -> Vec<TopicIdx> {
        graph::closure(&self.prereqs, idx.as_u32(), self.topics.len())
            .into_iter()
            .map(TopicIdx)
            .collect()
    }

    /// Every transitive dependent of a topic, ascending by load index.
    pub fn descendants(&self, idx: TopicIdx) -> Vec<TopicIdx> {
        graph::closure(&self.dependents, idx.as_u32(), self.topics.len())
            .into_iter()
            .map(TopicIdx)
            .collect()
    }

    /// The topological order, precomputed at build: Kahn's algorithm with a
    /// min-heap on the load index (spec section 3).
    ///
    /// A cycle makes the order shorter than [`Self::topic_count`];
    /// [`Self::find_cycle`] names the cycle.
    pub fn topo_order(&self) -> &[TopicIdx] {
        &self.topo
    }

    /// One prerequisite cycle, starting at the re-entered node, or `None` when
    /// the prerequisite graph is a DAG (parity trap 11).
    pub fn find_cycle(&self) -> Option<Vec<TopicIdx>> {
        graph::find_cycle(&self.prereqs, self.topics.len())
            .map(|cycle| cycle.into_iter().map(TopicIdx).collect())
    }

    // -- encompassing maps ------------------------------------------------- //

    /// The number of nodes of the encompassing maps: the topics plus the phantom
    /// targets (parity trap 7).
    pub fn enc_node_count(&self) -> usize {
        self.topics.len() + self.phantom_ids.len()
    }

    /// The encompassing node of a topic.
    pub fn enc_node(&self, idx: TopicIdx) -> EncNode {
        EncNode(idx.as_u32())
    }

    /// The topic behind an encompassing node, or `None` for a phantom node.
    pub fn topic_of_enc_node(&self, node: EncNode) -> Option<TopicIdx> {
        (node.index() < self.topics.len()).then(|| TopicIdx(node.as_u32()))
    }

    /// The external string id of an encompassing node, or `""` when the node
    /// belongs to another build.
    pub fn enc_node_id(&self, node: EncNode) -> &str {
        if let Some(topic) = self.topics.get(node.index()) {
            return topic.id.as_str();
        }
        node.index()
            .checked_sub(self.topics.len())
            .and_then(|slot| self.phantom_ids.get(slot))
            .map_or("", String::as_str)
    }

    /// The encompassing node of an external string id.
    pub fn enc_node_by_id(&self, id: &str) -> Option<EncNode> {
        match self.by_id.get(id) {
            Some(idx) => Some(EncNode(idx.as_u32())),
            None => self.phantom_by_id.get(id).copied(),
        }
    }

    /// The forward encompassing edges out of a node, in insertion order.
    /// A weight-0 edge is absent (parity trap 8).
    pub fn enc_forward(&self, node: EncNode) -> impl ExactSizeIterator<Item = EncLink> + '_ {
        self.enc.edges_from(node.index()).iter().map(EncLink::from)
    }

    /// The reverse encompassing edges into a node, in insertion order. Every
    /// declared edge is here, weight 0 included (parity trap 8).
    pub fn enc_reverse(&self, node: EncNode) -> impl ExactSizeIterator<Item = EncLink> + '_ {
        self.enc_rev
            .edges_from(node.index())
            .iter()
            .map(EncLink::from)
    }

    /// The number of forward encompassing entries.
    pub fn enc_forward_count(&self) -> usize {
        self.enc.edge_count()
    }

    /// The number of reverse encompassing entries.
    pub fn enc_reverse_count(&self) -> usize {
        self.enc_rev.edge_count()
    }

    /// `W(src -> b)` for every node `b`, indexed by node number. `src` holds
    /// `1.0`; an unreached node holds `0.0`. This is the downward implicit
    /// credit of PEDAGOGY 4.
    pub fn reach_weights(&self, src: TopicIdx) -> Vec<f64> {
        graph::relax(&self.enc, src.as_u32(), self.enc_node_count())
    }

    /// `W(a -> b)`: the max over encompassing paths of the product of the edge
    /// weights.
    ///
    /// `W(a -> a)` is `1.0`. A node that is unreachable, or reachable only
    /// through a weight-0 edge, gives `0.0`. The relaxation replays the 1.0
    /// stack order, so the float agrees to the last bit (parity trap 9).
    pub fn encompassing_weight(&self, a: TopicIdx, b: TopicIdx) -> f64 {
        if a == b {
            return 1.0;
        }
        self.reach_weights(a).get(b.index()).copied().unwrap_or(0.0)
    }

    /// `W(a -> b)` addressed by external id, for the FIRe knockout predicate
    /// (1.0 `Graph.encompassing_weight`, `cadus/graph.py:455-465`).
    ///
    /// An id with no encompassing node gives `0.0`, and `a == b` gives `1.0`
    /// before any lookup, the same as 1.0.
    pub fn encompassing_weight_by_id(&self, a: &str, b: &str) -> f64 {
        if a == b {
            return 1.0;
        }
        let (Some(src), Some(dst)) = (self.enc_node_by_id(a), self.enc_node_by_id(b)) else {
            return 0.0;
        };
        let weights = graph::relax(&self.enc, src.as_u32(), self.enc_node_count());
        weights.get(dst.index()).copied().unwrap_or(0.0)
    }

    /// `W(src -> b)` for every node `b` with a positive weight, as `(id, weight)`
    /// pairs SORTED BY ID (1.0 `Graph.reach_weights`, `cadus/graph.py:435-443`).
    ///
    /// This is the downward implicit credit of PEDAGOGY 4. A phantom target is
    /// present, because a dangling prerequisite id enters the encompassing maps
    /// and the FIRe engine then makes state for it (parity trap T15).
    ///
    /// 1.0 iterates this map with `sorted(weights.items())`, so the pairs come
    /// back in byte order of the id, which is the Python code-point order of a
    /// UTF-8 id (parity trap T18). `src` itself holds `1.0`; every 1.0 caller
    /// skips it. An id with no encompassing node gives an EMPTY list, because
    /// its only 1.0 entry is that skipped self weight.
    pub fn reach_weights_by_id(&self, src: &str) -> Vec<(&str, f64)> {
        match self.enc_node_by_id(src) {
            Some(node) => self.weights_by_id(&graph::relax(
                &self.enc,
                node.as_u32(),
                self.enc_node_count(),
            )),
            None => Vec::new(),
        }
    }

    /// `W(a -> dst)` for every node `a` with a positive weight, as `(id, weight)`
    /// pairs SORTED BY ID (1.0 `Graph.upward_weights`, `cadus/graph.py:445-453`).
    ///
    /// This is the upward failure penalty of PEDAGOGY 4. The ordering rule and
    /// the empty-list rule of [`Curriculum::reach_weights_by_id`] hold here too.
    pub fn upward_weights_by_id(&self, dst: &str) -> Vec<(&str, f64)> {
        match self.enc_node_by_id(dst) {
            Some(node) => self.weights_by_id(&graph::relax(
                &self.enc_rev,
                node.as_u32(),
                self.enc_node_count(),
            )),
            None => Vec::new(),
        }
    }

    /// The local neighborhood that seeds an untouched topic's ability
    /// (1.0 `Graph.neighborhood`, `cadus/graph.py:408-431`), SORTED BY ID.
    ///
    /// The union is: the direct prerequisites that are topics, the key
    /// prerequisite edges, the key prerequisites of every knowledge point, the
    /// `encompassings_extra` targets, the sources of every reverse encompassing
    /// edge (weight-0 edges included, which is why the reverse map keeps them),
    /// and the topics of the same module. The topic itself and every dangling id
    /// drop out at the end.
    ///
    /// 1.0 returns a SET and the caller iterates it, so the order is
    /// hash-randomized there (parity trap T5). This returns the sorted order,
    /// which removes the class of bug.
    ///
    /// An id with no topic gives an EMPTY list. 1.0 raises `KeyError` there; the
    /// 2.0 fold reports no panic on any event stream, and an empty neighborhood
    /// makes [`crate::fire::initial_ability`] fall back to its neutral prior.
    pub fn neighborhood(&self, id: &str) -> Vec<&str> {
        let Some((idx, topic)) = self
            .by_id
            .get(id)
            .and_then(|&idx| self.topics.get(idx.index()).map(|topic| (idx, topic)))
        else {
            return Vec::new();
        };
        let mut out: BTreeSet<&str> = BTreeSet::new();
        for prereq in self.prerequisites(idx) {
            out.insert(self.id_of(prereq));
        }
        for edge in &topic.prerequisites {
            if edge.key {
                out.insert(edge.id.as_str());
            }
        }
        for kp in &topic.knowledge_points {
            for key in &kp.key_prerequisites {
                out.insert(key.as_str());
            }
        }
        for edge in &topic.encompassings_extra {
            out.insert(edge.id.as_str());
        }
        for link in self.enc_reverse(self.enc_node(idx)) {
            out.insert(self.enc_node_id(link.target));
        }
        for &sibling in self.topics_in_module(self.module_of(idx)) {
            out.insert(self.id_of(sibling));
        }
        out.remove(id);
        out.into_iter()
            .filter(|other| self.idx_of(other).is_some())
            .collect()
    }

    /// Turn a node-indexed weight vector into the `(id, weight)` pairs of 1.0,
    /// sorted by id. A zero weight is absent, because 1.0 never stores one.
    fn weights_by_id(&self, weights: &[f64]) -> Vec<(&str, f64)> {
        let mut out: Vec<(&str, f64)> = (0_u32..)
            .zip(weights.iter().copied())
            .filter(|&(_, weight)| weight > 0.0)
            .map(|(node, weight)| (self.enc_node_id(EncNode::from_u32(node)), weight))
            .collect();
        out.sort_by(|left, right| left.0.cmp(right.0));
        out
    }
}
