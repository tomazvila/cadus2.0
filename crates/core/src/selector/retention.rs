//! The selector side of the delayed retention probe (D-F11).
//!
//! [`retention_probe`] is the ONE hook a session composer calls. It reads the
//! probe policy, the lesson pass instants the context already carries, and the
//! probes the fold recorded, and it answers with at most one [`ProbePlan`].
//!
//! The hook is deliberately thin and pure. It composes no task and it appends no
//! event: the serve path decides which item the plan serves, and
//! [`crate::retention::unseen_item`] holds the rule that the item must be one the
//! learner never saw.

use std::collections::BTreeMap;

use crate::config::Config;
use crate::curriculum::Curriculum;
use crate::learner::TopicState;
use crate::retention::{ProbePlan, due_probe};

use super::context::SessionContext;

/// The retention probe this session owes, or `None` (D-F11).
///
/// The answer is `None` when the policy is off, when the session already carries
/// its probe, when no lesson pass is old enough, or when the context carries no
/// `learned_at` map.
#[must_use]
pub fn retention_probe(
    ctx: &SessionContext<'_>,
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    cfg: &Config,
    t_us: i64,
) -> Option<ProbePlan> {
    let learned_at = ctx.learned_at?;
    let retention = ctx.retention?;
    due_probe(
        graph,
        states,
        retention,
        &cfg.retention,
        learned_at,
        ctx.session_id,
        t_us,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curriculum::Topic;
    use crate::event::KpProgress;
    use crate::fire::testing::{DAY_US, graph, knowledge_point, topic};
    use crate::retention::RetentionState;

    /// Course `c` with one topic `t1` and its knowledge point `t1k1`.
    fn tree() -> Curriculum {
        graph(vec![Topic {
            knowledge_points: vec![knowledge_point("t1k1", &[])],
            ..topic("t1", &[])
        }])
    }

    /// `t1` with a passed knowledge point.
    fn states() -> BTreeMap<String, TopicState> {
        let mut state = TopicState::default();
        state
            .kp_progress
            .insert("t1k1".to_owned(), KpProgress::Passed);
        BTreeMap::from([("t1".to_owned(), state)])
    }

    #[test]
    fn a_context_with_no_retention_state_probes_nothing() {
        let learned = BTreeMap::from([("t1".to_owned(), 0_i64)]);
        let ctx = SessionContext::default()
            .with_session_id("s1")
            .with_learned_at(Some(&learned));
        assert_eq!(
            retention_probe(&ctx, &states(), &tree(), &Config::default(), 90 * DAY_US),
            None
        );
    }

    #[test]
    fn the_hook_answers_the_due_probe() {
        let learned = BTreeMap::from([("t1".to_owned(), 0_i64)]);
        let retention = RetentionState::default();
        let ctx = SessionContext::default()
            .with_session_id("s1")
            .with_learned_at(Some(&learned))
            .with_retention(Some(&retention));
        let plan = retention_probe(&ctx, &states(), &tree(), &Config::default(), 30 * DAY_US)
            .expect("a probe");
        assert_eq!(plan.kp, "t1k1");
        assert_eq!(plan.delay_days, 7);
    }

    #[test]
    fn a_context_with_no_learned_at_map_probes_nothing() {
        let retention = RetentionState::default();
        let ctx = SessionContext::default()
            .with_session_id("s1")
            .with_retention(Some(&retention));
        assert_eq!(
            retention_probe(&ctx, &states(), &tree(), &Config::default(), 90 * DAY_US),
            None
        );
    }
}
