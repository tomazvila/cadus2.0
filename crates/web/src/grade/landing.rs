//! The rewrite of a stored attempt after the background check accepted it.
//!
//! The worker appends the `regraded` correction in the transaction that
//! settles the job, so the log already holds the attempt as correct. What the
//! worker cannot do is the lesson logic, which lives here: the pass rule, the
//! close event with its XP, and the task progress. The poll route calls
//! [`land_accepted`] on the first poll that reads a done, accepted job. The
//! `landed_at` column makes the rewrite run once per job.

use super::route::advance_and_fold;
use super::*;

/// The note the corrected attempt carries in the re-evaluation.
const LANDED_NOTE: &str = "equivalence (background model)";

/// The rows with every `regraded` outcome applied to the attempt it names,
/// so the pass rule reads the attempts as the log now stands.
fn corrected_rows(rows: &[EventRow]) -> Vec<EventRow> {
    let mut outcomes = std::collections::BTreeMap::new();
    for row in rows {
        if let Event::Regraded(correction) = &row.event {
            for fixed in &correction.attempts {
                if let Some(outcome) = &fixed.outcome {
                    outcomes.insert(
                        fixed.attempt_id.clone(),
                        (outcome.clone(), fixed.work_quality),
                    );
                }
            }
        }
    }
    rows.iter()
        .map(|row| {
            let mut row = row.clone();
            if let Event::Attempt(body) = &mut row.event
                && let Some((outcome, quality)) = outcomes.get(&body.attempt_id)
            {
                body.correct = *outcome == AttemptOutcome::Correct;
                body.outcome = outcome.clone();
                body.work_quality = *quality;
            }
            row
        })
        .collect()
}

/// Rewrite the stored attempt of an accepted job and return the XP it earned.
///
/// Best effort: a session that is closed or rolled over, or an attempt outside
/// the open session, leaves the rewrite undone and the `regraded` correction
/// of the worker stands. The reply never fails on it.
///
/// # Errors
///
/// Returns [`ApiError`] when a statement fails; the transaction rolls back and
/// the next poll tries again.
pub async fn land_accepted(
    state: &AppState,
    user_id: Uuid,
    job_id: Uuid,
    attempt_id: &str,
) -> Result<Option<f64>, ApiError> {
    let content = crate::session::content(state)?;
    let (_, now) = crate::session::now_pair();
    let Ok(Open {
        mut tx,
        events,
        mut scratch,
        plan,
        readiness,
    }) = open(state, content, user_id, now, true).await
    else {
        tracing::info!(%job_id, "equivalence: no open session; the rewrite waits");
        return Ok(None);
    };
    if !store(
        state,
        cadus_store::equivalence::claim_landing(&mut *tx, job_id),
    )
    .await?
    {
        tx.rollback().await.map_err(db_failed)?;
        return Ok(None);
    }
    let rows = corrected_rows(&events);
    let at = rows.iter().position(
        |row| matches!(&row.event, Event::Attempt(body) if body.attempt_id == attempt_id),
    );
    let task = at.and_then(|at| match &rows[at].event {
        Event::Attempt(body) => find(&plan, &body.task_id).ok().cloned(),
        _ => None,
    });
    let (Some(at), Some(task)) = (at, task) else {
        save_and_commit(state, tx, user_id, &scratch).await?;
        return Ok(None);
    };
    let Event::Attempt(original) = &rows[at].event else {
        return Ok(None);
    };
    let mut corrected = original.clone();
    corrected.correct = true;
    corrected.outcome = AttemptOutcome::Correct;
    corrected.work_quality = WorkQuality::NearlyPerfect;
    corrected.error_tags.clear();
    corrected.grader_note = Some(LANDED_NOTE.to_owned());
    // The progression re-runs only when this attempt is still the last of its
    // task: a learner who already moved on keeps the progress they made, and
    // the corrected evidence reaches the next pass-rule read through the log.
    let later = rows[at + 1..]
        .iter()
        .any(|row| matches!(&row.event, Event::Attempt(body) if body.task_id == corrected.task_id));
    let reevaluate = !later && matches!(task.task_type, TaskType::Lesson | TaskType::Review);
    let mut xp = None;
    if reevaluate {
        let moved = advance_and_fold(
            state,
            content,
            &mut tx,
            user_id,
            &task,
            &corrected,
            &rows[..at],
            &readiness,
            false,
        )
        .await?;
        xp = moved.xp;
        if task.task_type == TaskType::Lesson && moved.status != STATUS_CONTINUE {
            let progress = progress_for(&mut scratch, &task, &content.curriculum);
            if task_moved_on(progress, task.task_type, &moved) {
                clear_task_scratch(&mut scratch, &task.task_id);
            }
        }
    }
    if let Some(xp) = xp {
        store(
            state,
            cadus_store::equivalence::set_landed_xp(&mut *tx, job_id, xp),
        )
        .await?;
    }
    save_and_commit(state, tx, user_id, &scratch).await?;
    Ok(xp)
}
