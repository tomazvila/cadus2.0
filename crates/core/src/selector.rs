//! The selector: the ordered session plan of PEDAGOGY 5 to 8 (spec section 6).
//!
//! The port keeps every ordering rule of 1.0 `cadus/selector.py`, because the plan
//! is a list and a list compares element by element. The rules that decide the
//! order are:
//!
//! - Every 1.0 `sorted()` becomes a sort by topic id in byte order (trap T18).
//! - Every 1.0 loop over a SET becomes a loop over the sorted ids (trap T5).
//! - `max()` and the greedy `>` of [`compress`] keep the FIRST maximum, so a tie
//!   breaks to the lowest id.
//! - The float sum of [`importance`] runs over SORTED review targets and goes
//!   through [`neumaier_sum`](crate::numeric::neumaier_sum) (trap T1).
//!
//! # What is NOT reproduced
//!
//! 1.0 samples the quiz with `random.Random.sample`, the Mersenne-Twister
//! algorithm of CPython (trap T11). 2.0 does NOT reproduce that sequence. Quiz
//! sampling goes through the [`QuizSampler`] trait, and [`SeededSampler`] is 2.0's
//! own seeded generator. The seed contract is therefore CHANGED, on purpose: the
//! sampled ids are recorded on the `task_served` event, so a replay reads the ids
//! from the log and never re-samples them.
//!
//! 1.0 also carries `_backfill_legacy_flags`, a transitional read of the display
//! prose that recovers `is_remediation` and `nearly_due` from a plan serialized
//! before those fields existed. 2.0 starts learners fresh (decision O1), so no
//! such plan exists and the port drops that path. The typed fields are the only
//! source of both facts here.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::{Days, NaiveDate};

use crate::config::Config;
use crate::curriculum::{Curriculum, TopicIdx};
use crate::event::{EventError, KpProgress, Slug, TaskType, Timestamp};
use crate::fire::{ReviewState, has_review_history, memory_at, review_state};
use crate::learner::{PendingRemediation, QuizState, TopicState};
use crate::numeric::{neumaier_sum, round_dp, round_half_even_i64_saturating, to_datetime};
pub use crate::xp::is_mastered;

/// One day, in microseconds.
const DAY_US: i64 = 86_400_000_000;

/// The importance bonus of a core topic (`selector.py:78`).
pub const CORE_BONUS: f64 = 0.5;

/// The review difficulty target, the 80-85% sweet spot (`selector.py:81`).
pub const DIFFICULTY_TARGET: &str = "80-85% expected accuracy";

/// The window that makes a learned topic "recent" for a quiz (`selector.py:84`).
pub const QUIZ_RECENT_DAYS: i64 = 14;

/// The per-question quiz time multiplier (`selector.py:87`).
pub const QUIZ_TIME_FACTOR: f64 = 1.5;

/// The delay before a failed quiz becomes re-eligible (`selector.py:96`).
pub const QUIZ_RETAKE_DELAY_DAYS: i64 = 1;

/// The quiz difficulty bands, easiest first (`selector.py:105-109`).
pub const QUIZ_DIFFICULTY_TARGETS: [&str; 3] = [
    DIFFICULTY_TARGET,
    "75-80% expected accuracy",
    "70-75% expected accuracy",
];

/// The quiz questions drawn from the recent stratum (`selector.py:744`).
pub const QUIZ_RECENT_TARGET: usize = 4;

/// The quiz questions drawn from the mid stratum (`selector.py:745`).
pub const QUIZ_MID_TARGET: usize = 2;

/// The automaticity bar a drill target stays below (`selector.py:113`).
pub const DRILL_MASTERY_ABILITY: f64 = 0.95;

/// The drill cadence, in days (`selector.py:116`).
pub const DRILL_INTERVAL_DAYS: f64 = 3.5;

/// The remediation kind of a quiz miss (`selector.py:120`).
pub const REMEDIATION_QUIZ_MISS: &str = "quiz_miss";

/// The remediation kind of a second failure at one knowledge point.
pub const REMEDIATION_REPEAT_FAIL: &str = "repeat_fail";

/// The remediation kind of a lesson failure.
pub const REMEDIATION_LESSON_FAIL: &str = "lesson_fail";

/// The fewest components a multi-step task needs (`selector.py:139`).
pub const MULTISTEP_MIN_COMPONENTS: usize = 3;

/// The most components a multi-step task carries (`selector.py:140`).
pub const MULTISTEP_MAX_COMPONENTS: usize = 4;

/// One multi-step task is owed per this many mastered reviewable topics.
pub const MULTISTEP_CADENCE: i64 = 4;

/// The multi-step kill switch (`selector.py:145`). 1.0 ships it on.
pub const MULTISTEP_ENABLED: bool = true;

/// The empty topic-id set a [`SessionContext`] defaults to.
static NO_IDS: BTreeSet<String> = BTreeSet::new();

/// The empty remediation queue a [`SessionContext`] defaults to.
static NO_REMEDIATION: [PendingRemediation; 0] = [];

// --------------------------------------------------------------------------- //
// A set of topics, addressed by arena index (D1)
// --------------------------------------------------------------------------- //

/// A set of curriculum topics, held as one bit per arena index (D1).
///
/// 1.0 passes `set[str]` between the selector helpers. A set of owned strings
/// costs one allocation per member, and the L1 budget composes a session over
/// 1,090 topics, so the port carries the membership as a dense bit vector and
/// converts to ids only where an id leaves the module.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TopicSet {
    /// One flag per arena topic index.
    members: Vec<bool>,
    /// The number of set flags.
    count: usize,
}

impl TopicSet {
    /// An empty set sized for `graph`.
    #[must_use]
    pub fn empty(graph: &Curriculum) -> Self {
        Self {
            members: vec![false; graph.topic_count()],
            count: 0,
        }
    }

    /// Add one topic. Adding a member twice keeps the count right.
    pub fn insert(&mut self, idx: TopicIdx) {
        if let Some(slot) = self.members.get_mut(idx.index())
            && !*slot
        {
            *slot = true;
            self.count += 1;
        }
    }

    /// Add one topic by id. An id with no topic is ignored.
    pub fn insert_id(&mut self, graph: &Curriculum, id: &str) {
        if let Some(idx) = graph.idx_of(id) {
            self.insert(idx);
        }
    }

    /// Remove one topic.
    pub fn remove(&mut self, idx: TopicIdx) {
        if let Some(slot) = self.members.get_mut(idx.index())
            && *slot
        {
            *slot = false;
            self.count -= 1;
        }
    }

    /// Whether the set holds a topic.
    #[must_use]
    pub fn contains(&self, idx: TopicIdx) -> bool {
        self.members.get(idx.index()).copied().unwrap_or(false)
    }

    /// Whether the set holds the topic of an id. An id with no topic is absent.
    #[must_use]
    pub fn contains_id(&self, graph: &Curriculum, id: &str) -> bool {
        graph.idx_of(id).is_some_and(|idx| self.contains(idx))
    }

    /// The number of topics in the set.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.count
    }

    /// Whether the set is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Every member, ascending by arena index.
    pub fn indices(&self) -> impl Iterator<Item = TopicIdx> + '_ {
        self.members
            .iter()
            .enumerate()
            .filter(|&(_, &member)| member)
            .filter_map(|(index, _)| u32::try_from(index).ok().map(TopicIdx::from_u32))
    }

    /// Every member id, SORTED by id in byte order (trap T18).
    #[must_use]
    pub fn sorted_ids<'g>(&self, graph: &'g Curriculum) -> Vec<&'g str> {
        let mut out: Vec<&str> = self.indices().map(|idx| graph.id_of(idx)).collect();
        out.sort_unstable();
        out
    }

    /// Every member id as an owned, sorted set.
    #[must_use]
    pub fn to_id_set(&self, graph: &Curriculum) -> BTreeSet<String> {
        self.sorted_ids(graph)
            .into_iter()
            .map(ToOwned::to_owned)
            .collect()
    }

    /// This set restricted to the members of `other` (`self & other`).
    #[must_use]
    pub fn intersect(&self, other: &Self) -> Self {
        let mut out = Self {
            members: vec![false; self.members.len()],
            count: 0,
        };
        for (index, (&left, &right)) in self.members.iter().zip(other.members.iter()).enumerate() {
            if left
                && right
                && let Some(slot) = out.members.get_mut(index)
            {
                *slot = true;
                out.count += 1;
            }
        }
        out
    }

    /// Whether every member of this set is a member of `other` (`self <= other`).
    #[must_use]
    pub fn is_subset(&self, other: &Self) -> bool {
        self.members
            .iter()
            .zip(other.members.iter())
            .all(|(&left, &right)| !left || right)
    }
}

/// The topics of a course, or every topic when the scope is unset
/// (`_course_scope`, `selector.py:163-165`).
#[must_use]
pub fn course_scope(graph: &Curriculum, course_id: Option<&str>) -> TopicSet {
    let mut out = TopicSet::empty(graph);
    match course_id {
        Some(course) => {
            for &idx in graph.topics_in_course(course) {
                out.insert(idx);
            }
        }
        None => {
            for idx in every_index(graph) {
                out.insert(idx);
            }
        }
    }
    out
}

/// Every arena topic index, ascending.
fn every_index(graph: &Curriculum) -> impl Iterator<Item = TopicIdx> + '_ {
    (0..graph.topic_count()).filter_map(|index| u32::try_from(index).ok().map(TopicIdx::from_u32))
}

/// The topics currently mastered for frontier purposes
/// (`mastered_set`, `selector.py:157-160`).
///
/// A topic absent from `states` is untouched, so it is not mastered.
#[must_use]
pub fn mastered_set(states: &BTreeMap<String, TopicState>, graph: &Curriculum) -> TopicSet {
    let mut out = TopicSet::empty(graph);
    for (id, state) in states {
        if is_mastered(state) {
            out.insert_id(graph, id);
        }
    }
    out
}

