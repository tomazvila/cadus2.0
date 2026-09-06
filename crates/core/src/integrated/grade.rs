//! Grading one integrated task: every step, then the final answer (D-F10).
//!
//! The whole item is graded in one call, because the item is one problem. Each
//! field is decided by [`check_contract`], the same acceptance rule the rest of
//! the service uses (D-F1). Nothing here calls a model, and nothing here reads
//! the learner's prose: a reasoning note is recorded and never graded.

use serde::{Deserialize, Serialize};

use crate::answer::{Outcome, check_contract};

use super::model::{Field, IntegratedItem};

/// The id the final answer carries in a submission and in a grade.
pub const FINAL_FIELD_ID: &str = "final";

/// One answered field of a submission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldResponse {
    /// The step id, or [`FINAL_FIELD_ID`] for the final answer.
    pub id: String,
    /// What the learner wrote.
    pub answer: String,
    /// How many hints of this field's ladder the learner opened.
    #[serde(default)]
    pub hints_used: usize,
}

/// One learner submission of a whole integrated task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Submission {
    /// The method option the learner chose, when the item offers a choice.
    #[serde(default)]
    pub method: Option<String>,
    /// The answered steps, in any order.
    #[serde(default)]
    pub steps: Vec<FieldResponse>,
    /// The final answer.
    pub final_answer: FieldResponse,
    /// The learner's own words about the model they used.
    ///
    /// It is recorded, shown back, and NEVER graded: no rule of this module
    /// reads it, so no prose is auto-corrected.
    #[serde(default)]
    pub reasoning: Option<String>,
}

impl Submission {
    /// The response for `id`, when the submission holds one.
    fn response(&self, id: &str) -> Option<&FieldResponse> {
        self.steps.iter().find(|step| step.id == id)
    }
}

/// The decision on one field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldGrade {
    /// The step id, or [`FINAL_FIELD_ID`].
    pub id: String,
    /// True when the learner submitted an answer for this field.
    pub answered: bool,
    /// True when the answer matches an authored answer under the contract.
    pub correct: bool,
    /// True when the checker reached no verdict (D-F2). No credit follows.
    pub ungraded: bool,
    /// True when the value matched only under the period-grouping reading.
    pub notation: bool,
    /// True when the learner opened a hint of this field before answering.
    pub assisted: bool,
    /// The knowledge points this field exercises.
    pub skills: Vec<String>,
}

/// The decision on the method choice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MethodGrade {
    /// The option the learner chose, when they chose one.
    pub chosen: Option<String>,
    /// True when the chosen method solves the scenario.
    pub correct: bool,
    /// Why the chosen method works, or why it fails.
    pub why: Option<String>,
}

/// The decision on one whole integrated task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegratedGrade {
    /// The item that was graded.
    pub item_id: String,
    /// The digest of the served statement (D-F9 `item_digest`).
    pub item_digest: String,
    /// The method choice, when the item offers one.
    pub method: Option<MethodGrade>,
    /// One decision per authored step, in author order.
    pub steps: Vec<FieldGrade>,
    /// The decision on the final answer.
    pub final_grade: FieldGrade,
    /// How many steps are correct.
    pub correct_steps: usize,
    /// How many steps the item has.
    pub total_steps: usize,
    /// True when the final answer is correct.
    pub solved: bool,
    /// True when the learner opened any hint of the task.
    pub assisted: bool,
    /// True when some field reached no verdict, so a human must grade it.
    pub ungraded: bool,
    /// The knowledge points the correct fields exercise, in first-seen order.
    ///
    /// This is the attribution a progression hook credits (D-F9). A field that
    /// is wrong or ungraded credits nothing.
    pub skills_credited: Vec<String>,
    /// What the final number means, released with the grade.
    pub interpretation: String,
    /// True when the learner wrote a reasoning note.
    ///
    /// The note is stored beside the attempt and shown back to the learner. The
    /// service never marks it right or wrong.
    pub reasoning_recorded: bool,
}

