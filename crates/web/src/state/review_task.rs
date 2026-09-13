//! Persist the original review task without changing the core selector format.

use cadus_core::event::TaskType;
use cadus_core::selector::Task;
use serde::{Deserialize, Serialize};

/// Server-owned task metadata frozen when a review is first served.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RecordedReview(
    /// The complete task chosen by the server, before later replanning.
    #[serde(with = "TaskRecord")]
    pub Task,
);

#[derive(Serialize, Deserialize)]
#[serde(remote = "Task", deny_unknown_fields)]
struct TaskRecord {
    integrated_item_id: Option<String>,
    integrated_assessment_of: Option<String>,
    task_id: String,
    task_type: TaskType,
    topic: Option<String>,
    n_problems: Option<i64>,
    mix: Vec<String>,
    difficulty_target: Option<String>,
    recent_problem_hashes: Vec<String>,
    start_at_kp: Option<String>,
    time_budget_secs: Option<i64>,
    why: String,
    gap_fill: bool,
    gap_return_to: Option<String>,
    component_topics: Vec<String>,
    is_remediation: bool,
    nearly_due: bool,
    confirm: bool,
    probe_delay_days: Option<u32>,
    probe_kp: Option<String>,
}
