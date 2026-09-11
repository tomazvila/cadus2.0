//! The selector: the ordered session plan of PEDAGOGY 5 to 8 (spec section 6).
//!
//! The port keeps every ordering rule of 1.0 `cadus/selector.py`, because the plan
//! is a list and a list compares element by element. The rules that decide the
//! order are:
//!
//! - Every 1.0 `sorted()` becomes a sort by topic id in byte order (trap T18).
//! - Every 1.0 loop over a SET becomes a loop over the sorted ids (trap T5).
//! - `max()` and the greedy `>` of [`compress`] keep the FIRST maximum, so a tie
//!   breaks to the lowest id.
//! - The float sum of [`importance`] runs over SORTED review targets and goes
//!   through [`neumaier_sum`](crate::numeric::neumaier_sum) (trap T1).
//!
//! # What is NOT reproduced
//!
//! 1.0 samples the quiz with `random.Random.sample`, the Mersenne-Twister
//! algorithm of CPython (trap T11). 2.0 does NOT reproduce that sequence. Quiz
//! sampling goes through the [`QuizSampler`] trait, and [`SeededSampler`] is 2.0's
//! own seeded generator. The seed contract is therefore CHANGED, on purpose: the
//! sampled ids are recorded on the `task_served` event, so a replay reads the ids
//! from the log and never re-samples them.
//!
//! 1.0 also carries `_backfill_legacy_flags`, a transitional read of the display
//! prose that recovers `is_remediation` and `nearly_due` from a plan serialized
//! before those fields existed. 2.0 starts learners fresh (decision O1), so no
//! such plan exists and the port drops that path. The typed fields are the only
//! source of both facts here.

mod compose;
mod compress;
mod confirm;
mod context;
mod eligible;
mod frontier;
mod gap_fill;
mod interleave;
mod multistep;
mod plan;
mod quiz;
mod reserve;
mod retention;
mod review;
mod task;
mod topic_set;
mod trigger;

pub use crate::xp::{is_inferred, is_known, is_practiced};

pub use compose::compose_session;
pub use compress::{Compression, compress};
pub use confirm::{CONFIRM_PROBLEMS, confirmations};
pub use context::SessionContext;
pub use gap_fill::{
    blocking_gap_ancestors, gap_course_for, gap_fill_chain_for_stack, is_course_complete,
    resolve_gap_fill_stack, serveable_gap_frontier,
};
pub use interleave::{SlotKind, arrange_lessons, assign_ids, interleave};
pub use multistep::{multistep_components, multistep_is_due, remediation_tasks};
pub use plan::{BlockedTask, Constraints, SessionPlan};
pub use quiz::{
    QuizPlan, QuizQuestion, QuizSampler, SeededSampler, quiz_budget, quiz_composer,
    quiz_difficulty_target, quiz_is_due, quiz_retake_available_at, utc_date,
};
pub use reserve::{ValidityContext, reserve_open_plan, task_still_valid};
pub use retention::retention_probe;
pub use review::{
    due_reviews, importance, in_retry_delay, nearly_due, order_lessons, retry_available_at,
    review_mix,
};
pub use task::{Task, start_kp};
pub use topic_set::{TopicSet, course_scope, frontier, known_set, practiced_set};
pub use trigger::{remediation_for_quiz_miss, remediation_for_repeat_fail, schedule_drills};

/// One day, in microseconds.
const DAY_US: i64 = 86_400_000_000;

/// The importance bonus of a core topic (`selector.py:78`).
pub const CORE_BONUS: f64 = 0.5;

/// The review difficulty target, the 80-85% sweet spot (`selector.py:81`).
pub const DIFFICULTY_TARGET: &str = "80-85% expected accuracy";

/// The window that makes a learned topic "recent" for a quiz (`selector.py:84`).
pub const QUIZ_RECENT_DAYS: i64 = 14;

/// The per-question quiz time multiplier (`selector.py:87`).
pub const QUIZ_TIME_FACTOR: f64 = 1.5;

/// The delay before a failed quiz becomes re-eligible (`selector.py:96`).
pub const QUIZ_RETAKE_DELAY_DAYS: i64 = 1;

/// The quiz difficulty bands, easiest first (`selector.py:105-109`).
pub const QUIZ_DIFFICULTY_TARGETS: [&str; 3] = [
    DIFFICULTY_TARGET,
    "75-80% expected accuracy",
    "70-75% expected accuracy",
];

/// The quiz questions drawn from the recent stratum (`selector.py:744`).
pub const QUIZ_RECENT_TARGET: usize = 4;

/// The quiz questions drawn from the mid stratum (`selector.py:745`).
pub const QUIZ_MID_TARGET: usize = 2;

/// The automaticity bar a drill target stays below (`selector.py:113`).
pub const DRILL_MASTERY_ABILITY: f64 = 0.95;

/// The drill cadence, in days (`selector.py:116`).
pub const DRILL_INTERVAL_DAYS: f64 = 3.5;

/// The remediation kind of a quiz miss (`selector.py:120`).
pub const REMEDIATION_QUIZ_MISS: &str = "quiz_miss";

/// The remediation kind of a second failure at one knowledge point.
pub const REMEDIATION_REPEAT_FAIL: &str = "repeat_fail";

/// The remediation kind of a lesson failure.
pub const REMEDIATION_LESSON_FAIL: &str = "lesson_fail";

/// The remediation kind of a failed confirmation item (D-F6).
///
/// The target keeps its `Placed` status and the plan serves its LESSON, which is
/// the peel-back the book asks for at p.377.
pub const REMEDIATION_CONFIRM_FAILED: &str = "confirm_failed";

/// The fewest components a multi-step task needs (`selector.py:139`).
pub const MULTISTEP_MIN_COMPONENTS: usize = 3;

/// The most components a multi-step task carries (`selector.py:140`).
pub const MULTISTEP_MAX_COMPONENTS: usize = 4;

/// One multi-step task is owed per this many mastered reviewable topics.
pub const MULTISTEP_CADENCE: i64 = 4;

/// The multi-step kill switch (`selector.py:145`). 1.0 ships it on.
pub const MULTISTEP_ENABLED: bool = true;
