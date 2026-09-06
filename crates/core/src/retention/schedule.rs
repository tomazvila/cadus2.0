//! The delayed-probe schedule: which knowledge point a session probes (D-F11).
//!
//! The clock is the LESSON PASS instant of the topic, `learned_at`, the same map
//! the selector already carries in its [`SessionContext`](crate::selector::SessionContext).
//! Nothing here reads a wall clock of its own: the caller hands `now_us` over, so a
//! test drives the date.
//!
//! The rules, in order:
//!
//! 1. If `retention.enabled` is off, or no delay is configured, probe nothing.
//! 2. If the session already carries `max_per_session` probes, probe nothing.
//! 3. A knowledge point is a candidate when its lesson passed, the topic has a
//!    `learned_at` instant, and one configured delay at or below the elapsed days
//!    is not probed yet.
//! 4. The MOST OVERDUE candidate wins, then the lowest topic id, then the lowest
//!    knowledge point id. The order is total, so the plan is deterministic.
//!
//! The item the probe serves must be UNSEEN. [`unseen_item`] applies that rule
//! against the topic's recent problems and the digests of the earlier probes.

use std::collections::{BTreeMap, BTreeSet};

use crate::curriculum::Curriculum;
use crate::event::KpProgress;
use crate::learner::TopicState;

use super::policy::RetentionConfig;
use super::state::RetentionState;

/// The microseconds of one day. The elapsed days round DOWN.
const DAY_US: i64 = 86_400_000_000;

/// One scheduled retention probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbePlan {
    /// The topic of the knowledge point.
    pub topic: String,
    /// The knowledge point the probe tests.
    pub kp: String,
    /// The configured delay, in days, the probe reports under.
    pub delay_days: u32,
    /// The days between the lesson pass and now. It is at least `delay_days`.
    pub elapsed_days: u32,
}

/// The days from `learned_at` to `now_us`, rounded down. A future stamp gives 0.
fn elapsed_days(learned_us: i64, now_us: i64) -> u32 {
    let delta = now_us.saturating_sub(learned_us);
    if delta <= 0 {
        return 0;
    }
    u32::try_from(delta / DAY_US).unwrap_or(u32::MAX)
}

/// The probe this session owes, or `None` (D-F11).
///
/// See the module documentation for the rules. `learned_at` maps a topic id to the
/// microsecond instant of its lesson pass.
#[must_use]
pub fn due_probe(
    graph: &Curriculum,
    topics: &BTreeMap<String, TopicState>,
    retention: &RetentionState,
    cfg: &RetentionConfig,
    learned_at: &BTreeMap<String, i64>,
    session_id: &str,
    now_us: i64,
) -> Option<ProbePlan> {
    let delays = cfg.delays();
    if !cfg.enabled || delays.is_empty() {
        return None;
    }
    let served = u32::try_from(retention.probes_in_session(session_id)).unwrap_or(u32::MAX);
    if served >= cfg.max_per_session {
        return None;
    }
    let mut best: Option<(u32, ProbePlan)> = None;
    for (topic, state) in topics {
        let Some(learned_us) = learned_at.get(topic) else {
            continue;
        };
        let elapsed = elapsed_days(*learned_us, now_us);
        let Some(idx) = graph.idx_of(topic) else {
            continue;
        };
        for kp in graph.knowledge_points(idx) {
            let id = kp.id.as_str();
            if state.kp_progress.get(id) != Some(&KpProgress::Passed) {
                continue;
            }
            let Some(delay) = delays
                .iter()
                .copied()
                .find(|delay| *delay <= elapsed && !retention.is_done(id, *delay))
            else {
                continue;
            };
            let overdue = elapsed.saturating_sub(delay);
            let plan = ProbePlan {
                topic: topic.clone(),
                kp: id.to_owned(),
                delay_days: delay,
                elapsed_days: elapsed,
            };
            let better = match &best {
                None => true,
                Some((best_overdue, best_plan)) => {
                    overdue > *best_overdue
                        || (overdue == *best_overdue
                            && (&plan.topic, &plan.kp) < (&best_plan.topic, &best_plan.kp))
                }
            };
            if better {
                best = Some((overdue, plan));
            }
        }
    }
    best.map(|(_, plan)| plan)
}

