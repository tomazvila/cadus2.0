//! Active worked examples: the optional `step_check` and `try_first` blocks of a
//! knowledge point (NEW IN 2.0).
//!
//! Both blocks sit beside the authored teach page of the knowledge point and
//! turn its reading into an action:
//!
//! - `step_check` points at one step of the worked example and asks why that
//!   step holds. The learner picks one of three or four options before the
//!   practice button appears (self-explanation, Chi).
//! - `try_first` is a motivating problem the learner attempts BEFORE the worked
//!   example, for a knowledge point that introduces a genuinely new idea
//!   (productive failure, Kapur). Any outcome is fine: the attempt never counts
//!   for mastery, XP, or scheduling.
//!
//! Hard Rule 1 holds for both: the teach route sends the question, the options
//! and the try-first problem, and only the check route, after the learner
//! committed, returns `answer`, `why` and `reveal`.
//!
//! The blocks are absent from the canonical dump, so they do not move the M1
//! curriculum hash. They are part of the serialized topics that
//! [`super::review_context_digest`] hashes, so adding or editing a block moves
//! the curriculum review context and stales every approval stamped under the
//! old one. `cadus_store::content::restamp_content_currency` re-stamps the
//! approved rows at boot, and that keeps approved content serving after such an
//! edit.

use serde::{Deserialize, Serialize};

use crate::answer::AnswerContract;

/// The fewest options a step check offers.
pub const STEP_CHECK_MIN_OPTIONS: usize = 3;

/// The most options a step check offers.
pub const STEP_CHECK_MAX_OPTIONS: usize = 4;

/// The longest option of a step check is at most this many times the shortest,
/// as the fraction [`STEP_CHECK_LENGTH_RATIO_NUM`] over this denominator.
pub const STEP_CHECK_LENGTH_RATIO_DEN: usize = 5;

/// The numerator of the longest-to-shortest option length bound (8 / 5 = 1.6).
pub const STEP_CHECK_LENGTH_RATIO_NUM: usize = 8;

/// Which step of the worked example a step check points at.
///
/// A number is the 1-based position of the step. A string is a fragment of the
/// step's text: the first step that contains it is the step. The text form
/// survives a re-authored page that inserts a step; a page that drops the
/// fragment drops the check rather than point at the wrong step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StepRef {
    /// The 1-based position of the step.
    Number(u32),
    /// A fragment of the step's text.
    Text(String),
}

impl StepRef {
    /// The 0-based index of the step this reference names in `steps`, or
    /// `None` when no step matches.
    #[must_use]
    pub fn resolve(&self, steps: &[String]) -> Option<usize> {
        match self {
            Self::Number(number) => {
                let index = usize::try_from(*number).ok()?.checked_sub(1)?;
                (index < steps.len()).then_some(index)
            }
            Self::Text(fragment) => {
                let fragment = fragment.trim();
                if fragment.is_empty() {
                    return None;
                }
                steps.iter().position(|step| step.contains(fragment))
            }
        }
    }
}

/// A focused self-explanation question on one step of the worked example.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepCheck {
    /// The step the question is about.
    pub step: StepRef,
    /// The question, e.g. "Which rule justifies step 2?".
    pub question: String,
    /// Three or four short options; the wrong ones are plausible misconceptions.
    pub options: Vec<String>,
    /// The correct option, verbatim one of `options`.
    pub answer: String,
    /// One sentence shown after the pick.
    pub why: String,
}

