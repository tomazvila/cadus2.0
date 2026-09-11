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

/// The graded outcome of one attempt (D-F2).
///
/// The third value is the one v1 has no room for. A checker that refuses an answer,
/// and an answer kind that no checker decides, both give [`AttemptOutcome::Ungraded`].
/// An ungraded attempt is NOT a miss: the fold ignores it, and no learner state moves.
///
/// The wire form is the external tag: `"correct"`, `"incorrect"`, and
/// `{"ungraded":{"reason":"..."}}`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptOutcome {
    /// The checker decided the answer is mathematically correct (C4).
    Correct,
    /// The checker decided the answer is mathematically wrong (C4).
    Incorrect,
    /// No deterministic verdict exists. The reason names why.
    Ungraded {
        /// Why the answer has no verdict, in one phrase.
        reason: String,
    },
}

impl AttemptOutcome {
    /// The outcome that `correct` alone spells.
    ///
    /// `correct: bool` stays on the wire and equals `outcome == Correct`, so a
    /// v1 reader keeps its meaning and a v2 row repeats nothing (C2).
    #[must_use]
    pub const fn of_correct(correct: bool) -> Self {
        if correct {
            Self::Correct
        } else {
            Self::Incorrect
        }
    }

    /// Whether `correct` alone carries this outcome.
    ///
    /// The writer skips a derivable outcome, so a v1 row that this build reads and
    /// writes back keeps its bytes (C2).
    #[must_use]
    pub const fn is_derivable(&self) -> bool {
        !matches!(*self, Self::Ungraded { .. })
    }

    /// Whether the outcome has no deterministic verdict.
    #[must_use]
    pub const fn is_ungraded(&self) -> bool {
        matches!(*self, Self::Ungraded { .. })
    }

    /// The reason of an ungraded outcome, or `None` for a decided one.
    #[must_use]
    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::Ungraded { reason } => Some(reason.as_str()),
            Self::Correct | Self::Incorrect => None,
        }
    }

    /// The wire spelling of the variant, without the reason.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match *self {
            Self::Correct => "correct",
            Self::Incorrect => "incorrect",
            Self::Ungraded { .. } => "ungraded",
        }
    }
}

/// Where the served item came from (D-F9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemSource {
    /// An authored exemplar of the knowledge point.
    Exemplar,
    /// One instantiation of an approved template.
    Template,
    /// One step of an integrated task.
    Integrated,
    /// A delayed retention probe.
    Probe,
    /// A future generated ordinary item.
    Generator,
}

/// Whether the learner saw this item digest before (D-F9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Exposure {
    /// The first time this digest reached the learner.
    First,
    /// A digest the learner already saw.
    Repeat,
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

    #[test]
    fn the_third_outcome_reads_and_writes_its_wire_form() {
        let decided = [
            (AttemptOutcome::Correct, "\"correct\"", true),
            (AttemptOutcome::Incorrect, "\"incorrect\"", false),
        ];
        for (outcome, text, correct) in decided {
            assert_eq!(AttemptOutcome::of_correct(correct), outcome);
            assert!(outcome.is_derivable());
            assert!(!outcome.is_ungraded());
            assert_eq!(outcome.reason(), None);
            assert_eq!(
                serde_json::to_string(&outcome).expect("the outcome writes"),
                text
            );
        }
        let ungraded = AttemptOutcome::Ungraded {
            reason: "no deterministic verdict for a proof".to_owned(),
        };
        assert!(!ungraded.is_derivable());
        assert!(ungraded.is_ungraded());
        assert_eq!(ungraded.as_str(), "ungraded");
        assert_eq!(
            ungraded.reason(),
            Some("no deterministic verdict for a proof")
        );
        let text = serde_json::to_string(&ungraded).expect("the outcome writes");
        assert_eq!(
            text,
            r#"{"ungraded":{"reason":"no deterministic verdict for a proof"}}"#
        );
        let back: AttemptOutcome = serde_json::from_str(&text).expect("the outcome reads");
        assert_eq!(back, ungraded);
        assert_eq!(AttemptOutcome::Correct.as_str(), "correct");
        assert_eq!(AttemptOutcome::Incorrect.as_str(), "incorrect");
    }

    #[test]
    fn the_evidence_enumerations_spell_their_wire_names() {
        let sources = [
            (ItemSource::Exemplar, "\"exemplar\""),
            (ItemSource::Template, "\"template\""),
            (ItemSource::Integrated, "\"integrated\""),
            (ItemSource::Probe, "\"probe\""),
        ];
        for (source, text) in sources {
            assert_eq!(serde_json::to_string(&source).expect("a name writes"), text);
        }
        for (exposure, text) in [
            (Exposure::First, "\"first\""),
            (Exposure::Repeat, "\"repeat\""),
        ] {
            assert_eq!(
                serde_json::to_string(&exposure).expect("a name writes"),
                text
            );
        }
    }
}
