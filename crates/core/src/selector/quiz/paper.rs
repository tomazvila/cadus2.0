//! The paper form of an exam set topic (D-VBE3, D-VBE4).
//!
//! A set topic stands for one part of the state exam. A part II set holds four
//! pools of items. A part I set holds two pools: one-point items and two-point
//! items. A paper round asks the first items of each pool, in authored order.
//! An item that a model grades goes last.

use std::collections::BTreeMap;

use crate::curriculum::Curriculum;
use crate::event::QuizResult;

/// The number of items that a paper takes from each of the first four
/// knowledge points, in order. This is the one definition of the form.
/// It is the form of part II.
pub const PAPER_FORM: [usize; 4] = [10, 5, 3, 2];

/// The number of items that a part I paper takes from each of its two pools:
/// 24 one-point items and 8 two-point items, 40 points in all (D-VBE4).
pub const PAPER_FORM_I: [usize; 2] = [24, 8];

/// The points of part I of the state exam.
pub const PART_I_POINTS: u32 = 40;

/// The points of part II of the state exam.
pub const PART_II_POINTS: u32 = 60;

/// The points of both parts together.
pub const EXAM_POINTS: u32 = 100;

/// The lowest exam points that pass the exam (the sheet: 25 of 100).
pub const EXAM_PASS_POINTS: u32 = 25;

/// The lowest exam points that give the score 100. The sheet gives the score
/// `x + 5` for 25 to 95 points, so 95 points give 100.
pub const EXAM_TOP_POINTS: u32 = 95;

/// The suffix of the id of a part I set topic.
pub const PART_I_SUFFIX: &str = "-i";

/// A part of the state exam.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExamPart {
    /// Part I: 40 points, short items of 1 or 2 points.
    One,
    /// Part II: 60 points, long items.
    Two,
}

impl ExamPart {
    /// The part number, 1 or 2.
    #[must_use]
    pub const fn number(self) -> u8 {
        match self {
            Self::One => 1,
            Self::Two => 2,
        }
    }

    /// The part for a number, or `None` for any other number.
    #[must_use]
    pub const fn from_number(number: u8) -> Option<Self> {
        match number {
            1 => Some(Self::One),
            2 => Some(Self::Two),
            _ => None,
        }
    }

    /// The items that a paper of this part takes from each pool, in order.
    #[must_use]
    pub const fn form(self) -> &'static [usize] {
        match self {
            Self::One => &PAPER_FORM_I,
            Self::Two => &PAPER_FORM,
        }
    }

    /// The points of this part.
    #[must_use]
    pub const fn max_points(self) -> u32 {
        match self {
            Self::One => PART_I_POINTS,
            Self::Two => PART_II_POINTS,
        }
    }

    /// The points of this part that stand for the score 100: the share of
    /// [`EXAM_TOP_POINTS`] that this part holds, rounded up.
    #[must_use]
    pub const fn target_points(self) -> u32 {
        (EXAM_TOP_POINTS * self.max_points()).div_ceil(EXAM_POINTS)
    }

    /// The share of this part that a paper must reach to count as right.
    #[must_use]
    pub fn pass_share(self) -> f64 {
        f64::from(self.target_points()) / f64::from(self.max_points())
    }

    /// The name of this part in a sentence: `part I` or `part II`.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::One => "part I",
            Self::Two => "part II",
        }
    }
}

/// The part of the set topic `set`. The topic field `exam_part` wins. A topic
/// with no field is part I when its id ends in `-i`, and part II otherwise.
#[must_use]
pub fn part_of(graph: &Curriculum, set: &str) -> ExamPart {
    let from_field = graph
        .idx_of(set)
        .and_then(|idx| graph.topic(idx))
        .and_then(|topic| topic.exam_part)
        .and_then(ExamPart::from_number);
    from_field.unwrap_or(if set.ends_with(PART_I_SUFFIX) {
        ExamPart::One
    } else {
        ExamPart::Two
    })
}

/// The form key of a set topic id: the id without the part I suffix. The two
/// set topics of one form share this key.
#[must_use]
pub fn form_key(set: &str) -> &str {
    set.strip_suffix(PART_I_SUFFIX).unwrap_or(set)
}

/// The other part of the same form: the set topic of the same course with the
/// same [`form_key`] and the other part. `None` when the form has one part only.
#[must_use]
pub fn sibling_set(graph: &Curriculum, set: &str) -> Option<String> {
    let idx = graph.idx_of(set)?;
    let part = part_of(graph, set);
    let key = form_key(set);
    graph
        .topics_in_course(graph.course_of(idx))
        .iter()
        .filter_map(|&other| graph.topic(other))
        .map(|topic| topic.id.as_str())
        .find(|id| *id != set && form_key(id) == key && part_of(graph, id) != part)
        .map(str::to_owned)
}

/// Whether the paper of `set` serves every item of every knowledge point, in
/// authored order (the topic field `paper_form: whole`), and not the form of its
/// part.
#[must_use]
pub fn is_whole_paper(graph: &Curriculum, set: &str) -> bool {
    graph
        .idx_of(set)
        .and_then(|idx| graph.topic(idx))
        .is_some_and(|topic| topic.paper_form.as_deref() == Some("whole"))
}

