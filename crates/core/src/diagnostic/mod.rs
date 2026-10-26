//! The adaptive placement diagnostic (`cadus/diagnostic.py`, PEDAGOGY section 9).
//!
//! The module is a port of the 1.0 file of the same name. Every function is a
//! deterministic function of `(state, graph, config, answer)`. It reads no clock,
//! opens no connection, and calls no model, so a replay of the same answers gives
//! the same placement.
//!
//! The four steps of one placement:
//!
//! 1. [`init_session`] builds the universe (the enrolled course plus its
//!    foundations, less the mastery floor) and the probe set (the compression of
//!    that universe at `diag.coverage_radius`, less the floor and less every topic
//!    with no `diagnostic_exemplar`).
//! 2. [`answer_weight`] prices one answer. A correct answer at or under the
//!    expected time weighs a full 1.0, and a slower correct answer discounts
//!    toward [`SLOW_WEIGHT_FLOOR`]. An incorrect answer always weighs 1.0.
//! 3. [`apply_answer`] folds that weight into the running plus-minus tally. A
//!    correct answer credits the topic AND every prerequisite ancestor in full. An
//!    incorrect one debits the topic in full and every post-requisite descendant
//!    by [`MISS_DESCENDANT_SHARE`] of the weight, so one miss leaves the region
//!    above it undetermined and a second miss in the branch settles it. A leaf
//!    topic also passes `diag.sibling_credit` of the same signed weight to its
//!    sibling leaves in its module, which is the only lateral signal a leaf emits.
//! 4. [`next_probe`] asks the undetermined probe topic whose answer settles the
//!    most of the remaining graph, and [`placement`] turns the final tally into
//!    the placement credits.
//!
//! WHAT THIS MODULE DOES NOT DO. It never seeds a FIRe state. `placement` carries
//! the raw balance of each placed topic, and `Projector::on_diagnostic_placed`
//! alone turns a balance into `repNum`, `memoryBase` and an interval, off the
//! `diagnostic_placed` event. Two writers of that rule give two answers after a
//! replay, so there is exactly one.

use std::collections::{BTreeMap, BTreeSet};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::curriculum::{Curriculum, TopicIdx};

mod probe;

pub use probe::probe_set;

/// The undetermined band of PEDAGOGY section 9: a topic is DETERMINED once the
/// magnitude of its balance reaches this value.
///
/// It equals `diag.conditional_max` in the default configuration and it stays a
/// separate constant, because the two answer different questions. This one ends
/// the questioning; `conditional_max` flags a thin placement.
pub const DETERMINED_THRESHOLD: f64 = 1.0;

/// The share of a miss weight that each post-requisite descendant of the asked
/// topic takes.
///
/// One miss is a question, and two misses in a branch are an answer. With a full
/// share, one slip at a mid-level topic settled the whole course above it as
/// "not known". With half, a descendant reaches [`DETERMINED_THRESHOLD`] only
/// after a second miss in its branch, so the region keeps its probes.
pub const MISS_DESCENDANT_SHARE: f64 = 0.5;

/// The floor of the accuracy-time weight of a correct answer.
pub const SLOW_WEIGHT_FLOOR: f64 = 0.3;

/// An in-progress diagnostic: the `diag_states.state` document.
///
/// `balances` is BOTH the running tally and the definition of the universe. Its
/// key set is every in-scope topic outside the mastery floor, so the scope and the
/// tally cannot disagree. The map keeps INSERTION ORDER, and `init_session`
/// inserts in sorted id order, because the `diagnostic_placed` event carries the
/// balances and the refresh path of the projector reads them in order.
///
/// `probe_set` is the subset the diagnostic is allowed to ASK. `answered` is the
/// ordered list of topics it did ask. A stored key this struct does not name is
/// ignored on load, so an older row needs no migration.
///
/// `missed` and `second` serve the second phase. `missed` holds the weight of the
/// first direct answer of each topic that the learner missed. `second` holds the
/// topics that were asked a second time, and whether that second answer was
/// correct. A row stored before the second phase has neither key and reads as
/// empty.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DiagState {
    /// The enrolled course, or `None` for the whole curriculum.
    #[serde(default)]
    pub course: Option<String>,
    /// The topics the diagnostic asks from, sorted.
    #[serde(default)]
    pub probe_set: Vec<String>,
    /// The plus-minus tally over the universe, in sorted id order.
    #[serde(default)]
    pub balances: IndexMap<String, f64>,
    /// The topics already asked, in the order they were asked.
    #[serde(default)]
    pub answered: Vec<String>,
    /// Whether the run is in the second phase, which re-asks thin and missed topics.
    #[serde(default)]
    pub supplemental: bool,
    /// The weight of the first direct answer of each topic that was missed.
    #[serde(default)]
    pub missed: IndexMap<String, f64>,
    /// The topics asked a second time. The value is true when that answer was correct.
    #[serde(default)]
    pub second: IndexMap<String, bool>,
}

