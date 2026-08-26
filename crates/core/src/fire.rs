//! The FIRe spaced-repetition engine (R5, spec sections 4.9 and 7).
//!
//! Every function here is a deterministic function of `(state | states, attempt,
//! curriculum, config, t)` and returns new values. Nothing reads a clock, nothing
//! mutates its inputs, and nothing panics, so the event log stays re-projectable.
//!
//! ## Parity
//!
//! The port copies 1.0 `cadus/fire.py` instruction for instruction, including the
//! parts a rewrite is tempted to improve:
//!
//! - Python `max` and `min` are NOT `f64::max` and `f64::min`. Python returns the
//!   FIRST argument unless the second compares strictly better, which fixes the
//!   result for a signed zero and for `NaN`. [`py_max`] and [`py_min`] reproduce
//!   that, so `max(0.0, -0.0)` stays `0.0` and never turns a `repNum` into `-0.0`.
//! - 1.0 iterates `sorted(weights.items())`, so the propagation order is the byte
//!   order of the topic id (trap T18). The arena hands the pairs back sorted.
//! - 1.0 iterates the `neighborhood()` SET, whose order is hash-randomized
//!   (trap T5). The sorted order replaces it, and [`neumaier_sum`] reproduces the
//!   CPython `sum()` that the mean divides (trap T1).
//! - The credit and the penalty legs are ASYMMETRIC on purpose: downward credit
//!   reads the RECIPIENT's memory, and an upward penalty is the attempted topic's
//!   `raw` scaled by `W`. The forced-explicit gate covers credit only, and the
//!   `t0 is None` gate covers penalties only.

use std::collections::BTreeMap;
use std::hint::black_box;

use indexmap::IndexMap;

use crate::config::Config;
use crate::curriculum::Curriculum;
use crate::event::{Timestamp, TopicStatus, WorkQuality};
use crate::learner::TopicState;
use crate::numeric::{days_between, neumaier_sum};

/// The grade `q` of each work-quality tier, in tier order (`fire.py:40-47`).
///
/// These are the GRADING values of the FIRe raw delta. They are not the XP
/// multipliers of [`crate::config::XpTiers`], which the config carries.
pub const QUALITY_Q: [(WorkQuality, f64); 6] = [
    (WorkQuality::Perfect, 1.0),
    (WorkQuality::NearlyPerfect, 0.85),
    (WorkQuality::Passable, 0.7),
    (WorkQuality::NearlyPassable, 0.4),
    (WorkQuality::Poor, 0.15),
    (WorkQuality::Blowoff, 0.0),
];

/// The grade at or above which one attempt counts as a pass (`fire.py:52`).
pub const PASS_QUALITY_THRESHOLD: f64 = 0.7;

/// The memory level at or below which a topic enters the nearly-due band.
///
/// Test prep never raises it: it is a fixed lookahead window (`fire.py:57`).
pub const NEARLY_DUE_THRESHOLD: f64 = 0.6;

/// The raised due threshold of a topic in the test-prep set (`fire.py:73`).
pub const TEST_PREP_DUE_THRESHOLD: f64 = 0.7;

/// The largest review interval, in days (`fire.py:76`).
pub const INTERVAL_CAP_DAYS: f64 = 730.0;

/// The factor that discounts the positive credit of an assisted pass
/// (`fire.py:89`). An assisted MISS keeps its full negative raw delta.
pub const ASSISTED_CREDIT: f64 = 0.5;

/// The neutral ability prior of a topic with no touched neighbor.
pub const NEUTRAL_ABILITY: f64 = 0.5;

/// Python `max(a, b)` for two floats.
///
/// CPython returns `a` unless `b > a`, so `max(0.0, -0.0)` is `0.0` and
/// `max(0.0, NaN)` is `0.0`. `f64::max` picks either representation of zero and
/// drops a `NaN`, which is a different function. The 1.0 floors and clamps go
/// through this one, so a `repNum` floor never emits `-0.0`.
#[must_use]
pub fn py_max(a: f64, b: f64) -> f64 {
    if b > a { b } else { a }
}

/// Python `min(a, b)` for two floats. CPython returns `a` unless `b < a`.
#[must_use]
pub fn py_min(a: f64, b: f64) -> f64 {
    if b < a { b } else { a }
}

/// Python `max(lo, min(hi, x))`, the 1.0 `_clamp` (`fire.py:139-140`).
#[must_use]
pub fn clamp(x: f64, lo: f64, hi: f64) -> f64 {
    py_max(lo, py_min(hi, x))
}