/// The share of the points of a paper of `set` that counts as right: the share
/// of [`EXAM_TOP_POINTS`] in 100. A `whole` paper takes that share of the sum
/// of all its item points, rounded up to a whole point.
#[must_use]
pub fn pass_share_of(graph: &Curriculum, set: &str) -> f64 {
    if !is_whole_paper(graph, set) {
        return part_of(graph, set).pass_share();
    }
    let total: u32 = graph
        .idx_of(set)
        .map(|idx| {
            graph
                .knowledge_points(idx)
                .iter()
                .flat_map(|point| point.exemplars.iter())
                .map(|item| u32::from(item.points.unwrap_or(1)))
                .sum()
        })
        .unwrap_or(0);
    if total == 0 {
        return 1.0;
    }
    f64::from((EXAM_TOP_POINTS * total).div_ceil(EXAM_POINTS)) / f64::from(total)
}

/// The set id inside a paper `quiz_id`: `<session>-round-paper-<set>-<n>`.
#[must_use]
pub fn paper_set_of(quiz_id: &str) -> Option<&str> {
    let (_, rest) = quiz_id.split_once("-round-paper-")?;
    let (set, number) = rest.rsplit_once('-')?;
    (!number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())).then_some(set)
}

/// The score of the state exam for a sum of exam points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExamScore {
    /// Under 25 points: the exam is not passed.
    NotPassed,
    /// The score from 30 to 100.
    Score(u32),
}

/// The score by the sheet of the National Agency for Education: 25 to 95
/// points give `x + 5`, 96 to 100 points give 100, and under 25 points the exam
/// is not passed. Fractional points count by their whole part.
#[must_use]
pub fn exam_score(points: u32) -> ExamScore {
    if points < EXAM_PASS_POINTS {
        ExamScore::NotPassed
    } else if points > EXAM_TOP_POINTS {
        ExamScore::Score(100)
    } else {
        ExamScore::Score(points + 5)
    }
}

/// The plain line for the exam points of both parts.
#[must_use]
pub fn exam_line(points: u32) -> String {
    match exam_score(points) {
        ExamScore::Score(score) => format!("Exam points {points} of {EXAM_POINTS}. Score {score}."),
        ExamScore::NotPassed => format!(
            "Exam points {points} of {EXAM_POINTS}. Not passed ({EXAM_PASS_POINTS} points pass)."
        ),
    }
}

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
    let whole = is_whole_paper(graph, set);
    if points.len() < if whole { 1 } else { 2 } {
        return Err(PaperRefusal::TooFewPoints);
    }
    let mut decided: Vec<PaperItem> = Vec::new();
    let mut written: Vec<PaperItem> = Vec::new();
    let part_form = part_of(graph, set).form();
    for (position, point) in points.iter().enumerate() {
        let count = if whole {
            usize::MAX
        } else if let Some(&count) = part_form.get(position) {
            count
        } else {
            break;
        };
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

/// The exam sets of a course: its topics whose first knowledge points hold at
/// least the items of the form of their part. Empty when the course has no
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
            if topic.paper_form.as_deref() == Some("whole") {
                return topic
                    .knowledge_points
                    .iter()
                    .any(|point| !point.exemplars.is_empty());
            }
            let form = part_of(graph, topic.id.as_str()).form();
            topic.knowledge_points.len() >= form.len()
                && topic
                    .knowledge_points
                    .iter()
                    .zip(form)
                    .all(|(point, &count)| point.exemplars.len() >= count)
        })
        .map(|topic| topic.id.as_str().to_owned())
        .collect()
}

/// The newest result of each exam set, from the quiz results in the order of
/// the log (oldest first). A result of an ordinary round is not in the map.
#[must_use]
pub fn newest_paper_results<'a>(
    results: impl IntoIterator<Item = &'a QuizResult>,
) -> BTreeMap<String, &'a QuizResult> {
    let mut newest = BTreeMap::new();
    for result in results {
        if let Some(set) = paper_set_of(&result.quiz_id) {
            newest.insert(set.to_owned(), result);
        }
    }
    newest
}

/// The whole exam points that a finished paper of `set` earned: the score times
/// the points of the part, cut to whole points. `None` for a result that has
/// unmarked answers.
#[must_use]
pub fn paper_exam_points(graph: &Curriculum, set: &str, result: &QuizResult) -> Option<u32> {
    if result.inconclusive {
        return None;
    }
    let max = part_of(graph, set).max_points();
    Some(whole_points(result.score * f64::from(max), max))
}

/// Cut a point count to whole points, at most `max`.
#[must_use]
pub fn whole_points(points: f64, max: u32) -> u32 {
    let cut = (points + 1e-9).floor().max(0.0);
    // The cast is exact: `cut` is a whole number from 0 to `max`.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "cut is a whole number from 0 up"
    )]
    let whole = cut.min(f64::from(max)) as u32;
    whole
}