/// The learnable topics: not mastered, every prerequisite mastered
/// (`Graph.frontier`, `graph.py:375-389`).
#[must_use]
pub fn frontier(graph: &Curriculum, mastered: &TopicSet) -> TopicSet {
    let mut out = TopicSet::empty(graph);
    for idx in every_index(graph) {
        if mastered.contains(idx) {
            continue;
        }
        if graph
            .prerequisites(idx)
            .all(|prereq| mastered.contains(prereq))
        {
            out.insert(idx);
        }
    }
    out
}

// --------------------------------------------------------------------------- //
// Encompassing-weight cache
// --------------------------------------------------------------------------- //

/// A memo of `W(src -> *)` per source topic.
///
/// 1.0 caches the relaxation on the graph (`graph.py:298-299`). The 2.0 arena
/// recomputes it per call, and [`compress`] asks for the same source many times,
/// so the selector holds the memo for the length of one composition. The floats
/// are the arena floats, so the memo changes no value.
struct ReachCache<'g> {
    /// The curriculum the weights come from.
    graph: &'g Curriculum,
    /// `src -> [(target, weight)]`, each list sorted by target id.
    entries: HashMap<&'g str, Vec<(&'g str, f64)>>,
}

impl<'g> ReachCache<'g> {
    /// An empty memo over `graph`.
    fn new(graph: &'g Curriculum) -> Self {
        Self {
            graph,
            entries: HashMap::new(),
        }
    }

    /// `W(src -> *)`, sorted by target id. An id with no topic gives an empty list.
    fn weights(&mut self, src: &str) -> &[(&'g str, f64)] {
        let graph = self.graph;
        let key: &'g str = match graph.idx_of(src) {
            Some(idx) => graph.id_of(idx),
            None => return &[],
        };
        self.entries
            .entry(key)
            .or_insert_with(|| graph.reach_weights_by_id(key))
    }

    /// `W(a -> b)`, the way `Curriculum::encompassing_weight_by_id` computes it.
    fn weight(&mut self, a: &str, b: &str) -> f64 {
        if a == b {
            return 1.0;
        }
        let weights = self.weights(a);
        weights
            .binary_search_by(|probe| probe.0.cmp(b))
            .ok()
            .and_then(|position| weights.get(position))
            .map_or(0.0, |&(_, weight)| weight)
    }
}

// --------------------------------------------------------------------------- //
// Retry delay, due reviews, nearly-due reviews
// --------------------------------------------------------------------------- //

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
fn float_then_id(left: &(f64, String), right: &(f64, String)) -> Ordering {
    left.0
        .partial_cmp(&right.0)
        .unwrap_or(Ordering::Equal)
        .then_with(|| left.1.cmp(&right.1))
}

// --------------------------------------------------------------------------- //
// Compression — greedy weighted knockout set cover
// --------------------------------------------------------------------------- //

/// The result of [`compress`] (`Compression`, `selector.py:478-497`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Compression {
    /// The due topics that must be served as explicit reviews, sorted.
    pub surviving: Vec<String>,
    /// Per covering task, the sorted due topics it knocks out. It never holds
    /// the covering task itself.
    pub knockouts: BTreeMap<String, Vec<String>>,
}

/// A subset of the due list, one bit per due topic.
///
/// 1.0 rebuilds `_covers(c, uncovered)` as a `set[str]` on every greedy round
/// (`selector.py:500-504`), which is quadratic in the due count and allocates a
/// string set per candidate. The port computes each candidate's cover ONCE over
/// the whole due list and intersects the bits per round. The greedy choice is
/// the same choice: `_covers(c, pool)` is exactly `cover(c) & pool`, and the bit
/// order is the sorted due order, so the FIRST strict maximum is the same
/// candidate and every recorded knockout list is already sorted.
type DueMask = Vec<u64>;

/// An all-zero mask over `n` due topics.
fn empty_mask(n: usize) -> DueMask {
    vec![0_u64; n.div_ceil(64)]
}

/// Set the bit of one due topic.
fn set_bit(mask: &mut DueMask, index: usize) {
    if let Some(word) = mask.get_mut(index / 64) {
        *word |= 1_u64 << (index % 64);
    }
}

/// Whether the bit of one due topic is set.
fn get_bit(mask: &DueMask, index: usize) -> bool {
    mask.get(index / 64)
        .is_some_and(|word| word & (1_u64 << (index % 64)) != 0)
}

/// The number of due topics a mask holds.
fn count_mask(mask: &DueMask) -> usize {
    mask.iter().map(|word| word.count_ones() as usize).sum()
}

/// The number of due topics in `left & right`.
fn and_count(left: &DueMask, right: &DueMask) -> usize {
    left.iter()
        .zip(right.iter())
        .map(|(a, b)| (a & b).count_ones() as usize)
        .sum()
}

/// `left & right`.
fn and_mask(left: &DueMask, right: &DueMask) -> DueMask {
    left.iter().zip(right.iter()).map(|(a, b)| a & b).collect()
}

/// Clear every bit of `mask` that `cover` holds.
fn clear_mask(mask: &mut DueMask, cover: &DueMask) {
    for (slot, bits) in mask.iter_mut().zip(cover.iter()) {
        *slot &= !bits;
    }
}

/// The ids of the due topics a mask holds, in the sorted due order.
fn ids_of_mask(mask: &DueMask, due_list: &[String], skip: Option<usize>) -> Vec<String> {
    due_list
        .iter()
        .enumerate()
        .filter(|&(index, _)| Some(index) != skip && get_bit(mask, index))
        .map(|(_, id)| id.clone())
        .collect()
}

/// The due topics `candidate` knocks out, excluding itself
/// (`_covers`, `selector.py:500-504`), as a mask over the sorted due list.
fn cover_mask(
    candidate: &str,
    due_list: &[String],
    due_index: &HashMap<&str, usize>,
    cfg: &Config,
    cache: &mut ReachCache<'_>,
) -> DueMask {
    let mut mask = empty_mask(due_list.len());
    if cfg.fire.knockout_weight <= 0.0 {
        // Every weight, present or absent, clears a non-positive threshold.
        for (index, id) in due_list.iter().enumerate() {
            if id.as_str() != candidate {
                set_bit(&mut mask, index);
            }
        }
        return mask;
    }
    for &(target, weight) in cache.weights(candidate) {
        if target != candidate
            && weight >= cfg.fire.knockout_weight
            && let Some(&index) = due_index.get(target)
        {
            set_bit(&mut mask, index);
        }
    }
    mask
}

/// The greedy weighted set cover of the due reviews (`compress`, `selector.py:506-590`).
///
/// Phase 1 spends the free candidates — frontier lessons and nearly-due topics —
/// then phase 2 serves the rest explicitly, each surviving review dominoing the
/// others it covers. Both phases keep the FIRST strict maximum over the sorted
/// candidates, so a tie breaks to the lowest id.
///
/// `frontier_candidates` is the exact set of frontier lessons the caller serves.
/// `None` falls back to the global retry-filtered frontier, for a caller that
/// compresses alone.
#[must_use]
pub fn compress(
    due: &[String],
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    cfg: &Config,
    t_us: i64,
    frontier_candidates: Option<&BTreeSet<String>>,
) -> Compression {
    let mut cache = ReachCache::new(graph);
    compress_with(
        due,
        states,
        graph,
        cfg,
        t_us,
        frontier_candidates,
        &mut cache,
    )
}

/// [`compress`] over a shared weight memo.
fn compress_with(
    due: &[String],
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    cfg: &Config,
    t_us: i64,
    frontier_candidates: Option<&BTreeSet<String>>,
    cache: &mut ReachCache<'_>,
) -> Compression {
    let due_set: BTreeSet<String> = due
        .iter()
        .filter(|id| graph.idx_of(id).is_some())
        .cloned()
        .collect();
    if due_set.is_empty() {
        return Compression::default();
    }

    let default = TopicState::default();
    let mut free_candidates: BTreeSet<String> = match frontier_candidates {
        Some(given) => given.clone(),
        None => {
            let mastered = mastered_set(states, graph);
            frontier(graph, &mastered)
                .sorted_ids(graph)
                .into_iter()
                .filter(|id| !in_retry_delay(states.get(*id).unwrap_or(&default), cfg, t_us))
                .map(ToOwned::to_owned)
                .collect()
        }
    };
    free_candidates.extend(nearly_due(states, graph, cfg, t_us));
    let free_candidates: Vec<String> = free_candidates
        .into_iter()
        .filter(|id| !due_set.contains(id))
        .collect();

    // The sorted due list is the bit order of every mask below.
    let due_list: Vec<String> = due_set.into_iter().collect();
    let due_index: HashMap<&str, usize> = due_list
        .iter()
        .enumerate()
        .map(|(index, id)| (id.as_str(), index))
        .collect();

    let mut knockouts: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut uncovered = empty_mask(due_list.len());
    for index in 0..due_list.len() {
        set_bit(&mut uncovered, index);
    }

    // Phase 1: the free candidates, served for progression anyway.
    let free_covers: Vec<DueMask> = free_candidates
        .iter()
        .map(|candidate| cover_mask(candidate, &due_list, &due_index, cfg, cache))
        .collect();
    while count_mask(&uncovered) > 0 {
        let mut best: Option<usize> = None;
        let mut best_count = 0_usize;
        for (position, cover) in free_covers.iter().enumerate() {
            let count = and_count(cover, &uncovered);
            if count > best_count {
                best = Some(position);
                best_count = count;
            }
        }
        let (Some(position), Some(cover)) = (best, best.and_then(|at| free_covers.get(at))) else {
            break;
        };
        let Some(winner) = free_candidates.get(position) else {
            break;
        };
        let taken = and_mask(cover, &uncovered);
        knockouts.insert(winner.clone(), ids_of_mask(&taken, &due_list, None));
        clear_mask(&mut uncovered, &taken);
    }

    // Phase 2: the remaining due topics, served explicitly and dominoing.
    let due_covers: Vec<DueMask> = due_list
        .iter()
        .map(|candidate| cover_mask(candidate, &due_list, &due_index, cfg, cache))
        .collect();
    let mut surviving: Vec<String> = Vec::new();
    let mut remaining = uncovered;
    while count_mask(&remaining) > 0 {
        let mut best: Option<usize> = None;
        let mut best_count = 0_usize;
        for (index, cover) in due_covers.iter().enumerate() {
            if !get_bit(&remaining, index) {
                continue;
            }
            // A due topic always covers itself, and `cover` never holds it, so
            // the size of `{c} | covers(c, remaining)` is one more than the
            // intersection count. Only the winner's mask is materialized.
            let count = and_count(cover, &remaining) + 1;
            if count > best_count {
                best_count = count;
                best = Some(index);
            }
        }
        // Every topic covers itself, so the count is at least one and `best` is set.
        let (Some(index), Some(cover)) = (best, best.and_then(|at| due_covers.get(at))) else {
            break;
        };
        let mut taken = and_mask(cover, &remaining);
        set_bit(&mut taken, index);
        let Some(winner) = due_list.get(index) else {
            break;
        };
        let others = ids_of_mask(&taken, &due_list, Some(index));
        if !others.is_empty() {
            knockouts.insert(winner.clone(), others);
        }
        surviving.push(winner.clone());
        clear_mask(&mut remaining, &taken);
    }

    surviving.sort_unstable();
    Compression {
        surviving,
        knockouts,
    }
}