/// The FIRe grade of a work-quality tier (`fire.py:150-152`).
#[must_use]
pub const fn quality_q(quality: WorkQuality) -> f64 {
    match quality {
        WorkQuality::Perfect => 1.0,
        WorkQuality::NearlyPerfect => 0.85,
        WorkQuality::Passable => 0.7,
        WorkQuality::NearlyPassable => 0.4,
        WorkQuality::Poor => 0.15,
        WorkQuality::Blowoff => 0.0,
    }
}

/// Whether one attempt's tier counts as a pass, `q >= 0.7` (`fire.py:155-157`).
#[must_use]
pub fn is_pass_quality(quality: WorkQuality) -> bool {
    quality_q(quality) >= PASS_QUALITY_THRESHOLD
}

/// Whether a state carries a review history, `status in {learning, placed}`
/// (`fire.py:186-195`).
///
/// This is the precondition that makes the memory bands mean anything: an
/// untouched topic has `memoryBase == 0`, so a bare `memory <= 0.5` test would
/// call it due forever, and a mastery-floor topic is assumed known.
#[must_use]
pub fn has_review_history(state: &TopicState) -> bool {
    matches!(state.status, TopicStatus::Learning | TopicStatus::Placed)
}

/// The review band of a topic (`fire.py:198-211`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReviewState {
    /// Not on the review schedule at all. The memory number carries no meaning.
    OffSchedule,
    /// At or below the due threshold. Serve an explicit review.
    Due,
    /// Above the due threshold and at or below the nearly-due threshold.
    NearlyDue,
    /// On the schedule with memory still fresh. Nothing is owed.
    OnSchedule,
}

impl ReviewState {
    /// The 1.0 string value of the band.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OffSchedule => "off_schedule",
            Self::Due => "due",
            Self::NearlyDue => "nearly_due",
            Self::OnSchedule => "on_schedule",
        }
    }
}

/// `0.5 ** exponent`, through the platform `pow` (trap T21).
///
/// The base is a literal, so LLVM rewrites `0.5_f64.powf(x)` into `exp2(-x)` in an
/// optimized build. glibc `exp2` and glibc `pow` differ by one unit in the last
/// place, and CPython `0.5 ** x` calls `pow`, so the rewrite makes the release fold
/// diverge from the 1.0 fold while the debug fold agrees with it.
/// [`black_box`] hides the base from the optimizer and keeps the real `pow` call.
/// Do NOT remove it, and do not write a literal base at any other `powf` site.
#[must_use]
fn pow_half(exponent: f64) -> f64 {
    black_box(0.5_f64).powf(exponent)
}

/// The memory of a topic at `t_us`: `memoryBase * 0.5 ** (days / interval)`
/// (`fire.py:169-179`).
///
/// A topic with no `t0`, or with a non-positive interval, has no decay reference,
/// so its undecayed `memoryBase` comes back.
///
/// The result is not finite where CPython raises `OverflowError` in the same
/// expression. [`apply_attempt_checked`] reports that state instead of folding it.
#[must_use]
pub fn memory_at(state: &TopicState, t_us: i64) -> f64 {
    let Some(t0) = state.t0 else {
        return state.memory_base;
    };
    if state.interval_days <= 0.0 {
        return state.memory_base;
    }
    let exponent = days_between(t0.micros(), t_us) / state.interval_days;
    state.memory_base * pow_half(exponent)
}

/// The review band of `state` at `t_us` (`fire.py:214-248`).
///
/// `due` is tested BEFORE `nearly_due` and the order is load-bearing in both
/// threshold regimes. Under the default pair (0.5 < 0.6) due-first is what keeps
/// the bands disjoint. Under test prep (0.7 > 0.6) the implication inverts: a
/// test-prep topic at memory 0.65 is due and is NOT nearly due.
#[must_use]
pub fn review_state(state: &TopicState, t_us: i64, cfg: &Config, test_prep: bool) -> ReviewState {
    if !has_review_history(state) {
        return ReviewState::OffSchedule;
    }
    let memory = memory_at(state, t_us);
    let due_threshold = if test_prep {
        TEST_PREP_DUE_THRESHOLD
    } else {
        cfg.fire.due_threshold
    };
    if memory <= due_threshold {
        return ReviewState::Due;
    }
    if memory <= NEARLY_DUE_THRESHOLD {
        return ReviewState::NearlyDue;
    }
    ReviewState::OnSchedule
}