impl DiagState {
    /// The count of questions asked: first answers and second answers.
    #[must_use]
    pub fn question_count(&self) -> usize {
        self.answered.len() + self.second.len()
    }

    /// Whether the next answer of `topic` is its second answer.
    #[must_use]
    pub fn awaits_second(&self, topic: &str) -> bool {
        self.supplemental
            && self.answered.iter().any(|asked| asked == topic)
            && !self.second.contains_key(topic)
    }

    /// Whether the topic is in the universe.
    #[must_use]
    pub fn in_universe(&self, topic: &str) -> bool {
        self.balances.contains_key(topic)
    }
}

/// The outcome of [`placement`].
///
/// `placed` maps a placed topic to its plus-minus balance, which is the evidence
/// and not a seeded state. It is the payload of the `diagnostic_placed` event.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlacementResult {
    /// The placed topics and their balances, in sorted id order.
    pub placed: IndexMap<String, f64>,
    /// The placed topics whose balance is thin, sorted.
    pub conditional: Vec<String>,
    /// The untouched topics a supplemental diagnostic covers, sorted.
    pub supplemental_candidates: Vec<String>,
}

impl PlacementResult {
    /// The balances the `diagnostic_placed` event carries.
    ///
    /// Each one is rounded to six decimals, as 1.0 rounds them, so a replay of
    /// the log gives the same seeded state on every machine.
    #[must_use]
    pub fn event_balances(&self) -> IndexMap<String, f64> {
        self.placed
            .iter()
            .map(|(topic, balance)| (topic.clone(), (balance * 1e6).round() / 1e6))
            .collect()
    }
}

/// The course and its foundations, or the whole graph for `None`.
#[must_use]
pub fn scope(graph: &Curriculum, course: Option<&str>) -> BTreeSet<String> {
    let Some(course) = course else {
        return graph
            .topics()
            .iter()
            .map(|topic| topic.id.as_str().to_owned())
            .collect();
    };
    let mut out: BTreeSet<String> = BTreeSet::new();
    for &idx in graph.topics_in_course(course) {
        out.insert(graph.id_of(idx).to_owned());
        for ancestor in graph.ancestors(idx) {
            out.insert(graph.id_of(ancestor).to_owned());
        }
    }
    out
}

/// The mastery floor of a course, as ids. An absent course has an empty floor.
fn floor_ids(graph: &Curriculum, course: Option<&str>) -> BTreeSet<String> {
    let Some(course) = course else {
        return BTreeSet::new();
    };
    graph
        .mastery_floor(course)
        .unwrap_or_default()
        .into_iter()
        .map(|idx| graph.id_of(idx).to_owned())
        .collect()
}

/// Whether the topic has no dependent inside its own course.
#[must_use]
pub fn is_leaf(graph: &Curriculum, idx: TopicIdx) -> bool {
    let course = graph.course_of(idx);
    !graph
        .descendants(idx)
        .into_iter()
        .any(|dep| graph.course_of(dep) == course)
}

/// The in-universe leaves of the topic's module, the topic itself excluded.
fn sibling_leaves(graph: &Curriculum, idx: TopicIdx, state: &DiagState) -> Vec<String> {
    let module = graph.module_of(idx);
    let id = graph.id_of(idx);
    graph
        .topics_in_module(module)
        .iter()
        .filter(|&&other| {
            let other_id = graph.id_of(other);
            other_id != id && state.in_universe(other_id) && is_leaf(graph, other)
        })
        .map(|&other| graph.id_of(other).to_owned())
        .collect()
}

// --------------------------------------------------------------------------- //
// Session start
// --------------------------------------------------------------------------- //

