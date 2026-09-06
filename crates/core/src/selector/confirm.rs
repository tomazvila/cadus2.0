//! The confirmation item of an inferred topic (D-F6, audit finding l).
//!
//! A placement and a course mastery floor give CREDIT, not evidence: neither
//! carries one direct answer on the topic. The book calls the repair
//! "conditional completion" and warns about "falling backwards" (p.377), so the
//! course owes every inferred topic one direct item before it counts as
//! practiced.
//!
//! The item is an ordinary review task with [`Task::confirm`] set. It records a
//! review attempt, and the fold reads the marker off the `task_served` event:
//! - A PASSED confirmation moves the topic to `Learning` with the review's own
//!   FIRe outcome.
//! - A FAILED confirmation keeps the topic `Placed` and queues the lesson.
//!
//! The rate is `mastery.max_per_session` items, and the nearest-due memory wins,
//! so one session never turns into a placement re-test.

use std::collections::{BTreeMap, BTreeSet};

use crate::config::Config;
use crate::curriculum::Curriculum;
use crate::fire::memory_at;
use crate::learner::TopicState;
use crate::xp::is_inferred;

use super::review::float_then_id;
use super::task::{Task, review_shell};
use super::topic_set::course_scope;

/// The number of problems one confirmation item serves.
pub const CONFIRM_PROBLEMS: i64 = 1;

/// The inferred topics of the course whose confirmation is due, nearest-due
/// memory first (D-F6).
///
/// `busy` holds the topics the plan already serves; a topic serves one task per
/// session, so a busy topic waits. An empty answer means the course owes no
/// confirmation, and `mastery.confirm_inferred` off always answers empty.
#[must_use]
pub fn confirmations(
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    cfg: &Config,
    t_us: i64,
    course_id: Option<&str>,
    busy: &BTreeSet<String>,
) -> Vec<String> {
    if !cfg.mastery.confirm_inferred || cfg.mastery.max_per_session == 0 {
        return Vec::new();
    }
    let scope = course_scope(graph, course_id);
    let mut ranked: Vec<(f64, String)> = states
        .iter()
        .filter(|(id, state)| {
            is_inferred(state) && !busy.contains(*id) && scope.contains_id(graph, id)
        })
        .map(|(id, state)| (memory_at(state, t_us), id.clone()))
        .collect();
    ranked.sort_by(float_then_id);
    ranked.truncate(cfg.mastery.max_per_session);
    ranked.into_iter().map(|(_, id)| id).collect()
}

/// Build one confirmation task (D-F6).
///
/// It is a review task of ONE item. `confirm` is the marker the serve route
/// copies onto `task_served` and the fold reads back.
pub(super) fn confirm_task(
    tid: &str,
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
) -> Task {
    let why = "confirmation; the course inferred this topic and never tested it".to_owned();
    Task {
        confirm: true,
        ..review_shell(tid, states, graph, CONFIRM_PROBLEMS, why)
    }
}
