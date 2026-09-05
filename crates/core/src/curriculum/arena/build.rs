//! The arena build: the port of 1.0 `Graph.__init__` (`cadus/graph.py:229-321`).

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use super::super::graph::{self, Csr, EncCsr, EncEdge};
use super::super::load::{RawCurriculum, RawUnit};
use super::super::model::{Topic, Unit};
use super::{Curriculum, CurriculumError, EncNode, TopicIdx};
use crate::fire::py_max;

impl Curriculum {
    /// Build the arena from a parsed tree.
    ///
    /// Content defects do not stop the build: a dangling prerequisite drops out
    /// of the adjacency, a topic with no knowledge point loads, a cycle loads.
    /// The lint of U3 reports them. Only a duplicate topic id is an error, the
    /// same as 1.0 (trap 13, trap 14).
    pub fn build(raw: RawCurriculum) -> Result<Self, CurriculumError> {
        Self::build_bounded(raw, u32::MAX)
    }

    /// Build the arena with `limit` as the largest topic count an index addresses.
    fn build_bounded(raw: RawCurriculum, limit: u32) -> Result<Self, CurriculumError> {
        let RawCurriculum { catalog, units } = raw;

        let unit_count = units.len();
        let total: usize = units.iter().map(|unit| unit.unit.topics.len()).sum();
        if total > limit as usize {
            return Err(CurriculumError::TooManyTopics {
                count: total,
                limit,
            });
        }

        let mut table = TopicTable::with_capacity(total);
        for RawUnit { unit, .. } in units {
            table.add_unit(unit)?;
        }

        let count = table.topics.len();
        let mut builder = EncBuilder::new(count);
        // The walk order is the load order, the same as the 1.0 loop over
        // `self.topics.items()`. It fixes the insertion order of both
        // encompassing maps, which `relax` replays (trap 9).
        let prereq_lists: Vec<Vec<u32>> = table
            .topics
            .iter()
            .enumerate()
            .map(|(position, topic)| table.parents_of(&mut builder, node(position), topic))
            .collect();
        // 1.0 keeps both directions in a `set`. The sorted lists give the same
        // content and a stable order, and the transpose keeps them sorted.
        let dependent_lists = graph::transpose(&prereq_lists, count);
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

        let (forward, reverse, phantom_ids, phantom_by_id) = builder.finish();
        Ok(Self {
            unit_count,
            topics: table.topics,
            by_id: table.by_id,
            labels: table.labels,
            topic_course: table.topic_course,
            topic_module: table.topic_module,
            topic_unit: table.topic_unit,
            prereqs,
            dependents,
            enc: EncCsr::from_lists(&forward),
            enc_rev: EncCsr::from_lists(&reverse),
            phantom_ids,
            phantom_by_id,
            topo,
            courses: catalog.courses,
            course_by_id,
            topics_by_course: table.topics_by_course,
            topics_by_module: table.topics_by_module,
        })
    }
}

/// The node number of a load position. The build checks the topic count against
/// the index range first, so the conversion never saturates.
fn node(position: usize) -> u32 {
    u32::try_from(position).unwrap_or(u32::MAX)
}

/// The topics of a build in load order, with the interned labels and the
/// per-course and per-module lists.
struct TopicTable {
    topics: Vec<Topic>,
    by_id: HashMap<String, TopicIdx>,
    labels: Vec<String>,
    label_by_text: HashMap<String, u32>,
    topic_course: Vec<u32>,
    topic_module: Vec<u32>,
    topic_unit: Vec<u32>,
    topics_by_course: HashMap<String, Vec<TopicIdx>>,
    topics_by_module: HashMap<String, Vec<TopicIdx>>,
}

impl TopicTable {
    /// An empty table with room for `total` topics.
    fn with_capacity(total: usize) -> Self {
        Self {
            topics: Vec::with_capacity(total),
            by_id: HashMap::with_capacity(total),
            labels: Vec::new(),
            label_by_text: HashMap::new(),
            topic_course: Vec::with_capacity(total),
            topic_module: Vec::with_capacity(total),
            topic_unit: Vec::with_capacity(total),
            topics_by_course: HashMap::new(),
            topics_by_module: HashMap::new(),
        }
    }