/// Start a diagnostic over the course and its foundations.
///
/// The probe set drops every floored topic, because the floor is auto-mastered
/// and never probed, and every topic with no `diagnostic_exemplar`, because there
/// is no question to ask. The balances start at zero over the whole universe.
#[must_use]
pub fn init_session(graph: &Curriculum, cfg: &Config, course: Option<&str>) -> DiagState {
    let in_scope = scope(graph, course);
    let floor = floor_ids(graph, course);
    let mut balances: IndexMap<String, f64> = IndexMap::new();
    for id in &in_scope {
        if !floor.contains(id) {
            balances.insert(id.clone(), 0.0);
        }
    }
    let probes = probe_set(graph, course, cfg.diag.coverage_radius);
    let probe_set: Vec<String> = probes
        .into_iter()
        .filter(|id| {
            !floor.contains(id)
                && graph
                    .idx_of(id)
                    .and_then(|idx| graph.topic(idx))
                    .is_some_and(|topic| topic.diagnostic_exemplar.is_some())
        })
        .collect();
    DiagState {
        course: course.map(ToOwned::to_owned),
        probe_set,
        balances,
        answered: Vec::new(),
        supplemental: false,
        missed: IndexMap::new(),
        second: IndexMap::new(),
    }
}

// --------------------------------------------------------------------------- //
// One answer
// --------------------------------------------------------------------------- //

/// The plus-minus weight of one answer.
///
/// A correct answer weighs `clamp(expected_time / actual_secs, 0.3, 1.0)`: a full
/// 1.0 at or under the expected time, and a discount toward the floor when the
/// learner was slower. A missing or non-positive time counts as fast. An
/// incorrect answer always weighs 1.0, because a failure counts in full.
#[must_use]
pub fn answer_weight(correct: bool, expected_time_secs: f64, actual_secs: f64) -> f64 {
    if !correct {
        return 1.0;
    }
    if actual_secs <= 0.0 {
        return 1.0;
    }
    (expected_time_secs / actual_secs).clamp(SLOW_WEIGHT_FLOOR, 1.0)
}

/// Fold one weighted answer into the tally.
///
/// A correct answer adds the weight to the topic and to every prerequisite
/// ancestor, in full. An incorrect one subtracts it from the topic in full and
/// subtracts [`MISS_DESCENDANT_SHARE`] of it from every post-requisite
/// descendant. A leaf topic also moves `diag.sibling_credit` of the
/// signed weight to its sibling leaves, so a missed leaf spreads negative evidence
/// the same way a correct one spreads positive evidence.
///
/// Only in-universe topics change. The topic joins `answered` once.
pub fn apply_answer(
    state: &mut DiagState,
    graph: &Curriculum,
    topic: &str,
    correct: bool,
    weight: f64,
    cfg: &Config,
) {
    let Some(idx) = graph.idx_of(topic) else {
        return;
    };
    let delta = if correct { weight } else { -weight };
    // The asked topic takes the full delta. A credit reaches each ancestor in
    // full. A miss reaches each descendant at `MISS_DESCENDANT_SHARE`.
    let mut affected: BTreeMap<String, f64> = BTreeMap::new();
    affected.insert(topic.to_owned(), delta);
    if correct {
        for other in graph.ancestors(idx) {
            affected.insert(graph.id_of(other).to_owned(), delta);
        }
    } else {
        for other in graph.descendants(idx) {
            affected.insert(graph.id_of(other).to_owned(), delta * MISS_DESCENDANT_SHARE);
        }
    }
    for (id, change) in affected {
        if let Some(balance) = state.balances.get_mut(&id) {
            *balance += change;
        }
    }

    if is_leaf(graph, idx) {
        let lateral = cfg.diag.sibling_credit * delta;
        // Every sibling leaf is in the universe, so the entry is always present.
        for sibling in sibling_leaves(graph, idx, state) {
            state
                .balances
                .entry(sibling)
                .and_modify(|balance| *balance += lateral);
        }
    }

    if !state.answered.iter().any(|asked| asked == topic) {
        state.answered.push(topic.to_owned());
        if !correct {
            state.missed.insert(topic.to_owned(), weight);
        }
    }
}

