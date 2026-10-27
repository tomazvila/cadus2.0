//! Persist the task and the name of one on-demand quiz round (D-QZ1).

use serde::{Deserialize, Serialize};

use super::RecordedReview;

/// A quiz round that the learner asked for, kept until the session ends.
///
/// The plan composer does not make this task. The round route records it here,
/// and `restore_session_tasks` puts it back in the plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedRound {
    /// The complete quiz task that the server composed.
    pub task: RecordedReview,
    /// The question count the learner asked for.
    pub length: i64,
    /// The scope the learner picked: `course` or `all`.
    pub scope: String,
    /// The course id for the scope `course`.
    #[serde(default)]
    pub course: Option<String>,
    /// The course name for the scope `course`.
    #[serde(default)]
    pub course_name: Option<String>,
}