/// Grade one field against its authored answers.
fn grade_field(
    id: &str,
    field: &Field,
    skills: &[String],
    answer: Option<&FieldResponse>,
) -> FieldGrade {
    let mut grade = FieldGrade {
        id: id.to_owned(),
        answered: answer.is_some(),
        correct: false,
        ungraded: false,
        notation: false,
        assisted: answer.is_some_and(|response| response.hints_used > 0),
        skills: skills.to_vec(),
    };
    let Some(response) = answer else {
        return grade;
    };
    let mut undecided = false;
    for expected in std::iter::once(&field.answer).chain(field.accept_also.iter()) {
        match check_contract(expected, &response.answer, field.contract) {
            Outcome::Decided(verdict) if verdict.correct => {
                grade.correct = true;
                grade.notation = verdict.notation;
                return grade;
            }
            Outcome::Decided(_) => {}
            Outcome::Undecidable(_) => undecided = true,
        }
    }
    // Every authored form was checked and none matched. An undecidable check on
    // the way means the service has no verdict to stand on, so the field is
    // ungraded and credits nothing (D-F2).
    grade.ungraded = undecided;
    grade
}

/// The knowledge points of a step, as wire strings.
fn skills_of(slugs: &[crate::curriculum::Slug]) -> Vec<String> {
    slugs.iter().map(|slug| slug.as_str().to_owned()).collect()
}

/// Grade the method choice of `item` against `chosen`.
fn grade_method(item: &IntegratedItem, chosen: Option<&String>) -> Option<MethodGrade> {
    let method = item.method.as_ref()?;
    let picked = chosen.and_then(|id| {
        method
            .options
            .iter()
            .find(|option| option.id.as_str() == id)
    });
    Some(MethodGrade {
        chosen: chosen.cloned(),
        correct: picked.is_some_and(|option| option.correct),
        why: picked.and_then(|option| option.why.clone()),
    })
}

/// Grade one whole integrated task.
///
/// A step the submission leaves out is `answered: false` and credits nothing:
/// an unanswered step is not a wrong step, and the reply tells them apart.
#[must_use]
pub fn grade(item: &IntegratedItem, submission: &Submission) -> IntegratedGrade {
    let steps: Vec<FieldGrade> = item
        .steps
        .iter()
        .map(|step| {
            let id = step.id.as_str();
            grade_field(
                id,
                &step.ask,
                &skills_of(&step.skills),
                submission.response(id),
            )
        })
        .collect();
    let final_grade = grade_field(
        FINAL_FIELD_ID,
        &item.final_answer.ask,
        &skills_of(&item.final_answer.skills),
        Some(&submission.final_answer),
    );
    let correct_steps = steps.iter().filter(|step| step.correct).count();
    let assisted = steps
        .iter()
        .chain(std::iter::once(&final_grade))
        .any(|field| field.assisted);
    let ungraded = steps
        .iter()
        .chain(std::iter::once(&final_grade))
        .any(|field| field.ungraded);
    let mut skills_credited: Vec<String> = Vec::new();
    for field in steps.iter().chain(std::iter::once(&final_grade)) {
        if !field.correct {
            continue;
        }
        for skill in &field.skills {
            if !skills_credited.contains(skill) {
                skills_credited.push(skill.clone());
            }
        }
    }
    IntegratedGrade {
        item_id: item.id.as_str().to_owned(),
        item_digest: item.digest(),
        method: grade_method(item, submission.method.as_ref()),
        correct_steps,
        total_steps: steps.len(),
        solved: final_grade.correct,
        assisted,
        ungraded,
        skills_credited,
        interpretation: item.final_answer.interpretation.clone(),
        reasoning_recorded: submission
            .reasoning
            .as_ref()
            .is_some_and(|note| !note.trim().is_empty()),
        steps,
        final_grade,
    }
}