/// The review interval, in days, of a possibly fractional `rep_num`
/// (`fire.py:256-275`).
///
/// The table interpolates linearly between adjacent indices. A `rep_num` at or
/// beyond the last index clamps to the last entry, with no extrapolation, and a
/// `rep_num` at or below zero clamps to the first. Every result caps at
/// [`INTERVAL_CAP_DAYS`]. An empty table gives `0.0`, the 1.0 defensive branch.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "an interval table holds a handful of entries"
)]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the branch proves the float is finite, at least 0.0, and below the last index"
)]
pub fn interval_for(rep_num: f64, cfg: &Config) -> f64 {
    let table = &cfg.fire.interval_table;
    let Some(&tail) = table.last() else {
        return 0.0;
    };
    let r = py_max(0.0, rep_num);
    let last = table.len() - 1;
    let floor = r.floor();
    let value = if floor >= last as f64 {
        tail
    } else {
        let i = floor as usize;
        match (table.get(i), table.get(i + 1)) {
            (Some(&low), Some(&high)) => {
                let frac = r - floor;
                low + frac * (high - low)
            }
            _ => tail,
        }
    };
    py_min(value, INTERVAL_CAP_DAYS)
}

/// The learning speed of a topic: `clamp((0.5 + ability) / (0.5 + difficulty))`
/// (`fire.py:278-283`). A speed-2 topic accrues twice the repetitions of speed-1.
#[must_use]
pub fn speed_for(ability: f64, difficulty: f64, cfg: &Config) -> f64 {
    let (lo, hi) = cfg.fire.speed_clamp;
    let raw = (0.5 + ability) / (0.5 + difficulty);
    clamp(raw, lo, hi)
}

/// The overdue decay multiplier of a state (`fire.py:286-296`).
///
/// `min(decay_cap, 1 + max(0, days / interval - 1))`. It never drops below 1, so
/// failing an overdue topic pushes `repNum` back harder.
#[must_use]
pub fn decay_for(state: &TopicState, t_us: i64, cfg: &Config) -> f64 {
    let Some(t0) = state.t0 else {
        return 1.0;
    };
    if state.interval_days <= 0.0 {
        return 1.0;
    }
    let overdue = days_between(t0.micros(), t_us) / state.interval_days - 1.0;
    py_min(cfg.fire.decay_cap, 1.0 + py_max(0.0, overdue))
}

/// The raw credit of one attempt at grade `quality_grade` and memory `memory_now`
/// (`fire.py:304-335`).
///
/// A pass earns `q * earlyFactor`, where the early-repetition discount is
/// `clamp((1 - memory) / (1 - due_threshold), early_floor, 1)`. An `assisted`
/// pass scales that credit by [`ASSISTED_CREDIT`]. A miss returns `-(1 - q)`,
/// with no early discount and NO assisted discount.
#[must_use]
pub fn raw_delta(
    quality_grade: f64,
    memory_now: f64,
    passed: bool,
    cfg: &Config,
    assisted: bool,
) -> f64 {
    if !passed {
        return -(1.0 - quality_grade);
    }
    let span = 1.0 - cfg.fire.due_threshold;
    let early = if span <= 0.0 {
        1.0
    } else {
        clamp((1.0 - memory_now) / span, cfg.fire.early_floor, 1.0)
    };
    let credit = quality_grade * early;
    if assisted {
        credit * ASSISTED_CREDIT
    } else {
        credit
    }
}

/// The difficulty of a topic, or `0.5` when the curriculum has no such topic
/// (`projector.py:585-587`).
#[must_use]
pub fn difficulty(graph: &Curriculum, topic: &str) -> f64 {
    graph
        .idx_of(topic)
        .and_then(|idx| graph.topic(idx))
        .map_or(0.5, |found| found.difficulty)
}

/// The ability seed of an untouched topic: the mean ability of its touched
/// neighbors (`fire.py:379-393`).
///
/// Only a neighbor that is present in `states` AND is not `untouched` counts.
/// With no such neighbor there is no local evidence, so the neutral prior
/// [`NEUTRAL_ABILITY`] comes back.
///
/// 1.0 sums the abilities with `sum()`, so [`neumaier_sum`] is the summation here
/// (trap T1), and the neighborhood arrives sorted (trap T5).
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "the neighbor count is far below 2**53"
)]
pub fn initial_ability(
    topic: &str,
    graph: &Curriculum,
    states: &BTreeMap<String, TopicState>,
    _cfg: &Config,
) -> f64 {
    let abilities: Vec<f64> = graph
        .neighborhood(topic)
        .into_iter()
        .filter_map(|other| states.get(other))
        .filter(|state| state.status != TopicStatus::Untouched)
        .map(|state| state.ability)
        .collect();
    if abilities.is_empty() {
        return NEUTRAL_ABILITY;
    }
    neumaier_sum(&abilities) / abilities.len() as f64
}