// --------------------------------------------------------------------------- //
// Lesson importance ordering
// --------------------------------------------------------------------------- //

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
const fn count_as_float(count: usize) -> f64 {
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
fn order_lessons_with(
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

// --------------------------------------------------------------------------- //
// Quiz composition
// --------------------------------------------------------------------------- //

/// The sampler the quiz composer draws with.
///
/// 1.0 draws with `random.Random.sample`, the CPython Mersenne-Twister algorithm
/// (trap T11). 2.0 does NOT reproduce that sequence, so the draw lives behind
/// this trait: a caller supplies [`SeededSampler`], or a fixed sampler in a test,
/// or a replay sampler that re-serves the ids a `task_served` event recorded.
pub trait QuizSampler {
    /// Draw `k` distinct members of `population`, in draw order.
    ///
    /// The implementation returns `k` members when `k <= population.len()`, and
    /// never more than `population.len()`.
    fn sample(&mut self, population: &[String], k: usize) -> Vec<String>;
}

/// The default seeded sampler of 2.0.
///
/// It is a SplitMix64 generator behind a partial Fisher-Yates shuffle. The draw
/// is a pure function of the seed and the population, so a replay with the same
/// seed draws the same ids. It is NOT the CPython `random.sample` sequence
/// (trap T11), and it does not try to be: the sampled ids go on the
/// `task_served` event, and a replay reads them from the log.
#[derive(Debug, Clone)]
pub struct SeededSampler {
    /// The generator state.
    state: u64,
}

/// The odd increment of SplitMix64.
const SPLITMIX_GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;

impl SeededSampler {
    /// A sampler seeded with `seed`.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// The next 64 raw bits (SplitMix64).
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(SPLITMIX_GAMMA);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number below `bound`. A `bound` of zero gives zero.
    fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            return 0;
        }
        let scaled = (u128::from(self.next_u64()) * (bound as u128)) >> 64;
        usize::try_from(scaled).unwrap_or(0).min(bound - 1)
    }
}

impl QuizSampler for SeededSampler {
    fn sample(&mut self, population: &[String], k: usize) -> Vec<String> {
        let mut pool: Vec<String> = population.to_vec();
        let take = k.min(pool.len());
        let mut out: Vec<String> = Vec::with_capacity(take);
        for index in 0..take {
            let pick = index + self.below(pool.len() - index);
            pool.swap(index, pick);
            if let Some(chosen) = pool.get(index) {
                out.push(chosen.clone());
            }
        }
        out
    }
}

/// One stratified quiz question (`QuizQuestion`, `selector.py:653-658`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuizQuestion {
    /// The topic the question asks about.
    pub topic: String,
    /// The timed budget: the authored expected time times 1.5, rounded.
    pub time_budget_secs: i64,
    /// The stratum the topic came from: `recent`, `mid`, or `old`.
    pub stratum: &'static str,
}

/// A composed quiz (`QuizPlan`, `selector.py:661-673`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuizPlan {
    /// The questions, in draw order.
    pub questions: Vec<QuizQuestion>,
}

impl QuizPlan {
    /// The sampled topic ids, in draw order.
    #[must_use]
    pub fn topics(&self) -> Vec<&str> {
        self.questions
            .iter()
            .map(|question| question.topic.as_str())
            .collect()
    }

    /// The total timed budget of the quiz.
    #[must_use]
    pub fn total_time_budget_secs(&self) -> i64 {
        self.questions.iter().fold(0_i64, |total, question| {
            total.saturating_add(question.time_budget_secs)
        })
    }
}

/// An authored integer as a float. An authored count never leaves the exact
/// integer range of an `f64`.
#[expect(
    clippy::cast_precision_loss,
    reason = "an authored count never leaves the exact f64 integer range"
)]
const fn i64_as_float(count: i64) -> f64 {
    count as f64
}

/// The per-question time budget: the authored expected time times 1.5, rounded
/// half to even (`_quiz_budget`, `selector.py:676-678`).
///
/// The input is an authored count, which the `i64` range holds, so the budget uses
/// the saturating rounding form and reports no error.
#[must_use]
pub fn quiz_budget(graph: &Curriculum, tid: &str) -> i64 {
    let seconds = graph
        .idx_of(tid)
        .and_then(|idx| graph.topic(idx))
        .map_or(0, |topic| topic.expected_time_secs);
    round_half_even_i64_saturating(i64_as_float(seconds) * QUIZ_TIME_FACTOR)
}

/// A microsecond delta as seconds, the way `timedelta.total_seconds` reads it.
#[expect(
    clippy::cast_precision_loss,
    reason = "the age key only orders topics, and 1.0 reads the same float"
)]
fn micros_as_seconds(delta_us: i64) -> f64 {
    delta_us as f64 / 1_000_000.0
}