    /// Add every topic of one unit file, in authored order.
    ///
    /// The course of a topic is the authored `course` field of the file, never
    /// the directory the file came from (`cadus/graph.py:272`).
    fn add_unit(&mut self, unit: Unit) -> Result<(), CurriculumError> {
        let course = self.intern(unit.course.as_str());
        let module = self.intern(&unit.module);
        let unit_name = self.intern(&unit.unit);
        let course_list = self
            .topics_by_course
            .entry(unit.course.as_str().to_owned())
            .or_default();
        let module_list = self
            .topics_by_module
            .entry(unit.module.clone())
            .or_default();
        for topic in unit.topics {
            let idx = TopicIdx(node(self.topics.len()));
            match self.by_id.entry(topic.id.as_str().to_owned()) {
                Entry::Occupied(seen) => {
                    return Err(CurriculumError::DuplicateTopicId {
                        id: seen.key().clone(),
                    });
                }
                Entry::Vacant(slot) => {
                    slot.insert(idx);
                }
            }
            self.topics.push(topic);
            self.topic_course.push(course);
            self.topic_module.push(module);
            self.topic_unit.push(unit_name);
            course_list.push(idx);
            module_list.push(idx);
        }
        Ok(())
    }

    /// Intern one label and return its key.
    fn intern(&mut self, text: &str) -> u32 {
        if let Some(key) = self.label_by_text.get(text) {
            return *key;
        }
        let key = node(self.labels.len());
        self.labels.push(text.to_owned());
        self.label_by_text.insert(text.to_owned(), key);
        key
    }

    /// The prerequisite nodes of one topic, ascending and distinct, and both
    /// encompassing maps fed with every edge of the topic.
    ///
    /// A prerequisite whose target is not a topic is dropped from the
    /// adjacency; the same target stays in the encompassing maps (trap 7).
    fn parents_of(&self, builder: &mut EncBuilder, src: u32, topic: &Topic) -> Vec<u32> {
        let mut parents: Vec<u32> = Vec::new();
        for edge in &topic.prerequisites {
            if let Some(parent) = self.by_id.get(edge.id.as_str()) {
                parents.push(parent.as_u32());
            }
            builder.add(&self.by_id, src, edge.id.as_str(), edge.weight);
        }
        for edge in &topic.encompassings_extra {
            builder.add(&self.by_id, src, edge.id.as_str(), edge.weight);
        }
        parents.sort_unstable();
        parents.dedup();
        parents
    }
}

/// Builds both encompassing maps in the 1.0 insertion order.
struct EncBuilder {
    forward: HashMap<u32, Vec<EncEdge>>,
    reverse: HashMap<u32, Vec<EncEdge>>,
    phantom_ids: Vec<String>,
    phantom_by_id: HashMap<String, EncNode>,
    topic_count: usize,
}

/// The two maps and the phantom table of a finished build.
type EncMaps = (
    Vec<Vec<EncEdge>>,
    Vec<Vec<EncEdge>>,
    Vec<String>,
    HashMap<String, EncNode>,
);

impl EncBuilder {
    fn new(topic_count: usize) -> Self {
        Self {
            forward: HashMap::new(),
            reverse: HashMap::new(),
            phantom_ids: Vec::new(),
            phantom_by_id: HashMap::new(),
            topic_count,
        }
    }

    /// The node of a target id. An id with no topic gets a phantom node, the way
    /// 1.0 `_enc_rev.setdefault(dst, {})` makes a phantom key (trap 7).
    fn node_of(&mut self, by_id: &HashMap<String, TopicIdx>, id: &str) -> u32 {
        if let Some(idx) = by_id.get(id) {
            return idx.as_u32();
        }
        if let Some(node) = self.phantom_by_id.get(id) {
            return node.as_u32();
        }
        let raw = node(self.topic_count + self.phantom_ids.len());
        self.phantom_ids.push(id.to_owned());
        self.phantom_by_id.insert(id.to_owned(), EncNode(raw));
        raw
    }

    /// Record `src -> dst` in both maps, exactly as 1.0 `_add_enc`
    /// (`cadus/graph.py:296-321`).
    ///
    /// The forward map takes the weight only while it beats the 0.0 default, so
    /// a weight-0 edge never appears. The reverse map takes every first edge,
    /// weight 0 included, and then keeps the maximum. The asymmetry is the 1.0
    /// behavior and it is load-bearing (trap 8).
    fn add(&mut self, by_id: &HashMap<String, TopicIdx>, src: u32, dst_id: &str, weight: f64) {
        let dst = self.node_of(by_id, dst_id);
        merge_edge(self.forward.entry(src).or_default(), dst, weight, false);
        merge_edge(self.reverse.entry(dst).or_default(), src, weight, true);
    }

