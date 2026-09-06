//! The drill schedule and the two remediation triggers
//! (`selector.py:845-903`).
//!
//! They read a topic state and answer WHAT the plan owes, and the builders of
//! `super::task` then shape the tasks.

use std::collections::{BTreeMap, BTreeSet};

use crate::curriculum::Curriculum;
use crate::event::{EventError, Slug};
use crate::learner::{PendingRemediation, TopicState};
use crate::numeric::round_half_even_i64_saturating;
use crate::xp::is_known;

use super::quiz::i64_as_float;
use super::{
    DAY_US, DRILL_INTERVAL_DAYS, DRILL_MASTERY_ABILITY, REMEDIATION_QUIZ_MISS,
    REMEDIATION_REPEAT_FAIL,
};

/// The drill-tagged topics due for a timed drill, SORTED
/// (`schedule_drills`, `selector.py:845-869`).
///
/// A topic qualifies when it is drill-tagged, known, still below the
/// automaticity bar, and outside the drill cadence window. `last_drill_at` maps
/// a topic id to the UTC microseconds of its last drill.
///
/// The cadence window rounds two constants, so it uses the saturating rounding
/// form and reports no error.
#[must_use]
pub fn schedule_drills(
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    t_us: i64,
    last_drill_at: Option<&BTreeMap<String, i64>>,
) -> Vec<String> {
    let window_us = round_half_even_i64_saturating(DRILL_INTERVAL_DAYS * i64_as_float(DAY_US));
    let mut out: Vec<String> = Vec::new();
    for topic in graph.topics() {
        if !topic.drill {
            continue;
        }
        let id = topic.id.as_str();
        let Some(state) = states.get(id) else {
            continue;
        };
        if !is_known(state) || state.ability >= DRILL_MASTERY_ABILITY {
            continue;
        }
        if let Some(last) = last_drill_at.and_then(|map| map.get(id).copied())
            && t_us.saturating_sub(last) < window_us
        {
            continue;
        }
        out.push(id.to_owned());
    }
    out.sort_unstable();
    out
}

/// The quiz-miss trigger: one remedial review of the missed topic
/// (`remediation_for_quiz_miss`, `selector.py:877-879`).
///
/// # Errors
///
/// Returns [`EventError`] when `topic` is not a legal slug.
pub fn remediation_for_quiz_miss(topic: &str) -> Result<PendingRemediation, EventError> {
    Ok(PendingRemediation {
        kind: REMEDIATION_QUIZ_MISS.to_owned(),
        targets: vec![Slug::new(topic)?],
    })
}

/// The repeat-fail trigger: remedial work on the key prerequisites of the failed
/// knowledge point (`remediation_for_repeat_fail`, `selector.py:882-903`).
///
/// The targets are the key prerequisites that are themselves topics, sorted.
/// [`compose_session`] serves each as a review or a lesson, by mastery.
#[must_use]
pub fn remediation_for_repeat_fail(
    topic: &str,
    failed_kp: &str,
    graph: &Curriculum,
) -> PendingRemediation {
    let mut key_prereqs: BTreeSet<&str> = BTreeSet::new();
    if let Some(idx) = graph.idx_of(topic) {
        for kp in graph.knowledge_points(idx) {
            if kp.id.as_str() == failed_kp {
                for key in &kp.key_prerequisites {
                    key_prereqs.insert(key.as_str());
                }
            }
        }
    }
    PendingRemediation {
        kind: REMEDIATION_REPEAT_FAIL.to_owned(),
        targets: key_prereqs
            .into_iter()
            .filter(|id| graph.idx_of(id).is_some())
            .filter_map(|id| Slug::new(id).ok())
            .collect(),
    }
}