/// One exponential-moving-average step, expressed as a delta (`fire.py:396-398`).
fn ewma_delta(ability: f64, target: f64, alpha: f64) -> f64 {
    alpha * (target - ability)
}

/// The per-topic ability DELTAS of an attempt on `topic` (`fire.py:369-401`).
///
/// The attempted topic always takes an `alpha` step toward 1 (correct) or 0
/// (incorrect). A correct answer then propagates DOWN to encompassed topics and
/// an incorrect one propagates UP to dependents, each as a `W`-scaled step. Only
/// a topic already in `states` is adjusted.
///
/// The returned map keeps the 1.0 insertion order (trap T6): the attempted topic
/// first, then the neighbors in sorted-id order. The caller applies it in that
/// order.
#[must_use]
pub fn ability_update(
    states: &BTreeMap<String, TopicState>,
    topic: &str,
    correct: bool,
    graph: &Curriculum,
    cfg: &Config,
) -> IndexMap<String, f64> {
    let alpha = cfg.ability.ewma_alpha;
    let target = if correct { 1.0 } else { 0.0 };

    let base = states.get(topic).map_or(0.0, |state| state.ability);
    let mut deltas: IndexMap<String, f64> = IndexMap::new();
    deltas.insert(topic.to_owned(), ewma_delta(base, target, alpha));

    let weights = if correct {
        graph.reach_weights_by_id(topic)
    } else {
        graph.upward_weights_by_id(topic)
    };
    for (other, weight) in weights {
        if other == topic || weight <= 0.0 {
            continue;
        }
        let Some(state) = states.get(other) else {
            continue;
        };
        deltas.insert(
            other.to_owned(),
            ewma_delta(state.ability, target, alpha * weight),
        );
    }
    deltas
}

/// Apply one raw delta to a single state (`fire.py:409-431`).
///
/// ```text
/// repNum     <- max(0, repNum + speed * decay^failed * raw)
/// memoryBase <- max(0, memory(t) + raw)
/// t0         <- t
/// interval   <- interval_for(repNum)
/// ```
///
/// The decay factor is the state's OWN overdueness and applies on a miss only.
/// Every other field of the state carries through unchanged.
#[must_use]
pub fn apply_update(
    state: &TopicState,
    raw: f64,
    t_us: i64,
    failed: bool,
    cfg: &Config,
) -> TopicState {
    apply_update_inner(state, raw, t_us, failed, cfg).0
}

/// [`apply_update`] and whether every decay value it read was finite.
///
/// The flag is `false` where the memory or the decay factor left the finite range.
/// CPython raises `OverflowError` at that input in `0.5 ** exponent`, and the 1.0
/// fold builds no model. For an infinite exponent CPython returns `inf` without an
/// error, and the port stops there too: a non-finite `memoryBase` writes JSON
/// `null`, and the stored model then reads back as an error.
fn apply_update_inner(
    state: &TopicState,
    raw: f64,
    t_us: i64,
    failed: bool,
    cfg: &Config,
) -> (TopicState, bool) {
    let factor = if failed {
        decay_for(state, t_us, cfg)
    } else {
        1.0
    };
    let memory = memory_at(state, t_us);
    let new_rep = py_max(0.0, state.rep_num + state.speed * factor * raw);
    let new_base = py_max(0.0, memory + raw);
    let mut next = state.clone();
    next.rep_num = new_rep;
    next.memory_base = new_base;
    next.t0 = Some(Timestamp::from_micros(t_us));
    next.interval_days = interval_for(new_rep, cfg);
    (next, factor.is_finite() && memory.is_finite())
}

/// The outcome of one graded task on a topic — the input of [`apply_attempt`].
///
/// `passed` and `quality` are decoupled on purpose. A lesson sets
/// `passed = is_pass_quality(quality)`, while a review passes on its
/// order-sensitive trajectory ([`grade_review`]), which is independent of the
/// aggregate tier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptResult {
    /// The topic the task graded.
    pub topic: String,
    /// Whether the task passed.
    pub passed: bool,
    /// The aggregate work-quality tier of the task.
    pub quality: WorkQuality,
    /// Whether the learner reached the result with a hint or a reference.
    pub assisted: bool,
}