impl StepCheck {
    /// Refuse a check a learner cannot answer.
    ///
    /// # Errors
    ///
    /// The message names the broken rule in words an author acts on.
    pub fn validate(&self) -> Result<(), String> {
        match &self.step {
            StepRef::Number(0) => {
                return Err("step_check.step: steps are numbered from 1".to_owned());
            }
            StepRef::Text(text) if text.trim().is_empty() => {
                return Err(
                    "step_check.step: give a step number or a non-empty fragment of the step text"
                        .to_owned(),
                );
            }
            _ => {}
        }
        non_empty(&self.question, "step_check.question")?;
        non_empty(&self.why, "step_check.why")?;
        let count = self.options.len();
        if !(STEP_CHECK_MIN_OPTIONS..=STEP_CHECK_MAX_OPTIONS).contains(&count) {
            return Err(format!(
                "step_check.options: give {STEP_CHECK_MIN_OPTIONS} to {STEP_CHECK_MAX_OPTIONS} \
                 options, not {count}"
            ));
        }
        for (index, option) in self.options.iter().enumerate() {
            non_empty(option, &format!("step_check.options.{index}"))?;
            if self.options[..index]
                .iter()
                .any(|earlier| earlier.trim() == option.trim())
            {
                return Err(format!(
                    "step_check.options.{index}: the option {option:?} appears twice"
                ));
            }
        }
        if !self
            .options
            .iter()
            .any(|option| option.trim() == self.answer.trim())
        {
            return Err(format!(
                "step_check.answer {:?} is not one of its options {:?}; copy the correct option \
                 verbatim into 'answer'",
                self.answer, self.options
            ));
        }
        self.validate_option_lengths()
    }

    /// Refuse a check whose option lengths give the answer away.
    ///
    /// A learner who guesses the longest option, or the one that stands out,
    /// must not score: the correct option is not strictly longer than every
    /// other option, and the longest option is at most 1.6 times the shortest.
    fn validate_option_lengths(&self) -> Result<(), String> {
        let lengths: Vec<usize> = self
            .options
            .iter()
            .map(|option| option.trim().chars().count())
            .collect();
        let answer = self.answer.trim();
        let answer_length = answer.chars().count();
        let longest_other = self
            .options
            .iter()
            .zip(&lengths)
            .filter(|(option, _)| option.trim() != answer)
            .map(|(_, length)| *length)
            .max()
            .unwrap_or(0);
        if answer_length > longest_other {
            return Err(format!(
                "step_check.answer is the unique longest option ({answer_length} characters, the \
                 next longest has {longest_other}); a learner can guess it from its length, so \
                 trim it or lengthen a distractor"
            ));
        }
        let longest = lengths.iter().copied().max().unwrap_or(0);
        let shortest = lengths.iter().copied().min().unwrap_or(0);
        if longest * STEP_CHECK_LENGTH_RATIO_DEN > shortest * STEP_CHECK_LENGTH_RATIO_NUM {
            return Err(format!(
                "step_check.options: the longest option has {longest} characters and the \
                 shortest {shortest}; keep the longest within 1.6 times the shortest so no \
                 option stands out"
            ));
        }
        Ok(())
    }

    /// Whether `choice` is the correct option.
    #[must_use]
    pub fn is_correct(&self, choice: &str) -> bool {
        choice.trim() == self.answer.trim()
    }

    /// Whether `choice` is one of the offered options.
    #[must_use]
    pub fn offers(&self, choice: &str) -> bool {
        self.options
            .iter()
            .any(|option| option.trim() == choice.trim())
    }
}

/// A motivating problem the learner attempts before the worked example.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TryFirst {
    /// The problem statement.
    pub problem: String,
    /// The checker policy of `answer`.
    pub answer_contract: AnswerContract,
    /// The expected answer, graded by the ordinary checker.
    pub answer: String,
    /// One or two sentences linking the attempt to the idea of the lesson.
    pub reveal: String,
}

