//! The frontier of the course scope, split by the lesson-fail retry delay, and
//! the plan it reports.

use std::collections::{BTreeMap, BTreeSet};

use crate::config::Config;
use crate::curriculum::Curriculum;
use crate::event::Timestamp;
use crate::learner::TopicState;

use super::interleave::{SlotKind, constraints_of};
use super::plan::SessionPlan;
use super::review::{in_retry_delay, retry_available_at};
use super::task::Task;
use super::topic_set::{TopicSet, course_scope, frontier, mastered_set};

/// The frontier of the course scope, split by the lesson-fail retry delay.
pub(super) struct Frontier {
    /// The mastered set the frontier came from.
    pub(super) mastered: TopicSet,
    /// The topics of the course scope.
    pub(super) course_topics: TopicSet,
    /// The frontier inside the course scope, before the gap-fill chain filter.
    pub(super) topics: TopicSet,
    /// The frontier lessons the plan serves, sorted: inside the chain and not
    /// in a retry delay.
    pub(super) available: Vec<String>,
    /// When the first retry-delayed lesson reopens, while no lesson is available.
    pub(super) blocked_until: Option<i64>,
}

impl Frontier {
    /// The frontier of `course_id`, restricted to `chain` when one is given.
    pub(super) fn new(
        states: &BTreeMap<String, TopicState>,
        graph: &Curriculum,
        cfg: &Config,
        t_us: i64,
        course_id: Option<&str>,
        chain: Option<&BTreeSet<String>>,
    ) -> Self {
        let default = TopicState::default();
        let mastered = mastered_set(states, graph);
        let course_topics = course_scope(graph, course_id);
        let topics = frontier(graph, &mastered).intersect(&course_topics);
        let mut ids: Vec<String> = topics
            .sorted_ids(graph)
            .into_iter()
            .map(ToOwned::to_owned)
            .collect();
        if let Some(chain) = chain {
            ids.retain(|tid| chain.contains(tid));
        }
        let (blocked, available): (Vec<String>, Vec<String>) = ids
            .into_iter()
            .partition(|tid| in_retry_delay(states.get(tid).unwrap_or(&default), cfg, t_us));
        // A blocked frontier is one with lessons and none available. With no
        // lesson at all `blocked` is empty and the minimum is `None` as well.
        let blocked_until = if available.is_empty() {
            blocked
                .iter()
                .filter_map(|tid| states.get(tid))
                .filter_map(|state| retry_available_at(state, cfg))
                .min()
        } else {
            None
        };
        Self {
            mastered,
            course_topics,
            topics,
            available,
            blocked_until,
        }
    }

    /// The plan of `tasks` over this frontier, with the constraint report of
    /// the interleaved `seq`.
    pub(super) fn plan(
        &self,
        session: &str,
        tasks: Vec<Task>,
        quiz_due: bool,
        course_complete: bool,
        seq: &[(SlotKind, String)],
        cfg: &Config,
    ) -> SessionPlan {
        SessionPlan {
            session: session.to_owned(),
            tasks,
            quiz_due,
            constraints: constraints_of(seq, !self.available.is_empty(), cfg),
            course_complete,
            frontier_blocked_until: self.blocked_until.map(Timestamp::from_micros),
            // The composer fills the readiness list; the frontier knows nothing
            // about the content store.
            blocked: Vec::new(),
        }
    }
}