/// Fold the second answer of a topic into the tally.
///
/// The second answer moves the asked topic. It does not credit an ancestor and
/// it does not debit a descendant. Four cases:
///
/// - First miss, second correct: the miss is undone. The topic gets back the
///   miss weight and takes the second weight. Each descendant and each sibling
///   leaf gets back what the miss took from it.
/// - First miss, second miss: the miss stands. Only the record changes.
/// - First correct, second correct: the topic takes the second weight and leaves
///   the conditional list in [`placement`].
/// - First correct, second miss: the balance of the topic goes to zero. The
///   topic is not placed. The ancestors keep the credit of the first answer.
///
/// A weight of zero with `correct == false` is an answer that could not be
/// marked. It only records the second ask. The call does nothing for a topic
/// outside the universe, a topic not asked before, or a topic already asked twice.
pub fn apply_second_answer(
    state: &mut DiagState,
    graph: &Curriculum,
    topic: &str,
    correct: bool,
    weight: f64,
    cfg: &Config,
) {
    let asked_before = state.answered.iter().any(|asked| asked == topic);
    if !state.in_universe(topic) || !asked_before || state.second.contains_key(topic) {
        return;
    }
    let first_miss = state.missed.get(topic).copied();
    match (first_miss, correct) {
        (Some(miss), true) => {
            let mut back: BTreeMap<String, f64> = BTreeMap::new();
            back.insert(topic.to_owned(), miss + weight);
            if let Some(idx) = graph.idx_of(topic) {
                for other in graph.descendants(idx) {
                    back.insert(graph.id_of(other).to_owned(), miss * MISS_DESCENDANT_SHARE);
                }
                if is_leaf(graph, idx) {
                    let lateral = cfg.diag.sibling_credit * miss;
                    for sibling in sibling_leaves(graph, idx, state) {
                        back.insert(sibling, lateral);
                    }
                }
            }
            for (id, change) in back {
                if let Some(balance) = state.balances.get_mut(&id) {
                    *balance += change;
                }
            }
        }
        (Some(_), false) => {}
        (None, true) => {
            if let Some(balance) = state.balances.get_mut(topic) {
                *balance += weight;
            }
        }
        (None, false) => {
            if weight > 0.0
                && let Some(balance) = state.balances.get_mut(topic)
            {
                *balance = 0.0;
            }
        }
    }
    state.second.insert(topic.to_owned(), correct);
}

// --------------------------------------------------------------------------- //
// The next question
// --------------------------------------------------------------------------- //

/// The in-universe topics whose evidence is still inconclusive.
#[must_use]
pub fn undetermined_set(state: &DiagState) -> BTreeSet<String> {
    state
        .balances
        .iter()
        .filter(|(_, balance)| balance.abs() < DETERMINED_THRESHOLD)
        .map(|(id, _)| id.clone())
        .collect()
}

/// The next topic to ask, or `None` when the diagnostic is complete.
///
/// The candidates are the probe topics that are still undetermined and not yet
/// asked. The winner settles the most of the remaining graph: the count of its
/// undetermined ancestors plus its undetermined descendants. A tie takes the
/// lowest id. The answer is `None` once no candidate remains, or once
/// `diag.max_questions` answers stand.
#[must_use]
pub fn next_probe(state: &DiagState, graph: &Curriculum, cfg: &Config) -> Option<String> {
    if state.question_count() as i64 >= cfg.diag.max_questions {
        return None;
    }
    let undetermined = undetermined_set(state);
    let asked: BTreeSet<&str> = state.answered.iter().map(String::as_str).collect();
    let mut best: Option<(usize, &str)> = None;
    for probe in &state.probe_set {
        if !undetermined.contains(probe) || asked.contains(probe.as_str()) {
            continue;
        }
        let Some(idx) = graph.idx_of(probe) else {
            continue;
        };
        let score = graph
            .ancestors(idx)
            .into_iter()
            .chain(graph.descendants(idx))
            .filter(|&other| undetermined.contains(graph.id_of(other)))
            .count();
        // A STRICT maximum over the sorted probe set, so a tie takes the lowest id.
        let better = match best {
            None => true,
            Some((best_score, best_id)) => {
                score > best_score || (score == best_score && probe.as_str() < best_id)
            }
        };
        if better {
            best = Some((score, probe.as_str()));
        }
    }
    best.map(|(_, id)| id.to_owned())
}

/// The next question of the run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Next {
    /// The first ask of this topic.
    First(String),
    /// The second ask of this topic, with another exemplar.
    Second(String),
}

/// Whether a placed balance is thin: above zero and at or under
/// `diag.conditional_max`.
#[must_use]
pub fn is_conditional(balance: f64, cfg: &Config) -> bool {
    balance > 0.0 && balance <= cfg.diag.conditional_max
}

