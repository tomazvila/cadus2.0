//! The serve-layer half of the mixed review block
//! (`cadus_core::selector::next_mixed_review`).
//!
//! A run of two or more ordinary reviews in the plan serves its questions in an
//! interleaved order. Each review stays its own task: its problems are drawn by
//! [`super::install_next`] for that task, its attempts carry its own task id,
//! and its `review_result` is written by the answer that completes it. Only the
//! choice of WHICH review's next question goes on screen moves here:
//!
//! - the serve route hands back the block's live problem, or installs the
//!   question the rule picks, whichever review the client asked for;
//! - the answer route installs the next question of the review the rule picks,
//!   unless the answered review owes its corrective practice first.
//!
//! A payload of a block names its `task_id`, so the client answers it on the
//! right task, and carries `mixed_review` with the block's progress.

use super::*;
use cadus_core::selector::{ReviewSlot, mixed_review_block, next_mixed_review};

/// The mixed review block of `task_id` in this plan, or empty.
pub(crate) fn block_of<'plan>(
    plan: &'plan SessionPlan,
    scratch: &WebState,
    task_id: &str,
) -> Vec<&'plan Task> {
    mixed_review_block(&plan.tasks, task_id, |id| scratch.plan_progress(id).1)
}

/// The review of `block` the rule serves next, after `last`.
fn choose(
    session: &str,
    block: &[&Task],
    scratch: &WebState,
    last: Option<&str>,
) -> Option<String> {
    let slots: Vec<ReviewSlot<'_>> = block
        .iter()
        .map(|task| {
            let (remaining, done) = match scratch.tasks.get(&task.task_id) {
                Some(progress) => (progress.total - progress.answered, progress.done),
                None => (task.n_problems.unwrap_or_default(), false),
            };
            ReviewSlot {
                task_id: task.task_id.as_str(),
                remaining: remaining.max(0),
                open: !done,
            }
        })
        .collect();
    next_mixed_review(session, &slots, last).map(str::to_owned)
}

/// The review of `block` answered last in this session's window.
fn last_answered(events: &[EventRow], block: &[&Task]) -> Option<String> {
    events.iter().rev().find_map(|row| match &row.event {
        Event::Attempt(attempt) if block.iter().any(|task| task.task_id == attempt.task_id) => {
            Some(attempt.task_id.clone())
        }
        _ => None,
    })
}

/// The task a serve of `task_id` serves, when the mixed review block moves it.
///
/// `None` keeps the request as it stands: no block, a live problem or owed
/// corrective practice on the task itself, or a block with nothing left.
pub(crate) fn serve_target(
    plan: &SessionPlan,
    scratch: &WebState,
    events: &[EventRow],
    task_id: &str,
) -> Option<String> {
    if scratch.served.contains_key(task_id) || scratch.feedback_practice.contains_key(task_id) {
        return None;
    }
    let block = block_of(plan, scratch, task_id);
    if block.is_empty() {
        return None;
    }
    // A live problem, or corrective practice a review still owes, keeps its
    // review on screen: owed practice must follow its miss.
    if let Some(live) = block.iter().find(|task| {
        scratch.served.contains_key(&task.task_id)
            || scratch.feedback_practice.contains_key(&task.task_id)
    }) {
        return Some(live.task_id.clone());
    }
    let last = last_answered(events, &block);
    choose(&plan.session, &block, scratch, last.as_deref())
}

/// The review whose question follows an answer on `task`, when the mixed review
/// block hands on to ANOTHER review.
///
/// `scratch` already holds the answer's progress. Owed corrective practice
/// keeps the answered review on screen.
pub(crate) fn after_answer(plan: &SessionPlan, scratch: &WebState, task: &Task) -> Option<String> {
    if scratch.feedback_practice.contains_key(&task.task_id) {
        return None;
    }
    let block = block_of(plan, scratch, &task.task_id);
    if block.is_empty() {
        return None;
    }
    choose(&plan.session, &block, scratch, Some(&task.task_id)).filter(|next| *next != task.task_id)
}

/// Name the task and the block progress on the payload of a block's problem.
///
/// A task outside a block keeps its payload unchanged.
pub(crate) fn stamp(payload: &mut Value, plan: &SessionPlan, scratch: &WebState, task_id: &str) {
    let block = block_of(plan, scratch, task_id);
    if block.is_empty() {
        return;
    }
    let (answered, total) = block
        .iter()
        .fold((0_i64, 0_i64), |(answered, total), task| {
            let (done_count, _) = scratch.plan_progress(&task.task_id);
            let size = task.n_problems.unwrap_or_default();
            (answered + done_count.min(size), total + size)
        });
    payload["task_id"] = json!(task_id);
    payload["mixed_review"] = json!({
        "position": (answered + 1).min(total.max(1)),
        "total": total,
    });
}
