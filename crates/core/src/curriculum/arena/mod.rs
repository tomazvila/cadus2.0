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

mod build;
mod queries;

use std::collections::HashMap;
use std::path::Path;

use super::finding::Finding;
use super::graph::{Csr, EncCsr, EncEdge};
use super::load::{ParseError, load_raw_curriculum};
use super::model::{Course, KnowledgePoint, Topic};

/// Define one `u32` index type of the arena.
macro_rules! u32_index {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u32);

        impl $name {
            /// Wrap a raw index. An index out of range yields `None` or an empty
            /// slice from every query, so this never makes a panic reachable.
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
    };
}

u32_index!(
    /// A topic of one build, numbered by load order (D2).
    TopicIdx
);

u32_index!(
    /// A node of the encompassing maps: every topic, plus every dangling
    /// `encompassings_extra` target 1.0 keeps as a phantom key (trap 7).
    ///
    /// The first [`Curriculum::topic_count`] nodes are the topics, in load order, so
    /// `EncNode(t.as_u32())` is the node of `TopicIdx(t)`.
    EncNode
);

/// A knowledge point inside one topic, numbered by authored order (D2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KpIdx(u16);

impl KpIdx {
    /// Wrap a raw index.
    pub const fn from_u16(raw: u16) -> Self {
        Self(raw)
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