/// The next question: a first probe, or in the second phase a second probe.
///
/// When [`next_probe`] gives a topic, that topic is the answer. When it gives
/// none and fewer than `diag.max_questions` questions stand, the second phase
/// asks each of these topics once, in this order:
///
/// 1. Topics with a direct first miss, the lowest balance first.
/// 2. Topics with a thin positive balance (see [`is_conditional`]), the lowest
///    balance first.
///
/// A tie takes the lowest id. A topic that was never asked directly gets no
/// second probe. The closure `has_second` tells if a topic has a second exemplar;
/// the contract rule lives in the web crate. The first call that returns
/// [`Next::Second`] sets `supplemental`.
pub fn next_question(
    state: &mut DiagState,
    graph: &Curriculum,
    cfg: &Config,
    has_second: impl Fn(&str) -> bool,
) -> Option<Next> {
    if let Some(topic) = next_probe(state, graph, cfg) {
        return Some(Next::First(topic));
    }
    if state.question_count() as i64 >= cfg.diag.max_questions {
        return None;
    }
    let mut best: Option<(u8, f64, &str)> = None;
    for topic in &state.answered {
        if state.second.contains_key(topic) || !has_second(topic) {
            continue;
        }
        let Some(&balance) = state.balances.get(topic) else {
            continue;
        };
        let rank = if state.missed.contains_key(topic) {
            0
        } else if is_conditional(balance, cfg) {
            1
        } else {
            continue;
        };
        let better = best.is_none_or(|(best_rank, best_balance, best_id)| {
            (rank, balance, topic.as_str()) < (best_rank, best_balance, best_id)
        });
        if better {
            best = Some((rank, balance, topic.as_str()));
        }
    }
    let topic = best?.2.to_owned();
    state.supplemental = true;
    Some(Next::Second(topic))
}

// --------------------------------------------------------------------------- //
// Placement
// --------------------------------------------------------------------------- //

/// Turn the final tally into placement credits, conditional flags and the
/// supplemental candidates.
///
/// A positive balance places the topic and carries the balance itself. A placed
/// balance at or under `diag.conditional_max` is thin, so the topic is also
/// conditional. A zero balance on a topic that was never asked becomes a
/// supplemental candidate. A negative balance, and a zero on a topic that WAS
/// asked, place nothing: the learner does not know it.
#[must_use]
pub fn placement(state: &DiagState, cfg: &Config) -> PlacementResult {
    let asked: BTreeSet<&str> = state.answered.iter().map(String::as_str).collect();
    let sorted: BTreeMap<&str, f64> = state
        .balances
        .iter()
        .map(|(id, balance)| (id.as_str(), *balance))
        .collect();
    let mut placed: IndexMap<String, f64> = IndexMap::new();
    let mut conditional: Vec<String> = Vec::new();
    let mut supplemental: Vec<String> = Vec::new();
    for (id, balance) in sorted {
        if balance > 0.0 {
            placed.insert(id.to_owned(), balance);
            if is_conditional(balance, cfg) && state.second.get(id) != Some(&true) {
                conditional.push(id.to_owned());
            }
        } else if balance == 0.0 && !asked.contains(id) {
            supplemental.push(id.to_owned());
        }
    }
    conditional.sort();
    supplemental.sort();
    PlacementResult {
        placed,
        conditional,
        supplemental_candidates: supplemental,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fire::testing::{ladder, topic};

    /// A root with two leaves in one module, and a floored base under the root.
    fn fork() -> Curriculum {
        ladder(&[(
            "c",
            &["base"],
            vec![
                topic("base", &[]),
                topic("root", &[("base", 1.0, true)]),
                topic("left", &[("root", 1.0, true)]),
                topic("right", &[("root", 1.0, true)]),
            ],
        )])
    }

    #[test]
    fn a_session_asks_settles_and_places() {
        let tree = fork();
        let cfg = Config::default();
        let mut state = init_session(&tree, &cfg, Some("c"));
        assert_eq!(state.probe_set, ["left", "right"]);
        assert!(!state.in_universe("base"));
        assert_eq!(scope(&tree, None).len(), 4);
        assert_eq!(answer_weight(true, 30.0, 60.0), 0.5);
        assert_eq!(answer_weight(false, 30.0, 60.0), 1.0);
        assert_eq!(next_probe(&state, &tree, &cfg).as_deref(), Some("left"));
        apply_answer(&mut state, &tree, "left", true, 1.0, &cfg);
        apply_answer(&mut state, &tree, "ghost", true, 1.0, &cfg);
        assert_eq!(state.balances["root"], 1.0);
        assert_eq!(state.balances["right"], cfg.diag.sibling_credit);
        assert_eq!(undetermined_set(&state).len(), 1);
        assert_eq!(next_probe(&state, &tree, &cfg).as_deref(), Some("right"));
        apply_answer(&mut state, &tree, "right", false, 1.0, &cfg);
        assert_eq!(next_probe(&state, &tree, &cfg), None);
        let placed = placement(&state, &cfg);
        assert_eq!(placed.placed.len(), 2);
        assert!(placed.event_balances().contains_key("left"));
        assert!(!is_leaf(
            &tree,
            tree.idx_of("root").expect("root is a topic")
        ));
    }
}