/// Compose the stratified quiz of PEDAGOGY 7 (`quiz_composer`, `selector.py:681-754`).
///
/// The strata are 4 recent, 2 mid, and 2 old, scaled down when the learner knows
/// fewer topics than the quiz needs. A short stratum backfills from the rest, so
/// the quiz reaches its full length whenever enough topics are learned.
///
/// `learned_at` maps a topic id to the UTC microseconds it was first mastered.
/// Without it, recency falls back to the `repNum` proxy of 1.0 and no topic
/// counts as recent.
#[must_use]
pub fn quiz_composer(
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    cfg: &Config,
    t_us: i64,
    sampler: &mut dyn QuizSampler,
    learned_at: Option<&BTreeMap<String, i64>>,
) -> QuizPlan {
    let learned: Vec<String> = states
        .iter()
        .filter(|(id, state)| graph.idx_of(id).is_some() && has_review_history(state))
        .map(|(id, _)| id.clone())
        .collect();
    if learned.is_empty() {
        return QuizPlan::default();
    }

    let age_key = |tid: &str| -> (f64, String) {
        match learned_at.and_then(|map| map.get(tid).copied()) {
            // 1.0 keys on `-(t - learned_at).total_seconds()`, so an older topic
            // sorts first.
            Some(at) => (-micros_as_seconds(t_us.saturating_sub(at)), tid.to_owned()),
            None => (
                -states.get(tid).map_or(0.0, |state| state.rep_num),
                tid.to_owned(),
            ),
        }
    };
    let is_recent = |tid: &str| -> bool {
        learned_at
            .and_then(|map| map.get(tid).copied())
            .is_some_and(|at| t_us.saturating_sub(at) <= QUIZ_RECENT_DAYS * DAY_US)
    };

    let mut recent: Vec<String> = learned.iter().filter(|id| is_recent(id)).cloned().collect();
    let mut older: Vec<String> = learned
        .iter()
        .filter(|id| !is_recent(id))
        .cloned()
        .collect();
    recent.sort_by(|left, right| float_then_id(&age_key(left), &age_key(right)));
    older.sort_by(|left, right| float_then_id(&age_key(left), &age_key(right)));

    let half = older.len() / 2;
    let old_pool: Vec<String> = older.get(..half).unwrap_or(&[]).to_vec();
    let mid_pool: Vec<String> = older.get(half..).unwrap_or(&[]).to_vec();

    let questions = usize::try_from(cfg.quiz.questions).unwrap_or(0);
    let total = questions.min(learned.len());
    let want_recent = QUIZ_RECENT_TARGET.min(total);
    let want_mid = QUIZ_MID_TARGET.min(total - want_recent);
    let want_old = total - want_recent - want_mid;

    let mut picked: Vec<(String, &'static str)> = Vec::new();
    let mut used: BTreeSet<String> = BTreeSet::new();
    take_stratum(
        sampler,
        &recent,
        want_recent,
        "recent",
        &mut picked,
        &mut used,
    );
    take_stratum(sampler, &mid_pool, want_mid, "mid", &mut picked, &mut used);
    take_stratum(sampler, &old_pool, want_old, "old", &mut picked, &mut used);

    if picked.len() < total {
        let mut leftover: Vec<String> = learned
            .iter()
            .filter(|id| !used.contains(*id))
            .cloned()
            .collect();
        leftover.sort_unstable();
        let need = total - picked.len();
        take_stratum(sampler, &leftover, need, "mid", &mut picked, &mut used);
    }

    QuizPlan {
        questions: picked
            .into_iter()
            .map(|(tid, stratum)| QuizQuestion {
                time_budget_secs: quiz_budget(graph, &tid),
                topic: tid,
                stratum,
            })
            .collect(),
    }
}

/// Draw one stratum of the quiz (`take`, `selector.py:736-741`).
fn take_stratum(
    sampler: &mut dyn QuizSampler,
    pool: &[String],
    k: usize,
    stratum: &'static str,
    picked: &mut Vec<(String, &'static str)>,
    used: &mut BTreeSet<String>,
) {
    let available: Vec<String> = pool
        .iter()
        .filter(|id| !used.contains(*id))
        .cloned()
        .collect();
    let k = k.min(available.len());
    if k == 0 {
        return;
    }
    for tid in sampler.sample(&available, k) {
        used.insert(tid.clone());
        picked.push((tid, stratum));
    }
}

/// The quiz difficulty target for a high-score streak
/// (`quiz_difficulty_target`, `selector.py:757-770`).
///
/// The streak is clamped into the band range, so any value is safe.
#[must_use]
pub fn quiz_difficulty_target(high_score_streak: i64) -> &'static str {
    let last = QUIZ_DIFFICULTY_TARGETS.len().saturating_sub(1);
    let index = usize::try_from(high_score_streak.max(0))
        .unwrap_or(last)
        .min(last);
    QUIZ_DIFFICULTY_TARGETS
        .get(index)
        .copied()
        .unwrap_or(DIFFICULTY_TARGET)
}

/// The date a failed quiz becomes re-eligible
/// (`quiz_retake_available_at`, `selector.py:773-786`).
///
/// `None` when no retake is pending, or when no last-quiz date is recorded.
#[must_use]
pub fn quiz_retake_available_at(quiz_state: Option<&QuizState>) -> Option<NaiveDate> {
    let state = quiz_state?;
    if !state.retake_pending {
        return None;
    }
    let days = u64::try_from(QUIZ_RETAKE_DELAY_DAYS).ok()?;
    state.last_at?.checked_add_days(Days::new(days))
}

/// The UTC calendar date of an instant, the way 1.0 reads `t.date()` off a naive
/// UTC datetime (trap T8). `None` when the instant leaves the representable range.
#[must_use]
pub fn utc_date(t_us: i64) -> Option<NaiveDate> {
    to_datetime(t_us).ok().map(|when| when.date_naive())
}

/// Whether a quiz is due (`quiz_is_due`, `selector.py:789-838`).
///
/// A pending retake supersedes the cadence: it becomes due only after the retake
/// delay. `active_study_days` makes the day cadence advance on the days the
/// learner actually studied; without it the cadence counts calendar days.
#[must_use]
pub fn quiz_is_due(
    quiz_state: Option<&QuizState>,
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    cfg: &Config,
    t_us: i64,
    active_study_days: Option<&[NaiveDate]>,
) -> bool {
    let learned = states
        .iter()
        .filter(|(id, state)| graph.idx_of(id).is_some() && has_review_history(state))
        .count();
    if i64::try_from(learned).unwrap_or(i64::MAX) < cfg.quiz.questions {
        return false;
    }
    let Some(state) = quiz_state else {
        return true;
    };
    let Some(today) = utc_date(t_us) else {
        return false;
    };
    if state.retake_pending {
        return quiz_retake_available_at(Some(state)).is_none_or(|at| today >= at);
    }
    let Some(last_at) = state.last_at else {
        return true;
    };
    if state.xp_since >= cfg.quiz.cadence_xp {
        return true;
    }
    if let Some(days) = active_study_days {
        let active: BTreeSet<NaiveDate> = days
            .iter()
            .copied()
            .filter(|day| last_at < *day && *day <= today)
            .collect();
        return i64::try_from(active.len()).unwrap_or(i64::MAX) >= cfg.quiz.cadence_days;
    }
    (today - last_at).num_days() >= cfg.quiz.cadence_days
}

// --------------------------------------------------------------------------- //
// Drills
// --------------------------------------------------------------------------- //

/// The drill-tagged topics due for a timed drill, SORTED
/// (`schedule_drills`, `selector.py:845-869`).
///
/// A topic qualifies when it is drill-tagged, mastered, still below the
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
    for idx in every_index(graph) {
        let Some(topic) = graph.topic(idx) else {
            continue;
        };
        if !topic.drill {
            continue;
        }
        let id = topic.id.as_str();
        let Some(state) = states.get(id) else {
            continue;
        };
        if !is_mastered(state) || state.ability >= DRILL_MASTERY_ABILITY {
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

// --------------------------------------------------------------------------- //
// Remediation queue helpers
// --------------------------------------------------------------------------- //

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

// --------------------------------------------------------------------------- //
// Tasks and the session plan
// --------------------------------------------------------------------------- //

/// One served task (`Task`, `model.py:628-672`).
///
/// `topic` is the topic id of a single-topic task. It is `None` for a quiz and
/// for a multi-step task, which span several topics; those list their topics in
/// `mix` and `component_topics`.
///
/// `why` is display prose for the client. NOTHING reads a scheduling decision
/// back out of it: [`Task::is_remediation`] and [`Task::nearly_due`] are the
/// typed facts the re-serve keys on.
#[derive(Debug, Clone, PartialEq)]
pub struct Task {
    /// The content-stable id, assigned by [`assign_ids`].
    pub task_id: String,
    /// The kind of task.
    pub task_type: TaskType,
    /// The topic id, for a single-topic task.
    pub topic: Option<String>,
    /// The number of problems the task serves.
    pub n_problems: Option<i64>,
    /// The review question mix, or the sampled quiz topics.
    pub mix: Vec<String>,
    /// The difficulty target of a review, a quiz, or a multi-step task.
    pub difficulty_target: Option<String>,
    /// The recently served problem digests to avoid (Hard Rule 4).
    pub recent_problem_hashes: Vec<String>,
    /// The knowledge point a lesson resumes at.
    pub start_at_kp: Option<String>,
    /// The timed budget of a quiz or a drill.
    pub time_budget_secs: Option<i64>,
    /// The display prose. It is never parsed.
    pub why: String,
    /// Whether a lesson fills an out-of-course prerequisite gap.
    pub gap_fill: bool,
    /// The course a gap-fill lesson returns to.
    pub gap_return_to: Option<String>,
    /// The ordered component topics of a multi-step task.
    pub component_topics: Vec<String>,
    /// Whether the task serves the remediation queue.
    pub is_remediation: bool,
    /// Whether a review is served before its due date.
    pub nearly_due: bool,
}

impl Default for Task {
    /// The 1.0 field defaults. `task_type` has no 1.0 default, so the port picks
    /// the most common one; every builder here sets it explicitly.
    fn default() -> Self {
        Self {
            task_id: String::new(),
            task_type: TaskType::Lesson,
            topic: None,
            n_problems: None,
            mix: Vec::new(),
            difficulty_target: None,
            recent_problem_hashes: Vec::new(),
            start_at_kp: None,
            time_budget_secs: None,
            why: String::new(),
            gap_fill: false,
            gap_return_to: None,
            component_topics: Vec::new(),
            is_remediation: false,
            nearly_due: false,
        }
    }
}

/// The session constraints reported with a plan (`_constraints`, `selector.py:1499-1519`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Constraints {
    /// Whether the lesson share reaches `selector.lesson_ratio_min`.
    pub lesson_ratio_ok: bool,
    /// The lesson share of the interleaved sequence, rounded to 4 places.
    pub lesson_ratio: f64,
    /// Whether no review run exceeds `selector.max_reviews_per_lesson`.
    pub throttle_ok: bool,
    /// The number of reviews in the sequence.
    pub reviews: i64,
    /// The number of lessons in the sequence.
    pub lessons: i64,
}

/// The composed session (`SessionPlan`, `model.py:675-683`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionPlan {
    /// The session id the task ids are keyed on.
    pub session: String,
    /// The tasks, in serve order.
    pub tasks: Vec<Task>,
    /// Whether a quiz is due.
    pub quiz_due: bool,
    /// The throttle and ratio report.
    pub constraints: Constraints,
    /// Whether every topic of the course scope is mastered.
    pub course_complete: bool,
    /// When the first retry-delayed frontier lesson reopens.
    pub frontier_blocked_until: Option<Timestamp>,
}

/// The knowledge point a lesson resumes at (`_start_kp`, `selector.py:910-916`).
///
/// It is the first knowledge point not yet passed, or the first one when every
/// point has passed. A topic that authors no knowledge point gives `None`.
#[must_use]
pub fn start_kp(graph: &Curriculum, tid: &str, state: &TopicState) -> Option<String> {
    let idx = graph.idx_of(tid)?;
    let kps = graph.knowledge_points(idx);
    for kp in kps {
        if state.kp_progress.get(kp.id.as_str()) != Some(&KpProgress::Passed) {
            return Some(kp.id.as_str().to_owned());
        }
    }
    kps.first().map(|kp| kp.id.as_str().to_owned())
}

/// The knocked-out count of a covering task.
fn knockout_count(knockouts: &BTreeMap<String, Vec<String>>, tid: &str) -> usize {
    knockouts.get(tid).map_or(0, Vec::len)
}

/// Build a review task (`_review_task`, `selector.py:919-975`).
fn review_task(
    tid: &str,
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    cfg: &Config,
    knockouts: &BTreeMap<String, Vec<String>>,
    nearly: bool,
    frontier_blocked: bool,
) -> Task {
    let n_ko = knockout_count(knockouts, tid);
    let mut why = if nearly {
        let mut text = "nearly-due review".to_owned();
        if frontier_blocked {
            text.push_str(" (brought forward; frontier blocked)");
        }
        text
    } else {
        "due review".to_owned()
    };
    if n_ko > 0 {
        why.push_str(&format!(
            "; knocks out {n_ko} other due topic(s) via encompassing"
        ));
    }
    let default = TopicState::default();
    let state = states.get(tid).unwrap_or(&default);
    Task {
        task_id: String::new(),
        task_type: TaskType::Review,
        topic: Some(tid.to_owned()),
        n_problems: Some(cfg.review.questions),
        mix: review_mix(graph, tid),
        difficulty_target: Some(DIFFICULTY_TARGET.to_owned()),
        recent_problem_hashes: state.last_problems.clone(),
        why,
        nearly_due: nearly,
        ..Task::default()
    }
}

/// Build a frontier lesson task (`_lesson_task`, `selector.py:978-1010`).
fn lesson_task(
    tid: &str,
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    course_topics: &TopicSet,
    knockouts: &BTreeMap<String, Vec<String>>,
    gap_fill: bool,
    gap_return_to: Option<&str>,
) -> Task {
    let n_dep = graph.idx_of(tid).map_or(0, |idx| {
        graph
            .descendants(idx)
            .into_iter()
            .filter(|dependent| course_topics.contains(*dependent))
            .count()
    });
    let n_ko = knockout_count(knockouts, tid);
    let mut parts: Vec<String> = if gap_fill {
        let mut parts = vec!["gap-fill lesson".to_owned()];
        if let Some(course) = gap_return_to {
            parts.push(format!("unblocks {course}"));
        }
        parts
    } else {
        vec![
            "frontier lesson".to_owned(),
            format!("{n_dep} in-course dependents"),
        ]
    };
    if graph
        .idx_of(tid)
        .and_then(|idx| graph.topic(idx))
        .is_some_and(|topic| topic.core)
    {
        parts.push("core".to_owned());
    }
    let mut why = parts.join(", ");
    if n_ko > 0 {
        why.push_str(&format!("; knocks out {n_ko} due review(s)"));
    }
    let default = TopicState::default();
    Task {
        task_id: String::new(),
        task_type: TaskType::Lesson,
        topic: Some(tid.to_owned()),
        start_at_kp: start_kp(graph, tid, states.get(tid).unwrap_or(&default)),
        why,
        gap_fill,
        gap_return_to: gap_return_to.map(ToOwned::to_owned),
        ..Task::default()
    }
}

/// Build the quiz task (`_quiz_task`, `selector.py:1013-1030`).
fn quiz_task(plan: &QuizPlan, difficulty_streak: i64) -> Task {
    let n_recent = plan
        .questions
        .iter()
        .filter(|question| question.stratum == "recent")
        .count();
    let n_questions = plan.questions.len();
    Task {
        task_id: String::new(),
        task_type: TaskType::Quiz,
        topic: None,
        n_problems: i64::try_from(n_questions).ok(),
        mix: plan
            .questions
            .iter()
            .map(|question| format!("topic:{}", question.topic))
            .collect(),
        difficulty_target: Some(quiz_difficulty_target(difficulty_streak).to_owned()),
        time_budget_secs: Some(plan.total_time_budget_secs()),
        why: format!("quiz due; {n_questions} questions ({n_recent} recent, all-history)"),
        ..Task::default()
    }
}

/// Build a drill task (`_drill_task`, `selector.py:1033-1041`).
fn drill_task(tid: &str, cfg: &Config) -> Task {
    let target = cfg.drill.target_secs;
    Task {
        task_id: String::new(),
        task_type: TaskType::Drill,
        topic: Some(tid.to_owned()),
        n_problems: Some(cfg.drill.questions),
        time_budget_secs: Some(target.saturating_mul(cfg.drill.questions)),
        why: format!("automaticity drill; target {target}s/question"),
        ..Task::default()
    }
}

// --------------------------------------------------------------------------- //
// The multi-step integration task
// --------------------------------------------------------------------------- //

/// Whether the periodic multi-step cadence fires
/// (`multistep_is_due`, `selector.py:1050-1068`).
///
/// One integration task is owed per [`MULTISTEP_CADENCE`] mastered reviewable
/// topics, and `n_closed` spends the owed slots. It never fires for a learner
/// with no review history.
#[must_use]
pub const fn multistep_is_due(n_mastered_reviewable: i64, n_closed: i64) -> bool {
    n_mastered_reviewable >= MULTISTEP_CADENCE
        && n_closed < n_mastered_reviewable / MULTISTEP_CADENCE
}

/// Order the components of a multi-step task
/// (`multistep_components`, `selector.py:1055-1068`).
///
/// The order is `(in-set ancestor count, id)` ascending, so a prerequisite leads
/// its dependents. The list is capped at [`MULTISTEP_MAX_COMPONENTS`].
#[must_use]
pub fn multistep_components(candidates: &[String], graph: &Curriculum) -> Vec<String> {
    let pool: BTreeSet<String> = candidates.iter().cloned().collect();
    let mut in_pool = TopicSet::empty(graph);
    for tid in &pool {
        in_pool.insert_id(graph, tid);
    }
    let mut keyed: Vec<(usize, String)> = pool
        .into_iter()
        .map(|tid| {
            let depth = graph.idx_of(&tid).map_or(0, |idx| {
                graph
                    .ancestors(idx)
                    .into_iter()
                    .filter(|ancestor| in_pool.contains(*ancestor))
                    .count()
            });
            (depth, tid)
        })
        .collect();
    keyed.sort();
    keyed
        .into_iter()
        .take(MULTISTEP_MAX_COMPONENTS)
        .map(|(_, tid)| tid)
        .collect()
}

/// Build the multi-step integration task (`_multistep_task`, `selector.py:1071-1105`).
///
/// The recently served digests of every component are pooled, deduped, and kept
/// in component order, so every part avoids what the learner just solved.
fn multistep_task(
    components: &[String],
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
) -> Task {
    let names: Vec<&str> = components
        .iter()
        .filter_map(|tid| graph.idx_of(tid))
        .filter_map(|idx| graph.topic(idx))
        .map(|topic| topic.name.as_str())
        .collect();
    let mut recent: Vec<String> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let default = TopicState::default();
    for component in components {
        for digest in &states.get(component).unwrap_or(&default).last_problems {
            if seen.insert(digest.clone()) {
                recent.push(digest.clone());
            }
        }
    }
    let n_parts = components.len();
    let joined = names.join(", ");
    Task {
        task_id: String::new(),
        task_type: TaskType::MultiStep,
        topic: None,
        n_problems: i64::try_from(n_parts).ok(),
        component_topics: components.to_vec(),
        difficulty_target: Some(DIFFICULTY_TARGET.to_owned()),
        recent_problem_hashes: recent,
        why: format!(
            "multi-part integration ({n_parts} parts, one per mastered skill in a novel \
             combination); compresses {n_parts} reviews into one task [{joined}]"
        ),
        ..Task::default()
    }
}

/// Build the queued remediation tasks (`_remediation_tasks`, `selector.py:1108-1152`).
///
/// A mastered target gets a remedial review; an unmastered one gets its lesson,
/// the peel-back of PEDAGOGY 8. Each target is served once, first occurrence wins.
#[must_use]
pub fn remediation_tasks(
    pending: &[PendingRemediation],
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    cfg: &Config,
) -> Vec<Task> {
    let mut tasks: Vec<Task> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let default = TopicState::default();
    for item in pending {
        for target in &item.targets {
            let id = target.as_str();
            if seen.contains(id) || graph.idx_of(id).is_none() {
                continue;
            }
            seen.insert(id.to_owned());
            let state = states.get(id).unwrap_or(&default);
            let kind = &item.kind;
            let task = if is_mastered(state) {
                Task {
                    task_id: String::new(),
                    task_type: TaskType::Review,
                    topic: Some(id.to_owned()),
                    n_problems: Some(cfg.review.questions),
                    mix: review_mix(graph, id),
                    difficulty_target: Some(DIFFICULTY_TARGET.to_owned()),
                    recent_problem_hashes: state.last_problems.clone(),
                    why: format!("remediation ({kind}); remedial review"),
                    is_remediation: true,
                    ..Task::default()
                }
            } else {
                Task {
                    task_id: String::new(),
                    task_type: TaskType::Lesson,
                    topic: Some(id.to_owned()),
                    start_at_kp: start_kp(graph, id, state),
                    why: format!("remediation ({kind}); peel-back lesson"),
                    is_remediation: true,
                    ..Task::default()
                }
            };
            tasks.push(task);
        }
    }
    tasks
}

// --------------------------------------------------------------------------- //
// Interleaving
// --------------------------------------------------------------------------- //

/// One slot of the interleaved sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotKind {
    /// A review of a learned topic.
    Review,
    /// A frontier lesson.
    Lesson,
}

