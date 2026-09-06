//! The clocks, the serve index, the progress row, and the client-safe payload
//! of one hand-off.

use super::*;

/// Whole seconds from `started_at` to `now`, never below zero.
///
/// A clock that reads backwards gives the learner time the quiz never had, so a
/// stamp in the future answers 0. `f64 as i64` saturates in Rust, and the value
/// is a count of seconds, so the cast keeps the number it is given.
fn elapsed_secs(now: f64, started_at: f64) -> i64 {
    let secs = (now - started_at).floor();
    if secs.is_finite() && secs > 0.0 {
        secs as i64
    } else {
        0
    }
}

/// The seconds the whole-quiz clock has run, or `None` off a quiz (QUIZ-budget).
///
/// The FIRST quiz serve stamps the clock and answers 0; every later serve of the
/// open quiz reads that stamp, so a page reload resumes the running clock
/// instead of handing the whole budget back (M6-review-2, V6). The stamp is
/// server state: the client keeps no clock of its own across a reload.
pub(super) fn quiz_elapsed(
    scratch: &mut WebState,
    task_id: &str,
    task_type: TaskType,
    now: f64,
) -> Option<i64> {
    if task_type != TaskType::Quiz {
        return None;
    }
    let started_at = scratch.start_quiz_clock(task_id, now);
    Some(elapsed_secs(now, started_at))
}

/// The index the next serve takes (`_next_serve_index`, `api.py:398-407`).
///
/// A quiz and a multi-step task index by ANSWERED count, so a mid-task reload
/// re-serves the current unanswered part instead of skipping ahead (R2).
/// Everything else indexes by served count.
pub(super) fn next_serve_index(task_type: TaskType, progress: &TaskProgress) -> i64 {
    match task_type {
        TaskType::Quiz | TaskType::MultiStep => progress.answered,
        _ => progress.served,
    }
}

/// Install the progress row of a task when it has none
/// (`_progress_for`, `api.py:567-583`).
///
/// The serve path may install it. `GET /api/session/plan` may NOT: trap W3.
pub(crate) fn progress_for<'state>(
    scratch: &'state mut WebState,
    task: &Task,
    graph: &Curriculum,
) -> &'state mut TaskProgress {
    scratch
        .tasks
        .entry(task.task_id.clone())
        .or_insert_with(|| TaskProgress {
            task_id: task.task_id.clone(),
            task_type: task.task_type.as_str().to_string(),
            total: task.n_problems.unwrap_or_default(),
            served: 0,
            answered: 0,
            done: false,
            current_kp: match task.task_type {
                TaskType::Lesson => task
                    .topic
                    .as_deref()
                    .and_then(|topic| lesson_kp(None, task, graph, topic)),
                _ => None,
            },
        })
}

/// The client-safe serve payload (`_serve_payload`, `api.py:501-527`).
///
/// It names every field it emits, so `expected` and `solution_sketch` cannot
/// reach the client by accident (Hard Rule 1).
///
/// `total` is the TASK's `n_problems`, not the stored `TaskProgress.total`. A
/// lesson names no count and 1.0 answers `null` for it (`api.py:520`); the D-S6
/// row flattens the absent count to 0, so reading it back would tell the client
/// a lesson has zero problems.
///
/// `quiz_elapsed_secs` is the EIGHTH key, and a QUIZ serve alone carries it
/// (M6-review-2, V6). Every other task type emits the seven keys of 1.0, so no
/// other route and no other payload changes shape.
pub(super) fn serve_payload(
    served: &ServedProblem,
    task: &Task,
    graph: &Curriculum,
    drill_secs: i64,
    quiz_elapsed_secs: Option<i64>,
) -> Value {
    let (time_budget_secs, countdown) = if task.task_type == TaskType::Drill {
        (Some(drill_secs), true)
    } else {
        (expected_time(graph, served.topic.as_deref()), false)
    };
    let mut payload = json!({
        "problem_id": served.problem_id,
        "index": served.index + 1,
        "total": task.n_problems,
        "text": served.text,
        "kp": served.kp,
        "time_budget_secs": time_budget_secs,
        "countdown": countdown,
    });
    if let Some(elapsed) = quiz_elapsed_secs {
        payload["quiz_elapsed_secs"] = json!(elapsed);
    }
    payload
}

/// The authored solve time of a topic, when the arena holds the topic.
fn expected_time(graph: &Curriculum, topic_id: Option<&str>) -> Option<i64> {
    let idx = graph.idx_of(topic_id?)?;
    // `idx_of` gave the index, so `topic` is `Some`; `map` keeps that proof
    // without a second failure edge.
    graph.topic(idx).map(|topic| topic.expected_time_secs)
}