impl TryFirst {
    /// Refuse a try-first problem the checker cannot grade or that collides
    /// with a served exemplar.
    ///
    /// `exemplar_problems` are the authored problems of the same knowledge
    /// point. The check route reveals the try-first answer, so a try-first
    /// problem that is also a served exemplar would hand that answer out
    /// before the learner attempts it (Hard Rule 1).
    ///
    /// # Errors
    ///
    /// The message names the broken rule in words an author acts on.
    pub fn validate<'a>(
        &self,
        mut exemplar_problems: impl Iterator<Item = &'a str>,
    ) -> Result<(), String> {
        non_empty(&self.problem, "try_first.problem")?;
        non_empty(&self.answer, "try_first.answer")?;
        non_empty(&self.reveal, "try_first.reveal")?;
        if let Err(reason) = self.answer_contract.validate_expected(&self.answer) {
            return Err(format!(
                "try_first.answer {:?} does not grade under its answer_contract: {}",
                self.answer, reason.reason
            ));
        }
        if exemplar_problems.any(|problem| problem.trim() == self.problem.trim()) {
            return Err(
                "try_first.problem repeats an exemplar of this knowledge point; the reveal would \
                 hand out a served answer (Hard Rule 1), so use a different problem"
                    .to_owned(),
            );
        }
        Ok(())
    }
}

fn non_empty(value: &str, field: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{field}: must be a non-empty string"))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check() -> StepCheck {
        StepCheck {
            step: StepRef::Number(2),
            question: "Which rule justifies step 2?".to_owned(),
            options: vec!["aa".to_owned(), "bb".to_owned(), "cc".to_owned()],
            answer: "bb".to_owned(),
            why: "Because.".to_owned(),
        }
    }

    #[test]
    fn a_step_reference_resolves_by_position_or_fragment() {
        let steps = vec!["Add 5: x = 7.".to_owned(), "Divide by 3: x = 2.".to_owned()];
        assert_eq!(StepRef::Number(2).resolve(&steps), Some(1));
        assert_eq!(StepRef::Number(3).resolve(&steps), None);
        assert_eq!(StepRef::Number(0).resolve(&steps), None);
        assert_eq!(
            StepRef::Text("Divide by".to_owned()).resolve(&steps),
            Some(1)
        );
        assert_eq!(StepRef::Text("Multiply".to_owned()).resolve(&steps), None);
    }

    #[test]
    fn an_answer_outside_the_options_is_refused_by_name() {
        let mut bad = check();
        bad.answer = "dd".to_owned();
        let message = bad.validate().unwrap_err();
        assert!(message.contains("step_check.answer \"dd\" is not one of its options"));
        assert!(check().validate().is_ok());
    }

    #[test]
    fn option_count_and_duplicates_are_refused() {
        let mut two = check();
        two.options.truncate(2);
        assert!(
            two.validate()
                .unwrap_err()
                .contains("give 3 to 4 options, not 2")
        );
        let mut twice = check();
        twice.options[2] = "aa".to_owned();
        assert!(twice.validate().unwrap_err().contains("appears twice"));
    }

    #[test]
    fn an_answer_longer_than_every_other_option_is_refused() {
        let mut longest = check();
        longest.options[1] = "bbb".to_owned();
        longest.answer = "bbb".to_owned();
        let message = longest.validate().unwrap_err();
        assert!(
            message.contains("the unique longest option (3 characters, the next longest has 2)"),
            "{message}"
        );
        // A tie for the longest option gives nothing away.
        let mut tied = longest.clone();
        tied.options[0] = "aaa".to_owned();
        assert!(tied.validate().is_ok(), "{:?}", tied.validate());
        // Length counts characters, not bytes.
        let mut accented = check();
        accented.options[1] = "\u{e9}\u{e9}".to_owned();
        accented.answer = "\u{e9}\u{e9}".to_owned();
        assert!(accented.validate().is_ok(), "{:?}", accented.validate());
    }

    #[test]
    fn an_option_far_longer_than_the_shortest_is_refused() {
        let mut ratio = check();
        ratio.options = vec![
            "a".repeat(10),
            "b".repeat(16),
            "c".repeat(16),
            "d".repeat(16),
        ];
        ratio.answer = "b".repeat(16);
        assert!(
            ratio.validate().is_ok(),
            "1.6 times the shortest is allowed"
        );
        ratio.options[2] = "c".repeat(17);
        let message = ratio.validate().unwrap_err();
        assert!(
            message.contains("the longest option has 17 characters and the shortest 10"),
            "{message}"
        );
    }
}
