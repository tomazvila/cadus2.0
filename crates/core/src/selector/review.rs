//! The review side of the plan: the retry delay, the due and nearly-due
//! reviews, the lesson importance, and the review question mix.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use crate::config::Config;
use crate::curriculum::Curriculum;
use crate::event::KpProgress;
use crate::fire::{ReviewState, memory_at, review_state};
use crate::learner::TopicState;
use crate::numeric::neumaier_sum;

use super::topic_set::{ReachCache, TopicSet};
use super::{CORE_BONUS, DAY_US};

/// When a failed frontier lesson may be retried, in UTC microseconds
/// (`_retry_available_at`, `selector.py:402-417`).
///
/// `None` when the topic records no lesson failure, or when the sum leaves the
/// representable range.
#[must_use]
pub fn retry_available_at(state: &TopicState, cfg: &Config) -> Option<i64> {
    let t0 = state.t0?.micros();
    let failed = state
        .kp_progress
        .values()
        .any(|progress| matches!(progress, KpProgress::FailedOnce | KpProgress::FailedTwice));
    if !failed {
        return None;
    }
    cfg.lesson
        .retry_delay_days
        .checked_mul(DAY_US)
        .and_then(|delay| t0.checked_add(delay))
}

/// Whether a frontier topic is blocked by a lesson-fail retry delay
/// (`in_retry_delay`, `selector.py:420-423`).
#[must_use]
pub fn in_retry_delay(state: &TopicState, cfg: &Config, t_us: i64) -> bool {
    retry_available_at(state, cfg).is_some_and(|at| t_us < at)
}

/// The due review topics, SORTED (`due_reviews`, `selector.py:431-454`).
///
/// The band already drops a topic with no review history, so an untouched topic
/// and a mastery-floor topic never appear here.
#[must_use]
pub fn due_reviews(
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    cfg: &Config,
    t_us: i64,
    test_prep_topics: &BTreeSet<String>,
) -> Vec<String> {
    let mut out: Vec<String> = states
        .iter()
        .filter(|(id, state)| {
            graph.idx_of(id).is_some()
                && review_state(state, t_us, cfg, test_prep_topics.contains(*id))
                    == ReviewState::Due
        })
        .map(|(id, _)| id.clone())
        .collect();
    out.sort_unstable();
    out
}

/// The reviewable topics that are nearly due, soonest-due first
/// (`_nearly_due`, `selector.py:457-472`).
///
/// The order is `(memory_at(state, t), id)` ascending. The classification takes
/// no test-prep override: the domino candidate set is a lookahead, not a
/// promotion.
#[must_use]
pub fn nearly_due(
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    cfg: &Config,
    t_us: i64,
) -> Vec<String> {
    let mut out: Vec<(f64, String)> = states
        .iter()
        .filter(|(id, state)| {
            graph.idx_of(id).is_some()
                && review_state(state, t_us, cfg, false) == ReviewState::NearlyDue
        })
        .map(|(id, state)| (memory_at(state, t_us), id.clone()))
        .collect();
    out.sort_by(float_then_id);
    out.into_iter().map(|(_, id)| id).collect()
}

/// The Python tuple order `(float, id)`: the float first, the id on a tie.
pub(super) fn float_then_id(left: &(f64, String), right: &(f64, String)) -> Ordering {
    left.0
        .partial_cmp(&right.0)
        .unwrap_or(Ordering::Equal)
        .then_with(|| left.1.cmp(&right.1))
}

/// The encompassing credit a lesson would send to the review targets
/// (`_knockout_mass`, `selector.py:598-602`).
///
/// 1.0 sums over a SET (trap T5), so the port sums over the SORTED targets, and
/// the sum is the CPython `sum()` of trap T1.
fn knockout_mass(tid: &str, review_targets: &BTreeSet<String>, cache: &mut ReachCache<'_>) -> f64 {
    let values: Vec<f64> = review_targets
        .iter()
        .map(|target| cache.weight(tid, target))
        .collect();
    neumaier_sum(&values)
}

/// A dependent count as a float. The curriculum holds about 1,100 topics, so the
/// count is exact in an `f64`.
#[expect(
    clippy::cast_precision_loss,
    reason = "a dependent count never leaves the exact f64 integer range"
)]
pub(super) const fn count_as_float(count: usize) -> f64 {
    count as f64
}

/// [`importance`] over a shared weight memo.
fn importance_with(
    tid: &str,
    graph: &Curriculum,
    review_targets: &BTreeSet<String>,
    course_topics: &TopicSet,
    cache: &mut ReachCache<'_>,
) -> f64 {
    let mass = knockout_mass(tid, review_targets, cache);
    let (dependents, core) = match graph.idx_of(tid) {
        Some(idx) => {
            let count = graph
                .descendants(idx)
                .into_iter()
                .filter(|dependent| course_topics.contains(*dependent))
                .count();
            (count, graph.topic(idx).is_some_and(|topic| topic.core))
        }
        None => (0, false),
    };
    let bonus = if core { CORE_BONUS } else { 0.0 };
    mass + count_as_float(dependents) + bonus
}