/// Reorder the importance-ranked lessons across their modules
/// (`_arrange_lessons`, `selector.py:1160-1187`).
///
/// At each step it takes from a module other than the last one, preferring the
/// module with the most lessons left; a tie prefers the module that appeared
/// first. It falls back to a same-module lesson only when nothing else remains.
#[must_use]
pub fn arrange_lessons(lessons: &[String], graph: &Curriculum) -> Vec<String> {
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut module_order: Vec<String> = Vec::new();
    for tid in lessons {
        let module = graph
            .idx_of(tid)
            .map_or_else(String::new, |idx| graph.module_of(idx).to_owned());
        if !groups.contains_key(&module) {
            groups.insert(module.clone(), Vec::new());
            module_order.push(module.clone());
        }
        if let Some(group) = groups.get_mut(&module) {
            group.push(tid.clone());
        }
    }

    let mut result: Vec<String> = Vec::with_capacity(lessons.len());
    let mut last: Option<String> = None;
    loop {
        let non_empty: Vec<&String> = module_order
            .iter()
            .filter(|module| groups.get(*module).is_some_and(|group| !group.is_empty()))
            .collect();
        if non_empty.is_empty() {
            break;
        }
        let mut avail: Vec<&String> = non_empty
            .iter()
            .copied()
            .filter(|module| Some(module.as_str()) != last.as_deref())
            .collect();
        if avail.is_empty() {
            avail = non_empty;
        }
        // Python `max` keeps the FIRST maximum of `(len(group), -order_index)`.
        // Walking `avail` in module order makes `-order_index` descend, so a
        // strict `>` on the length alone reproduces it.
        let mut best: Option<&String> = None;
        let mut best_len = 0_usize;
        for module in avail {
            let len = groups.get(module).map_or(0, Vec::len);
            if best.is_none() || len > best_len {
                best = Some(module);
                best_len = len;
            }
        }
        let Some(module) = best.cloned() else {
            break;
        };
        if let Some(group) = groups.get_mut(&module)
            && !group.is_empty()
        {
            result.push(group.remove(0));
        }
        last = Some(module);
    }
    result
}

