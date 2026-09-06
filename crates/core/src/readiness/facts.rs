//! The curriculum half of the readiness audit: the facts one pass over the
//! arena settles, before the store says a word.

use std::collections::{BTreeMap, BTreeSet};

use crate::curriculum::{Curriculum, KnowledgePoint, Topic};
use crate::learner::problem_text_hash;
use crate::pool::kp_key;

use super::visual::visual_needed;
use super::{ContentIndex, HELD_OUT_MINIMUM};

/// What the curriculum alone says about one knowledge point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KpFacts {
    /// The serving key `"<topic>/<kp>"`.
    pub kp_key: String,
    /// The topic id.
    pub topic_id: String,
    /// The knowledge-point id.
    pub kp_id: String,
    /// The author indexes of the exemplars the grammar decides, ascending, with
    /// a repeated problem statement counted once.
    pub decidable: Vec<usize>,
    /// The author index of the held-out exemplar: the LAST decidable one when
    /// the knowledge point holds [`HELD_OUT_MINIMUM`] of them or more.
    pub held_out: Option<usize>,
    /// Every practice exemplar carries a `solution_sketch`.
    pub solutions: bool,
    /// The topic text names a visual (a heuristic), or the author wrote one.
    pub visual_needed: bool,
    /// The authored visuals that pass [`crate::visual::VisualSpec::validate`].
    pub valid_visuals: usize,
    /// The authored visuals that FAIL the check.
    ///
    /// A failed visual counts as absent, so it never clears the visual blocker.
    /// The count reaches the report, because an author fixes a broken figure and
    /// never writes a second one beside it.
    pub broken_visuals: usize,
}

impl KpFacts {
    /// The decidable exemplars practice draws from: all of them, less the
    /// held-out one.
    #[must_use]
    pub fn practice_exemplars(&self) -> usize {
        self.decidable
            .len()
            .saturating_sub(usize::from(self.held_out.is_some()))
    }

    /// The decidable items practice draws from, templates included.
    #[must_use]
    pub fn practice_items<C: ContentIndex + ?Sized>(&self, content: &C) -> usize {
        self.practice_exemplars()
            .saturating_add(content.approved_templates(&self.kp_key))
    }
}

/// The curriculum half of the audit, built once per process.
///
/// The build costs one [`canonical_form`] call per authored exemplar answer, so
/// a caller builds it beside the arena and shares it. Every later question is a
/// map lookup.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReadinessIndex {
    /// The facts, in curriculum order.
    kps: Vec<KpFacts>,
    /// Serving key to its position in `kps`.
    by_key: BTreeMap<String, usize>,
    /// Topic id to the positions of its knowledge points, in authored order.
    topic_kps: BTreeMap<String, Vec<usize>>,
    /// Topic ids, in load order.
    topic_order: Vec<String>,
    /// Topic id to the prerequisite topic ids the tree also holds.
    topic_prereqs: BTreeMap<String, Vec<String>>,
    /// Topic id to the course of its unit file.
    course_of: BTreeMap<String, String>,
}

impl ReadinessIndex {
    /// Read the curriculum half of the audit out of the arena.
    #[must_use]
    pub fn build(curriculum: &Curriculum) -> Self {
        let mut index = Self::default();
        for (position, topic) in curriculum.topics().iter().enumerate() {
            let topic_id = topic.id.as_str().to_owned();
            let course = topic_course(curriculum, position);
            index.course_of.insert(topic_id.clone(), course);
            index.topic_order.push(topic_id.clone());
            index
                .topic_prereqs
                .insert(topic_id.clone(), existing_prereqs(curriculum, topic));
            let mut positions = Vec::new();
            for kp in &topic.knowledge_points {
                positions.push(index.kps.len());
                let facts = kp_facts(topic, kp);
                index.by_key.insert(facts.kp_key.clone(), index.kps.len());
                index.kps.push(facts);
            }
            index.topic_kps.insert(topic_id, positions);
        }
        index
    }