    /// The two maps as one edge list per node, topics first and phantoms after.
    fn finish(self) -> EncMaps {
        let node_count = self.topic_count + self.phantom_ids.len();
        (
            lists_of(self.forward, node_count),
            lists_of(self.reverse, node_count),
            self.phantom_ids,
            self.phantom_by_id,
        )
    }
}

/// Raise the weight of the edge to `target`, or add the edge. A new edge with
/// weight 0 enters the list only when `keep_zero` is set.
fn merge_edge(list: &mut Vec<EncEdge>, target: u32, weight: f64, keep_zero: bool) {
    match list.iter_mut().find(|edge| edge.target == target) {
        Some(edge) => edge.weight = py_max(edge.weight, weight),
        None if keep_zero || weight > 0.0 => list.push(EncEdge { target, weight }),
        None => {}
    }
}

/// One edge list per node, in node order. A node with no edge gets an empty list.
fn lists_of(mut map: HashMap<u32, Vec<EncEdge>>, node_count: usize) -> Vec<Vec<EncEdge>> {
    (0_u32..)
        .take(node_count)
        .map(|node| map.remove(&node).unwrap_or_default())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::super::load::RawCurriculum;
    use super::super::super::model::{AnswerKind, Catalog, Course, PrereqEdge, Slug, Topic, Unit};
    use super::super::CurriculumError;
    use super::*;

    /// One unit of one course with `count` topics, each one a prerequisite of
    /// the next.
    fn raw(count: u32) -> RawCurriculum {
        let topics = (0..count)
            .map(|index| Topic {
                id: Slug::new(format!("t{index}")).unwrap(),
                name: format!("t{index}"),
                core: true,
                difficulty: 0.5,
                drill: false,
                answer_kind: AnswerKind::Numeric,
                expected_time_secs: 30,
                prerequisites: index
                    .checked_sub(1)
                    .map(|parent| PrereqEdge {
                        id: Slug::new(format!("t{parent}")).unwrap(),
                        weight: 0.5,
                        key: false,
                    })
                    .into_iter()
                    .collect(),
                encompassings_extra: Vec::new(),
                knowledge_points: Vec::new(),
                diagnostic_exemplar: None,
                anki_seeds: Vec::new(),
            })
            .collect();
        RawCurriculum {
            catalog: Catalog {
                courses: vec![Course {
                    id: Slug::new("c").unwrap(),
                    name: "C".to_owned(),
                    order: 1,
                    mastery_floor: Vec::new(),
                    mastery_floor_course: None,
                }],
            },
            units: vec![RawUnit {
                course_id: "c".to_owned(),
                file_name: "00.yaml".to_owned(),
                unit: Unit {
                    unit: "u".to_owned(),
                    course: Slug::new("c").unwrap(),
                    module: "m".to_owned(),
                    topics,
                },
                first_load_index: 0,
            }],
        }
    }

    /// A tree with more topics than the index range holds is the one build
    /// error a topic count causes.
    #[test]
    fn a_topic_count_past_the_index_limit_stops_the_build() {
        let error = Curriculum::build_bounded(raw(3), 2).err();
        assert_eq!(
            error,
            Some(CurriculumError::TooManyTopics { count: 3, limit: 2 })
        );
        assert_eq!(
            error.map(|error| error.to_string()),
            Some("3 topics exceed the 2 an index can address".to_owned())
        );
        let mut repeated = raw(2);
        let first = repeated.units[0].unit.topics[0].clone();
        repeated.units[0].unit.topics.push(first);
        assert_eq!(
            Curriculum::build_bounded(repeated, 10).err(),
            Some(CurriculumError::DuplicateTopicId {
                id: "t0".to_owned()
            })
        );
        let built = Curriculum::build_bounded(raw(2), 2).unwrap();
        assert_eq!(built.topic_count(), 2);
        assert_eq!(built.topo_order(), [TopicIdx(0), TopicIdx(1)]);
        assert_eq!(built.courses().len(), 1);
    }
}