/// Interleave reviews and lessons (`_interleave`, `selector.py:1190-1219`).
///
/// Reviews come first by priority, but the throttle forces a lesson once
/// `selector.max_reviews_per_lesson` reviews have run without one. A lesson
/// resets the counter.
#[must_use]
pub fn interleave(reviews: &[String], lessons: &[String], cfg: &Config) -> Vec<(SlotKind, String)> {
    let mut seq: Vec<(SlotKind, String)> = Vec::with_capacity(reviews.len() + lessons.len());
    let mut ri = 0_usize;
    let mut li = 0_usize;
    let mut reviews_since_lesson = 0_i64;
    let max_run = cfg.selector.max_reviews_per_lesson;
    while ri < reviews.len() || li < lessons.len() {
        let force_lesson = li < lessons.len() && reviews_since_lesson >= max_run;
        if ri < reviews.len() && !force_lesson {
            if let Some(tid) = reviews.get(ri) {
                seq.push((SlotKind::Review, tid.clone()));
            }
            ri += 1;
            reviews_since_lesson += 1;
        } else if li < lessons.len() {
            if let Some(tid) = lessons.get(li) {
                seq.push((SlotKind::Lesson, tid.clone()));
            }
            li += 1;
            reviews_since_lesson = 0;
        } else {
            if let Some(tid) = reviews.get(ri) {
                seq.push((SlotKind::Review, tid.clone()));
            }
            ri += 1;
        }
    }
    seq
}

/// The longest run of consecutive reviews (`_max_review_run`, `selector.py:1222-1228`).
fn max_review_run(seq: &[(SlotKind, String)]) -> i64 {
    let mut run = 0_i64;
    let mut best = 0_i64;
    for &(kind, _) in seq {
        run = if kind == SlotKind::Review { run + 1 } else { 0 };
        best = best.max(run);
    }
    best
}

/// The throttle and ratio report (`_constraints`, `selector.py:1499-1519`).
fn constraints_of(
    seq: &[(SlotKind, String)],
    lessons_available: bool,
    cfg: &Config,
) -> Constraints {
    let n_review = seq
        .iter()
        .filter(|&&(kind, _)| kind == SlotKind::Review)
        .count();
    let n_lesson = seq.len() - n_review;
    let denom = seq.len();
    let ratio = if denom == 0 {
        0.0
    } else {
        count_as_float(n_lesson) / count_as_float(denom)
    };
    Constraints {
        lesson_ratio_ok: !lessons_available || denom == 0 || ratio >= cfg.selector.lesson_ratio_min,
        lesson_ratio: round_dp(ratio, 4),
        throttle_ok: !lessons_available
            || max_review_run(seq) <= cfg.selector.max_reviews_per_lesson,
        reviews: i64::try_from(n_review).unwrap_or(i64::MAX),
        lessons: i64::try_from(n_lesson).unwrap_or(i64::MAX),
    }
}

/// Assign the content-stable task ids (`_assign_ids`, `selector.py:1484-1496`).
///
/// The id is `{session}-{task_type}-{topic}`, or `{session}-{task_type}` for a
/// task that spans several topics. It is never positional: a recomposed plan
/// must not recycle a served id onto a different topic.
pub fn assign_ids(tasks: &mut [Task], session_id: &str) {
    for task in tasks {
        let kind = task.task_type.as_str();
        task.task_id = match task.topic.as_deref() {
            Some(topic) => format!("{session_id}-{kind}-{topic}"),
            None => format!("{session_id}-{kind}"),
        };
    }
}

// --------------------------------------------------------------------------- //
// Course completion and cross-course gap fill (PEDAGOGY 8, DD-1)
// --------------------------------------------------------------------------- //

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
    let owned;
    let mastered = match mastered {
        Some(given) => given,
        None => {
            owned = mastered_set(states, graph);
            &owned
        }
    };
    let course_topics = course_scope(graph, course_id);
    !course_topics.is_empty() && course_topics.is_subset(mastered)
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
    let owned;
    let mastered = match mastered {
        Some(given) => given,
        None => {
            owned = mastered_set(states, graph);
            &owned
        }
    };
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
    let owned;
    let mastered = match mastered {
        Some(given) => given,
        None => {
            owned = mastered_set(states, graph);
            &owned
        }
    };
    let course_topics = course_scope(graph, Some(course));
    let course_frontier = frontier(graph, mastered).intersect(&course_topics);
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
    if course_topics.is_subset(mastered) {
        return None; // Every course topic is mastered.
    }
    let missing = blocking_gap_ancestors(states, graph, Some(course), Some(mastered));
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
    let mut best: Option<(f64, &str)> = None;
    for &course in courses {
        let key = (course_order(graph, Some(course)), course);
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
    let owned;
    let mastered = match mastered {
        Some(given) => given,
        None => {
            owned = mastered_set(states, graph);
            &owned
        }
    };
    let mut chain = TopicSet::empty(graph);
    let parents = stack.get(..stack.len() - 1).unwrap_or(&[]);
    for parent in parents {
        let blockers = blocking_gap_ancestors(states, graph, Some(parent), Some(mastered));
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
    let owned;
    let mastered = match mastered {
        Some(given) => given,
        None => {
            owned = mastered_set(states, graph);
            &owned
        }
    };
    let tip = stack.last().map(String::as_str);
    let mut out = frontier(graph, mastered).intersect(&course_scope(graph, tip));
    if let Some(chain) = gap_fill_chain_for_stack(states, graph, stack, Some(mastered)) {
        out = out.intersect(&chain);
    }
    for idx in mastered.indices() {
        out.remove(idx);
    }
    out
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
    let mut seen: BTreeSet<String> = BTreeSet::new();
    seen.insert(base.to_owned());
    loop {
        let tip = stack.last().map(String::as_str);
        let mut gap = gap_course_for(states, graph, cfg, t_us, tip, Some(&mastered));
        if gap.is_none() {
            gap = deeper_gap_course(states, graph, &stack, &mastered);
        }
        let Some(course) = gap else {
            break;
        };
        if seen.contains(&course) {
            break;
        }
        seen.insert(course.clone());
        stack.push(course);
    }
    stack
}

/// The topics of the gap course that block the parent course
/// (`gap_fill_chain`, `selector.py:364-378`).
#[must_use]
pub fn gap_fill_chain(
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    effective_course: &str,
    parent_course: &str,
    mastered: Option<&TopicSet>,
) -> TopicSet {
    let needed = blocking_gap_ancestors(states, graph, Some(parent_course), mastered);
    needed.intersect(&course_scope(graph, Some(effective_course)))
}

// --------------------------------------------------------------------------- //
// compose_session
// --------------------------------------------------------------------------- //

/// The keyword context of `compose_session` (`selector.py:1240-1257`).
///
/// Every field has the 1.0 default, so a caller sets only what it knows.
#[derive(Debug, Clone)]
pub struct SessionContext<'a> {
    /// A still-open plan to re-serve instead of composing afresh.
    pub open_plan: Option<&'a SessionPlan>,
    /// The session id the task ids are keyed on.
    pub session_id: &'a str,
    /// The enrolled course, or `None` for the whole curriculum.
    pub course_id: Option<&'a str>,
    /// The remediation queue, in trigger order.
    pub pending_remediation: &'a [PendingRemediation],
    /// The quiz cadence state.
    pub quiz_state: Option<&'a QuizState>,
    /// Topic id to the UTC microseconds it was first mastered.
    pub learned_at: Option<&'a BTreeMap<String, i64>>,
    /// Topic id to the UTC microseconds of its last drill.
    pub last_drill_at: Option<&'a BTreeMap<String, i64>>,
    /// The blocking chain to serve while switched down into a gap course.
    pub gap_fill_chain: Option<&'a BTreeSet<String>>,
    /// The course a gap-fill lesson returns to.
    pub gap_return_to: Option<&'a str>,
    /// The distinct dates the learner studied, for the quiz cadence.
    pub active_study_days: Option<&'a [NaiveDate]>,
    /// The consecutive high-scoring quizzes, for the difficulty band.
    pub quiz_high_score_streak: i64,
    /// The topics classified at the raised test-prep due threshold.
    pub test_prep_topics: &'a BTreeSet<String>,
    /// The lifetime count of closed multi-step tasks.
    pub multistep_closed: i64,
    /// The task ids already closed this session.
    pub closed_task_ids: &'a BTreeSet<String>,
    /// The components of a multi-step task already served and still open.
    pub open_multistep_components: Option<&'a [String]>,
    /// The cap on the number of served tasks.
    pub n: Option<usize>,
}