impl AttemptResult {
    /// One unassisted attempt result, the 1.0 three-argument construction.
    #[must_use]
    pub fn new(topic: impl Into<String>, passed: bool, quality: WorkQuality) -> Self {
        Self {
            topic: topic.into(),
            passed,
            quality,
            assisted: false,
        }
    }

    /// The same result, marked assisted.
    #[must_use]
    pub fn with_assisted(mut self, assisted: bool) -> Self {
        self.assisted = assisted;
        self
    }
}

/// Which leg of the repetition flow applied one propagation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PropagationKind {
    /// Downward credit on a passed attempt.
    Credit,
    /// Upward penalty on a failed attempt.
    Penalty,
}

impl PropagationKind {
    /// The 1.0 string value of the kind.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Credit => "credit",
            Self::Penalty => "penalty",
        }
    }
}

/// One implicit update that [`apply_attempt`] actually applied to a neighbor
/// (`fire.py:118-133`).
///
/// A neighbor dropped by the min-credit or the forced-explicit rule is simply
/// ABSENT here, and its state comes back unchanged.
#[derive(Debug, Clone, PartialEq)]
pub struct Propagation {
    /// The neighbor the update landed on.
    pub topic: String,
    /// The signed raw delta fed into that neighbor's update.
    pub raw_delta: f64,
    /// The encompassing weight that scaled it.
    pub weight: f64,
    /// Which leg applied it.
    pub kind: PropagationKind,
}

/// Apply an attempt to the explicit topic, then propagate implicit credit
/// (`fire.py:439-525`).
///
/// Returns the NEW states and the propagations that actually landed. The input
/// map is never mutated.
///
/// The explicit topic takes the raw delta of its OWN memory. Then:
///
/// - a PASS with `raw > 0` sends downward credit to every encompassed topic at
///   `W`. The credit is `raw_delta(q, memory(recipient), pass) * W`, computed
///   from the RECIPIENT's own memory, so it never exceeds the direct credit the
///   same grade would give that topic. A recipient whose `speed` is below
///   `explicit_speed_threshold` is forced-explicit and absorbs nothing.
/// - a FAIL with `raw < 0` sends an upward penalty to every dependent at `W`. The
///   penalty is `raw * W`, applied with the RECIPIENT's own decay. A dependent
///   that was never practiced (`t0` is `None`) absorbs nothing: a `t0` stamped on
///   it would suppress its first-touch ability seed.
///
/// Either leg drops a propagation whose magnitude is below `min_credit`.
///
/// The states come back even where a decay value is not finite. The fold calls
/// [`apply_attempt_checked`], which reports that topic instead.
#[must_use]
pub fn apply_attempt(
    states: &BTreeMap<String, TopicState>,
    attempt: &AttemptResult,
    graph: &Curriculum,
    cfg: &Config,
    t_us: i64,
) -> (BTreeMap<String, TopicState>, Vec<Propagation>) {
    let (new_states, props, _bad) = apply_attempt_inner(states, attempt, graph, cfg, t_us);
    (new_states, props)
}

/// The topic whose decayed memory or decay factor is not a finite number.
///
/// CPython raises `OverflowError` at the same input (`0.5 ** exponent`), and the 1.0
/// fold then builds NO model. The 2.0 fold reports this instead of storing a model
/// that reads back as an error.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the decay of topic `{topic}` is not a finite number")]
pub struct NonFiniteDecay {
    /// The id of the topic whose decay left the finite range.
    pub topic: String,
}

/// [`apply_attempt`], with the non-finite decay reported instead of folded.
///
/// # Errors
///
/// Returns [`NonFiniteDecay`] for the FIRST topic, in the read order of
/// [`apply_attempt`], whose [`memory_at`] or [`decay_for`] value is not finite.
pub fn apply_attempt_checked(
    states: &BTreeMap<String, TopicState>,
    attempt: &AttemptResult,
    graph: &Curriculum,
    cfg: &Config,
    t_us: i64,
) -> Result<(BTreeMap<String, TopicState>, Vec<Propagation>), NonFiniteDecay> {
    let (new_states, props, bad) = apply_attempt_inner(states, attempt, graph, cfg, t_us);
    match bad {
        Some(topic) => Err(NonFiniteDecay { topic }),
        None => Ok((new_states, props)),
    }
}

