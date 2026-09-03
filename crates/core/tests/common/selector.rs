//! Builders the selector tests share: the 1.0 `_topic` / `_graph` / `_learned`
//! of `tests/test_selector.py`, the compositions with a quiet quiz, and the
//! reproducible random states of the property tests.

#![allow(clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use chrono::NaiveDate;

use cadus_core::config::Config;
use cadus_core::curriculum::model::{Exemplar, KnowledgePoint, PrereqEdge, Slug, Topic};
use cadus_core::curriculum::{AnswerKind, Curriculum, load_curriculum};
use cadus_core::event::{KpProgress, Timestamp, TopicStatus};
use cadus_core::learner::{PendingRemediation, QuizState, TopicState};
use cadus_core::selector::{SeededSampler, SessionContext, SessionPlan, compose_session};

use super::{T_US, knowledge_point};

/// The 1.0 `CFG = Config()` of every selector test.
#[must_use]
pub fn cfg() -> Config {
    Config::default()
}

/// One knowledge point with one exemplar, the 1.0 `_topic` default.
pub fn kp(id: &str, key_prerequisites: &[&str]) -> KnowledgePoint {
    let mut point = knowledge_point(id, key_prerequisites);
    point.exemplars = vec![Exemplar {
        problem: "p".to_owned(),
        answer: "a".to_owned(),
        solution_sketch: None,
    }];
    point
}

/// The 1.0 `_topic` builder (`tests/test_selector.py:69-95`).
pub struct TopicSpec {
    id: String,
    prereqs: Vec<(String, f64, bool)>,
    core: bool,
    drill: bool,
    expected: i64,
    kps: Vec<KnowledgePoint>,
    extra: Vec<(String, f64)>,
}

impl TopicSpec {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_owned(),
            prereqs: Vec::new(),
            core: true,
            drill: false,
            expected: 30,
            kps: vec![kp("kp1", &[])],
            extra: Vec::new(),
        }
    }

    pub fn prereqs(mut self, prereqs: &[(&str, f64, bool)]) -> Self {
        self.prereqs = prereqs
            .iter()
            .map(|&(id, weight, key)| (id.to_owned(), weight, key))
            .collect();
        self
    }

    pub fn drill(mut self, drill: bool) -> Self {
        self.drill = drill;
        self
    }

    pub fn core(mut self, core: bool) -> Self {
        self.core = core;
        self
    }

    pub fn expected(mut self, expected: i64) -> Self {
        self.expected = expected;
        self
    }

    pub fn kps(mut self, kps: Vec<KnowledgePoint>) -> Self {
        self.kps = kps;
        self
    }

    pub fn build(self) -> Topic {
        Topic {
            id: Slug::new(&self.id).unwrap(),
            name: self.id.clone(),
            core: self.core,
            difficulty: 0.3,
            drill: self.drill,
            answer_kind: AnswerKind::Numeric,
            expected_time_secs: self.expected,
            prerequisites: self
                .prereqs
                .iter()
                .map(|(id, weight, key)| PrereqEdge {
                    id: Slug::new(id).unwrap(),
                    weight: *weight,
                    key: *key,
                })
                .collect(),
            encompassings_extra: self
                .extra
                .iter()
                .map(|(id, weight)| PrereqEdge {
                    id: Slug::new(id).unwrap(),
                    weight: *weight,
                    key: false,
                })
                .collect(),
            knowledge_points: self.kps,
            diagnostic_exemplar: None,
            anki_seeds: Vec::new(),
        }
    }
}

/// A topic of module `M`, the 1.0 `_graph` default.
pub fn topic(id: &str) -> TopicSpec {
    TopicSpec::new(id)
}

/// The 1.0 `_graph` builder: one course `c`, every topic in module `M` unless
/// `modules` names another. The module order is the first-seen topic order.
pub fn graph_of(topics: Vec<Topic>, modules: &[(&str, &str)]) -> Curriculum {
    let module_of = |id: &str| -> String {
        modules
            .iter()
            .find(|(tid, _)| *tid == id)
            .map_or_else(|| "M".to_owned(), |(_, module)| (*module).to_owned())
    };
    let mut order: Vec<String> = Vec::new();
    let mut grouped: BTreeMap<String, Vec<Topic>> = BTreeMap::new();
    for tp in topics {
        let module = module_of(tp.id.as_str());
        if !grouped.contains_key(&module) {
            order.push(module.clone());
        }
        grouped.entry(module).or_default().push(tp);
    }
    let units: Vec<(&str, Vec<Topic>)> = order
        .iter()
        .map(|module| {
            (
                module.as_str(),
                grouped.get(module).cloned().unwrap_or_default(),
            )
        })
        .collect();
    super::graph_of_units(&units, "c")
}

