//! The lapse rule of D-L5: what the plan does when the learner returns after a
//! long break, or when one topic is long overdue.
//!
//! Two parts. A sitting after a break of [`LAPSE_BREAK_DAYS`] days or more holds
//! at most [`LAPSE_SITTING_TOPICS`] topics, foundations first. A review topic
//! overdue by more than [`LAPSE_OVERDUE_INTERVALS`] of its own intervals starts
//! with one question on a key prerequisite.

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;

use crate::config::Config;
use crate::curriculum::Curriculum;
use crate::fire::{ReviewState, review_state};
use crate::learner::TopicState;

use super::DAY_US;
use super::context::SessionContext;
use super::task::{Task, review_shell};

/// A break of this many days or more makes the next sitting a lapse sitting.
pub const LAPSE_BREAK_DAYS: i64 = 7;

/// A lapse sitting holds at most this many topics.
pub const LAPSE_SITTING_TOPICS: usize = 8;

/// A topic overdue by more than this many of its own intervals starts with a
/// prerequisite question.
pub const LAPSE_OVERDUE_INTERVALS: f64 = 3.0;

/// The whole days from the last study day to the day of `t_us`.
///
/// A study day is a day that holds an attempt. The result is 0 when the learner
/// studied today, and 0 when the context holds no study day at all.
#[must_use]
pub fn break_days(days: Option<&[NaiveDate]>, t_us: i64) -> i64 {
    let Some(today) = chrono::DateTime::from_timestamp_micros(t_us).map(|at| at.date_naive())
    else {
        return 0;
    };
    days.and_then(|days| days.iter().filter(|day| **day <= today).max())
        .map_or(0, |last| (today - *last).num_days())
}

/// Whether the context describes a sitting after a long break.
#[must_use]
pub fn is_lapse_sitting(ctx: &SessionContext<'_>, t_us: i64) -> bool {
    break_days(ctx.active_study_days, t_us) >= LAPSE_BREAK_DAYS
}

/// Cut the review and lesson lists of a lapse sitting to
/// [`LAPSE_SITTING_TOPICS`] topics.
///
/// The reviews go first, in course order, so the foundations of the course come
/// before the topics that depend on them. The lessons fill the room that is left.
pub(super) fn cap_sitting(
    reviews: &mut Vec<String>,
    lessons: &mut Vec<String>,
    graph: &Curriculum,
) {
    reviews.sort_by_key(|id| {
        (
            graph
                .idx_of(id)
                .map_or(usize::MAX, |idx| graph.load_index(idx)),
            id.clone(),
        )
    });
    reviews.truncate(LAPSE_SITTING_TOPICS);
    lessons.truncate(LAPSE_SITTING_TOPICS - reviews.len());
}

/// How many of its own intervals a topic is overdue.
///
/// The count is 0 for a topic with no start time or no interval.
fn overdue_intervals(state: &TopicState, t_us: i64) -> f64 {
    let Some(t0) = state.t0 else {
        return 0.0;
    };
    if state.interval_days <= 0.0 {
        return 0.0;
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "a span of microseconds only has to be close, not exact"
    )]
    let days = (t_us - t0.micros()) as f64 / DAY_US as f64;
    days / state.interval_days - 1.0
}

