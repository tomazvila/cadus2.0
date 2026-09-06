//! Persist the server-owned feedback chain and queued quiz skills.
use super::*;

/// Record feedback obligations and advance only on independent success.
pub(super) fn update_practice(
    scratch: &mut WebState,
    task_id: &str,
    served: &ServedProblem,
    recorded: &Attempt,
) {
    if (recorded.task_type != TaskType::Quiz || recorded.feedback_practice)
        && !recorded.outcome.is_ungraded()
        && (!recorded.correct || recorded.assisted)
    {
        let digest = problem_text_hash(&served.text);
        let mut digests = scratch
            .feedback_practice
            .get(task_id)
            .and_then(|value| value["digests"].as_array())
            .cloned()
            .unwrap_or_default();
        digests.push(json!(digest));
        let queue = scratch
            .feedback_practice
            .get(task_id)
            .map(|pending| pending["queue"].clone());
        scratch.feedback_practice.insert(task_id.to_owned(), json!({
            "queue": queue,
            "digest": digest, "digests": digests, "topic": served.serving_topic(), "record_topic": served.topic, "kp": served.kp,
        }));
    } else if recorded.correct && !recorded.assisted {
        finish_practice(scratch, task_id);
    }
}

/// Move to the next queued skill only after a fresh independent success.
fn finish_practice(scratch: &mut WebState, task_id: &str) {
    let Some(pending) = scratch.feedback_practice.remove(task_id) else {
        return;
    };
    let mut queue = pending["queue"].as_array().cloned().unwrap_or_default();
    if !queue.is_empty() {
        let mut next = queue.remove(0);
        next["queue"] = json!(queue);
        scratch.feedback_practice.insert(task_id.to_owned(), next);
    }
}
