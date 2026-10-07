//! Active worked examples: the optional `try_first` block of a knowledge point
//! (NEW IN 2.0).
//!
//! The block sits beside the authored teach page of the knowledge point and
//! turns its reading into an action. `try_first` is a motivating problem the
//! learner attempts BEFORE the worked example, for a knowledge point that
//! introduces a genuinely new idea (productive failure, Kapur). Any outcome is
//! fine: the attempt never counts for mastery, XP, or scheduling.
//!
//! Hard Rule 1 holds: the teach route sends the try-first problem, and only the
//! check route, after the learner committed, returns `answer` and `reveal`.
//!
//! The block is absent from the canonical dump, so it does not move the M1
//! curriculum hash. It is part of the serialized topics that
//! [`super::review_context_digest`] hashes, so adding or editing a block moves
//! the curriculum review context and stales every approval stamped under the
//! old one. `cadus_store::content::restamp_content_currency` re-stamps the
//! approved rows at boot, and that keeps approved content serving after such an
//! edit.

use serde::{Deserialize, Serialize};

use crate::answer::AnswerContract;

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
