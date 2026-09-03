//! Builders the FIRe attempt tests share: the p.364 states, one pass or one miss
//! on a topic, and the two-topic pair graph of the propagation gates.

use std::collections::BTreeMap;

use indexmap::IndexMap;

use cadus_core::config::Config;
use cadus_core::curriculum::Curriculum;
use cadus_core::event::WorkQuality;
use cadus_core::fire::{AttemptResult, Propagation, ability_update, apply_attempt};
use cadus_core::learner::TopicState;

use super::{Learned, T_US, graph, p364_graph, plain_topic};

/// The states the 1.0 tests build with a dict comprehension over the graph.
pub fn states_of(entries: &[(&str, TopicState)]) -> BTreeMap<String, TopicState> {
    entries
        .iter()
        .map(|(id, state)| ((*id).to_owned(), state.clone()))
        .collect()
}

/// The three p.364 topics, each built by `make`.
pub fn p364_states(make: impl Fn() -> TopicState) -> BTreeMap<String, TopicState> {
    states_of(&[
        ("addition", make()),
        ("one-digit-mult", make()),
        ("two-digit-mult", make()),
    ])
}

/// A perfect, unassisted pass on `two-digit-mult` at `T`, with the default config.
pub fn pass_two_digit(
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
) -> (BTreeMap<String, TopicState>, Vec<Propagation>) {
    apply_attempt(
        states,
        &AttemptResult::new("two-digit-mult", true, WorkQuality::Perfect),
        graph,
        &Config::default(),
        T_US,
    )
}

/// `two-digit-mult` encompasses `addition` at `weight`, and nothing else.
pub fn pair_graph(weight: f64) -> Curriculum {
    graph(vec![
        plain_topic("addition", &[]),
        plain_topic("two-digit-mult", &[("addition", weight, false)]),
    ])
}

/// One solo topic, a poor miss on it, and the state after that miss.
pub fn solo_miss(state: TopicState) -> TopicState {
    let graph = graph(vec![plain_topic("solo", &[])]);
    let attempt = AttemptResult::new("solo", false, WorkQuality::Poor);
    let (next, _) = apply_attempt(
        &states_of(&[("solo", state)]),
        &attempt,
        &graph,
        &Config::default(),
        T_US,
    );
    next["solo"].clone()
}

/// One solo topic, a perfect pass on it, and the state after that pass.
pub fn solo_pass(id: &str, state: TopicState) -> TopicState {
    let graph = graph(vec![plain_topic(id, &[])]);
    let attempt = AttemptResult::new(id, true, WorkQuality::Perfect);
    let (next, _) = apply_attempt(
        &states_of(&[(id, state)]),
        &attempt,
        &graph,
        &Config::default(),
        T_US,
    );
    next[id].clone()
}

/// The two touched neighbors of the seeding tests, at abilities 0.4 and 0.8.
pub fn seed_states() -> BTreeMap<String, TopicState> {
    states_of(&[
        ("addition", Learned::new(0.5).ability(0.4).state()),
        ("one-digit-mult", Learned::new(0.5).ability(0.8).state()),
    ])
}

/// The p.364 states at ability 0.5, and the deltas of one attempt on `topic`.
pub fn abilities_after(topic: &str, correct: bool) -> IndexMap<String, f64> {
    let states = p364_states(|| Learned::new(0.5).ability(0.5).state());
    ability_update(&states, topic, correct, &p364_graph(), &Config::default())
}
