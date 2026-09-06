//! The session plan: the constraint report, the ordered task list, and the
//! tasks the readiness rule of D-F5 stopped (`model.py:675-683`).

use crate::event::{TaskType, Timestamp};
use crate::readiness::Blocker;

use super::task::Task;

/// The session constraints reported with a plan (`_constraints`, `selector.py:1499-1519`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Constraints {
    /// Whether the lesson share reaches `selector.lesson_ratio_min`.
    pub lesson_ratio_ok: bool,
    /// The lesson share of the interleaved sequence, rounded to 4 places.
    pub lesson_ratio: f64,
    /// Whether no review run exceeds `selector.max_reviews_per_lesson`.
    pub throttle_ok: bool,
    /// The number of reviews in the sequence.
    pub reviews: i64,
    /// The number of lessons in the sequence.
    pub lessons: i64,
}

/// One planned task the readiness rule of D-F5 stopped.
///
/// The task is not served and it is not hidden: the plan carries it, and the
/// SPA prints the topic and the conditions it failed. The composer then takes
/// the next ready task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockedTask {
    /// The kind of task the composer wanted to serve.
    pub task_type: TaskType,
    /// The topic id.
    pub topic: String,
    /// The knowledge point of a blocked lesson.
    pub kp: Option<String>,
    /// The unmet conditions, in [`Blocker`] order.
    pub blockers: Vec<Blocker>,
}

/// The composed session (`SessionPlan`, `model.py:675-683`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionPlan {
    /// The session id the task ids are keyed on.
    pub session: String,
    /// The tasks, in serve order.
    pub tasks: Vec<Task>,
    /// Whether a quiz is due.
    pub quiz_due: bool,
    /// The throttle and ratio report.
    pub constraints: Constraints,
    /// Whether every topic of the course scope is mastered.
    pub course_complete: bool,
    /// When the first retry-delayed frontier lesson reopens.
    pub frontier_blocked_until: Option<Timestamp>,
    /// The planned tasks the readiness rule of D-F5 stopped, in serve order.
    pub blocked: Vec<BlockedTask>,
}
