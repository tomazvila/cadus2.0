//! The readiness eligibility rule of D-F5: which planned task the service
//! actually serves.
//!
//! # The rule
//!
//! A LESSON is served only when its knowledge point is teachable, practicable
//! and assessable. A REVIEW, a QUIZ question, a DRILL and a multi-step
//! component need `practicable` and nothing more: each one revisits a skill the
//! learner already met, so no teach page and no held-out item is owed.
//!
//! A task the rule stops is not dropped in silence. It leaves the serve list
//! and enters [`SessionPlan::blocked`](super::SessionPlan) with the conditions
//! it failed, the plan takes the next ready task, and the SPA prints the list.
//! Audit finding (j) is what happens without the rule: the service serves
//! practice for a lesson it never taught.
//!
//! # The two guards
//!
//! The rule runs only when the caller gives a
//! [`ReadinessGate`](crate::readiness::ReadinessGate) AND
//! `Config::readiness::enforce` is on. A caller with no content index therefore
//! plans as it did before this unit, which is what the parity tests need.
//!
//! # What the rule does NOT touch
//!
//! The remediation queue. A remediation task is owed work the learner earned
//! with a miss, and dropping it loses the trigger that put it there. The queue
//! is served, and the audit reports its knowledge points like any other.

use std::collections::BTreeMap;

use crate::config::Config;
use crate::curriculum::Curriculum;
use crate::event::TaskType;
use crate::learner::TopicState;
use crate::readiness::{Blocker, ReadinessGate};

use super::plan::BlockedTask;
use super::quiz::QuizPlan;
use super::task::start_kp;

/// The readiness gate of one composition, or `None` when the rule is off.
pub(super) fn gate_of<'a>(
    cfg: &Config,
    gate: Option<&'a dyn ReadinessGate>,
) -> Option<&'a dyn ReadinessGate> {
    if cfg.readiness.enforce { gate } else { None }
}

/// Take the blocked lessons out of `lessons` and report them.
pub(super) fn hold_lessons(
    gate: &dyn ReadinessGate,
    graph: &Curriculum,
    states: &BTreeMap<String, TopicState>,
    lessons: &mut Vec<String>,
) -> Vec<BlockedTask> {
    let default = TopicState::default();
    let mut held: Vec<BlockedTask> = Vec::new();
    lessons.retain(|tid| {
        let state = states.get(tid).unwrap_or(&default);
        let Some(kp) = start_kp(graph, tid, state) else {
            // A topic with no knowledge point teaches nothing to gate.
            return true;
        };
        let blockers = gate.lesson_blockers(tid, &kp);
        if blockers.is_empty() {
            return true;
        }
        held.push(BlockedTask {
            task_type: TaskType::Lesson,
            topic: tid.clone(),
            kp: Some(kp),
            blockers,
        });
        false
    });
    held
}

/// Take the topics with no practicable knowledge point out of `topics` and
/// report them as blocked tasks of `task_type`.
pub(super) fn hold_topics(
    gate: &dyn ReadinessGate,
    task_type: TaskType,
    topics: &mut Vec<String>,
) -> Vec<BlockedTask> {
    let mut held: Vec<BlockedTask> = Vec::new();
    topics.retain(|tid| {
        if gate.topic_practicable(tid) {
            return true;
        }
        held.push(BlockedTask {
            task_type,
            topic: tid.clone(),
            kp: None,
            blockers: vec![Blocker::Practicable],
        });
        false
    });
    held
}

/// Take the questions of topics with no practicable knowledge point out of the
/// quiz and report them.
pub(super) fn hold_quiz(gate: &dyn ReadinessGate, plan: &mut QuizPlan) -> Vec<BlockedTask> {
    let mut held: Vec<BlockedTask> = Vec::new();
    plan.questions.retain(|question| {
        if gate.topic_practicable(&question.topic) {
            return true;
        }
        held.push(BlockedTask {
            task_type: TaskType::Quiz,
            topic: question.topic.clone(),
            kp: None,
            blockers: vec![Blocker::Practicable],
        });
        false
    });
    held
}