/// The one-question prerequisite reviews that stand before overdue topics.
///
/// The map is keyed by the overdue topic. The prerequisite is a key
/// prerequisite of one of its knowledge points that is due or nearly due, and
/// that the plan does not serve in another task. The first one in id order wins.
/// A topic with no such prerequisite gets no question.
pub(super) fn prerequisite_questions(
    served: &[String],
    busy: &BTreeSet<String>,
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    cfg: &Config,
    t_us: i64,
) -> BTreeMap<String, Task> {
    let mut taken: BTreeSet<String> = busy.clone();
    let mut out: BTreeMap<String, Task> = BTreeMap::new();
    for id in served {
        let Some(state) = states.get(id) else {
            continue;
        };
        if overdue_intervals(state, t_us) <= LAPSE_OVERDUE_INTERVALS {
            continue;
        }
        let Some(idx) = graph.idx_of(id) else {
            continue;
        };
        let keys: BTreeSet<&str> = graph
            .knowledge_points(idx)
            .iter()
            .flat_map(|point| point.key_prerequisites.iter().map(|slug| slug.as_str()))
            .collect();
        let pick = keys.into_iter().find(|key| {
            !taken.contains(*key)
                && graph.idx_of(key).is_some()
                && states.get(*key).is_some_and(|prior| {
                    matches!(
                        review_state(prior, t_us, cfg, false),
                        ReviewState::Due | ReviewState::NearlyDue
                    )
                })
        });
        let Some(key) = pick else {
            continue;
        };
        let nearly = states
            .get(key)
            .is_some_and(|prior| review_state(prior, t_us, cfg, false) == ReviewState::NearlyDue);
        taken.insert(key.to_owned());
        out.insert(
            id.clone(),
            Task {
                nearly_due: nearly,
                ..review_shell(
                    key,
                    states,
                    graph,
                    1,
                    format!("one question on a prerequisite of {id}, which is long overdue"),
                )
            },
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curriculum::Slug;
    use crate::event::{TaskType, Timestamp};
    use crate::fire::testing::{DAY_US as TEST_DAY_US, T_US, graph, learned, topic};
    use crate::selector::{SeededSampler, compose_session};

    fn day(text: &str) -> NaiveDate {
        text.parse().expect("a date")
    }

    /// A state learned `days` days before `T`, on a ten-day interval.
    fn aged(days: i64) -> TopicState {
        TopicState {
            t0: Some(Timestamp::from_micros(T_US - days * TEST_DAY_US)),
            ..learned(0.9)
        }
    }

    #[test]
    fn the_break_counts_whole_days_since_the_last_attempt_day() {
        // 2026-11-02 12:00 UTC.
        let t_us = 1_793_620_800_000_000;
        assert_eq!(break_days(None, t_us), 0);
        assert_eq!(break_days(Some(&[]), t_us), 0);
        assert_eq!(
            break_days(Some(&[day("2026-10-12"), day("2026-10-19")]), t_us),
            14
        );
        assert_eq!(break_days(Some(&[day("2026-11-02")]), t_us), 0);
        assert_eq!(break_days(Some(&[day("2026-10-26")]), t_us), 7);
    }

    /// A topic overdue by more than three intervals gets one question on a key
    /// prerequisite that is due. A topic overdue by less gets none.
    #[test]
    fn a_long_overdue_topic_starts_with_one_prerequisite_question() {
        let mut late = topic("late", &[("pre", 0.3, true)]);
        late.knowledge_points[0].key_prerequisites = vec![Slug::new("pre").expect("a slug")];
        let mut soon = topic("soon", &[("pre", 0.3, true)]);
        soon.knowledge_points[0].key_prerequisites = vec![Slug::new("pre").expect("a slug")];
        let tree = graph(vec![topic("pre", &[]), late, soon]);
        let cfg = Config::default();
        let states: BTreeMap<String, TopicState> = [
            ("pre".to_owned(), aged(30)),
            // 50 days on a 10-day interval: 4 intervals overdue.
            ("late".to_owned(), aged(50)),
            // 30 days on a 10-day interval: 2 intervals overdue.
            ("soon".to_owned(), aged(30)),
        ]
        .into();
        let busy: BTreeSet<String> = ["late".to_owned(), "soon".to_owned()].into();
        let served = ["late".to_owned(), "soon".to_owned()];
        let found = prerequisite_questions(&served, &busy, &states, &tree, &cfg, T_US);
        assert_eq!(found.len(), 1);
        let question = &found["late"];
        assert_eq!(question.topic.as_deref(), Some("pre"));
        assert_eq!(question.task_type, TaskType::Review);
        assert_eq!(question.n_problems, Some(1));
        // A prerequisite the plan already serves gives no second question.
        let taken: BTreeSet<String> = ["late".to_owned(), "pre".to_owned()].into();
        assert!(prerequisite_questions(&served, &taken, &states, &tree, &cfg, T_US).is_empty());
    }

    /// After a break of seven days the sitting holds at most eight topics, in
    /// course order. Without a break the plan keeps every due topic.
    #[test]
    fn a_sitting_after_a_break_holds_eight_topics_foundations_first() {
        let ids: Vec<String> = (0..12).map(|n| format!("t{n:02}")).collect();
        let topics = ids.iter().map(|id| topic(id, &[])).collect();
        let tree = graph(topics);
        let cfg = Config::default();
        let states: BTreeMap<String, TopicState> =
            ids.iter().map(|id| (id.clone(), learned(0.5))).collect();
        let studied = [
            chrono::DateTime::from_timestamp_micros(T_US - 9 * TEST_DAY_US)
                .expect("a date")
                .date_naive(),
        ];
        let plan_of = |days: Option<&[NaiveDate]>| {
            let ctx = SessionContext::default().with_active_study_days(days);
            compose_session(&states, &tree, &cfg, T_US, &mut SeededSampler::new(1), &ctx)
        };
        // The topics a plan serves: its reviews and the parts of its multi-step task.
        let reviews = |plan: &super::super::SessionPlan| -> Vec<String> {
            plan.tasks
                .iter()
                .flat_map(|task| match task.task_type {
                    TaskType::Review => task.topic.clone().into_iter().collect(),
                    TaskType::MultiStep => task.component_topics.clone(),
                    _ => Vec::new(),
                })
                .collect()
        };
        let lapse = plan_of(Some(&studied));
        let held = reviews(&lapse);
        assert_eq!(held.len(), LAPSE_SITTING_TOPICS);
        let mut course_order = ids.clone();
        course_order.sort_by_key(|id| tree.idx_of(id).map(|idx| tree.load_index(idx)));
        let mut sorted = held.clone();
        sorted.sort();
        course_order.truncate(LAPSE_SITTING_TOPICS);
        course_order.sort();
        assert_eq!(sorted, course_order);
        let recent = [
            chrono::DateTime::from_timestamp_micros(T_US - 2 * TEST_DAY_US)
                .expect("a date")
                .date_naive(),
        ];
        assert_eq!(reviews(&plan_of(Some(&recent))).len(), 12);
        assert_eq!(reviews(&plan_of(None)).len(), 12);
    }
}
