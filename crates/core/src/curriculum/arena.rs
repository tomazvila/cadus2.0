//! The curriculum arena (D1, D2).
//!
//! [`Curriculum`] holds the whole curriculum in one immutable, `Arc`-shareable
//! value: topics in a `Vec` in load order, external string ids interned to
//! indices, both prerequisite directions as CSR, both encompassing maps, and the
//! topological order precomputed. A scheduler traversal is an index walk.
//!
//! ## The index boundary (D2)
//!
//! [`TopicIdx`] and [`KpIdx`] are valid only inside one build of one tree.
//! Anything persisted keeps the string id and converts here, at the boundary,
//! with [`Curriculum::idx_of`] and [`Curriculum::id_of`].
//!
//! ## Parity
//!
//! The build copies 1.0 `Graph.__init__` (`cadus/graph.py:229-321`):
//!
//! - A prerequisite whose target is not a topic is dropped from both adjacency
//!   directions; the same target stays in the encompassing maps (trap 7).
//! - The forward encompassing map keeps a weight only while it beats the 0.0
//!   default, so a weight-0 edge is absent; the reverse map records every edge,
//!   weight 0 included (trap 8). `neighborhood()` in 1.0 reads the reverse keys,
//!   which is why the two differ.
//! - A repeated course id keeps the LAST catalog entry, because 1.0 writes the
//!   dict comprehension `{c.id: c for c in catalog.courses}`.
//! - A duplicate topic id is the one content defect that stops a build
//!   (trap 14); every graph-stage lint code is tolerated (trap 13).
//!   [`load_curriculum`] still blocks on a fatal parse-stage finding, the same
//!   as 1.0 `Graph.load`.

use std::collections::hash_map::Entry;
use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use super::finding::Finding;
use super::graph::{self, Csr, EncCsr, EncEdge};
use super::load::{ParseError, RawCurriculum, load_raw_curriculum};
use super::model::{Course, KnowledgePoint, Topic};

/// A topic of one build, numbered by load order (D2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TopicIdx(u32);

impl TopicIdx {
    /// Wrap a raw index. An index out of range yields `None` or an empty slice
    /// from every query, so this never makes a panic reachable.
    pub const fn from_u32(raw: u32) -> Self {
        Self(raw)
    }

    /// The raw index.
    pub const fn as_u32(self) -> u32 {
        self.0
    }

    /// The raw index as a `usize`.
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// A knowledge point inside one topic, numbered by authored order (D2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KpIdx(u16);

impl KpIdx {
    /// Wrap a raw index.
    pub const fn from_u16(raw: u16) -> Self {
        Self(raw)
    }

    /// The raw index.
    pub const fn as_u16(self) -> u16 {
        self.0
    }

    /// The raw index as a `usize`.
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// A node of the encompassing maps: every topic, plus every dangling
/// `encompassings_extra` target 1.0 keeps as a phantom key (trap 7).
///
/// The first [`Curriculum::topic_count`] nodes are the topics, in load order, so
/// `EncNode(t.as_u32())` is the node of `TopicIdx(t)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EncNode(u32);

impl EncNode {
    /// Wrap a raw index.
    pub const fn from_u32(raw: u32) -> Self {
        Self(raw)
    }

    /// The raw index.
    pub const fn as_u32(self) -> u32 {
        self.0
    }