/// The lesson importance of PEDAGOGY 5.3 (`importance`, `selector.py:605-614`).
///
/// It is the knockout mass, plus the in-course dependent count, plus the core
/// bonus. Higher is served first.
#[must_use]
pub fn importance(
    tid: &str,
    graph: &Curriculum,
    review_targets: &BTreeSet<String>,
    course_topics: &TopicSet,
) -> f64 {
    let mut cache = ReachCache::new(graph);
    importance_with(tid, graph, review_targets, course_topics, &mut cache)
}

/// [`order_lessons`] over a shared weight memo.
pub(super) fn order_lessons_with(
    lessons: &[String],
    graph: &Curriculum,
    review_targets: &BTreeSet<String>,
    course_topics: &TopicSet,
    cache: &mut ReachCache<'_>,
) -> Vec<String> {
    let mut keyed: Vec<(f64, String)> = lessons
        .iter()
        .map(|tid| {
            (
                -importance_with(tid, graph, review_targets, course_topics, cache),
                tid.clone(),
            )
        })
        .collect();
    keyed.sort_by(float_then_id);
    keyed.into_iter().map(|(_, tid)| tid).collect()
}

/// The frontier lessons ordered by descending importance, id ascending on a tie
/// (`order_lessons`, `selector.py:617-629`).
#[must_use]
pub fn order_lessons(
    lessons: &[String],
    graph: &Curriculum,
    review_targets: &BTreeSet<String>,
    course_topics: &TopicSet,
) -> Vec<String> {
    let mut cache = ReachCache::new(graph);
    order_lessons_with(lessons, graph, review_targets, course_topics, &mut cache)
}

/// The review question mix: the topic's knowledge points, then its component
/// skills, sorted (`review_mix`, `selector.py:637-647`).
#[must_use]
pub fn review_mix(graph: &Curriculum, tid: &str) -> Vec<String> {
    let Some(topic) = graph.idx_of(tid).and_then(|idx| graph.topic(idx)) else {
        return Vec::new();
    };
    let mut mix: Vec<String> = topic
        .knowledge_points
        .iter()
        .map(|kp| kp.id.as_str().to_owned())
        .collect();
    let mut components: BTreeSet<&str> = BTreeSet::new();
    for edge in &topic.prerequisites {
        if edge.key {
            components.insert(edge.id.as_str());
        }
    }
    for kp in &topic.knowledge_points {
        for key in &kp.key_prerequisites {
            components.insert(key.as_str());
        }
    }
    mix.extend(
        components
            .into_iter()
            .map(|component| format!("component:{component}")),
    );
    mix
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Timestamp;
    use crate::fire::testing::{DAY_US, T_US, graph, learned, topic};

    #[test]
    fn the_review_side_orders_and_prices_the_topics() {
        let cfg = Config::default();
        let tree = graph(vec![
            topic("a", &[]),
            topic("b", &[("a", 0.9, true)]),
            topic("c", &[("a", 0.2, false)]),
        ]);
        let mut failed = TopicState {
            t0: Some(Timestamp::from_micros(T_US)),
            ..TopicState::default()
        };
        failed
            .kp_progress
            .insert("kp1".to_owned(), KpProgress::FailedOnce);
        assert!(in_retry_delay(&failed, &cfg, T_US));
        assert!(!in_retry_delay(&failed, &cfg, T_US + 2 * DAY_US));
        assert_eq!(retry_available_at(&TopicState::default(), &cfg), None);
        assert_eq!(retry_available_at(&learned(0.5), &cfg), None);

        let states: BTreeMap<String, TopicState> = [
            ("a".to_owned(), learned(0.5)),
            ("b".to_owned(), learned(0.55)),
            ("c".to_owned(), learned(0.51)),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            due_reviews(&states, &tree, &cfg, T_US, &BTreeSet::new()),
            ["a"]
        );
        assert_eq!(nearly_due(&states, &tree, &cfg, T_US), ["c", "b"]);
        let scope = TopicSet::empty(&tree);
        let targets: BTreeSet<String> = ["a".to_owned(), "b".to_owned()].into();
        assert_eq!(importance("ghost", &tree, &targets, &scope), 0.0);
        assert_eq!(
            importance("b", &tree, &targets, &scope),
            0.9 + 1.0 + CORE_BONUS
        );
        assert_eq!(
            order_lessons(&["c".to_owned(), "b".to_owned()], &tree, &targets, &scope),
            ["b", "c"]
        );
        assert_eq!(review_mix(&tree, "b"), ["kp1", "component:a"]);
        assert!(review_mix(&tree, "ghost").is_empty());
    }
}
