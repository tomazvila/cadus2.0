//! The prerequisite and diagnostic coverage audit of unit f10.
//!
//! # The three questions
//!
//! 1. **Does a prerequisite edge point at a topic the learner can practice?**
//!    An edge whose target is absent from the tree teaches nothing, and an edge
//!    whose target has no practicable knowledge point sends a learner who fails
//!    the dependent topic to a topic with no question.
//! 2. **Can the diagnostic ask about this topic?** `crate::diagnostic` drops
//!    every topic with no `diagnostic_exemplar` from the probe set, and the
//!    grader drops an exemplar whose authored answer the grammar does not
//!    decide. Both leave a topic the placement never measures.
//! 3. **What does the course ASSUME?** A course seeds its `mastery_floor` topics
//!    as mastered, so no lesson and no probe ever visits them. That is an
//!    assumption, and this audit inventories it with the evidence that supports
//!    it: a decidable diagnostic item confirms the assumption, and a practicable
//!    knowledge point remediates it.
//!
//! # What the audit never does
//!
//! It grants no readiness. A floor topic with no confirmation item stays a floor
//! topic with no confirmation item, and the report says so.

use std::collections::{BTreeMap, BTreeSet};

use crate::curriculum::Curriculum;

use super::facts::ReadinessIndex;

/// What the diagnostic can ask about one topic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DiagnosticState {
    /// The topic authors a `diagnostic_exemplar` and the grammar decides it.
    Decidable,
    /// The topic authors one and the grammar does NOT decide the answer, so the
    /// probe cannot be graded.
    Undecidable,
    /// The topic authors none, so the probe set drops the topic.
    Missing,
}

impl DiagnosticState {
    /// The wire value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Decidable => "decidable",
            Self::Undecidable => "undecidable",
            Self::Missing => "missing",
        }
    }
}

/// The coverage of one topic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicCoverage {
    /// The topic id.
    pub topic_id: String,
    /// The course of the unit file.
    pub course: String,
    /// What the diagnostic can ask.
    pub diagnostic: DiagnosticState,
    /// The authored prerequisite edges.
    pub prerequisites: usize,
    /// The prerequisite ids the tree does not hold. The arena drops each one
    /// from both adjacency directions, so the learner never reaches it.
    pub dangling: Vec<String>,
    /// The prerequisite topics the tree holds and that have no practicable
    /// knowledge point.
    pub unpracticable: Vec<String>,
    /// The knowledge points of the topic.
    pub knowledge_points: usize,
    /// The knowledge points with at least three decidable practice items.
    pub practicable_kps: usize,
    /// The courses that seed this topic as mastered (`mastery_floor`), sorted.
    ///
    /// A course names a floor with `mastery_floor` or with
    /// `mastery_floor_course`, and the second form seeds every topic of every
    /// earlier course. A topic of Foundations therefore reaches this list
    /// through the courses that build on it, and not through Foundations.
    pub assumed_by: Vec<String>,
}

impl TopicCoverage {
    /// Whether any course seeds this topic as mastered.
    #[must_use]
    pub fn assumed_mastery(&self) -> bool {
        !self.assumed_by.is_empty()
    }

    /// Whether a learner can practice this topic at all.
    #[must_use]
    pub const fn practicable(&self) -> bool {
        self.practicable_kps > 0
    }

    /// Whether an ASSUMED topic carries the evidence the assumption needs.
    ///
    /// The evidence is two-part, and the audit asks for both: a decidable
    /// diagnostic item CONFIRMS the assumption, and a practicable knowledge
    /// point REMEDIATES it when the confirmation fails. A floor topic with
    /// neither is an assumption with no evidence at all.
    #[must_use]
    pub const fn floor_evidence(&self) -> FloorEvidence {
        FloorEvidence {
            confirmable: matches!(self.diagnostic, DiagnosticState::Decidable),
            remediable: self.practicable_kps > 0,
        }
    }
}

/// The evidence behind one assumed-mastery topic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FloorEvidence {
    /// A decidable diagnostic item can confirm the assumption.
    pub confirmable: bool,
    /// A practicable knowledge point can remediate a failed assumption.
    pub remediable: bool,
}

impl FloorEvidence {
    /// Whether both halves hold.
    #[must_use]
    pub const fn complete(self) -> bool {
        self.confirmable && self.remediable
    }
}

/// The coverage of every topic of one curriculum.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PrereqCoverage {
    /// One row per topic, in curriculum load order.
    pub topics: Vec<TopicCoverage>,
}

impl PrereqCoverage {
    /// Read the coverage out of the arena and the curriculum half of the audit.
    ///
    /// The call is pure and costs one pass over the topics after
    /// [`ReadinessIndex::build`].
    #[must_use]
    pub fn build(curriculum: &Curriculum, index: &ReadinessIndex) -> Self {
        let floors = floor_topics(curriculum);
        let practicable = practicable_topics(index);
        let mut topics = Vec::new();
        for topic_id in index.topics() {
            let facts = index.topic(topic_id);
            let practicable_kps = facts
                .iter()
                .filter(|kp| practicable_kp(kp.practice_exemplars()))
                .count();
            let prereqs = index.prerequisites(topic_id);
            let unpracticable = prereqs
                .iter()
                .filter(|id| !practicable.contains(*id))
                .cloned()
                .collect();
            topics.push(TopicCoverage {
                topic_id: topic_id.clone(),
                course: index.course_of(topic_id).to_owned(),
                diagnostic: diagnostic_state(curriculum, topic_id),
                prerequisites: authored_prereqs(curriculum, topic_id),
                dangling: dangling_prereqs(curriculum, topic_id),
                unpracticable,
                knowledge_points: facts.len(),
                practicable_kps,
                assumed_by: floors.get(topic_id).cloned().unwrap_or_default(),
            });
        }
        Self { topics }
    }