/// The answer kind the checker reads for a statement of this topic.
pub(super) fn answer_kind_of(graph: &Curriculum, topic_id: &str) -> Option<String> {
    let idx = graph.idx_of(topic_id)?;
    // `idx_of` gave the index, so `topic` is `Some`; `map` keeps that proof.
    graph
        .topic(idx)
        .map(|topic| topic.answer_kind.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::fixture::{graph, task};
    use super::*;

    /// One live problem of `topic` at index 0, with no clock.
    fn served(topic: Option<&str>) -> ServedProblem {
        ServedProblem {
            problem_id: "p1".to_string(),
            task_id: "t1".to_string(),
            topic: topic.map(str::to_string),
            serve_topic: None,
            kp: Some("kp1".to_string()),
            answer_kind: None,
            text: "Give 7.".to_string(),
            expected: cadus_core::pool::PoolAnswer {
                answer_contract: None,
                v: 1,
                answer: "7".to_string(),
            },
            solution_sketch: None,
            started_at: 0.0,
            hints_given: Vec::new(),
            index: 0,
            rework: None,
        }
    }

    #[test]
    fn elapsed_seconds_floor_and_never_go_below_zero() {
        assert_eq!(elapsed_secs(105.9, 100.0), 5);
        assert_eq!(elapsed_secs(100.0, 105.0), 0);
        assert_eq!(elapsed_secs(f64::NAN, 100.0), 0);
    }

    #[test]
    fn the_quiz_clock_stamps_once_and_only_on_a_quiz() {
        let mut scratch = WebState::for_session("s1");
        assert_eq!(
            quiz_elapsed(&mut scratch, "q", TaskType::Lesson, 10.0),
            None
        );
        assert_eq!(
            quiz_elapsed(&mut scratch, "q", TaskType::Quiz, 10.0),
            Some(0)
        );
        assert_eq!(
            quiz_elapsed(&mut scratch, "q", TaskType::Quiz, 25.5),
            Some(15)
        );
    }

    #[test]
    fn the_serve_index_counts_answers_on_a_quiz_and_serves_elsewhere() {
        let progress = TaskProgress {
            served: 3,
            answered: 2,
            ..TaskProgress::default()
        };
        assert_eq!(next_serve_index(TaskType::Quiz, &progress), 2);
        assert_eq!(next_serve_index(TaskType::MultiStep, &progress), 2);
        assert_eq!(next_serve_index(TaskType::Review, &progress), 3);
    }

    #[test]
    fn a_progress_row_is_installed_once_and_a_lesson_starts_at_its_first_point() {
        let mut scratch = WebState::for_session("s1");
        let lesson = task(TaskType::Lesson, Some("addition"));
        let row = progress_for(&mut scratch, &lesson, &graph());
        assert_eq!(row.current_kp.as_deref(), Some("kp1"));
        assert_eq!(row.task_type, "lesson");
        row.served = 4;
        assert_eq!(progress_for(&mut scratch, &lesson, &graph()).served, 4);

        let review = task(TaskType::Review, Some("addition"));
        assert_eq!(
            progress_for(&mut scratch, &review, &graph()).current_kp,
            None
        );
    }

    #[test]
    fn a_drill_counts_down_its_budget_and_a_lesson_reads_the_topic_time() {
        let mut drill = task(TaskType::Drill, Some("addition"));
        drill.n_problems = Some(20);
        let payload = serve_payload(&served(Some("addition")), &drill, &graph(), 45, None);
        assert_eq!(payload["time_budget_secs"], 45);
        assert_eq!(payload["countdown"], true);
        assert_eq!(payload["total"], 20);
        assert_eq!(payload["index"], 1);
        assert_eq!(payload.get("quiz_elapsed_secs"), None);

        let lesson = task(TaskType::Lesson, Some("addition"));
        let payload = serve_payload(&served(Some("addition")), &lesson, &graph(), 45, None);
        assert_eq!(payload["time_budget_secs"], 30);
        assert_eq!(payload["countdown"], false);
        assert_eq!(payload["total"], Value::Null);
    }

    #[test]
    fn a_quiz_payload_carries_its_clock_and_an_unknown_topic_has_no_budget() {
        let quiz = task(TaskType::Quiz, None);
        let payload = serve_payload(&served(Some("nowhere")), &quiz, &graph(), 45, Some(90));
        assert_eq!(payload["quiz_elapsed_secs"], 90);
        assert_eq!(payload["time_budget_secs"], Value::Null);
        let payload = serve_payload(&served(None), &quiz, &graph(), 45, Some(0));
        assert_eq!(payload["time_budget_secs"], Value::Null);
    }

    #[test]
    fn the_answer_kind_is_the_topic_s_kind_or_nothing() {
        assert_eq!(
            answer_kind_of(&graph(), "addition").as_deref(),
            Some("numeric")
        );
        assert_eq!(answer_kind_of(&graph(), "nowhere"), None);
    }
}
