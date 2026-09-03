//! One graded attempt on a topic: the explicit update, then the implicit
//! propagation to the neighbors (`fire.py:439-525`).

use std::collections::BTreeMap;

use crate::config::Config;
use crate::curriculum::Curriculum;
use crate::event::WorkQuality;
use crate::learner::TopicState;

use super::{apply_update, memory_at, quality_q, raw_delta};

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
    let run = AttemptRun::new(states, attempt, graph, cfg, t_us).run();
    (run.states, run.props)
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
/// [`apply_attempt`], whose [`memory_at`] or [`super::decay_for`] value is not
/// finite.
pub fn apply_attempt_checked(
    states: &BTreeMap<String, TopicState>,
    attempt: &AttemptResult,
    graph: &Curriculum,
    cfg: &Config,
    t_us: i64,
) -> Result<(BTreeMap<String, TopicState>, Vec<Propagation>), NonFiniteDecay> {
    let run = AttemptRun::new(states, attempt, graph, cfg, t_us).run();
    match run.bad {
        Some(topic) => Err(NonFiniteDecay { topic }),
        None => Ok((run.states, run.props)),
    }
}

/// The one body of [`apply_attempt`] and [`apply_attempt_checked`].
///
/// `bad` is the first topic with a non-finite decay, in the order the run reads
/// the states. A later one never replaces it, because 1.0 stops at the first
/// raise. The fold of that attempt still runs, so both public forms take the
/// same path and only the report differs.
struct AttemptRun<'a> {
    graph: &'a Curriculum,
    cfg: &'a Config,
    t_us: i64,
    topic: &'a str,
    passed: bool,
    q: f64,
    assisted: bool,
    states: BTreeMap<String, TopicState>,
    props: Vec<Propagation>,
    bad: Option<String>,
}

impl<'a> AttemptRun<'a> {
    /// A run over a copy of `states`.
    fn new(
        states: &BTreeMap<String, TopicState>,
        attempt: &'a AttemptResult,
        graph: &'a Curriculum,
        cfg: &'a Config,
        t_us: i64,
    ) -> Self {
        Self {
            graph,
            cfg,
            t_us,
            topic: attempt.topic.as_str(),
            passed: attempt.passed,
            q: quality_q(attempt.quality),
            assisted: attempt.assisted,
            states: states.clone(),
            props: Vec::new(),
            bad: None,
        }
    }

    /// Record `id` as the first non-finite topic when `finite` is false.
    fn note(&mut self, id: &str, finite: bool) {
        if !finite && self.bad.is_none() {
            self.bad = Some(id.to_owned());
        }
    }

    /// The state of `id`, or a default state for a topic with no entry.
    fn state_of(&self, id: &str) -> TopicState {
        self.states.get(id).cloned().unwrap_or_default()
    }

    /// Apply the explicit update, then the leg the sign of `raw` selects.
    fn run(mut self) -> Self {
        let explicit = self.state_of(self.topic);
        let memory = memory_at(&explicit, self.t_us);
        self.note(self.topic, memory.is_finite());
        let raw = raw_delta(self.q, memory, self.passed, self.cfg, self.assisted);
        let (next, finite) = apply_update(&explicit, raw, self.t_us, !self.passed, self.cfg);
        self.note(self.topic, finite);
        self.states.insert(self.topic.to_owned(), next);

        if self.passed && raw > 0.0 {
            self.credit_leg();
        } else if !self.passed && raw < 0.0 {
            self.penalty_leg(raw);
        }
        self
    }

    /// Send downward credit to every encompassed topic, in sorted-id order.
    fn credit_leg(&mut self) {
        let graph = self.graph;
        for (target, weight) in graph.reach_weights_by_id(self.topic) {
            if target == self.topic || weight <= 0.0 {
                continue;
            }
            self.credit_one(target, weight);
        }
    }

    /// Credit one recipient from its OWN memory, unless a gate drops it.
    fn credit_one(&mut self, target: &str, weight: f64) {
        let recipient = self.state_of(target);
        if recipient.speed < self.cfg.fire.explicit_speed_threshold {
            return;
        }
        let recipient_memory = memory_at(&recipient, self.t_us);
        self.note(target, recipient_memory.is_finite());
        let credit = raw_delta(self.q, recipient_memory, true, self.cfg, self.assisted) * weight;
        if credit.abs() < self.cfg.fire.min_credit {
            return;
        }
        self.land(target, &recipient, credit, weight, PropagationKind::Credit);
    }

    /// Send an upward penalty to every dependent, in sorted-id order.
    fn penalty_leg(&mut self, raw: f64) {
        let graph = self.graph;
        for (target, weight) in graph.upward_weights_by_id(self.topic) {
            if target == self.topic || weight <= 0.0 {
                continue;
            }
            self.penalty_one(target, raw * weight, weight);
        }
    }