/// The 1.0 `_learned` builder (`tests/test_selector.py:107-129`): a state whose
/// `memory_at(T)` equals `memory`.
#[derive(Debug, Clone)]
pub struct LearnedSpec {
    memory: f64,
    status: TopicStatus,
    rep: f64,
    ability: f64,
    speed: f64,
    interval: f64,
    t0_us: i64,
    kp_progress: BTreeMap<String, KpProgress>,
}

impl LearnedSpec {
    pub fn new(memory: f64) -> Self {
        Self {
            memory,
            status: TopicStatus::Learning,
            rep: 3.0,
            ability: 0.6,
            speed: 1.2,
            interval: 10.0,
            t0_us: T_US,
            kp_progress: BTreeMap::new(),
        }
    }

    pub fn status(mut self, status: TopicStatus) -> Self {
        self.status = status;
        self
    }

    pub fn ability(mut self, ability: f64) -> Self {
        self.ability = ability;
        self
    }

    pub fn t0(mut self, t0_us: i64) -> Self {
        self.t0_us = t0_us;
        self
    }

    pub fn build(self) -> TopicState {
        TopicState {
            status: self.status,
            rep_num: self.rep,
            memory_base: self.memory,
            t0: Some(Timestamp::from_micros(self.t0_us)),
            interval_days: self.interval,
            ability: self.ability,
            speed: self.speed,
            kp_progress: self.kp_progress,
            ..TopicState::default()
        }
    }
}

/// The 1.0 `_learned(memory)` call with every default.
pub fn learned(memory: f64) -> TopicState {
    LearnedSpec::new(memory).build()
}

/// A frontier topic that failed its lesson at `failed_at`.
pub fn failed_lesson(failed_at_us: i64) -> TopicState {
    let mut kp_progress = BTreeMap::new();
    kp_progress.insert("kp1".to_owned(), KpProgress::FailedOnce);
    TopicState {
        status: TopicStatus::Frontier,
        t0: Some(Timestamp::from_micros(failed_at_us)),
        kp_progress,
        ..TopicState::default()
    }
}

/// A states map from `(id, state)` pairs.
pub fn states_of(pairs: Vec<(&str, TopicState)>) -> BTreeMap<String, TopicState> {
    pairs
        .into_iter()
        .map(|(id, state)| (id.to_owned(), state))
        .collect()
}

/// The id list of a slice of ids.
pub fn ids(values: &[&str]) -> Vec<String> {
    values.iter().map(|id| (*id).to_owned()).collect()
}

/// An id set from a slice of ids.
pub fn id_set(values: &[&str]) -> BTreeSet<String> {
    values.iter().map(|id| (*id).to_owned()).collect()
}

/// The 1.0 `seeded_rng(seed)` stand-in. It is 2.0's own generator (trap T11).
pub fn sampler(seed: u64) -> SeededSampler {
    SeededSampler::new(seed)
}

/// A quiz state that suppresses the quiz, the way the 1.0 tests pass
/// `QuizState(last_at=T.date())`.
pub fn quiz_quiet() -> QuizState {
    QuizState {
        last_at: Some(NaiveDate::from_ymd_opt(2026, 7, 14).unwrap()),
        xp_since: 0,
        retake_pending: false,
    }
}

/// The topic ids of the tasks of a plan, in serve order.
pub fn plan_topics(plan: &SessionPlan) -> Vec<String> {
    plan.tasks
        .iter()
        .filter_map(|task| task.topic.clone())
        .collect()
}

/// The task kinds of a plan, in serve order.
pub fn plan_kinds(plan: &SessionPlan) -> Vec<&'static str> {
    plan.tasks
        .iter()
        .map(|task| task.task_type.as_str())
        .collect()
}

/// `count` topics `{prefix}{index}` that each need `root` at `weight`, non-key.
#[must_use]
pub fn fan(prefix: &str, count: usize, weight: f64) -> Vec<Topic> {
    (0..count)
        .map(|index| {
            topic(&format!("{prefix}{index}"))
                .prereqs(&[("root", weight, false)])
                .build()
        })
        .collect()
}

/// A learned `root` at memory 0.9 and `count` due topics `r{index}` at 0.5.
#[must_use]
pub fn star_states(count: usize) -> BTreeMap<String, TopicState> {
    let mut states = states_of(vec![("root", learned(0.9))]);
    for index in 0..count {
        states.insert(format!("r{index}"), learned(0.5));
    }
    states
}

