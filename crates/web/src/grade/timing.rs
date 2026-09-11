//! Server-owned timing evidence; correctness and progression read the grade only.
use super::*;
use cadus_core::timing::{SpeedReading, TimingFacts, read};

/// Read the actual served topic and preserve interruption/cap exclusions.
pub(super) fn reading(
    graph: &Curriculum,
    task: &Task,
    served: &ServedProblem,
    grade: &Grade,
    secs: i64,
    assisted: bool,
    tags: &[String],
) -> SpeedReading {
    let topic = served
        .serving_topic()
        .and_then(|id| graph.idx_of(id))
        .and_then(|idx| graph.topic(idx));
    read(&TimingFacts {
        elapsed_secs: secs,
        expected_secs: topic.map_or(0, |topic| topic.expected_time_secs),
        correct: grade.correct,
        assisted,
        interrupted: served.timing_interrupted
            || grade.outcome.is_ungraded()
            || tags.iter().any(|tag| tag == TAG_TIMING_UNRELIABLE),
        routine: task.task_type == TaskType::Drill || topic.is_some_and(|topic| topic.drill),
    })
}