    /// Penalize one practiced dependent, unless a gate drops it.
    fn penalty_one(&mut self, target: &str, penalty: f64, weight: f64) {
        let recipient = self.state_of(target);
        if recipient.t0.is_none() {
            return;
        }
        if penalty.abs() < self.cfg.fire.min_credit {
            return;
        }
        self.land(
            target,
            &recipient,
            penalty,
            weight,
            PropagationKind::Penalty,
        );
    }

    /// Apply `delta` to `recipient` and record the propagation.
    fn land(
        &mut self,
        target: &str,
        recipient: &TopicState,
        delta: f64,
        weight: f64,
        kind: PropagationKind,
    ) {
        let failed = kind == PropagationKind::Penalty;
        let (next, finite) = apply_update(recipient, delta, self.t_us, failed, self.cfg);
        self.note(target, finite);
        self.states.insert(target.to_owned(), next);
        self.props.push(Propagation {
            topic: target.to_owned(),
            raw_delta: delta,
            weight,
            kind,
        });
    }
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

#[cfg(test)]
mod tests {
    use super::super::testing::{T_US, graph, learned, topic};
    use super::*;

    /// The p.364 graph: `c` encompasses `a` at 0.8 and `b` at 0.6.
    fn p364() -> Curriculum {
        graph(vec![
            topic("a", &[]),
            topic("b", &[]),
            topic("c", &[("a", 0.8, true), ("b", 0.6, false)]),
        ])
    }

    /// Every topic of `ids` learned at memory 0.5.
    fn states_of(ids: &[&str]) -> BTreeMap<String, TopicState> {
        ids.iter()
            .map(|id| ((*id).to_owned(), learned(0.5)))
            .collect()
    }

    #[test]
    fn a_pass_credits_the_encompassed_topics_in_sorted_order() {
        let cfg = Config::default();
        let tree = p364();
        let states = states_of(&["a", "b", "c"]);
        let attempt = AttemptResult::new("c", true, WorkQuality::Perfect).with_assisted(false);
        let (next, props) = apply_attempt(&states, &attempt, &tree, &cfg, T_US);
        assert_eq!(next["c"].memory_base, 1.5);
        let order: Vec<&str> = props.iter().map(|prop| prop.topic.as_str()).collect();
        assert_eq!(order, ["a", "b"]);
        assert_eq!(props[0].kind.as_str(), "credit");
        assert_eq!(props[0].raw_delta, 0.8);
        assert!(knockout("c", "a", &tree, &cfg));
        assert!(!knockout("c", "b", &tree, &cfg));
    }

    #[test]
    fn a_miss_penalizes_a_practiced_dependent_and_skips_a_fresh_one() {
        let cfg = Config::default();
        let tree = p364();
        let states = states_of(&["a", "c"]);
        let attempt = AttemptResult::new("a", false, WorkQuality::Poor);
        let (next, props) = apply_attempt_checked(&states, &attempt, &tree, &cfg, T_US)
            .expect("a finite state folds");
        assert_eq!(props.len(), 1);
        assert_eq!(props[0].kind, PropagationKind::Penalty);
        assert!(next["c"].rep_num < 3.0);
        // `b` was never practiced, so it absorbs nothing and stays absent.
        let fresh = states_of(&["a"]);
        let (after, none) = apply_attempt(&fresh, &attempt, &tree, &cfg, T_US);
        assert!(none.is_empty());
        assert!(!after.contains_key("c"));
    }

    #[test]
    fn the_gates_drop_a_forced_explicit_and_a_tiny_credit() {
        let cfg = Config::default();
        let tree = graph(vec![
            topic("slow", &[]),
            topic("faint", &[]),
            topic("top", &[("slow", 0.9, true), ("faint", 0.01, false)]),
        ]);
        let mut states = states_of(&["slow", "faint", "top"]);
        states.get_mut("slow").expect("the state exists").speed = 0.1;
        let attempt = AttemptResult::new("top", true, WorkQuality::Perfect);
        let (_, props) = apply_attempt(&states, &attempt, &tree, &cfg, T_US);
        assert!(props.is_empty());
    }

    #[test]
    fn a_non_finite_decay_names_the_first_topic() {
        let cfg = Config::default();
        let tree = graph(vec![topic("a", &[])]);
        let mut far = learned(1.0);
        far.interval_days = 4.5;
        far.t0 = Some(crate::event::Timestamp::from_micros(
            T_US + 12_418 * 86_400_000_000,
        ));
        let states: BTreeMap<String, TopicState> = [("a".to_owned(), far)].into_iter().collect();
        let attempt = AttemptResult::new("a", true, WorkQuality::Perfect);
        let error = apply_attempt_checked(&states, &attempt, &tree, &cfg, T_US).unwrap_err();
        assert_eq!(error.topic, "a");
    }

    #[test]
    fn grade_review_weights_the_later_questions_more() {
        let cfg = Config::default();
        assert_eq!(grade_review(&[], &cfg), (false, 0.0));
        assert!(grade_review(&[false, true, true], &cfg).0);
        assert!(!grade_review(&[true, true, false], &cfg).0);
    }
}
