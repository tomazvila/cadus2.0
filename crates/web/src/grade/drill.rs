//! Authoritative drill closure for replay-derived task spacing.
use super::*;
use cadus_core::event::DrillResult;

/// A drill closes after the original batch and its required independent practice.
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
        result: Some(Event::DrillResult(DrillResult {
            ts: current.ts,
            session: current.session.clone(),
            v: SchemaVersion::current(),
            task_id: task.task_id.clone(),
        })),
        ..Advance::carry_on()
    }
}