impl Default for SessionContext<'_> {
    fn default() -> Self {
        Self {
            open_plan: None,
            session_id: "s",
            course_id: None,
            pending_remediation: &NO_REMEDIATION,
            quiz_state: None,
            learned_at: None,
            last_drill_at: None,
            gap_fill_chain: None,
            gap_return_to: None,
            active_study_days: None,
            quiz_high_score_streak: 0,
            test_prep_topics: &NO_IDS,
            multistep_closed: 0,
            closed_task_ids: &NO_IDS,
            open_multistep_components: None,
            n: None,
        }
    }
}

impl<'a> SessionContext<'a> {
    /// Set the session id.
    #[must_use]
    pub const fn with_session_id(mut self, session_id: &'a str) -> Self {
        self.session_id = session_id;
        self
    }

    /// Set the enrolled course.
    #[must_use]
    pub const fn with_course(mut self, course_id: Option<&'a str>) -> Self {
        self.course_id = course_id;
        self
    }

    /// Set the remediation queue.
    #[must_use]
    pub const fn with_pending_remediation(mut self, pending: &'a [PendingRemediation]) -> Self {
        self.pending_remediation = pending;
        self
    }

    /// Set the quiz cadence state.
    #[must_use]
    pub const fn with_quiz_state(mut self, quiz_state: Option<&'a QuizState>) -> Self {
        self.quiz_state = quiz_state;
        self
    }

    /// Set the first-mastered times that date the quiz strata.
    #[must_use]
    pub const fn with_learned_at(mut self, learned_at: Option<&'a BTreeMap<String, i64>>) -> Self {
        self.learned_at = learned_at;
        self
    }

    /// Set the last-drill times that rate-limit the drills.
    #[must_use]
    pub const fn with_last_drill_at(
        mut self,
        last_drill_at: Option<&'a BTreeMap<String, i64>>,
    ) -> Self {
        self.last_drill_at = last_drill_at;
        self
    }

    /// Switch down into a gap course: serve only `chain`, and return to `back_to`.
    #[must_use]
    pub const fn with_gap_fill(
        mut self,
        chain: Option<&'a BTreeSet<String>>,
        back_to: Option<&'a str>,
    ) -> Self {
        self.gap_fill_chain = chain;
        self.gap_return_to = back_to;
        self
    }

    /// Set the active study days of the quiz cadence.
    #[must_use]
    pub const fn with_active_study_days(mut self, days: Option<&'a [NaiveDate]>) -> Self {
        self.active_study_days = days;
        self
    }

    /// Set the consecutive high-scoring quizzes.
    #[must_use]
    pub const fn with_quiz_streak(mut self, streak: i64) -> Self {
        self.quiz_high_score_streak = streak;
        self
    }

    /// Set the test-prep topics.
    #[must_use]
    pub const fn with_test_prep(mut self, topics: &'a BTreeSet<String>) -> Self {
        self.test_prep_topics = topics;
        self
    }

    /// Set the multi-step counters and the closed task ids.
    #[must_use]
    pub const fn with_multistep(
        mut self,
        closed: i64,
        closed_task_ids: &'a BTreeSet<String>,
    ) -> Self {
        self.multistep_closed = closed;
        self.closed_task_ids = closed_task_ids;
        self
    }

    /// Set the components of an open multi-step task.
    #[must_use]
    pub const fn with_open_multistep(mut self, components: Option<&'a [String]>) -> Self {
        self.open_multistep_components = components;
        self
    }

    /// Cap the number of served tasks.
    #[must_use]
    pub const fn with_limit(mut self, n: Option<usize>) -> Self {
        self.n = n;
        self
    }

    /// Re-serve an open plan instead of composing afresh.
    #[must_use]
    pub const fn with_open_plan(mut self, open_plan: Option<&'a SessionPlan>) -> Self {
        self.open_plan = open_plan;
        self
    }
}

/// Compose the ordered session plan of PEDAGOGY 5 (`compose_session`, `selector.py:1235-1481`).
///
/// The priority order is: the remediation queue, the compressed due reviews, the
/// frontier lessons by importance, the multi-step task, the quiz, then the
/// drills — with the review throttle, the lesson ratio, and the module
/// interleaving of PEDAGOGY 5.
///
/// With [`SessionContext::open_plan`] set it re-serves that plan minus the tasks
/// that are no longer valid, keeping their order and their ids.
#[must_use]
pub fn compose_session(
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    cfg: &Config,
    t_us: i64,
    sampler: &mut dyn QuizSampler,
    ctx: &SessionContext<'_>,
) -> SessionPlan {
    if let Some(open) = ctx.open_plan {
        return reserve_open_plan(open, states, graph, cfg, t_us, ctx);
    }
    let mut cache = ReachCache::new(graph);
    let default = TopicState::default();

    let mastered = mastered_set(states, graph);
    let course_topics = course_scope(graph, ctx.course_id);
    let mut frontier_topics: Vec<String> = frontier(graph, &mastered)
        .intersect(&course_topics)
        .sorted_ids(graph)
        .into_iter()
        .map(ToOwned::to_owned)
        .collect();
    if let Some(chain) = ctx.gap_fill_chain {
        frontier_topics.retain(|tid| chain.contains(tid));
    }

    let available: Vec<String> = frontier_topics
        .iter()
        .filter(|tid| !in_retry_delay(states.get(*tid).unwrap_or(&default), cfg, t_us))
        .cloned()
        .collect();
    let blocked: Vec<String> = frontier_topics
        .iter()
        .filter(|tid| in_retry_delay(states.get(*tid).unwrap_or(&default), cfg, t_us))
        .cloned()
        .collect();

    let course_complete = ctx.gap_fill_chain.is_none()
        && is_course_complete(states, graph, ctx.course_id, Some(&mastered));
    let mut frontier_blocked_until: Option<i64> = None;
    if !frontier_topics.is_empty() && available.is_empty() {
        frontier_blocked_until = blocked
            .iter()
            .filter_map(|tid| states.get(tid))
            .filter_map(|state| retry_available_at(state, cfg))
            .min();
    }

    // Priority 1: the remediation queue. It is computed first so the review and
    // lesson lists can dedupe against it.
    let remediation = remediation_tasks(ctx.pending_remediation, states, graph, cfg);
    let remediation_topics: BTreeSet<String> = remediation
        .iter()
        .filter_map(|task| task.topic.clone())
        .collect();

    let due = due_reviews(states, graph, cfg, t_us, ctx.test_prep_topics);
    let available_set: BTreeSet<String> = available.iter().cloned().collect();
    let comp = compress_with(
        &due,
        states,
        graph,
        cfg,
        t_us,
        Some(&available_set),
        &mut cache,
    );
    let surviving_set: BTreeSet<String> = comp.surviving.iter().cloned().collect();
    let mut surviving = comp.surviving.clone();
    surviving.sort_by(|left, right| {
        knockout_count(&comp.knockouts, right)
            .cmp(&knockout_count(&comp.knockouts, left))
            .then_with(|| left.cmp(right))
    });
    let nearly = nearly_due(states, graph, cfg, t_us);
    let nearly_set: BTreeSet<String> = nearly.iter().cloned().collect();
    // `comp.knockouts` is a sorted map, so its keys already come back sorted.
    let nearly_knockers: Vec<String> = comp
        .knockouts
        .keys()
        .filter(|key| nearly_set.contains(*key) && !surviving_set.contains(*key))
        .cloned()
        .collect();

    let nearly_served: BTreeSet<String>;
    let mut review_topics: Vec<String> = surviving;
    let mut lessons_ordered: Vec<String> = Vec::new();
    if frontier_blocked_until.is_some() {
        // The frontier is blocked: serve no lesson, bring the nearly-due forward.
        nearly_served = nearly
            .iter()
            .filter(|tid| !surviving_set.contains(*tid))
            .cloned()
            .collect();
        review_topics.extend(
            nearly
                .iter()
                .filter(|tid| nearly_served.contains(*tid))
                .cloned(),
        );
    } else {
        nearly_served = nearly_knockers.iter().cloned().collect();
        review_topics.extend(nearly_knockers.iter().cloned());
        let review_targets: BTreeSet<String> = due.iter().chain(nearly.iter()).cloned().collect();
        let ranked = order_lessons_with(
            &available,
            graph,
            &review_targets,
            &course_topics,
            &mut cache,
        );
        lessons_ordered = arrange_lessons(&ranked, graph);
    }

    // A remediation task supersedes the same topic's review or lesson.
    review_topics.retain(|tid| !remediation_topics.contains(tid));
    lessons_ordered.retain(|tid| !remediation_topics.contains(tid));

    // The multi-step integration task absorbs several due reviews.
    let n_reviewable = i64::try_from(
        states
            .iter()
            .filter(|(id, state)| graph.idx_of(id).is_some() && has_review_history(state))
            .count(),
    )
    .unwrap_or(i64::MAX);
    let multistep_id = format!("{}-{}", ctx.session_id, TaskType::MultiStep.as_str());
    let mut multistep: Option<Task> = None;
    let open_components = ctx.open_multistep_components.unwrap_or(&[]);
    if !open_components.is_empty() && !ctx.closed_task_ids.contains(&multistep_id) {
        // The session already served an open multi-step task. Re-serve it with
        // the EXACT components the `task_served` event recorded, so the close
        // credits the same topics.
        multistep = Some(multistep_task(open_components, states, graph));
        review_topics.retain(|tid| !open_components.contains(tid));
    } else if MULTISTEP_ENABLED
        && multistep_is_due(n_reviewable, ctx.multistep_closed)
        && !ctx.closed_task_ids.contains(&multistep_id)
    {
        let due_set: BTreeSet<&String> = due.iter().collect();
        let candidates: Vec<String> = review_topics
            .iter()
            .filter(|tid| {
                due_set.contains(*tid) && has_review_history(states.get(*tid).unwrap_or(&default))
            })
            .cloned()
            .collect();
        if candidates.len() >= MULTISTEP_MIN_COMPONENTS {
            let components = multistep_components(&candidates, graph);
            multistep = Some(multistep_task(&components, states, graph));
            review_topics.retain(|tid| !components.contains(tid));
        }
    }

    let seq = interleave(&review_topics, &lessons_ordered, cfg);

    let mut tasks: Vec<Task> = remediation;
    for (kind, tid) in &seq {
        let task = match *kind {
            SlotKind::Review => review_task(
                tid,
                states,
                graph,
                cfg,
                &comp.knockouts,
                nearly_served.contains(tid),
                frontier_blocked_until.is_some(),
            ),
            SlotKind::Lesson => lesson_task(
                tid,
                states,
                graph,
                &course_topics,
                &comp.knockouts,
                ctx.gap_fill_chain.is_some(),
                ctx.gap_return_to,
            ),
        };
        tasks.push(task);
    }
    if let Some(task) = multistep {
        tasks.push(task);
    }

    let quiz_due = quiz_is_due(
        ctx.quiz_state,
        states,
        graph,
        cfg,
        t_us,
        ctx.active_study_days,
    );
    if quiz_due {
        let plan = quiz_composer(states, graph, cfg, t_us, sampler, ctx.learned_at);
        if !plan.questions.is_empty() {
            tasks.push(quiz_task(&plan, ctx.quiz_high_score_streak));
        }
    }

    for tid in schedule_drills(states, graph, t_us, ctx.last_drill_at) {
        tasks.push(drill_task(&tid, cfg));
    }

    if let Some(limit) = ctx.n {
        tasks.truncate(limit);
    }
    assign_ids(&mut tasks, ctx.session_id);

    SessionPlan {
        session: ctx.session_id.to_owned(),
        tasks,
        quiz_due,
        constraints: constraints_of(&seq, !available.is_empty(), cfg),
        course_complete,
        frontier_blocked_until: frontier_blocked_until.map(Timestamp::from_micros),
    }
}

