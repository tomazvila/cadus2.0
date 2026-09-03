//! Course completion and the cross-course gap fill (PEDAGOGY 8, DD-1,
//! `selector.py:186-395`).

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use crate::config::Config;
use crate::curriculum::Curriculum;
use crate::learner::TopicState;

use super::quiz::i64_as_float;
use super::review::in_retry_delay;
use super::topic_set::{TopicSet, course_scope, frontier, mastered_set};

/// Whether every topic of the course scope is mastered
/// (`is_course_complete`, `selector.py:381-395`).
///
/// An empty frontier with un-mastered topics left is a cross-course gap block,
/// not completion.
#[must_use]
pub fn is_course_complete(
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    course_id: Option<&str>,
    mastered: Option<&TopicSet>,
) -> bool {
    let mastered = mastered_or(mastered, states, graph);
    let course_topics = course_scope(graph, course_id);
    !course_topics.is_empty() && course_topics.is_subset(&mastered)
}

/// The given mastered set, or the one computed from `states`.
fn mastered_or<'m>(
    given: Option<&'m TopicSet>,
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
) -> Cow<'m, TopicSet> {
    given.map_or_else(|| Cow::Owned(mastered_set(states, graph)), Cow::Borrowed)
}

/// The catalog `order` of a course. An unknown or unset course sorts last.
fn course_order(graph: &Curriculum, course_id: Option<&str>) -> f64 {
    course_id
        .and_then(|id| graph.course(id))
        .map_or(f64::INFINITY, |course| i64_as_float(course.order))
}

/// The un-mastered prerequisite ancestors of a course's un-mastered topics that
/// lie OUTSIDE the course (`blocking_gap_ancestors`, `selector.py:186-212`).
#[must_use]
pub fn blocking_gap_ancestors(
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    course_id: Option<&str>,
    mastered: Option<&TopicSet>,
) -> TopicSet {
    let Some(course) = course_id else {
        return TopicSet::empty(graph);
    };
    let mastered = mastered_or(mastered, states, graph);
    let course_topics = course_scope(graph, Some(course));
    let mut out = TopicSet::empty(graph);
    for idx in course_topics.indices() {
        if mastered.contains(idx) {
            continue;
        }
        for ancestor in graph.ancestors(idx) {
            if !mastered.contains(ancestor) && !course_topics.contains(ancestor) {
                out.insert(ancestor);
            }
        }
    }
    out
}

/// The nearest lower course to switch down into, or `None` when the course is
/// not cross-course blocked (`gap_course_for`, `selector.py:215-253`).
///
/// The switch is lazy: it returns `None` while the course still has a serveable
/// frontier lesson, while it is only retry-delayed, and when it is complete.
#[must_use]
pub fn gap_course_for(
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    cfg: &Config,
    t_us: i64,
    course_id: Option<&str>,
    mastered: Option<&TopicSet>,
) -> Option<String> {
    let course = course_id?;
    let mastered = mastered_or(mastered, states, graph);
    let course_topics = course_scope(graph, Some(course));
    let course_frontier = frontier(graph, &mastered).intersect(&course_topics);
    let default = TopicState::default();
    let has_available = course_frontier
        .sorted_ids(graph)
        .into_iter()
        .any(|id| !in_retry_delay(states.get(id).unwrap_or(&default), cfg, t_us));
    if has_available {
        return None; // Serve the in-course lessons first.
    }
    if !course_frontier.is_empty() {
        return None; // Only retry-delayed lessons remain: a delay, not a gap.
    }
    if course_topics.is_subset(&mastered) {
        return None; // Every course topic is mastered.
    }
    let missing = blocking_gap_ancestors(states, graph, Some(course), Some(&mastered));
    let current = course_order(graph, Some(course));
    let lower: BTreeSet<&str> = missing
        .indices()
        .map(|idx| graph.course_of(idx))
        .filter(|other| course_order(graph, Some(other)) < current)
        .collect();
    highest_course(graph, &lower)
}

