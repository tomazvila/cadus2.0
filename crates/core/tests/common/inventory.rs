//! The walk of one course for the answer-shape inventory (unit f1-inventory).
//!
//! The walk loads the checked-in curriculum tree with the real loader, visits
//! every topic, knowledge point and exemplar of one course in load order, and
//! runs the real answer grammar on every authored answer. It reports one
//! [`Row`] per exemplar.
//!
//! The grammar entry is [`canonical_form`], which normalizes, parses and
//! canonicalizes one answer string. The entry reads no answer kind, so the
//! verdict states whether the AUTHORED answer is inside the decidable grammar
//! and never whether the topic gate serves it.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};

use cadus_core::answer::canonical_form;
use cadus_core::curriculum::{Curriculum, Exemplar, KnowledgePoint, Topic};

use super::shape::{Shape, shape_of};

/// The course of the inventory (audit finding (i)).
pub const COURSE: &str = "foundations";

/// The verdict of a decided answer.
pub const DECIDED: &str = "decided";

/// One classified exemplar answer.
#[derive(Debug, Clone)]
pub struct Row {
    /// The course id.
    pub course: String,
    /// The unit of the topic. One unit is one file of the course directory.
    pub unit: String,
    /// The topic id.
    pub topic_id: String,
    /// The task complexity of the topic, as the wire value of `answer_kind`.
    pub answer_kind: &'static str,
    /// The knowledge point id.
    pub kp_id: String,
    /// The position of the exemplar inside the knowledge point.
    pub exemplar_index: usize,
    /// The authored answer, as the file holds it.
    pub answer: String,
    /// The shape of the authored answer.
    pub shape: Shape,
    /// `decided`, or `undecidable(<reason>)`.
    pub verdict: String,
    /// Whether the exemplar carries a solution sketch.
    pub has_solution_sketch: bool,
}

impl Row {
    /// The JSON line of the row.
    #[must_use]
    pub fn json(&self) -> String {
        serde_json::json!({
            "course": self.course,
            "unit": self.unit,
            "topic_id": self.topic_id,
            "answer_kind": self.answer_kind,
            "kp_id": self.kp_id,
            "exemplar_index": self.exemplar_index,
            "answer": self.answer,
            "shape": self.shape.as_str(),
            "verdict": self.verdict,
            "has_solution_sketch": self.has_solution_sketch,
        })
        .to_string()
    }

    /// Whether the grammar decides the authored answer.
    #[must_use]
    pub fn decided(&self) -> bool {
        self.verdict == DECIDED
    }
}

/// The verdict of the answer grammar on one authored answer.
#[must_use]
pub fn verdict_of(answer: &str) -> String {
    match canonical_form(answer) {
        Ok(_) => DECIDED.to_owned(),
        Err(refusal) => format!("undecidable({})", refusal.reason),
    }
}

/// Every exemplar answer of one course, in load order.
#[must_use]
pub fn rows(curriculum: &Curriculum, course: &str) -> Vec<Row> {
    let mut out = Vec::new();
    for idx in curriculum.topics_in_course(course) {
        let Some(topic) = curriculum.topic(*idx) else {
            continue;
        };
        let unit = curriculum.unit_of(*idx);
        for kp in &topic.knowledge_points {
            for (index, exemplar) in kp.exemplars.iter().enumerate() {
                out.push(row(course, unit, topic, kp, index, exemplar));
            }
        }
    }
    out
}

/// One row of the inventory.
fn row(
    course: &str,
    unit: &str,
    topic: &Topic,
    kp: &KnowledgePoint,
    index: usize,
    exemplar: &Exemplar,
) -> Row {
    Row {
        course: course.to_owned(),
        unit: unit.to_owned(),
        topic_id: topic.id.as_str().to_owned(),
        answer_kind: topic.answer_kind.as_str(),
        kp_id: kp.id.as_str().to_owned(),
        exemplar_index: index,
        answer: exemplar.answer.clone(),
        shape: shape_of(&exemplar.answer),
        verdict: verdict_of(&exemplar.answer),
        has_solution_sketch: exemplar.solution_sketch.is_some(),
    }
}

/// Every knowledge point of one course, in load order, with its topic.
#[must_use]
pub fn knowledge_points<'a>(
    curriculum: &'a Curriculum,
    course: &str,
) -> Vec<(&'a Topic, &'a KnowledgePoint)> {
    let mut out = Vec::new();
    for idx in curriculum.topics_in_course(course) {
        let Some(topic) = curriculum.topic(*idx) else {
            continue;
        };
        for kp in &topic.knowledge_points {
            out.push((topic, kp));
        }
    }
    out
}

/// The count of DISTINCT decidable exemplars of one knowledge point.
///
/// An exemplar counts when the grammar decides its authored answer. Two
/// exemplars with the same problem text and the same answer text are one item,
/// because the serve path draws one problem and not one row (D-F5,
/// "practicable").
#[must_use]
pub fn distinct_decidable(kp: &KnowledgePoint) -> usize {
    let mut seen = BTreeSet::new();
    for exemplar in &kp.exemplars {
        if canonical_form(&exemplar.answer).is_ok() {
            seen.insert((exemplar.problem.trim(), exemplar.answer.trim()));
        }
    }
    seen.len()
}

/// The distribution of [`distinct_decidable`] over the knowledge points of one
/// course: the count of knowledge points per number of distinct decidable
/// exemplars.
#[must_use]
pub fn distinct_decidable_counts(curriculum: &Curriculum, course: &str) -> BTreeMap<usize, usize> {
    let mut out = BTreeMap::new();
    for (_, kp) in knowledge_points(curriculum, course) {
        *out.entry(distinct_decidable(kp)).or_insert(0) += 1;
    }
    out
}

/// The count of rows per shape, keyed by the wire value of the shape.
#[must_use]
pub fn shape_counts(rows: &[Row]) -> BTreeMap<&'static str, usize> {
    let mut out = BTreeMap::new();
    for row in rows {
        *out.entry(row.shape.as_str()).or_insert(0) += 1;
    }
    out
}

/// The count of rows per verdict.
#[must_use]
pub fn verdict_counts(rows: &[Row]) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for row in rows {
        *out.entry(row.verdict.clone()).or_insert(0) += 1;
    }
    out
}