// --------------------------------------------------------------------------- //
// Re-serving an open plan
// --------------------------------------------------------------------------- //

/// The inputs [`task_still_valid`] tests a queued task against.
#[derive(Debug, Clone, Copy)]
pub struct ValidityContext<'a> {
    /// The topics the remediation queue still names.
    pub pending_targets: &'a BTreeSet<String>,
    /// The frontier of the course scope.
    pub frontier_topics: &'a TopicSet,
    /// Whether a quiz is due.
    pub quiz_due: bool,
    /// The topics a drill may serve.
    pub drill_eligible: &'a BTreeSet<String>,
    /// The topics classified at the raised test-prep due threshold.
    pub test_prep_topics: &'a BTreeSet<String>,
    /// The task ids already closed this session.
    pub closed_task_ids: &'a BTreeSet<String>,
}

/// Whether a queued task should still be served (`_task_still_valid`, `selector.py:1527-1605`).
///
/// A task drops when the reason it was queued no longer holds. A nearly-due
/// review stays valid across the whole `{due, nearly_due}` band, because memory
/// only decays: a strict `due` test would drop it the moment it crossed the due
/// line, and would silently take the reviews it knocks out with it.
#[must_use]
pub fn task_still_valid(
    task: &Task,
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    cfg: &Config,
    t_us: i64,
    ctx: &ValidityContext<'_>,
) -> bool {
    let default = TopicState::default();
    let band = |tid: &str, state: &TopicState| -> ReviewState {
        review_state(state, t_us, cfg, ctx.test_prep_topics.contains(tid))
    };
    match task.task_type {
        TaskType::Quiz => return ctx.quiz_due,
        TaskType::MultiStep => {
            if ctx.closed_task_ids.contains(&task.task_id) {
                return false;
            }
            return task.component_topics.iter().any(|component| {
                band(component, states.get(component).unwrap_or(&default)) == ReviewState::Due
            });
        }
        _ => {}
    }
    let Some(topic_id) = task.topic.as_deref() else {
        return false;
    };
    if task.is_remediation {
        return ctx.pending_targets.contains(topic_id);
    }
    match task.task_type {
        TaskType::Review => {
            let Some(state) = states.get(topic_id) else {
                return false;
            };
            let current = band(topic_id, state);
            if task.nearly_due {
                matches!(current, ReviewState::Due | ReviewState::NearlyDue)
            } else {
                current == ReviewState::Due
            }
        }
        TaskType::Lesson => {
            let state = states.get(topic_id).unwrap_or(&default);
            ctx.frontier_topics.contains_id(graph, topic_id)
                && !is_mastered(state)
                && !in_retry_delay(state, cfg, t_us)
        }
        TaskType::Drill => ctx.drill_eligible.contains(topic_id),
        _ => true,
    }
}

/// Re-serve an open plan minus the tasks that are no longer valid
/// (`_reserve_open_plan`, `selector.py:1630-1736`).
///
/// The surviving tasks keep their order and their ids, so the queue is stable.
/// Remediation queued after the plan was composed is prepended, because
/// PEDAGOGY 5 serves remediation first.
#[must_use]
pub fn reserve_open_plan(
    open_plan: &SessionPlan,
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    cfg: &Config,
    t_us: i64,
    ctx: &SessionContext<'_>,
) -> SessionPlan {
    let default = TopicState::default();
    let mastered = mastered_set(states, graph);
    let course_topics = course_scope(graph, ctx.course_id);
    let frontier_topics = frontier(graph, &mastered).intersect(&course_topics);
    let frontier_ids = frontier_topics.sorted_ids(graph);
    let available: Vec<&str> = frontier_ids
        .iter()
        .copied()
        .filter(|tid| !in_retry_delay(states.get(*tid).unwrap_or(&default), cfg, t_us))
        .collect();

    let course_complete = is_course_complete(states, graph, ctx.course_id, Some(&mastered));
    let mut frontier_blocked_until: Option<i64> = None;
    if !frontier_topics.is_empty() && available.is_empty() {
        frontier_blocked_until = frontier_ids
            .iter()
            .filter(|tid| in_retry_delay(states.get(**tid).unwrap_or(&default), cfg, t_us))
            .filter_map(|tid| states.get(*tid))
            .filter_map(|state| retry_available_at(state, cfg))
            .min();
    }

    let pending_targets: BTreeSet<String> = ctx
        .pending_remediation
        .iter()
        .flat_map(|item| item.targets.iter())
        .map(|target| target.as_str().to_owned())
        .collect();
    let quiz_due = quiz_is_due(
        ctx.quiz_state,
        states,
        graph,
        cfg,
        t_us,
        ctx.active_study_days,
    );
    let drill_eligible: BTreeSet<String> = schedule_drills(states, graph, t_us, ctx.last_drill_at)
        .into_iter()
        .collect();
    let validity = ValidityContext {
        pending_targets: &pending_targets,
        frontier_topics: &frontier_topics,
        quiz_due,
        drill_eligible: &drill_eligible,
        test_prep_topics: ctx.test_prep_topics,
        closed_task_ids: ctx.closed_task_ids,
    };

    let mut kept: Vec<Task> = open_plan
        .tasks
        .iter()
        .filter(|task| task_still_valid(task, states, graph, cfg, t_us, &validity))
        .cloned()
        .collect();

    // Remediation queued after this plan was composed is prepended, so it is not
    // hidden until the next session. Existing tasks keep their ids and order.
    let mut covered: BTreeSet<String> = kept.iter().filter_map(|task| task.topic.clone()).collect();
    let mut fresh: Vec<Task> = Vec::new();
    for mut task in remediation_tasks(ctx.pending_remediation, states, graph, cfg) {
        if let Some(topic) = task.topic.clone()
            && !covered.contains(&topic)
        {
            task.task_id = format!("{}-rem-{}", open_plan.session, topic);
            covered.insert(topic);
            fresh.push(task);
        }
    }
    fresh.extend(kept);
    kept = fresh;

    // A remediation task supersedes the same topic's plain review or lesson.
    let rem_topics: BTreeSet<String> = kept
        .iter()
        .filter(|task| task.is_remediation)
        .filter_map(|task| task.topic.clone())
        .collect();
    kept.retain(|task| {
        !(task
            .topic
            .as_ref()
            .is_some_and(|id| rem_topics.contains(id))
            && !task.is_remediation)
    });

    if let Some(limit) = ctx.n {
        kept.truncate(limit);
    }

    let seq: Vec<(SlotKind, String)> = kept
        .iter()
        .filter_map(|task| match (task.task_type, task.topic.as_ref()) {
            (TaskType::Lesson, Some(topic)) => Some((SlotKind::Lesson, topic.clone())),
            (TaskType::Review, Some(topic)) => Some((SlotKind::Review, topic.clone())),
            _ => None,
        })
        .collect();

    SessionPlan {
        session: open_plan.session.clone(),
        tasks: kept,
        quiz_due,
        constraints: constraints_of(&seq, !available.is_empty(), cfg),
        course_complete,
        frontier_blocked_until: frontier_blocked_until.map(Timestamp::from_micros),
    }
}
