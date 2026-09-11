//! Assistance comes from committed server reveals, independently of client counters.
use cadus_core::integrated::{FINAL_FIELD_ID, IntegratedGrade, Submission};
use std::collections::BTreeMap;

pub(super) fn submission(submission: &mut Submission, used: &BTreeMap<String, usize>) {
    for step in &mut submission.steps {
        step.hints_used = used.get(&step.id).copied().unwrap_or(0);
    }
    submission.final_answer.hints_used = used.get(FINAL_FIELD_ID).copied().unwrap_or(0);
}

pub(super) fn verdict(result: &mut IntegratedGrade, used: &BTreeMap<String, usize>) {
    // Revealing a hint also assists an unanswered field omitted from the payload.
    for step in &mut result.steps {
        step.assisted = used.get(&step.id).copied().unwrap_or(0) > 0;
    }
    result.final_grade.assisted = used.get(FINAL_FIELD_ID).copied().unwrap_or(0) > 0;
    result.assisted = result.final_grade.assisted || result.steps.iter().any(|step| step.assisted);
}

/// Older rows lack a frozen view; their recorded outcomes still own the receipt.
/// Legacy rows did not distinguish an omitted field from an empty response or
/// retain a notation warning. Those presentation flags use conservative defaults.
pub(super) fn replay(
    record: &cadus_core::event::IntegratedAttempt,
    item: &cadus_core::integrated::IntegratedItem,
) -> IntegratedGrade {
    use cadus_core::{
        event::{AttemptOutcome, IntegratedField},
        integrated::{FieldGrade, grade::MethodGrade},
    };
    if let Some(result) = &record.grade {
        return result.as_ref().clone();
    }
    let field = |value: &IntegratedField| FieldGrade {
        id: value.id.clone(),
        answered: !value.answer.is_empty(),
        correct: value.outcome == AttemptOutcome::Correct,
        ungraded: value.outcome.is_ungraded(),
        notation: false,
        assisted: value.assisted,
        skills: value.skills.clone(),
    };
    let steps: Vec<_> = record.steps.iter().map(field).collect();
    let mut final_grade = field(&record.final_field);
    final_grade.answered = true;
    IntegratedGrade {
        item_id: record.item_id.clone(),
        item_digest: record.item_digest.clone(),
        method: item.method.as_ref().map(|method| MethodGrade {
            chosen: record.method.clone(),
            correct: record.method_correct.unwrap_or(false),
            why: method
                .options
                .iter()
                .find(|option| Some(option.id.as_str()) == record.method.as_deref())
                .and_then(|option| option.why.clone()),
        }),
        correct_steps: steps.iter().filter(|step| step.correct).count(),
        total_steps: steps.len(),
        ungraded: final_grade.ungraded || steps.iter().any(|step| step.ungraded),
        steps,
        final_grade,
        solved: record.solved,
        assisted: record.assisted,
        skills_credited: record.skills_credited.clone(),
        interpretation: item.final_answer.interpretation.clone(),
        reasoning_recorded: record.reasoning_ungraded.is_some(),
    }
}
