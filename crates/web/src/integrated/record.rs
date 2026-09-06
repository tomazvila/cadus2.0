//! What the log keeps of an integrated task (D-F10, D-F9).
//!
//! Two events, and both are idempotent in the DATABASE: they carry an
//! `attempt_id`, and `events` holds a partial unique index on
//! `(user_id, attempt_id)`. A refresh, a double click, and a second browser tab
//! therefore write one row, not two, without a read-modify-write anywhere.
//!
//! The keys are derived from server state only — the session, the task id, and
//! the digest of the served item — so a client cannot choose one.

use cadus_core::answer::AnswerContract;
use cadus_core::event::{
    AttemptOutcome, IntegratedAttempt, IntegratedField, IntegratedServed, SchemaVersion, Timestamp,
};
use cadus_core::integrated::{FieldGrade, IntegratedGrade, IntegratedItem, Submission};

/// The idempotency key of the serve of `task_id` in `session`.
#[must_use]
pub fn served_key(session: &str, task_id: &str, digest: &str) -> String {
    format!("{session}:{task_id}:integrated-served:{digest}")
}

/// The idempotency key of the submission of `task_id` in `session`.
///
/// The digest is in the key, so an edited item is a NEW problem and the learner
/// may answer it again; the same item twice in one session is one attempt.
#[must_use]
pub fn attempt_key(session: &str, task_id: &str, digest: &str) -> String {
    format!("{session}:{task_id}:integrated-attempt:{digest}")
}

/// The serve event of one integrated task.
#[must_use]
pub fn served_event(
    item: &IntegratedItem,
    task_id: &str,
    session: &str,
    now: Timestamp,
) -> IntegratedServed {
    IntegratedServed {
        ts: now,
        session: Some(session.to_owned()),
        v: SchemaVersion::current(),
        task_id: task_id.to_owned(),
        item_id: item.id.as_str().to_owned(),
        item_digest: item.digest(),
        topic: item.topic.as_str().to_owned(),
        skills: item
            .skills()
            .iter()
            .map(|skill| skill.as_str().to_owned())
            .collect(),
    }
}

/// The outcome of one graded field (D-F2): an undecided field is UNGRADED, and
/// an ungraded field is not a miss.
fn outcome_of(grade: &FieldGrade) -> AttemptOutcome {
    if grade.ungraded {
        AttemptOutcome::Ungraded {
            reason: "the checker reached no verdict on this field".to_owned(),
        }
    } else if grade.correct {
        AttemptOutcome::Correct
    } else {
        AttemptOutcome::Incorrect
    }
}

/// One recorded field: what the learner wrote, under which contract, and how it
/// was decided.
fn field_of(grade: &FieldGrade, answer: &str, contract: AnswerContract) -> IntegratedField {
    IntegratedField {
        id: grade.id.clone(),
        answer: answer.to_owned(),
        contract,
        outcome: outcome_of(grade),
        assisted: grade.assisted,
        skills: grade.skills.clone(),
    }
}

/// The attempt event of one graded submission.
///
/// Every answer comes from the SUBMISSION and every verdict from the GRADE, so
/// the row holds the pair the checker actually decided. The authored answer is
/// never copied here: the log records what the learner wrote.
#[must_use]
pub fn attempt_event(
    item: &IntegratedItem,
    grade: &IntegratedGrade,
    submission: &Submission,
    task_id: &str,
    session: &str,
    now: Timestamp,
) -> IntegratedAttempt {
    let answer_of = |id: &str| {
        submission
            .steps
            .iter()
            .find(|step| step.id == id)
            .map_or("", |step| step.answer.as_str())
    };
    let steps = grade
        .steps
        .iter()
        .zip(item.steps.iter())
        .map(|(verdict, step)| field_of(verdict, answer_of(&verdict.id), step.ask.contract))
        .collect();
    IntegratedAttempt {
        ts: now,
        session: Some(session.to_owned()),
        v: SchemaVersion::current(),
        attempt_id: attempt_key(session, task_id, &grade.item_digest),
        task_id: task_id.to_owned(),
        item_id: grade.item_id.clone(),
        item_digest: grade.item_digest.clone(),
        topic: item.topic.as_str().to_owned(),
        method: grade
            .method
            .as_ref()
            .and_then(|method| method.chosen.clone()),
        method_correct: grade.method.as_ref().map(|method| method.correct),
        steps,
        final_field: field_of(
            &grade.final_grade,
            &submission.final_answer.answer,
            item.final_answer.ask.contract,
        ),
        skills_credited: grade.skills_credited.clone(),
        solved: grade.solved,
        assisted: grade.assisted,
        reasoning_ungraded: submission
            .reasoning
            .as_ref()
            .filter(|note| !note.trim().is_empty())
            .cloned(),
    }
}