/// The highest-ordered course of a set, the id breaking a tie
/// (`max(lower, key=(order, id))`).
fn highest_course(graph: &Curriculum, courses: &BTreeSet<&str>) -> Option<String> {
    highest_by_key(
        courses
            .iter()
            .map(|&course| (course_order(graph, Some(course)), course)),
    )
}

/// The id with the highest `(order, id)` key. Python `max` keeps the FIRST
/// maximum, and the keys arrive in sorted id order.
fn highest_by_key<'c>(keys: impl Iterator<Item = (f64, &'c str)>) -> Option<String> {
    let mut best: Option<(f64, &str)> = None;
    for key in keys {
        let better = match best {
            None => true,
            Some((order, id)) => key.0 > order || (key.0 == order && key.1 > id),
        };
        if better {
            best = Some(key);
        }
    }
    best.map(|(_, id)| id.to_owned())
}

/// The blocking chain to serve at the tip of a switched-down stack
/// (`gap_fill_chain_for_stack`, `selector.py:256-276`).
///
/// `None` when the stack is not switched down. Otherwise the topics of the tip
/// course that block ANY course above it — the union over the whole stack.
#[must_use]
pub fn gap_fill_chain_for_stack(
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    stack: &[String],
    mastered: Option<&TopicSet>,
) -> Option<TopicSet> {
    if stack.len() < 2 {
        return None;
    }
    let mastered = mastered_or(mastered, states, graph);
    let mut chain = TopicSet::empty(graph);
    let parents = stack.get(..stack.len() - 1).unwrap_or(&[]);
    for parent in parents {
        let blockers = blocking_gap_ancestors(states, graph, Some(parent), Some(&mastered));
        for idx in blockers.indices() {
            chain.insert(idx);
        }
    }
    let tip = stack.last().map(String::as_str);
    Some(chain.intersect(&course_scope(graph, tip)))
}

/// The topics [`compose_session`] can actually serve at the stack tip
/// (`serveable_gap_frontier`, `selector.py:279-297`).
#[must_use]
pub fn serveable_gap_frontier(
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    stack: &[String],
    mastered: Option<&TopicSet>,
) -> TopicSet {
    let mastered = mastered_or(mastered, states, graph);
    let tip = stack.last().map(String::as_str);
    let out = frontier(graph, &mastered).intersect(&course_scope(graph, tip));
    match gap_fill_chain_for_stack(states, graph, stack, Some(&mastered)) {
        Some(chain) => out.intersect(&chain),
        None => out,
    }
}

/// The next course to descend into when the tip can serve nothing
/// (`_deeper_gap_course`, `selector.py:335-361`).
fn deeper_gap_course(
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    stack: &[String],
    mastered: &TopicSet,
) -> Option<String> {
    if stack.len() < 2 {
        return None;
    }
    if !serveable_gap_frontier(states, graph, stack, Some(mastered)).is_empty() {
        return None; // The tip can serve something.
    }
    let tip = stack.last().map(String::as_str);
    let mut blockers = blocking_gap_ancestors(states, graph, tip, Some(mastered));
    let chain = gap_fill_chain_for_stack(states, graph, stack, Some(mastered))
        .unwrap_or_else(|| TopicSet::empty(graph));
    for idx in chain.indices() {
        for ancestor in graph.ancestors(idx) {
            if !mastered.contains(ancestor) {
                blockers.insert(ancestor);
            }
        }
    }
    let tip_order = course_order(graph, tip);
    let lower: BTreeSet<&str> = blockers
        .indices()
        .map(|idx| graph.course_of(idx))
        .filter(|other| {
            !stack.iter().any(|inside| inside == other)
                && course_order(graph, Some(other)) < tip_order
        })
        .collect();
    highest_course(graph, &lower)
}

