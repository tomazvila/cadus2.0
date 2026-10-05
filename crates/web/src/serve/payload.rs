//! The clocks, the serve index, the progress row, and the client-safe payload
//! of one hand-off.

use super::*;

use super::choices::label_choices;
use cadus_core::answer::AnswerContract;

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
    if task.task_type == TaskType::Review {
        scratch
            .review_tasks
            .entry(task.task_id.clone())
            .or_insert_with(|| crate::state::RecordedReview(task.clone()));
    }
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
/// `index` never passes `total`. A one-problem review answers a wrong attempt
/// with supplemental practice, and the stored serve counter ticks past the
/// single problem, so the raw `served.index + 1` would read "2 of 1". The clamp
/// is display-only: `served.index` still drives target rotation in
/// `next_serve_index`.
///
/// `quiz_elapsed_secs` is the EIGHTH key, and a QUIZ serve alone carries it
/// (M6-review-2, V6). Every non-quiz serve carries `hint_available beside the
/// seven keys of 1.0 (H-3, ISSUES.md); a quiz takes no hint, so its payload
/// keeps the exact eight-key shape.
///
/// `choices` is present only for a top-level Label contract: the option texts
/// in an order that comes from `problem_id` only (`choices.rs`). Each other
/// payload has no such key.
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
    // A review with one problem can store an index past its count after a
    // wrong answer queues the supplemental practice. The counter the UI reads
    // stays within the task's own problem count; every other task type keeps
    // the index it computed.
    let index = match task.n_problems {
        Some(total) if total > 0 => served.index.saturating_add(1).min(total),
        _ => served.index.saturating_add(1),
    };
    // A review states no question count before the answer: a retention probe
    // is one question where a review is four, so a per-task total would mark
    // the probe (D-F11). A review in a mixed block reads the block's progress.
    let total = if task.task_type == TaskType::Review {
        None
    } else {
        task.n_problems
    };
    let mut payload = json!({
        "problem_id": served.problem_id,
        "index": index,
        "total": total,
        "text": served.text,
        "kp": served.kp,
        "time_budget_secs": time_budget_secs,
        "countdown": countdown,
    });
    if served
        .rework
        .as_ref()
        .is_some_and(|value| value.get("digest").is_some())
    {
        payload["feedback_practice"] = json!(true);
        payload["countdown"] = json!(false);
    }
    if let Some(elapsed) = quiz_elapsed_secs {
        payload["quiz_elapsed_secs"] = json!(elapsed);
    }
    // A SELF-CHECK item (contract `none`, the tier-2 exemplar path) carries no
    // deterministic verdict at all, so the serve shows the worked solution
    // plainly: the learner reads the sketch and checks their own answer (the
    // attempt still grades UNGRADED — no verdict, no mastery credit). A graded
    // item reveals nothing here (Hard Rule 1): its solution waits for the
    // grade reply after the attempt commits.
    //
    // A written PROOF is the exception to the exception (D-PR1): its
    // solution waits for a pass or for the revision cap, so the learner
    // writes the proof before they read one.
    if served.expected.answer_contract == Some(AnswerContract::None)
        && !crate::proof_grading::is_proof_item(served)
        && let Some(solution) = served.solution_sketch.as_deref()
    {
        payload["solution"] = json!(solution);
    }
    let contract = served.expected.answer_contract.as_ref();
    if let Some(choices) = label_choices(contract, &served.problem_id) {
        payload["choices"] = json!(choices);
    }
    let visuals = visuals_of(graph, served, &served.problem_id);
    if !visuals.is_empty() {
        payload["visuals"] = json!(visuals);
    }
    payload
}

/// The drawn figures of the knowledge point this problem serves (unit f9).
///
/// The key stays OUT of the payload when the knowledge point authors no figure,
/// so every problem of today keeps the shape 1.0 gives it. A figure the check
/// refuses never reaches the learner: `render_all` drops it, and the readiness
/// audit reports the knowledge point as one with no visual.
fn visuals_of(
    graph: &Curriculum,
    served: &ServedProblem,
    problem_id: &str,
) -> Vec<cadus_core::visual::RenderedVisual> {
    let Some(kp_id) = served.kp.as_deref() else {
        return Vec::new();
    };
    let topic_id = served.serve_topic.as_deref().or(served.topic.as_deref());
    let Some(topic) = topic_id
        .and_then(|id| graph.idx_of(id))
        .and_then(|idx| graph.topic(idx))
    else {
        return Vec::new();
    };
    topic
        .knowledge_points
        .iter()
        .find(|kp| kp.id.as_str() == kp_id)
        .map(|kp| cadus_core::visual::render_all(&kp.visuals, problem_id))
        .unwrap_or_default()
}