/// The one body of [`apply_attempt`] and [`apply_attempt_checked`].
///
/// The third value is the first topic with a non-finite decay, in the order the
/// function reads the states. The fold of that attempt still runs, so both public
/// forms take the same path and only the report differs.
fn apply_attempt_inner(
    states: &BTreeMap<String, TopicState>,
    attempt: &AttemptResult,
    graph: &Curriculum,
    cfg: &Config,
    t_us: i64,
) -> (
    BTreeMap<String, TopicState>,
    Vec<Propagation>,
    Option<String>,
) {
    let mut new_states = states.clone();
    let topic = attempt.topic.as_str();
    let passed = attempt.passed;
    let q = quality_q(attempt.quality);
    let assisted = attempt.assisted;

    // The first topic whose decay left the finite range. A later one never
    // replaces it, because 1.0 stops at the first raise.
    let mut bad: Option<String> = None;
    let mut note = |id: &str, finite: bool| {
        if !finite && bad.is_none() {
            bad = Some(id.to_owned());
        }
    };

    let explicit = new_states.get(topic).cloned().unwrap_or_default();
    let memory = memory_at(&explicit, t_us);
    note(topic, memory.is_finite());
    let raw = raw_delta(q, memory, passed, cfg, assisted);
    let (next, finite) = apply_update_inner(&explicit, raw, t_us, !passed, cfg);
    note(topic, finite);
    new_states.insert(topic.to_owned(), next);

    let mut props: Vec<Propagation> = Vec::new();

    if passed && raw > 0.0 {
        for (target, weight) in graph.reach_weights_by_id(topic) {
            if target == topic || weight <= 0.0 {
                continue;
            }
            let recipient = new_states.get(target).cloned().unwrap_or_default();
            if recipient.speed < cfg.fire.explicit_speed_threshold {
                continue;
            }
            let recipient_memory = memory_at(&recipient, t_us);
            note(target, recipient_memory.is_finite());
            let credit = raw_delta(q, recipient_memory, true, cfg, assisted) * weight;
            if credit.abs() < cfg.fire.min_credit {
                continue;
            }
            let (next, finite) = apply_update_inner(&recipient, credit, t_us, false, cfg);
            note(target, finite);
            new_states.insert(target.to_owned(), next);
            props.push(Propagation {
                topic: target.to_owned(),
                raw_delta: credit,
                weight,
                kind: PropagationKind::Credit,
            });
        }
    } else if !passed && raw < 0.0 {
        for (target, weight) in graph.upward_weights_by_id(topic) {
            if target == topic || weight <= 0.0 {
                continue;
            }
            let recipient = new_states.get(target).cloned().unwrap_or_default();
            if recipient.t0.is_none() {
                continue;
            }
            let penalty = raw * weight;
            if penalty.abs() < cfg.fire.min_credit {
                continue;
            }
            let (next, finite) = apply_update_inner(&recipient, penalty, t_us, true, cfg);
            note(target, finite);
            new_states.insert(target.to_owned(), next);
            props.push(Propagation {
                topic: target.to_owned(),
                raw_delta: penalty,
                weight,
                kind: PropagationKind::Penalty,
            });
        }
    }

    (new_states, props, bad)
}

/// Whether practicing `candidate_topic` knocks out `due_topic`'s review
/// (`fire.py:539-543`): `W(candidate -> due) >= knockout_weight`.
#[must_use]
pub fn knockout(candidate_topic: &str, due_topic: &str, graph: &Curriculum, cfg: &Config) -> bool {
    graph.encompassing_weight_by_id(candidate_topic, due_topic) >= cfg.fire.knockout_weight
}

/// The order-sensitive review grade (`fire.py:539-554`).
///
/// Position weights `1..n`, normalized by their sum. The review passes only when
/// the weighted score reaches `review.pass_weighted` AND the FINAL question is
/// correct, so an improving trajectory passes and a deteriorating one fails. An
/// empty result list is `(false, 0.0)`.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "a review holds a handful of questions"
)]
#[expect(
    clippy::cast_possible_wrap,
    reason = "a review holds a handful of questions"
)]
pub fn grade_review(question_results: &[bool], cfg: &Config) -> (bool, f64) {
    let n = question_results.len();
    if n == 0 {
        return (false, 0.0);
    }
    let count = n as i64;
    let total_weight = (count * (count + 1)) as f64 / 2.0;
    let mut earned: i64 = 0;
    for (index, &correct) in question_results.iter().enumerate() {
        if correct {
            earned += index as i64 + 1;
        }
    }
    let score = earned as f64 / total_weight;
    let last_correct = question_results.last().copied().unwrap_or(false);
    let passed = score >= cfg.review.pass_weighted && last_correct;
    (passed, score)
}
