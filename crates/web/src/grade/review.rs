//! Review close and targeted confirmation (D-F7).
use super::*;
use cadus_core::event::ReviewResult;
use cadus_core::fire::assess_review;

/// The attempts with the background proof grader's landed verdicts applied.
///
/// Only the grader's own corrections apply here (their `grader_note` is
/// [`cadus_store::proof_grading::GRADER_NOTE`]); every other correction keeps
/// the path it had.
fn proof_verdicts(prior: &[EventRow], attempts: &[&Attempt]) -> Vec<Attempt> {
    let mut verdicts: std::collections::BTreeMap<&str, &cadus_core::event::RegradedAttempt> =
        std::collections::BTreeMap::new();
    for row in prior {
        if let Event::Regraded(correction) = &row.event {
            for corrected in &correction.attempts {
                if corrected.grader_note.as_deref() == Some(cadus_store::proof_grading::GRADER_NOTE)
                {
                    verdicts.insert(corrected.attempt_id.as_str(), corrected);
                }
            }
        }
    }
    attempts
        .iter()
        .map(|attempt| {
            let mut fixed = (*attempt).clone();
            if let Some(corrected) = verdicts.get(attempt.attempt_id.as_str())
                && fixed.outcome.is_ungraded()
                && let Some(outcome) = &corrected.outcome
            {
                fixed.correct = *outcome == AttemptOutcome::Correct;
                fixed.outcome = outcome.clone();
                fixed.work_quality = corrected.work_quality;
            }
            fixed
        })
        .collect()
}

/// Close a complete review from its recorded evidence.
pub(super) fn close_review(
    task: &Task,
    attempt: &Attempt,
    prior: &[EventRow],
    cfg: &Config,
) -> Advance {
    let needs_practice = !attempt.outcome.is_ungraded() && (!attempt.correct || attempt.assisted);
    if task.task_type != TaskType::Review
        || needs_practice
        || (attempt.feedback_practice && attempt.outcome.is_ungraded())
    {
        return Advance::carry_on();
    }
    let mut attempts: Vec<&Attempt> = prior
        .iter()
        .filter_map(|row| match &row.event {
            Event::Attempt(body) if body.task_id == task.task_id && !body.feedback_practice => {
                Some(body)
            }
            _ => None,
        })
        .collect();
    if !attempt.feedback_practice {
        attempts.push(attempt);
    }
    // A written proof the background grader already graded counts with its
    // verdict: a pass as correct, a needs-revision as a decided miss (D-PR1).
    let corrected = proof_verdicts(prior, &attempts);
    let attempts: Vec<&Attempt> = corrected.iter().collect();
    if i64::try_from(attempts.len()).unwrap_or(i64::MAX)
        < task.n_problems.unwrap_or(cfg.review.questions)
    {
        return Advance::carry_on();
    }
    let evidence = assess_review(&attempts, cfg);
    let quality = attempts
        .last()
        .map_or(attempt.work_quality, |last| last.work_quality);
    let xp = if evidence.inconclusive {
        0.0
    } else {
        task_xp(TaskType::Review, quality, cfg, 1, 0, false)
    };
    let remediation = evidence
        .confirmation_skills
        .iter()
        .filter_map(|skill| {
            let (topic, kp) = skill.split_once('/')?;
            Some(RemediationTriggered {
                ts: attempt.ts,
                session: attempt.session.clone(),
                v: SchemaVersion::current(),
                kind: format!("review_confirmation:{kp}"),
                source_topic: attempt.topic.clone(),
                targets: vec![Slug::new(topic).ok()?],
            })
        })
        .collect();
    Advance {
        status: if evidence.inconclusive {
            "task_inconclusive"
        } else if evidence.passed {
            STATUS_TASK_PASSED
        } else {
            STATUS_TASK_FAILED
        },
        result: Some(Event::ReviewResult(ReviewResult {
            ts: attempt.ts,
            session: attempt.session.clone(),
            v: SchemaVersion::current(),
            topic: attempt.topic.clone(),
            passed: evidence.passed,
            weighted_score: evidence.score,
            xp,
            quality_tier: quality,
            assisted: attempts.iter().any(|a| a.assisted),
            task_id: Some(task.task_id.clone()),
            inconclusive: evidence.inconclusive,
            confirmation_skills: evidence.confirmation_skills,
        })),
        remediation,
        xp: Some(round2(xp)),
        next_kp: None,
        proof_due: None,
    }
}