    /// Every knowledge point, in curriculum order.
    #[must_use]
    pub fn facts(&self) -> &[KpFacts] {
        &self.kps
    }

    /// The facts of one serving key.
    #[must_use]
    pub fn get(&self, kp_key: &str) -> Option<&KpFacts> {
        self.by_key.get(kp_key).and_then(|at| self.kps.get(*at))
    }

    /// The facts of the knowledge points of one topic, in authored order.
    #[must_use]
    pub fn topic(&self, topic_id: &str) -> Vec<&KpFacts> {
        self.topic_kps
            .get(topic_id)
            .map(|positions| {
                positions
                    .iter()
                    .filter_map(|at| self.kps.get(*at))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The topic ids, in load order.
    #[must_use]
    pub fn topics(&self) -> &[String] {
        &self.topic_order
    }

    /// The prerequisite topic ids of one topic that the tree also holds.
    #[must_use]
    pub fn prerequisites(&self, topic_id: &str) -> &[String] {
        self.topic_prereqs.get(topic_id).map_or(&[], Vec::as_slice)
    }

    /// The course of one topic, or `""` for a topic the tree does not hold.
    #[must_use]
    pub fn course_of(&self, topic_id: &str) -> &str {
        self.course_of.get(topic_id).map_or("", String::as_str)
    }
}

/// The course of the topic at one load position.
fn topic_course(curriculum: &Curriculum, position: usize) -> String {
    u32::try_from(position)
        .map(crate::curriculum::TopicIdx::from_u32)
        .map(|idx| curriculum.course_of(idx).to_owned())
        .unwrap_or_default()
}

/// The prerequisite topics of one topic that the tree also holds, sorted and
/// without a repeat.
///
/// A prerequisite whose target is not a topic is dropped, the same as the arena
/// drops it from both adjacency directions.
fn existing_prereqs(curriculum: &Curriculum, topic: &Topic) -> Vec<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    for edge in &topic.prerequisites {
        let id = edge.id.as_str();
        if curriculum.idx_of(id).is_some() {
            out.insert(id.to_owned());
        }
    }
    out.into_iter().collect()
}

/// The curriculum facts of one knowledge point.
///
/// The decidable list drops an exemplar the answer grammar refuses (V2) and an
/// exemplar whose problem statement repeats an earlier one, because the pool
/// serves one statement once ([`crate::pool::ExemplarSource`]).
fn kp_facts(topic: &Topic, kp: &KnowledgePoint) -> KpFacts {
    let mut decidable: Vec<usize> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for (index, exemplar) in kp.exemplars.iter().enumerate() {
        if exemplar.canonical_answer().is_err() {
            continue;
        }
        if !seen.insert(problem_text_hash(&exemplar.problem)) {
            continue;
        }
        decidable.push(index);
    }
    let held_out = if decidable.len() >= HELD_OUT_MINIMUM {
        decidable.last().copied()
    } else {
        None
    };
    let solutions = decidable
        .iter()
        .filter(|index| Some(**index) != held_out)
        .all(|index| {
            kp.exemplars
                .get(*index)
                .and_then(|exemplar| exemplar.solution_sketch.as_deref())
                .is_some_and(|sketch| !sketch.trim().is_empty())
        });
    let text = format!("{} {} {}", topic.id.as_str(), topic.name, kp.name);
    let valid_visuals = kp
        .visuals
        .iter()
        .filter(|visual| visual.validate().is_ok())
        .count();
    KpFacts {
        kp_key: kp_key(topic.id.as_str(), kp.id.as_str()),
        topic_id: topic.id.as_str().to_owned(),
        kp_id: kp.id.as_str().to_owned(),
        decidable,
        held_out,
        solutions,
        visual_needed: visual_needed(&text) || !kp.visuals.is_empty(),
        valid_visuals,
        broken_visuals: kp.visuals.len() - valid_visuals,
    }
}
