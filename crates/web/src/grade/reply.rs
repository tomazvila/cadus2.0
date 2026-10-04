//! The bodies the route hands back: the section 2.1 reply, the H3 stash
//! reply, and the quiz buffer the batch reveal reads.

use super::*;

/// Drop every scratch entry a finished task owns (`_clear_task_scratch`).
///
/// The buffers hold each question's hidden `expected` and its solution sketch,
/// and the D-S6 row is persisted, so a closed task must not keep them.
pub(super) fn clear_task_scratch(scratch: &mut WebState, task_id: &str) {
    scratch.served.remove(task_id);
    scratch.feedback_practice.remove(task_id);
    scratch.multistep.remove(task_id);
    scratch.task_memory.remove(task_id);
}

/// The outcome keys of a reply (D-F2).
///
/// A decided attempt names `outcome` and `correct`. An UNGRADED attempt names
/// `outcome` and `reason` and claims NO correctness: the `correct` key is absent,
/// because a claim of `false` reads as a miss and an ungraded attempt is not one.
pub(super) fn outcome_fields(recorded: &Attempt) -> Map<String, Value> {
    let mut map = Map::new();
    if let Some(timing) = recorded.timing {
        map.insert("timing".to_owned(), json!(timing));
        map.insert(
            "timing_reliable".to_owned(),
            json!(recorded.timing_reliable),
        );
    }
    map.insert("outcome".to_string(), json!(recorded.outcome.as_str()));
    match recorded.outcome.reason() {
        Some(reason) => {
            map.insert("reason".to_string(), json!(reason));
        }
        None => {
            map.insert("correct".to_string(), json!(recorded.correct));
        }
    }
    map
}

/// The client reply of section 2.1.
///
/// It names every field it emits. `solution` is revealed only after the attempt
/// commits, and `expected` never reaches the client on this path at all. A quiz
/// never reaches this reply (trap W7): its receipt is `quiz_receipt`.
#[expect(
    clippy::too_many_arguments,
    reason = "the reply names the attempt, the advance, the problem and each background field"
)]
pub(super) fn reply(
    recorded: &Attempt,
    moved: &Advance,
    served: &ServedProblem,
    next: Option<Value>,
    closed: bool,
    diagnosis: Value,
    equivalence: Value,
    proof_grading: Value,
) -> Value {
    // A bare `next: null` on an open task reads as "task over" (trap W5), so
    // an open task with no next problem says `next_unavailable` (trap W6).
    let unavailable = !closed && next.is_none();
    let ungraded = recorded.outcome.is_ungraded();
    let feedback_practice = !closed
        && (recorded.task_type != TaskType::Quiz || recorded.feedback_practice)
        && !ungraded
        && (!recorded.correct || recorded.assisted);
    let mut map = outcome_fields(recorded);
    for (key, value) in [
        ("attempt_id", json!(recorded.attempt_id)),
        ("work_quality", json!(recorded.work_quality)),
        ("error_tags", json!(recorded.error_tags)),
        ("secs", json!(recorded.secs.get())),
        ("task_status", json!(moved.status)),
        ("remediation", json!(moved.remediation_view())),
        ("next", json!(next)),
        ("diagnosis", diagnosis),
        // Amendment K (note 114): the background equivalence field. `null`
        // when the answer was never a miss; a pending job id or a cache
        // verdict otherwise.
        ("equivalence", equivalence),
        // Amendment K point 6: the background proof grading of an ungraded
        // written proof. `null` for every other attempt; a pending job id
        // the client polls otherwise.
        ("proof_grading", proof_grading),
    ] {
        map.insert(key.to_string(), value);
    }
    // The model's one-line reason rides beside the wrong verdict (note 114,
    // point 4) — in the reply and in the log, through the regraded fold of
    // the background path.
    if let Some(reason) = refused_reason(&map) {
        map.insert("equivalence_reason".to_string(), json!(reason));
    }
    if feedback_practice {
        map.insert("feedback_practice".to_owned(), json!(true));
        if unavailable {
            map.insert("feedback_blocked".to_owned(), json!(true));
        }
    }
    // A quiz reveals nothing until its batch reveal (trap W7), so no solution
    // and no re-solve text leaves this route for one. An UNGRADED attempt reveals
    // nothing either: Hard Rule 1 holds an answer back until the learner attempts
    // the problem, and an ungraded attempt is not a miss (D-F2).
    if let Some(solution) = &served.solution_sketch
        && !ungraded
    {
        map.insert("solution".to_string(), json!(solution));
    }
    if feedback_practice || (!recorded.correct && !ungraded) {
        let instruction = if feedback_practice {
            RE_SOLVE
        } else {
            "Review the worked solution, then continue."
        };
        map.insert("re_solve".to_string(), json!(instruction));
    }
    if unavailable {
        map.insert("next_unavailable".to_string(), json!(true));
    }
    if let Some(xp) = moved.xp {
        map.insert("xp".to_string(), json!(xp));
    }
    Value::Object(map)
}

/// The model's one-line why, when a refused cache verdict stands in the
/// equivalence field.
fn refused_reason(map: &Map<String, Value>) -> Option<String> {
    let field = map.get("equivalence")?;
    if field["status"] == "refused" {
        return field["reason"].as_str().map(str::to_owned);
    }
    None
}

/// Put one answered quiz question into the buffer the batch reveal reads.
///
/// The buffer holds the hidden solution sketch of each question, so it is the
/// one place a quiz keeps it; the reply carries none of it (trap W7).
pub(super) fn buffer_quiz_answer(
    scratch: &mut WebState,
    task_id: &str,
    served: &ServedProblem,
    recorded: &Attempt,
) {
    scratch
        .quizzes
        .entry(task_id.to_string())
        .or_default()
        .answers
        .push(json!({
            "problem_id": served.problem_id,
            "topic": served.topic,
            "kp": served.kp,
            "outcome": recorded.outcome.as_str(),
            "reason": recorded.outcome.reason(),
            "text": served.text,
            "given_answer": recorded.given_answer,
            "correct": recorded.correct,
            "secs": recorded.secs.get(),
            "solution_sketch": served.solution_sketch,
        }));
}