/// The digests the learner already saw for `topic`.
///
/// It is the topic's recent problems plus every digest an earlier probe used.
#[must_use]
pub fn seen_digests<'a>(
    topics: &'a BTreeMap<String, TopicState>,
    retention: &'a RetentionState,
    topic: &str,
) -> BTreeSet<&'a str> {
    let mut seen: BTreeSet<&str> = retention
        .digests
        .iter()
        .map(std::string::String::as_str)
        .collect();
    if let Some(state) = topics.get(topic) {
        seen.extend(state.last_problems.iter().map(std::string::String::as_str));
    }
    seen
}

/// The first candidate digest the learner never saw, in candidate order.
///
/// `None` means every candidate is seen; the caller then serves NO probe rather
/// than a repeat, because a repeat is not delayed-retention evidence.
#[must_use]
pub fn unseen_item<'a>(candidates: &'a [String], seen: &BTreeSet<&str>) -> Option<&'a str> {
    candidates
        .iter()
        .map(std::string::String::as_str)
        .find(|digest| !seen.contains(digest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curriculum::Topic;
    use crate::event::{AttemptOutcome, Exposure};
    use crate::fire::testing::{graph, knowledge_point, topic};
    use crate::retention::state::tests::probe;

    /// Topic `id` whose knowledge points are `kps`, in that order.
    fn with_kps(id: &str, kps: &[&str]) -> Topic {
        Topic {
            knowledge_points: kps.iter().map(|kp| knowledge_point(kp, &[])).collect(),
            ..topic(id, &[])
        }
    }

    /// Course `c`: `t1` with `t1k1`, and `t2` with `t2k1`.
    fn tree() -> Curriculum {
        graph(vec![with_kps("t1", &["t1k1"]), with_kps("t2", &["t2k1"])])
    }

    /// A topic state whose named knowledge points passed.
    fn passed(kps: &[&str]) -> TopicState {
        let mut state = TopicState::default();
        for kp in kps {
            state
                .kp_progress
                .insert((*kp).to_owned(), KpProgress::Passed);
        }
        state
    }

    /// One passed topic `t1` and its lesson pass at instant 0.
    fn one_topic() -> (BTreeMap<String, TopicState>, BTreeMap<String, i64>) {
        (
            BTreeMap::from([("t1".to_owned(), passed(&["t1k1"]))]),
            BTreeMap::from([("t1".to_owned(), 0_i64)]),
        )
    }

    #[test]
    fn no_probe_before_the_first_delay() {
        let (topics, learned) = one_topic();
        let plan = due_probe(
            &tree(),
            &topics,
            &RetentionState::default(),
            &RetentionConfig::default(),
            &learned,
            "s1",
            6 * DAY_US,
        );
        assert_eq!(plan, None);
    }

    #[test]
    fn the_seven_day_probe_opens_on_day_seven() {
        let (topics, learned) = one_topic();
        let plan = due_probe(
            &tree(),
            &topics,
            &RetentionState::default(),
            &RetentionConfig::default(),
            &learned,
            "s1",
            7 * DAY_US,
        )
        .expect("a probe");
        assert_eq!(plan.topic, "t1");
        assert_eq!(plan.kp, "t1k1");
        assert_eq!(plan.delay_days, 7);
        assert_eq!(plan.elapsed_days, 7);
    }

    #[test]
    fn one_probe_per_session_at_most() {
        let (topics, learned) = one_topic();
        let cfg = RetentionConfig::default();
        let mut retention = RetentionState::default();
        let mut event = probe(
            "t1k1",
            7,
            AttemptOutcome::Correct,
            false,
            Some(Exposure::First),
        );
        event.session = Some("s1".to_owned());
        retention.apply(&event, &cfg);
        let same = due_probe(
            &tree(),
            &topics,
            &retention,
            &cfg,
            &learned,
            "s1",
            40 * DAY_US,
        );
        assert_eq!(same, None, "the session already carried its probe");
        let next = due_probe(
            &tree(),
            &topics,
            &retention,
            &cfg,
            &learned,
            "s2",
            40 * DAY_US,
        )
        .expect("the next session probes");
        assert_eq!(next.delay_days, 30, "the 7-day probe is done");
    }

    #[test]
    fn a_disabled_policy_probes_nothing() {
        let (topics, learned) = one_topic();
        let cfg = RetentionConfig {
            enabled: false,
            ..RetentionConfig::default()
        };
        let plan = due_probe(
            &tree(),
            &topics,
            &RetentionState::default(),
            &cfg,
            &learned,
            "s1",
            90 * DAY_US,
        );
        assert_eq!(plan, None);
    }

    #[test]
    fn an_unpassed_knowledge_point_is_no_candidate() {
        let topics = BTreeMap::from([("t1".to_owned(), TopicState::default())]);
        let learned = BTreeMap::from([("t1".to_owned(), 0_i64)]);
        let plan = due_probe(
            &tree(),
            &topics,
            &RetentionState::default(),
            &RetentionConfig::default(),
            &learned,
            "s1",
            90 * DAY_US,
        );
        assert_eq!(plan, None);
    }

    #[test]
    fn the_most_overdue_candidate_wins() {
        let topics = BTreeMap::from([
            ("t1".to_owned(), passed(&["t1k1"])),
            ("t2".to_owned(), passed(&["t2k1"])),
        ]);
        // `t2` passed 40 days before now and `t1` passed 8 days before now.
        let learned = BTreeMap::from([("t1".to_owned(), 32 * DAY_US), ("t2".to_owned(), 0_i64)]);
        let plan = due_probe(
            &tree(),
            &topics,
            &RetentionState::default(),
            &RetentionConfig::default(),
            &learned,
            "s1",
            40 * DAY_US,
        )
        .expect("a probe");
        assert_eq!(plan.topic, "t2");
        assert_eq!(plan.delay_days, 7);
        assert_eq!(plan.elapsed_days, 40);
    }

    #[test]
    fn an_equal_overdue_pair_breaks_to_the_lowest_topic_id() {
        let topics = BTreeMap::from([
            ("t1".to_owned(), passed(&["t1k1"])),
            ("t2".to_owned(), passed(&["t2k1"])),
        ]);
        let learned = BTreeMap::from([("t1".to_owned(), 0_i64), ("t2".to_owned(), 0_i64)]);
        let plan = due_probe(
            &tree(),
            &topics,
            &RetentionState::default(),
            &RetentionConfig::default(),
            &learned,
            "s1",
            9 * DAY_US,
        )
        .expect("a probe");
        assert_eq!(plan.topic, "t1");
    }

    #[test]
    fn a_topic_with_no_lesson_pass_instant_is_no_candidate() {
        let (topics, _) = one_topic();
        let plan = due_probe(
            &tree(),
            &topics,
            &RetentionState::default(),
            &RetentionConfig::default(),
            &BTreeMap::new(),
            "s1",
            90 * DAY_US,
        );
        assert_eq!(plan, None);
    }

    #[test]
    fn every_configured_delay_runs_once_and_then_stops() {
        let (topics, learned) = one_topic();
        let cfg = RetentionConfig::default();
        let mut retention = RetentionState::default();
        let mut served = Vec::new();
        for (index, day) in [7_i64, 30, 90, 200].iter().enumerate() {
            let session = format!("s{index}");
            if let Some(plan) = due_probe(
                &tree(),
                &topics,
                &retention,
                &cfg,
                &learned,
                &session,
                day * DAY_US,
            ) {
                served.push(plan.delay_days);
                let mut event = probe(
                    &plan.kp,
                    plan.delay_days,
                    AttemptOutcome::Correct,
                    false,
                    Some(Exposure::First),
                );
                event.session = Some(session);
                retention.apply(&event, &cfg);
            }
        }
        assert_eq!(served, vec![7, 30, 90]);
        assert_eq!(
            due_probe(
                &tree(),
                &topics,
                &retention,
                &cfg,
                &learned,
                "s9",
                400 * DAY_US
            ),
            None,
            "every configured delay is done"
        );
    }

    #[test]
    fn the_unseen_rule_skips_a_recent_problem_and_an_earlier_probe() {
        let state = TopicState {
            last_problems: vec!["d1".to_owned()],
            ..TopicState::default()
        };
        let topics = BTreeMap::from([("t1".to_owned(), state)]);
        let retention = RetentionState {
            digests: vec!["d2".to_owned()],
            ..RetentionState::default()
        };
        let seen = seen_digests(&topics, &retention, "t1");
        let candidates = ["d1".to_owned(), "d2".to_owned(), "d3".to_owned()];
        assert_eq!(unseen_item(&candidates, &seen), Some("d3"));
        let only_seen = ["d1".to_owned(), "d2".to_owned()];
        assert_eq!(unseen_item(&only_seen, &seen), None);
    }
}
