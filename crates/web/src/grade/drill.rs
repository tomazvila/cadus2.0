//! Authoritative drill closure for replay-derived task spacing.
use super::*;
use cadus_core::event::DrillResult;

/// A drill closes after the original batch and its required independent practice.
///
/// The close names `task_passed` (H-5, ISSUES.md): the answer that emits the
/// `drill_result` is the completion moment, and a bare `continue` with no next
/// problem reads as "task over" to a client (trap W5) but never as DONE. A
/// drill earns no XP here, so `xp` stays out of the close (D-F2).
pub(super) fn close_drill(task: &Task, current: &Attempt, prior: &[EventRow]) -> Advance {
    let needs_practice = (!current.outcome.is_ungraded() && (!current.correct || current.assisted))
        || (current.feedback_practice && current.outcome.is_ungraded());
    if needs_practice || prior.iter().any(|row| matches!(&row.event, Event::DrillResult(result) if result.task_id == task.task_id)) {
        return Advance::carry_on();
    }
    let originals = prior
        .iter()
        .filter(|row| {
            matches!(&row.event,
        Event::Attempt(item) if item.task_id == task.task_id && !item.feedback_practice)
        })
        .count()
        + usize::from(!current.feedback_practice);
    if i64::try_from(originals).unwrap_or(i64::MAX) < task.n_problems.unwrap_or(i64::MAX) {
        return Advance::carry_on();
    }
    Advance {
        status: STATUS_TASK_PASSED,
        result: Some(Event::DrillResult(DrillResult {
            ts: current.ts,
            session: current.session.clone(),
            v: SchemaVersion::current(),
            task_id: task.task_id.clone(),
        })),
        ..Advance::carry_on()
    }
}
