//! The paper form of an exam set topic (D-VBE3).
//!
//! A set topic holds four pools of items. A paper round asks the first items of
//! each pool, in authored order. An item that a model grades goes last.

use crate::curriculum::Curriculum;

/// The number of items that a paper takes from each of the first four
/// knowledge points, in order. This is the one definition of the form.
pub const PAPER_FORM: [usize; 4] = [10, 5, 3, 2];

/// One item of a paper: a knowledge point and the position of the exemplar in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaperItem {
    /// The id of the knowledge point.
    pub kp: String,
    /// The 0-based position of the exemplar in the knowledge point.
    pub index: usize,
}

/// The prefix of a paper entry in the `mix` of a quiz task.
pub const PAPER_ENTRY: &str = "paper:";

impl PaperItem {
    /// The entry of this item in the `mix` of the quiz task of `set`:
    /// `paper:<set>:<kp>:<index>`.
    #[must_use]
    pub fn entry(&self, set: &str) -> String {
        format!("{PAPER_ENTRY}{set}:{}:{}", self.kp, self.index)
    }
}

/// Read a `mix` entry of a paper task as `(set, kp, index)`.
/// Any other entry gives `None`.
#[must_use]
pub fn parse_paper_entry(entry: &str) -> Option<(&str, &str, usize)> {
    let rest = entry.strip_prefix(PAPER_ENTRY)?;
    let mut fields = rest.splitn(3, ':');
    let set = fields.next()?;
    let kp = fields.next()?;
    let index = fields.next()?.parse().ok()?;
    Some((set, kp, index))
}

/// The reason a paper round cannot start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaperRefusal {
    /// The topic does not exist, or its course has no `exam_weights`.
    NotAnExamSet,
    /// The topic has fewer than 2 knowledge points.
    TooFewPoints,
}

impl PaperRefusal {
    /// The plain sentence for the learner.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::NotAnExamSet => "This topic is not an exam set. Pick an exam set of your course.",
            Self::TooFewPoints => {
                "This exam set has too few question groups to make a paper. Pick another exam set."
            }
        }
    }
}

/// The items of the paper of `set`, in the order the learner meets them.
///
/// # Errors
///
/// [`PaperRefusal`] when `set` is no topic of a course with `exam_weights`, or
/// when it holds fewer than 2 knowledge points.
pub fn paper_form(graph: &Curriculum, set: &str) -> Result<Vec<PaperItem>, PaperRefusal> {
    let idx = graph.idx_of(set).ok_or(PaperRefusal::NotAnExamSet)?;
    let weighted = graph
        .course(graph.course_of(idx))
        .is_some_and(|course| course.exam_weights.is_some());
    if !weighted {
        return Err(PaperRefusal::NotAnExamSet);
    }
    let points = graph.knowledge_points(idx);
    if points.len() < 2 {
        return Err(PaperRefusal::TooFewPoints);
    }
    let mut decided: Vec<PaperItem> = Vec::new();
    let mut written: Vec<PaperItem> = Vec::new();
    for (point, count) in points.iter().zip(PAPER_FORM) {
        for (index, exemplar) in point.exemplars.iter().enumerate().take(count) {
            let item = PaperItem {
                kp: point.id.as_str().to_owned(),
                index,
            };
            if exemplar.is_written() {
                written.push(item);
            } else {
                decided.push(item);
            }
        }
    }
    decided.extend(written);
    Ok(decided)
}

/// The exam sets of a course: its topics whose first four knowledge points
/// hold at least the items of [`PAPER_FORM`]. Empty when the course has no
/// `exam_weights`.
#[must_use]
pub fn exam_sets(graph: &Curriculum, course_id: &str) -> Vec<String> {
    let weighted = graph
        .course(course_id)
        .is_some_and(|course| course.exam_weights.is_some());
    if !weighted {
        return Vec::new();
    }
    graph
        .topics_in_course(course_id)
        .iter()
        .filter_map(|&idx| graph.topic(idx))
        .filter(|topic| {
            topic.knowledge_points.len() >= PAPER_FORM.len()
                && topic
                    .knowledge_points
                    .iter()
                    .zip(PAPER_FORM)
                    .all(|(point, count)| point.exemplars.len() >= count)
        })
        .map(|topic| topic.id.as_str().to_owned())
        .collect()
}