    /// The raw index as a `usize`.
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// One encompassing edge with its node typed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EncLink {
    /// The node at the far end of the edge.
    pub target: EncNode,
    /// The encompassing weight, in the closed range 0..=1.
    pub weight: f64,
}

impl From<&EncEdge> for EncLink {
    fn from(edge: &EncEdge) -> Self {
        Self {
            target: EncNode(edge.target),
            weight: edge.weight,
        }
    }
}

/// The curriculum cannot be represented as an arena.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CurriculumError {
    /// Two topics share one id. 1.0 `Graph.__init__` raises here
    /// (`cadus/graph.py:259-270`); a topic map cannot hold both.
    #[error("topic id '{id}' defined more than once")]
    DuplicateTopicId {
        /// The repeated id.
        id: String,
    },
    /// The tree holds more topics than an index can address.
    #[error("{count} topics exceed the {limit} an index can address")]
    TooManyTopics {
        /// The number of topics found.
        count: usize,
        /// The largest number an index can address.
        limit: u32,
    },
    /// The parse stage reported a fatal finding, so content was dropped. 1.0
    /// `Graph.load` raises `CurriculumError` here (`cadus/graph.py:328-334`);
    /// an arena built from the rest would misrepresent the curriculum
    /// (parity trap 13).
    #[error("{}", join_findings(findings))]
    FatalFindings {
        /// Every parse-stage finding, advisory ones included. 1.0 hands the
        /// whole list to `CurriculumError`, so the text names all of them.
        findings: Vec<Finding>,
    },
}

/// The 1.0 `CurriculumError` text: `[code] message`, joined with `; `.
fn join_findings(findings: &[Finding]) -> String {
    if findings.is_empty() {
        return "invalid curriculum".to_owned();
    }
    findings
        .iter()
        .map(|finding| format!("[{}] {}", finding.code, finding.message))
        .collect::<Vec<String>>()
        .join("; ")
}

/// A curriculum tree could not be read into an arena.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LoadError {
    /// The parse stage could not start.
    #[error(transparent)]
    Parse(#[from] ParseError),
    /// The parse stage ran, but the arena could not be built.
    #[error(transparent)]
    Curriculum(#[from] CurriculumError),
}

/// The immutable curriculum arena (D1). Share it through an `Arc`; nothing here
/// changes after the build.
#[derive(Debug, Clone, PartialEq)]
pub struct Curriculum {
    /// The number of unit files the parse stage read. 1.0 keeps the same number
    /// in `Graph.units` (`cadus/graph.py:242`), and the dump reports it.
    unit_count: usize,
    topics: Vec<Topic>,
    by_id: HashMap<String, TopicIdx>,
    /// The interned course, module, and unit names.
    labels: Vec<String>,
    topic_course: Vec<u32>,
    topic_module: Vec<u32>,
    topic_unit: Vec<u32>,
    /// Child -> parents, existing targets only.
    prereqs: Csr,
    /// Parent -> children, existing targets only.
    dependents: Csr,
    /// Forward encompassing map, positive weights only (trap 8).
    enc: EncCsr,
    /// Reverse encompassing map, every edge (trap 8).
    enc_rev: EncCsr,
    /// The ids of the phantom encompassing nodes, after the topics.
    phantom_ids: Vec<String>,
    phantom_by_id: HashMap<String, EncNode>,
    topo: Vec<TopicIdx>,
    courses: Vec<Course>,
    course_by_id: HashMap<String, usize>,
    topics_by_course: HashMap<String, Vec<TopicIdx>>,
    topics_by_module: HashMap<String, Vec<TopicIdx>>,
}

impl Curriculum {
    // -- build ------------------------------------------------------------ //

    /// Build the arena from a parsed tree.
    ///
    /// Content defects do not stop the build: a dangling prerequisite drops out
    /// of the adjacency, a topic with no knowledge point loads, a cycle loads.
    /// The lint of U3 reports them. Only a duplicate topic id is an error, the
    /// same as 1.0 (trap 13, trap 14).
    pub fn build(raw: RawCurriculum) -> Result<Self, CurriculumError> {
        let RawCurriculum { catalog, units } = raw;

        let unit_count = units.len();
        let total: usize = units.iter().map(|unit| unit.unit.topics.len()).sum();
        if total > u32::MAX as usize {
            return Err(CurriculumError::TooManyTopics {
                count: total,
                limit: u32::MAX,
            });
        }

        let mut topics: Vec<Topic> = Vec::with_capacity(total);
        let mut by_id: HashMap<String, TopicIdx> = HashMap::with_capacity(total);
        let mut labels: Vec<String> = Vec::new();
        let mut label_by_text: HashMap<String, u32> = HashMap::new();
        let mut topic_course: Vec<u32> = Vec::with_capacity(total);
        let mut topic_module: Vec<u32> = Vec::with_capacity(total);
        let mut topic_unit: Vec<u32> = Vec::with_capacity(total);

        for raw_unit in units {
            let unit = raw_unit.unit;
            // The course of a topic is the authored `course` field of the file,
            // never the directory the file came from (`cadus/graph.py:272`).
            let course = intern(&mut labels, &mut label_by_text, unit.course.as_str());
            let module = intern(&mut labels, &mut label_by_text, &unit.module);
            let unit_name = intern(&mut labels, &mut label_by_text, &unit.unit);
            for topic in unit.topics {
                let Ok(next) = u32::try_from(topics.len()) else {
                    return Err(CurriculumError::TooManyTopics {
                        count: topics.len(),
                        limit: u32::MAX,
                    });
                };
                match by_id.entry(topic.id.as_str().to_owned()) {
                    Entry::Occupied(seen) => {
                        return Err(CurriculumError::DuplicateTopicId {
                            id: seen.key().clone(),
                        });
                    }
                    Entry::Vacant(slot) => {
                        slot.insert(TopicIdx(next));
                    }
                }
                topics.push(topic);
                topic_course.push(course);
                topic_module.push(module);
                topic_unit.push(unit_name);
            }
        }

        let count = topics.len();
        let mut builder = EncBuilder::new(count);
        let mut prereq_lists: Vec<Vec<u32>> = vec![Vec::new(); count];
        let mut dependent_lists: Vec<Vec<u32>> = vec![Vec::new(); count];

        // The walk order is the load order, the same as the 1.0 loop over
        // `self.topics.items()`. It fixes the insertion order of both
        // encompassing maps, which `relax` replays (trap 9).
        for (position, topic) in topics.iter().enumerate() {
            let Ok(src) = u32::try_from(position) else {
                continue;
            };
            for edge in &topic.prerequisites {
                if let Some(parent) = by_id.get(edge.id.as_str()).copied() {
                    if let Some(list) = prereq_lists.get_mut(position) {
                        list.push(parent.as_u32());
                    }
                    if let Some(list) = dependent_lists.get_mut(parent.index()) {
                        list.push(src);
                    }
                }
                builder.add(&by_id, src, edge.id.as_str(), edge.weight);
            }
            for edge in &topic.encompassings_extra {
                builder.add(&by_id, src, edge.id.as_str(), edge.weight);
            }
        }

        // 1.0 keeps both directions in a `set`. Sorting by load index and
        // dropping the repeats gives the same content and a stable order.
        for list in prereq_lists.iter_mut().chain(dependent_lists.iter_mut()) {
            list.sort_unstable();
            list.dedup();
        }

        let prereqs = Csr::from_lists(&prereq_lists);
        let dependents = Csr::from_lists(&dependent_lists);
        let topo = graph::topo_order(&prereqs, &dependents, count)
            .into_iter()
            .map(TopicIdx)
            .collect();

        // 1.0 writes `{c.id: c for c in catalog.courses}`, so a repeated course
        // id keeps the LAST entry. `lint.rs` builds the same map the same way.
        let mut course_by_id = HashMap::with_capacity(catalog.courses.len());
        for (position, course) in catalog.courses.iter().enumerate() {
            course_by_id.insert(course.id.as_str().to_owned(), position);
        }

        let mut topics_by_course: HashMap<String, Vec<TopicIdx>> = HashMap::new();
        let mut topics_by_module: HashMap<String, Vec<TopicIdx>> = HashMap::new();
        for position in 0..count {
            let Ok(raw) = u32::try_from(position) else {
                continue;
            };
            let idx = TopicIdx(raw);
            if let Some(name) = topic_course
                .get(position)
                .and_then(|k| labels.get(*k as usize))
            {
                topics_by_course.entry(name.clone()).or_default().push(idx);
            }
            if let Some(name) = topic_module
                .get(position)
                .and_then(|k| labels.get(*k as usize))
            {
                topics_by_module.entry(name.clone()).or_default().push(idx);
            }
        }

        let EncBuilder {
            forward,
            reverse,
            phantom_ids,
            phantom_by_id,
            ..
        } = builder;

        Ok(Self {
            unit_count,
            topics,
            by_id,
            labels,
            topic_course,
            topic_module,
            topic_unit,
            prereqs,
            dependents,
            enc: EncCsr::from_lists(&forward),
            enc_rev: EncCsr::from_lists(&reverse),
            phantom_ids,
            phantom_by_id,
            topo,
            courses: catalog.courses,
            course_by_id,
            topics_by_course,
            topics_by_module,
        })
    }

    // -- topics and interning (D2) ---------------------------------------- //

    /// Every topic, in load order.
    pub fn topics(&self) -> &[Topic] {
        &self.topics
    }

    /// The number of topics.
    pub fn topic_count(&self) -> usize {
        self.topics.len()
    }

    /// The number of unit files the parse stage read. A file the parse stage
    /// dropped on a schema error is not here, the same as 1.0 `Graph.units`.
    pub fn unit_count(&self) -> usize {
        self.unit_count
    }

    /// One topic, or `None` when the index belongs to another build.
    pub fn topic(&self, idx: TopicIdx) -> Option<&Topic> {
        self.topics.get(idx.index())
    }

    /// The external string id of a topic, or `""` for an unknown index.
    pub fn id_of(&self, idx: TopicIdx) -> &str {
        self.topics
            .get(idx.index())
            .map_or("", |topic| topic.id.as_str())
    }

    /// The index of an external string id (D2). The lookup is one hash probe
    /// into the map the build made once.
    pub fn idx_of(&self, id: &str) -> Option<TopicIdx> {
        self.by_id.get(id).copied()
    }

    /// The load index of a topic. Topics sit in load order, so the load index is
    /// the index.
    pub fn load_index(&self, idx: TopicIdx) -> usize {
        idx.index()
    }

    /// The course of a topic — the authored `course` field of its unit file, not
    /// the directory (spec section 1).
    pub fn course_of(&self, idx: TopicIdx) -> &str {
        self.label(self.topic_course.get(idx.index()).copied())
    }

    /// The module of a topic.
    pub fn module_of(&self, idx: TopicIdx) -> &str {
        self.label(self.topic_module.get(idx.index()).copied())
    }

    /// The unit name of a topic.
    pub fn unit_of(&self, idx: TopicIdx) -> &str {
        self.label(self.topic_unit.get(idx.index()).copied())
    }

    /// The topics of a course, ascending by load index.
    pub fn topics_in_course(&self, course_id: &str) -> &[TopicIdx] {
        self.topics_by_course
            .get(course_id)
            .map_or(&[], Vec::as_slice)
    }

    /// The topics of a module, ascending by load index.
    pub fn topics_in_module(&self, module: &str) -> &[TopicIdx] {
        self.topics_by_module.get(module).map_or(&[], Vec::as_slice)
    }

    // -- knowledge points (D2) -------------------------------------------- //

    /// The knowledge points of a topic, in authored order.
    pub fn knowledge_points(&self, idx: TopicIdx) -> &[KnowledgePoint] {
        self.topics
            .get(idx.index())
            .map_or(&[], |topic| topic.knowledge_points.as_slice())
    }

    /// One knowledge point of a topic.
    pub fn knowledge_point(&self, idx: TopicIdx, kp: KpIdx) -> Option<&KnowledgePoint> {
        self.knowledge_points(idx).get(kp.index())
    }

    /// The index of a knowledge point inside a topic. A knowledge point id is
    /// unique inside its topic only, so the address is the pair (spec section 2).
    pub fn kp_idx_of(&self, idx: TopicIdx, kp_id: &str) -> Option<KpIdx> {
        let position = self
            .knowledge_points(idx)
            .iter()
            .position(|kp| kp.id.as_str() == kp_id)?;
        u16::try_from(position).ok().map(KpIdx)
    }

    // -- courses ----------------------------------------------------------- //

    /// The catalog, in file order — not in `order` order (parity trap 1).
    pub fn courses(&self) -> &[Course] {
        &self.courses
    }

    /// One course of the catalog.
    pub fn course(&self, course_id: &str) -> Option<&Course> {
        let position = self.course_by_id.get(course_id).copied()?;
        self.courses.get(position)
    }

    /// The mastery floor of a course, ascending by load index (PEDAGOGY 1).
    ///
    /// The floor is the authored `mastery_floor` list, plus — when
    /// `mastery_floor_course` names a course of the catalog — every topic of
    /// every course whose `order` is at or below the `order` of that course
    /// (parity trap 12). `None` means the catalog has no such course; 1.0 raises
    /// a `KeyError` there.
    ///
    /// A floor entry with no topic is dropped, because the arena speaks in
    /// indices. The lint of U3 grounds its reachability rule on the same
    /// intersection (`floor ∩ known`).
    pub fn mastery_floor(&self, course_id: &str) -> Option<Vec<TopicIdx>> {
        let course = self.course(course_id)?;
        let mut floor: Vec<TopicIdx> = course
            .mastery_floor
            .iter()
            .filter_map(|id| self.idx_of(id.as_str()))
            .collect();
        if let Some(named) = course.mastery_floor_course.as_ref()
            && let Some(reference) = self.course(named.as_str())
        {
            for other in &self.courses {
                if other.order <= reference.order {
                    floor.extend_from_slice(self.topics_in_course(other.id.as_str()));
                }
            }
        }
        floor.sort_unstable();
        floor.dedup();
        Some(floor)
    }

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

    /// `W(a -> dst)` for every node `a`, indexed by node number. This is the
    /// upward failure penalty of PEDAGOGY 4.
    pub fn upward_weights(&self, dst: TopicIdx) -> Vec<f64> {
        graph::relax(&self.enc_rev, dst.as_u32(), self.enc_node_count())
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
        let Some(idx) = self.idx_of(id) else {
            return Vec::new();
        };
        let Some(topic) = self.topic(idx) else {
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

    // -- helpers ----------------------------------------------------------- //

    /// One interned label, or `""` when the key is unknown.
    fn label(&self, key: Option<u32>) -> &str {
        key.and_then(|key| self.labels.get(key as usize))
            .map_or("", String::as_str)
    }
}

/// Read a curriculum tree into an arena, with the parse-stage findings.
///
/// The load tolerates every graph-stage defect and blocks on a fatal
/// parse-stage finding, the same as 1.0 `Graph.load` (parity trap 13). A fatal
/// finding means the parse stage dropped content, so the arena would
/// misrepresent the curriculum; the error carries every finding.
///
/// An advisory finding drops nothing, so the load continues and hands the
/// finding back. The lint runner of U3 reads the parse stage directly with
/// [`parse_curriculum`](super::load::parse_curriculum) and fails on any finding
/// at all, fatal or not.
pub fn load_curriculum(root: &Path) -> Result<(Curriculum, Vec<Finding>), LoadError> {
    let (raw, findings) = load_raw_curriculum(root)?;
    if findings.iter().any(|finding| finding.fatal) {
        return Err(LoadError::Curriculum(CurriculumError::FatalFindings {
            findings,
        }));
    }
    let curriculum = Curriculum::build(raw)?;
    Ok((curriculum, findings))
}

/// Intern one label and return its key.
fn intern(labels: &mut Vec<String>, by_text: &mut HashMap<String, u32>, text: &str) -> u32 {
    if let Some(key) = by_text.get(text) {
        return *key;
    }
    let key = u32::try_from(labels.len()).unwrap_or(u32::MAX);
    labels.push(text.to_owned());
    by_text.insert(text.to_owned(), key);
    key
}

/// Builds both encompassing maps in the 1.0 insertion order.
struct EncBuilder {
    forward: Vec<Vec<EncEdge>>,
    reverse: Vec<Vec<EncEdge>>,
    phantom_ids: Vec<String>,
    phantom_by_id: HashMap<String, EncNode>,
    topic_count: usize,
}

impl EncBuilder {
    fn new(topic_count: usize) -> Self {
        Self {
            forward: vec![Vec::new(); topic_count],
            reverse: vec![Vec::new(); topic_count],
            phantom_ids: Vec::new(),
            phantom_by_id: HashMap::new(),
            topic_count,
        }
    }

    /// The node of a target id. An id with no topic gets a phantom node, the way
    /// 1.0 `_enc_rev.setdefault(dst, {})` makes a phantom key (trap 7).
    fn node_of(&mut self, by_id: &HashMap<String, TopicIdx>, id: &str) -> Option<u32> {
        if let Some(idx) = by_id.get(id) {
            return Some(idx.as_u32());
        }
        if let Some(node) = self.phantom_by_id.get(id) {
            return Some(node.as_u32());
        }
        let raw = u32::try_from(self.topic_count + self.phantom_ids.len()).ok()?;
        self.phantom_ids.push(id.to_owned());
        self.phantom_by_id.insert(id.to_owned(), EncNode(raw));
        self.forward.push(Vec::new());
        self.reverse.push(Vec::new());
        Some(raw)
    }

    /// Record `src -> dst` in both maps, exactly as 1.0 `_add_enc`
    /// (`cadus/graph.py:296-321`).
    ///
    /// The forward map takes the weight only while it beats the 0.0 default, so
    /// a weight-0 edge never appears. The reverse map takes every first edge,
    /// weight 0 included, and then keeps the maximum. The asymmetry is the 1.0
    /// behavior and it is load-bearing (trap 8).
    fn add(&mut self, by_id: &HashMap<String, TopicIdx>, src: u32, dst_id: &str, weight: f64) {
        let Some(dst) = self.node_of(by_id, dst_id) else {
            return;
        };
        if let Some(list) = self.forward.get_mut(src as usize) {
            match list.iter_mut().find(|edge| edge.target == dst) {
                Some(edge) => {
                    if weight > edge.weight {
                        edge.weight = weight;
                    }
                }
                None => {
                    if weight > 0.0 {
                        list.push(EncEdge {
                            target: dst,
                            weight,
                        });
                    }
                }
            }
        }
        if let Some(list) = self.reverse.get_mut(dst as usize) {
            match list.iter_mut().find(|edge| edge.target == src) {
                Some(edge) => {
                    if weight > edge.weight {
                        edge.weight = weight;
                    }
                }
                None => list.push(EncEdge {
                    target: src,
                    weight,
                }),
            }
        }
    }
}
