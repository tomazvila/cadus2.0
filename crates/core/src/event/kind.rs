//! The enumerations of the 1.0 models (`model.py:37-89`).

use serde::{Deserialize, Serialize};

/// The kind of task an event reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TaskType {
    /// A lesson on one topic.
    #[serde(rename = "lesson")]
    Lesson,
    /// A spaced review of one topic.
    #[serde(rename = "review")]
    Review,
    /// A quiz over several topics.
    #[serde(rename = "quiz")]
    Quiz,
    /// A speed drill on one topic.
    #[serde(rename = "drill")]
    Drill,
    /// A placement diagnostic question.
    #[serde(rename = "diagnostic")]
    Diagnostic,
    /// A multi-step task over several component topics.
    #[serde(rename = "multi-step")]
    MultiStep,
}

impl TaskType {
    /// The 1.0 wire spelling of the variant (`TaskType`, `model.py:49-61`).
    ///
    /// The selector builds a task id out of it, so the text is load-bearing:
    /// a multi-step task id reads `{session}-multi-step`, with the hyphen.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Lesson => "lesson",
            Self::Review => "review",
            Self::Quiz => "quiz",
            Self::Drill => "drill",
            Self::Diagnostic => "diagnostic",
            Self::MultiStep => "multi-step",
        }
    }
}

/// The grader's work-quality tier. It also spells `quality_tier` on a result event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum WorkQuality {
    /// Complete and correct work.
    #[serde(rename = "perfect")]
    Perfect,
    /// Correct work with a cosmetic flaw.
    #[serde(rename = "nearly_perfect")]
    NearlyPerfect,
    /// Work that passes.
    #[serde(rename = "passable")]
    Passable,
    /// Work just below the pass line.
    #[serde(rename = "nearly_passable")]
    NearlyPassable,
    /// Work well below the pass line.
    #[serde(rename = "poor")]
    Poor,
    /// No real attempt.
    #[serde(rename = "blowoff")]
    Blowoff,
}

/// The scheduling status of one topic.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub enum TopicStatus {
    /// Never seen.
    #[default]
    #[serde(rename = "untouched")]
    Untouched,
    /// Ready to learn.
    #[serde(rename = "frontier")]
    Frontier,
    /// Learned and on the review schedule.
    #[serde(rename = "learning")]
    Learning,
    /// Placed by the diagnostic.
    #[serde(rename = "placed")]
    Placed,
    /// Below the mastery floor of the enrolled course.
    #[serde(rename = "floor")]
    Floor,
}

/// The progress of one knowledge point inside a topic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum KpProgress {
    /// The knowledge point passed.
    #[serde(rename = "passed")]
    Passed,
    /// The knowledge point failed once.
    #[serde(rename = "failed_once")]
    FailedOnce,
    /// The knowledge point failed twice.
    #[serde(rename = "failed_twice")]
    FailedTwice,
}

/// The shape of the answer a learner gave.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AnswerKind {
    /// A single number.
    #[serde(rename = "numeric")]
    Numeric,
    /// An algebraic expression.
    #[serde(rename = "expression")]
    Expression,
    /// A worked sequence of steps.
    #[serde(rename = "multi-step")]
    MultiStep,
    /// A proof.
    #[serde(rename = "proof")]
    Proof,
}

/// Why a course enrollment happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EnrollReason {
    /// The learner dropped to a prerequisite course to fill a gap.
    #[serde(rename = "gap-fill")]
    GapFill,
    /// The learner came back from a gap-fill course.
    #[serde(rename = "gap-return")]
    GapReturn,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_task_type_spells_its_wire_name() {
        let names = [
            (TaskType::Lesson, "lesson"),
            (TaskType::Review, "review"),
            (TaskType::Quiz, "quiz"),
            (TaskType::Drill, "drill"),
            (TaskType::Diagnostic, "diagnostic"),
            (TaskType::MultiStep, "multi-step"),
        ];
        for (kind, name) in names {
            assert_eq!(kind.as_str(), name);
            let quoted = format!("\"{name}\"");
            assert_eq!(serde_json::to_string(&kind).expect("a name writes"), quoted);
        }
    }
}