    /// The rows of one course.
    #[must_use]
    pub fn course(&self, course: &str) -> Vec<&TopicCoverage> {
        self.topics
            .iter()
            .filter(|row| row.course == course)
            .collect()
    }

    /// The course ids, in first-appearance order.
    #[must_use]
    pub fn courses(&self) -> Vec<String> {
        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        for row in &self.topics {
            if seen.insert(row.course.clone()) {
                out.push(row.course.clone());
            }
        }
        out
    }

    /// The counts one course reports.
    #[must_use]
    pub fn counts(&self, course: &str) -> CoverageCounts {
        let rows = self.course(course);
        let mut counts = CoverageCounts {
            topics: rows.len(),
            ..CoverageCounts::default()
        };
        for row in rows {
            match row.diagnostic {
                DiagnosticState::Decidable => counts.diagnostic_decidable += 1,
                DiagnosticState::Undecidable => counts.diagnostic_undecidable += 1,
                DiagnosticState::Missing => counts.diagnostic_missing += 1,
            }
            counts.prerequisites += row.prerequisites;
            counts.dangling += row.dangling.len();
            counts.unpracticable_edges += row.unpracticable.len();
            if row.practicable() {
                counts.practicable_topics += 1;
            }
            if row.assumed_mastery() {
                counts.assumed += 1;
                let evidence = row.floor_evidence();
                if evidence.confirmable {
                    counts.assumed_confirmable += 1;
                }
                if evidence.remediable {
                    counts.assumed_remediable += 1;
                }
                if !evidence.complete() {
                    counts.assumed_without_evidence += 1;
                }
            }
        }
        counts
    }
}

/// The totals of one course.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CoverageCounts {
    /// The topics of the course.
    pub topics: usize,
    /// The topics with at least one practicable knowledge point.
    pub practicable_topics: usize,
    /// The topics the diagnostic can ask about and grade.
    pub diagnostic_decidable: usize,
    /// The topics whose diagnostic answer the grammar refuses.
    pub diagnostic_undecidable: usize,
    /// The topics with no diagnostic item.
    pub diagnostic_missing: usize,
    /// The authored prerequisite edges.
    pub prerequisites: usize,
    /// The prerequisite edges whose target the tree does not hold.
    pub dangling: usize,
    /// The prerequisite edges that point at a topic with no practice.
    pub unpracticable_edges: usize,
    /// The topics the course seeds as mastered.
    pub assumed: usize,
    /// The seeded topics a decidable diagnostic item can confirm.
    pub assumed_confirmable: usize,
    /// The seeded topics a practicable knowledge point can remediate.
    pub assumed_remediable: usize,
    /// The seeded topics with one half of the evidence missing, or both.
    pub assumed_without_evidence: usize,
}

/// Every topic a course seeds as mastered, and the courses that seed it.
fn floor_topics(curriculum: &Curriculum) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for course in curriculum.courses() {
        let course_id = course.id.as_str();
        let Some(floor) = curriculum.mastery_floor(course_id) else {
            continue;
        };
        for idx in floor {
            out.entry(curriculum.id_of(idx).to_owned())
                .or_default()
                .insert(course_id.to_owned());
        }
    }
    out.into_iter()
        .map(|(topic, courses)| (topic, courses.into_iter().collect()))
        .collect()
}

/// Whether one knowledge point holds enough authored practice.
///
/// The count reads the EXEMPLARS alone, because this audit runs over the
/// curriculum and no store. An approved template raises the real number, and
/// [`super::ReadinessSet`] is the place that reads it.
fn practicable_kp(practice_exemplars: usize) -> bool {
    practice_exemplars >= super::PRACTICE_MINIMUM
}

/// The topic ids with at least one practicable knowledge point.
fn practicable_topics(index: &ReadinessIndex) -> BTreeSet<String> {
    let mut counts: BTreeMap<&str, bool> = BTreeMap::new();
    for facts in index.facts() {
        let ok = practicable_kp(facts.practice_exemplars());
        let entry = counts.entry(facts.topic_id.as_str()).or_insert(false);
        *entry = *entry || ok;
    }
    counts
        .into_iter()
        .filter(|(_, ok)| *ok)
        .map(|(id, _)| id.to_owned())
        .collect()
}

/// What the diagnostic can ask about one topic.
fn diagnostic_state(curriculum: &Curriculum, topic_id: &str) -> DiagnosticState {
    let exemplar = curriculum
        .idx_of(topic_id)
        .and_then(|idx| curriculum.topic(idx))
        .and_then(|topic| topic.diagnostic_exemplar.as_ref());
    match exemplar {
        None => DiagnosticState::Missing,
        Some(item) if item.canonical_answer().is_ok() => DiagnosticState::Decidable,
        Some(_) => DiagnosticState::Undecidable,
    }
}

/// The authored prerequisite edges of one topic, dangling ones included.
fn authored_prereqs(curriculum: &Curriculum, topic_id: &str) -> usize {
    curriculum
        .idx_of(topic_id)
        .and_then(|idx| curriculum.topic(idx))
        .map_or(0, |topic| topic.prerequisites.len())
}

/// The prerequisite ids of one topic that the tree does not hold.
fn dangling_prereqs(curriculum: &Curriculum, topic_id: &str) -> Vec<String> {
    let Some(topic) = curriculum
        .idx_of(topic_id)
        .and_then(|idx| curriculum.topic(idx))
    else {
        return Vec::new();
    };
    let mut out: BTreeSet<String> = BTreeSet::new();
    for edge in &topic.prerequisites {
        if curriculum.idx_of(edge.id.as_str()).is_none() {
            out.insert(edge.id.as_str().to_owned());
        }
    }
    out.into_iter().collect()
}