/// The effective enrollment stack derived from the base course and the mastery
/// (`resolve_gap_fill_stack`, `selector.py:300-332`).
///
/// `stack[0]` is the base course and `stack[-1]` is the course to serve. The
/// descent stops at the first course whose blocking chain holds something
/// serveable. Switch-back is implicit: a mastered gap shortens the stack.
#[must_use]
pub fn resolve_gap_fill_stack(
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    cfg: &Config,
    t_us: i64,
    base_course: Option<&str>,
) -> Vec<String> {
    let Some(base) = base_course else {
        return Vec::new();
    };
    let mastered = mastered_set(states, graph);
    let mut stack: Vec<String> = vec![base.to_owned()];
    // Every course pushed here has a lower order than the tip, so the descent
    // ends at the lowest course at the latest.
    loop {
        let tip = stack.last().map(String::as_str);
        let gap = gap_course_for(states, graph, cfg, t_us, tip, Some(&mastered))
            .or_else(|| deeper_gap_course(states, graph, &stack, &mastered));
        let Some(course) = gap else {
            break;
        };
        stack.push(course);
    }
    stack
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{KpProgress, Timestamp, TopicStatus};
    use crate::fire::testing::{T_US, ladder, topic};

    /// The three-course ladder of the 1.0 gap-fill tests.
    fn tree() -> Curriculum {
        ladder(&[
            (
                "low",
                &["low-a"],
                vec![topic("low-a", &[]), topic("low-b", &[("low-a", 1.0, true)])],
            ),
            (
                "mid",
                &[],
                vec![
                    topic("mid-a", &[("low-a", 1.0, true)]),
                    topic("mid-b", &[("low-b", 1.0, true)]),
                    topic("mid-free", &[]),
                ],
            ),
            ("top", &[], vec![topic("top-a", &[("mid-a", 1.0, true)])]),
        ])
    }

    /// A floor state.
    fn floor() -> TopicState {
        TopicState {
            status: TopicStatus::Floor,
            ..TopicState::default()
        }
    }

    #[test]
    fn the_descent_stops_at_the_course_that_serves_and_is_lazy_on_a_delay() {
        let cfg = Config::default();
        let tree = tree();
        let none: BTreeMap<String, TopicState> = BTreeMap::new();
        assert_eq!(
            resolve_gap_fill_stack(&none, &tree, &cfg, T_US, Some("top")),
            ["top", "mid", "low"]
        );
        assert!(resolve_gap_fill_stack(&none, &tree, &cfg, T_US, None).is_empty());
        assert!(!is_course_complete(&none, &tree, Some("nope"), None));
        assert!(blocking_gap_ancestors(&none, &tree, None, None).is_empty());
        assert_eq!(gap_course_for(&none, &tree, &cfg, T_US, None, None), None);
        assert!(gap_fill_chain_for_stack(&none, &tree, &["top".to_owned()], None).is_none());
        let stack = ["top", "mid", "low"].map(str::to_owned);
        assert_eq!(
            serveable_gap_frontier(&none, &tree, &stack, None).sorted_ids(&tree),
            ["low-a"]
        );
        assert!(serveable_gap_frontier(&none, &tree, &stack[..2], None).is_empty());

        // `mid-a` failed its lesson half a day ago: the delay is not a gap.
        let mut delayed = TopicState {
            t0: Some(Timestamp::from_micros(T_US)),
            ..TopicState::default()
        };
        delayed
            .kp_progress
            .insert("kp1".to_owned(), KpProgress::FailedOnce);
        let states: BTreeMap<String, TopicState> =
            [("low-a".to_owned(), floor()), ("mid-a".to_owned(), delayed)]
                .into_iter()
                .collect();
        assert_eq!(
            gap_course_for(&states, &tree, &cfg, T_US, Some("mid"), None),
            None
        );
        let all: BTreeMap<String, TopicState> = tree
            .topics()
            .iter()
            .map(|item| (item.id.as_str().to_owned(), floor()))
            .collect();
        assert!(is_course_complete(&all, &tree, Some("top"), None));
        assert_eq!(
            gap_course_for(&all, &tree, &cfg, T_US, Some("top"), None),
            None
        );
    }

    #[test]
    fn the_highest_key_keeps_the_first_of_a_tie_and_the_larger_id() {
        let keys = [(2.0, "b"), (1.0, "a"), (2.0, "c"), (2.0, "c")];
        assert_eq!(highest_by_key(keys.into_iter()).as_deref(), Some("c"));
        assert_eq!(highest_by_key(std::iter::empty()), None);
    }
}