/// Compose at `T` with sampler seed 1 and a quiet quiz, the 1.0 default call.
#[must_use]
pub fn compose_quiet(states: &BTreeMap<String, TopicState>, graph: &Curriculum) -> SessionPlan {
    let quiz = quiz_quiet();
    compose_with(
        states,
        graph,
        SessionContext::default().with_quiz_state(Some(&quiz)),
    )
}

/// Compose at `T` with sampler seed 1 and the given context.
#[must_use]
pub fn compose_with(
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    ctx: SessionContext<'_>,
) -> SessionPlan {
    compose_session(states, graph, &cfg(), T_US, &mut sampler(1), &ctx)
}

/// Compose with a quiet quiz and `pending` as the remediation queue.
#[must_use]
pub fn compose_with_remediation(
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    pending: &[PendingRemediation],
) -> SessionPlan {
    let quiz = quiz_quiet();
    compose_with(
        states,
        graph,
        SessionContext::default()
            .with_pending_remediation(pending)
            .with_quiz_state(Some(&quiz)),
    )
}

/// The `base -> lesson` graph whose lesson failed half a day ago, so the
/// frontier is blocked until `failed_at + 1 day`.
#[must_use]
pub fn blocked_frontier() -> (Curriculum, BTreeMap<String, TopicState>, i64) {
    let graph = graph_of(
        vec![
            topic("base").build(),
            topic("lesson").prereqs(&[("base", 0.9, true)]).build(),
        ],
        &[],
    );
    let failed_at = T_US - super::days(1) / 2;
    let states = states_of(vec![
        ("base", learned(0.55)),
        ("lesson", failed_lesson(failed_at)),
    ]);
    (graph, states, failed_at)
}

/// Ten topics `t0` to `t9`, all learned at memory 0.8.
#[must_use]
pub fn ten_learned() -> (Curriculum, BTreeMap<String, TopicState>) {
    let all: Vec<String> = (0..10).map(|index| format!("t{index}")).collect();
    let graph = graph_of(all.iter().map(|id| topic(id).build()).collect(), &[]);
    let states = all.iter().map(|id| (id.clone(), learned(0.8))).collect();
    (graph, states)
}

/// A SplitMix64 generator, so a property runs on REPRODUCIBLE states.
pub struct Rng {
    state: u64,
}

/// The output function of SplitMix64 over one state word.
fn splitmix_output(state: u64) -> u64 {
    let z = (state ^ (state >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    let z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl Rng {
    /// A generator seeded with `seed`.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// The next 64 raw bits.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        splitmix_output(self.state)
    }

    /// A float in `[0, 1)`.
    #[allow(clippy::cast_precision_loss)]
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1_u64 << 53) as f64
    }

    /// One fair coin.
    pub fn flip(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }
}

/// The checked-in curriculum tree.
#[must_use]
pub fn curriculum_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../curriculum")
}

/// The arena of the checked-in curriculum tree.
#[must_use]
pub fn real_curriculum() -> Curriculum {
    load_curriculum(&curriculum_root())
        .expect("the curriculum tree loads")
        .0
}

/// The first 60 topic ids of the real graph: a bounded slice, so one example
/// stays cheap while the encompassing weights stay real.
#[must_use]
pub fn pool_of(graph: &Curriculum) -> Vec<String> {
    graph
        .topics()
        .iter()
        .take(60)
        .map(|topic| topic.id.as_str().to_owned())
        .collect()
}

/// A random learned state on about half of `pool`, at a random memory.
pub fn random_states(pool: &[String], rng: &mut Rng) -> BTreeMap<String, TopicState> {
    let mut states: BTreeMap<String, TopicState> = BTreeMap::new();
    for tid in pool {
        if rng.flip() {
            states.insert(tid.clone(), learned(rng.unit()));
        }
    }
    states
}

/// The learner state of the L1 benchmark: 300 learned topics of the real
/// curriculum, a third of them due.
#[must_use]
pub fn bench_states(graph: &Curriculum) -> BTreeMap<String, TopicState> {
    let mut states = BTreeMap::new();
    for (index, topic) in graph.topics().iter().take(300).enumerate() {
        let memory = match index % 3 {
            0 => 0.4,  // due
            1 => 0.55, // nearly due
            _ => 0.9,  // on schedule
        };
        states.insert(topic.id.as_str().to_owned(), learned(memory));
    }
    states
}