/// Stamp `hint_available` on a serve payload when the store decides it (H-3).
///
/// The readiness set of the serve's own transaction answers whether the served
/// knowledge point holds an approved hint ladder (L5). A key that answers no
/// readiness — a payload whose serving key the index does not name — stamps
/// nothing, so a client that reads no flag assumes the historical behavior: ask,
/// and the refusal is `409 no_hint_ladder`. A flag of `false` is the SPA's
/// signal to hide the hint affordance, so no click spends a round trip on a
/// refusal the store already knew about.
pub(crate) fn stamp_hint_availability(
    payload: &mut Value,
    readiness: &ReadinessSet,
    task: &Task,
    served: &ServedProblem,
) {
    // A quiz takes no hint at all (`409 no_hints_in_quiz), so its payload keeps
    // the exact key set the quiz client reads.
    if task.task_type == TaskType::Quiz {
        return;
    }
    let Some(kp) = served.kp.as_deref() else {
        return;
    };
    let Some(topic) = served.serve_topic.as_deref().or(served.topic.as_deref()) else {
        return;
    };
    if let Some(found) = readiness.get(&kp_key(topic, kp)) {
        payload["hint_available"] = json!(found.hints);
    }
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
#[path = "payload_choices_tests.rs"]
mod choices_tests;

#[cfg(test)]
mod tests {
    use super::fixture::{graph, task};
    use super::*;

    /// One live problem of `topic` at index 0, with no clock.
    pub(super) fn served(topic: Option<&str>) -> ServedProblem {
        ServedProblem {
            timing_interrupted: false,
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
            handoff: None,
        }
    }

    /// An arena whose `figures/plot` knowledge point authors one number line.
    fn arena_with_a_visual() -> Curriculum {
        fixture::arena(&[json!({
            "id": "figures",
            "name": "The figures topic",
            "difficulty": 0.3,
            "answer_kind": "numeric",
            "expected_time_secs": 30,
            "knowledge_points": [{
                "id": "plot",
                "name": "Plot a point",
                "exemplars": [{"problem": "Plot 3.", "answer": "3"}],
                "visuals": [
                    {"kind": "number_line", "min": 0, "max": 5, "tick": 1,
                     "points": [{"at": 3}]},
                    {"kind": "number_line", "min": 5, "max": 0, "tick": 1},
                ],
            }],
        })])
    }

    /// The served problem of `figures/plot`.
    fn served_figure() -> ServedProblem {
        let mut problem = served(Some("figures"));
        problem.kp = Some("plot".to_string());
        problem
    }

    #[test]
    fn the_payload_carries_the_drawn_figures_of_the_knowledge_point() {
        let graph = arena_with_a_visual();
        let payload = serve_payload(
            &served_figure(),
            &task(TaskType::Lesson, Some("figures")),
            &graph,
            60,
            None,
        );
        let visuals = payload["visuals"].as_array().unwrap();
        // The second authored figure does not ascend, so the render drops it.
        assert_eq!(visuals.len(), 1);
        assert_eq!(visuals[0]["kind"], "number_line");
        assert!(visuals[0]["svg"].as_str().unwrap().contains("role=\"img\""));
        assert!(
            visuals[0]["svg"]
                .as_str()
                .unwrap()
                .contains("aria-labelledby=\"p1-0-title p1-0-desc\"")
        );
        assert_eq!(
            visuals[0]["text"],
            "A number line from 0 to 5 with a tick every 1. A filled point at 3."
        );
    }

    #[test]
    fn a_knowledge_point_with_no_authored_figure_keeps_the_payload_shape_of_one_zero() {
        let payload = serve_payload(
            &served(Some("addition")),
            &task(TaskType::Lesson, Some("addition")),
            &graph(),
            60,
            None,
        );
        assert!(payload.get("visuals").is_none());
    }

    #[test]
    fn the_figure_lookup_answers_nothing_for_an_unknown_topic_or_knowledge_point() {
        let graph = arena_with_a_visual();
        let mut unknown_kp = served_figure();
        unknown_kp.kp = Some("no-such-kp".to_string());
        assert!(visuals_of(&graph, &unknown_kp, "p1").is_empty());

        let mut no_kp = served_figure();
        no_kp.kp = None;
        assert!(visuals_of(&graph, &no_kp, "p1").is_empty());

        let mut unknown_topic = served_figure();
        unknown_topic.topic = Some("no-such-topic".to_string());
        assert!(visuals_of(&graph, &unknown_topic, "p1").is_empty());

        let mut no_topic = served_figure();
        no_topic.topic = None;
        assert!(visuals_of(&graph, &no_topic, "p1").is_empty());
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

    /// ISSUE-0: a one-problem review that has served and answered its single
    /// problem installs the queued practice at `progress.served == 1`, so the
    /// raw `served.index + 1` would read "2 of 1". The emitted index is clamped
    /// to the task's problem count.
    #[test]
    fn a_review_payload_never_reports_an_index_past_its_total() {
        let mut review = task(TaskType::Review, Some("addition"));
        review.n_problems = Some(1);
        // One problem went on screen and a wrong answer queued its practice.
        let mut problem = served(Some("addition"));
        problem.index = 1;
        let payload = serve_payload(&problem, &review, &graph(), 45, None);
        // A review states no total (D-F11), and its index stays within its count.
        assert!(payload["total"].is_null(), "{payload}");
        assert_eq!(payload["index"], 1);
        let mut drill = task(TaskType::Drill, Some("addition"));
        drill.n_problems = Some(20);
        assert_eq!(
            serve_payload(&problem, &drill, &graph(), 45, None)["total"],
            20,
            "a drill keeps its count"
        );
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
