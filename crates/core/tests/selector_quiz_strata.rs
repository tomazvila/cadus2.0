//! Selector part 2b: the quiz strata by age, the learned filter, the deep old
//! pool, the activity-day boundary, and the seeded draw.
//!
//! These pins are 2.0's own: the strata come from the composer of
//! `selector.py:681-754`, and the draw is the SplitMix64 sampler of 2.0
//! (trap T11), so its permutation is a literal of this build.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;

use cadus_core::learner::TopicState;
use cadus_core::selector::{QuizPlan, QuizSampler, quiz_composer, quiz_is_due};
use common::selector::{
    cfg, graph_of, id_set, ids, learned, quiz_last_on, sampler, ten_learned, topic,
};
use common::{T_US, days};

/// Four topics learned five days ago: the whole recent stratum.
const RECENT: [&str; 4] = ["ra", "rb", "rc", "rd"];

/// The states of `RECENT` plus `older`, all learned at memory 0.8.
fn states_with(older: &[&str]) -> BTreeMap<String, TopicState> {
    RECENT
        .iter()
        .chain(older.iter())
        .map(|id| ((*id).to_owned(), learned(0.8)))
        .collect()
}

/// The graph of `RECENT` plus `older`, every topic without a prerequisite.
fn graph_with(older: &[&str]) -> cadus_core::curriculum::Curriculum {
    graph_of(
        RECENT
            .iter()
            .chain(older.iter())
            .map(|id| topic(id).build())
            .collect(),
        &[],
    )
}

/// The learned dates: `RECENT` five days ago, then `(id, days ago)` pairs.
fn learned_at(older: &[(&str, i64)]) -> BTreeMap<String, i64> {
    let mut map: BTreeMap<String, i64> = RECENT
        .iter()
        .map(|id| ((*id).to_owned(), T_US - days(5)))
        .collect();
    for (id, age) in older {
        map.insert((*id).to_owned(), T_US - days(*age));
    }
    map
}

/// The topics of one stratum of a plan.
fn stratum(plan: &QuizPlan, name: &str) -> BTreeSet<String> {
    plan.questions
        .iter()
        .filter(|question| question.stratum == name)
        .map(|question| question.topic.clone())
        .collect()
}

#[test]
fn the_old_stratum_holds_the_oldest_half_by_learned_date() {
    // The ids sort `a, b, c, d`, but the ages make `b` and `d` the oldest two.
    let older = ["a", "b", "c", "d"];
    let dates = learned_at(&[("a", 20), ("b", 60), ("c", 30), ("d", 50)]);
    let plan = quiz_composer(
        &states_with(&older),
        &graph_with(&older),
        &cfg(),
        T_US,
        &mut sampler(7),
        Some(&dates),
    );
    assert_eq!(stratum(&plan, "old"), id_set(&["b", "d"]));
    assert_eq!(stratum(&plan, "mid"), id_set(&["a", "c"]));
    assert_eq!(stratum(&plan, "recent"), id_set(&RECENT));
}

#[test]
fn the_rep_proxy_ranks_the_highest_rep_as_the_oldest() {
    // Without a learned date the `repNum` stands in for the age: `z` at rep 4
    // and `y` at rep 3 are the old half, whatever the ids say.
    let older = ["w", "x", "y", "z"];
    let mut states = states_with(&older);
    for (index, id) in older.iter().enumerate() {
        states.get_mut(*id).unwrap().rep_num = 1.0 + index as f64;
    }
    let plan = quiz_composer(
        &states,
        &graph_with(&older),
        &cfg(),
        T_US,
        &mut sampler(7),
        Some(&learned_at(&[])),
    );
    assert_eq!(stratum(&plan, "old"), id_set(&["y", "z"]));
    assert_eq!(stratum(&plan, "mid"), id_set(&["w", "x"]));
}

#[test]
fn the_quiz_stops_at_its_length_when_the_old_pool_is_deep() {
    // Ten older topics give five old and five mid candidates; the quiz still
    // takes two of each after the four recent ones.
    let older: Vec<String> = (0..10).map(|index| format!("o{index}")).collect();
    let older_ids: Vec<&str> = older.iter().map(String::as_str).collect();
    let ages: Vec<(&str, i64)> = older_ids
        .iter()
        .enumerate()
        .map(|(index, id)| (*id, 20 + 10 * i64::try_from(index).unwrap()))
        .collect();
    let plan = quiz_composer(
        &states_with(&older_ids),
        &graph_with(&older_ids),
        &cfg(),
        T_US,
        &mut sampler(7),
        Some(&learned_at(&ages)),
    );
    assert_eq!(plan.questions.len(), 8);
    assert_eq!(stratum(&plan, "recent").len(), 4);
    assert_eq!(stratum(&plan, "mid").len(), 2);
    assert_eq!(stratum(&plan, "old").len(), 2);
    assert!(stratum(&plan, "old").is_subset(&id_set(&older_ids[5..])));
}

#[test]
fn the_composer_asks_only_graph_topics_with_a_review_history() {
    // `ghost` has a history but no topic; `fresh` is a topic with no history.
    let graph = graph_of(
        ["a", "b", "c", "fresh"]
            .into_iter()
            .map(|id| topic(id).build())
            .collect(),
        &[],
    );
    let mut states: BTreeMap<String, TopicState> = ["a", "b", "c", "ghost"]
        .into_iter()
        .map(|id| (id.to_owned(), learned(0.8)))
        .collect();
    states.insert("fresh".to_owned(), TopicState::default());
    let plan = quiz_composer(&states, &graph, &cfg(), T_US, &mut sampler(7), None);
    assert_eq!(plan.questions.len(), 3);
    assert_eq!(
        plan.topics().into_iter().collect::<BTreeSet<&str>>(),
        ["a", "b", "c"].into_iter().collect()
    );
}

#[test]
fn a_study_day_on_the_last_quiz_date_does_not_count() {
    let (graph, states) = ten_learned();
    let cfg = cfg();
    let quiz = quiz_last_on(NaiveDate::from_ymd_opt(2026, 7, 1).unwrap());
    let now = common::noon_us(2026, 7, 30);
    // Seven study days from July 1: the quiz day itself is not one of the
    // seven the cadence needs, so six count.
    let from_quiz_day: Vec<NaiveDate> = (1..=7)
        .map(|day| NaiveDate::from_ymd_opt(2026, 7, day).unwrap())
        .collect();
    assert!(!quiz_is_due(
        Some(&quiz),
        &states,
        &graph,
        &cfg,
        now,
        Some(&from_quiz_day)
    ));
    let after_quiz_day: Vec<NaiveDate> = (2..=8)
        .map(|day| NaiveDate::from_ymd_opt(2026, 7, day).unwrap())
        .collect();
    assert!(quiz_is_due(
        Some(&quiz),
        &states,
        &graph,
        &cfg,
        now,
        Some(&after_quiz_day)
    ));
}

#[test]
fn the_seeded_draw_is_a_fixed_permutation() {
    let population: Vec<String> = (0..9).map(|index| format!("t{index}")).collect();
    assert_eq!(
        sampler(7).sample(&population, 9),
        ids(&["t3", "t1", "t8", "t6", "t0", "t5", "t7", "t4", "t2"])
    );
    // A shorter draw is the prefix of the full one.
    assert_eq!(sampler(7).sample(&population, 3), ids(&["t3", "t1", "t8"]));
    assert_eq!(
        sampler(1).sample(&population, 9),
        ids(&["t5", "t6", "t8", "t0", "t1", "t2", "t3", "t4", "t7"])
    );
}
