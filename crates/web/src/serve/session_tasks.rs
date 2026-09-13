//! Keep server-served tasks addressable while their session remains open.

use super::*;
use cadus_core::learner::LearnerModel;
use cadus_core::selector::{DIFFICULTY_TARGET, review_mix};

/// Restore original review metadata and pending lesson/quiz feedback.
/// This only changes the response plan; listing a plan remains read-only.
pub(crate) fn restore_session_tasks(
    plan: &mut SessionPlan,
    scratch: &WebState,
    events: &[EventRow],
    graph: &Curriculum,
    model: &LearnerModel,
) {
    if scratch.session.as_deref() != Some(plan.session.as_str()) {
        return;
    }
    let prefix = format!("{}-review-", plan.session);
    for (id, progress) in &scratch.tasks {
        if progress.task_type != "review"
            || progress.task_id != *id
            || !id.starts_with(&prefix)
            || scratch
                .served
                .get(id)
                .is_some_and(|live| live.task_id != *id)
        {
            continue;
        }
        let recorded = scratch
            .review_tasks
            .get(id)
            .filter(|saved| saved.0.task_id == *id && saved.0.task_type == TaskType::Review)
            .map(|saved| saved.0.clone())
            .or_else(|| legacy_review(id, progress, events, scratch, graph, model, &plan.session));
        let Some(recorded) = recorded else {
            continue;
        };
        // A same-ID replan may also change the question count or review mix.
        // The task already handed to the learner remains authoritative.
        if let Some(current) = plan.tasks.iter_mut().find(|task| task.task_id == *id) {
            *current = recorded;
        } else {
            plan.tasks.push(recorded);
        }
    }
    // Closed reviews retain their progress, so existing serve/answer guards
    // still return task_complete and cannot append another attempt.
    for (id, progress) in &scratch.tasks {
        if plan.tasks.iter().any(|task| task.task_id == *id) {
            continue;
        }
        let pending = scratch.feedback_practice.get(id);
        let kind = match progress.task_type.as_str() {
            "quiz" if scratch.quizzes.contains_key(id) => TaskType::Quiz,
            "lesson" if pending.is_some() => TaskType::Lesson,
            _ => continue,
        };
        plan.tasks.push(Task {
            task_id: id.clone(),
            task_type: kind,
            n_problems: Some(progress.total),
            topic: pending
                .and_then(|p| p["record_topic"].as_str())
                .map(str::to_owned),
            start_at_kp: pending.and_then(|p| p["kp"].as_str()).map(str::to_owned),
            ..Task::default()
        });
    }
}

/// Recover a pre-snapshot review from this session's server-owned records.
/// Historical display prose is unavailable; current model/curriculum data
/// supplies anti-repeat exclusions and the ordinary review mix.
fn legacy_review(
    id: &str,
    progress: &TaskProgress,
    events: &[EventRow],
    scratch: &WebState,
    graph: &Curriculum,
    model: &LearnerModel,
    session: &str,
) -> Option<Task> {
    if progress.total <= 0 {
        return None;
    }
    let served = events.iter().find_map(|row| match &row.event {
        Event::TaskServed(served)
            if served.task_id == id
                && served.task_type == TaskType::Review
                && served.session.as_deref() == Some(session) =>
        {
            Some(served)
        }
        _ => None,
    })?;
    let topic = served.topic.as_ref()?.as_str();
    let index = graph.idx_of(topic)?;
    if served
        .kp
        .as_ref()
        .is_some_and(|kp| graph.kp_idx_of(index, kp.as_str()).is_none())
    {
        return None;
    }
    let probe_kp = if served.probe_delay_days.is_some() {
        scratch
            .served
            .get(id)
            .and_then(|live| live.kp.clone())
            .or_else(|| {
                scratch
                    .feedback_practice
                    .get(id)
                    .and_then(|pending| pending["kp"].as_str())
                    .map(str::to_owned)
            })
            .or_else(|| {
                events.iter().rev().find_map(|row| match &row.event {
                    Event::Attempt(attempt)
                        if attempt.task_id == id && attempt.session.as_deref() == Some(session) =>
                    {
                        attempt.kp.as_ref().map(|kp| kp.as_str().to_owned())
                    }
                    _ => None,
                })
            })
    } else {
        None
    };
    if served.probe_delay_days.is_some() && probe_kp.is_none() && !progress.done {
        return None;
    }
    Some(Task {
        task_id: id.to_owned(),
        task_type: TaskType::Review,
        topic: Some(topic.to_owned()),
        n_problems: Some(progress.total),
        mix: if served.kp.is_some() {
            Vec::new()
        } else {
            review_mix(graph, topic)
        },
        difficulty_target: Some(DIFFICULTY_TARGET.to_owned()),
        recent_problem_hashes: model
            .topics
            .get(topic)
            .map(|state| state.last_problems.clone())
            .unwrap_or_default(),
        start_at_kp: served.kp.as_ref().map(|kp| kp.as_str().to_owned()),
        why: "Continue the review already started.".to_owned(),
        is_remediation: served.kp.is_some(),
        confirm: served.confirm,
        probe_delay_days: served.probe_delay_days,
        probe_kp,
        ..Task::default()
    })
}
