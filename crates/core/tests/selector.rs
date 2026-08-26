//! The selector of spec section 6, pinned against the ORDER assertions of the
//! 1.0 suite (`tests/test_selector.py`, `tests/test_gap_fill.py`).
//!
//! Every expected value here is a LITERAL taken from the 1.0 test that pins it.
//! Nothing is re-derived from the code under test.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]
#![allow(clippy::too_many_lines)]

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use chrono::NaiveDate;

use cadus_core::config::Config;
use cadus_core::curriculum::load::{RawCurriculum, RawUnit};
use cadus_core::curriculum::model::{
    Catalog, Course, Exemplar, KnowledgePoint, PrereqEdge, Slug, Topic, Unit,
};
use cadus_core::curriculum::{AnswerKind, Curriculum, load_curriculum};
use cadus_core::event::{KpProgress, TaskType, Timestamp, TopicStatus};
use cadus_core::learner::{PendingRemediation, QuizState, TopicState};
use cadus_core::selector::{
    DIFFICULTY_TARGET, DRILL_INTERVAL_DAYS, DRILL_MASTERY_ABILITY, QUIZ_RECENT_DAYS,
    QUIZ_RETAKE_DELAY_DAYS, REMEDIATION_QUIZ_MISS, SeededSampler, SessionContext, SessionPlan,
    Task, arrange_lessons, blocking_gap_ancestors, compose_session, compress, course_scope,
    due_reviews, frontier, gap_course_for, gap_fill_chain_for_stack, importance, in_retry_delay,
    interleave, mastered_set, multistep_components, multistep_is_due, nearly_due, order_lessons,
    quiz_budget, quiz_composer, quiz_difficulty_target, quiz_is_due, remediation_for_quiz_miss,
    remediation_for_repeat_fail, resolve_gap_fill_stack, review_mix, schedule_drills,
    serveable_gap_frontier,
};

use common::{DAY_US, T_US, days};

/// The 1.0 `CFG = Config()` of every selector test.
fn cfg() -> Config {
    Config::default()
}

// --------------------------------------------------------------------------- //
// Builders — the 1.0 `_topic` / `_graph` / `_learned` of `tests/test_selector.py`
// --------------------------------------------------------------------------- //

/// One knowledge point with one exemplar, the 1.0 `_topic` default.
fn kp(id: &str, key_prerequisites: &[&str]) -> KnowledgePoint {
    KnowledgePoint {
        id: Slug::new(id).unwrap(),
        name: id.to_owned(),
        key_prerequisites: key_prerequisites
            .iter()
            .map(|key| Slug::new(key).unwrap())
            .collect(),
        exemplars: vec![Exemplar {
            problem: "p".to_owned(),
            answer: "a".to_owned(),
            solution_sketch: None,
        }],
        constraints: None,
    }
}

/// The 1.0 `_topic` builder (`tests/test_selector.py:69-95`).
struct TopicSpec {
    id: String,
    prereqs: Vec<(String, f64, bool)>,
    core: bool,
    drill: bool,
    expected: i64,
    kps: Vec<KnowledgePoint>,
    extra: Vec<(String, f64)>,
}

impl TopicSpec {
    fn new(id: &str) -> Self {
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

    fn prereqs(mut self, prereqs: &[(&str, f64, bool)]) -> Self {
        self.prereqs = prereqs
            .iter()
            .map(|&(id, weight, key)| (id.to_owned(), weight, key))
            .collect();
        self
    }

    fn drill(mut self, drill: bool) -> Self {
        self.drill = drill;
        self
    }

    fn core(mut self, core: bool) -> Self {
        self.core = core;
        self
    }

    fn expected(mut self, expected: i64) -> Self {
        self.expected = expected;
        self
    }

    fn kps(mut self, kps: Vec<KnowledgePoint>) -> Self {
        self.kps = kps;
        self
    }

    fn build(self) -> Topic {
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
fn topic(id: &str) -> TopicSpec {
    TopicSpec::new(id)
}

/// The 1.0 `_graph` builder: one course `c`, every topic in module `M` unless
/// `modules` names another. The module order is the first-seen topic order.
fn graph_of(topics: Vec<Topic>, modules: &[(&str, &str)]) -> Curriculum {
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
    common::graph_of_units(&units, "c")
}

/// The 1.0 `_learned` builder (`tests/test_selector.py:107-129`): a state whose
/// `memory_at(T)` equals `memory`.
#[derive(Debug, Clone)]
struct LearnedSpec {
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
    fn new(memory: f64) -> Self {
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

    fn status(mut self, status: TopicStatus) -> Self {
        self.status = status;
        self
    }

    fn ability(mut self, ability: f64) -> Self {
        self.ability = ability;
        self
    }

    fn t0(mut self, t0_us: i64) -> Self {
        self.t0_us = t0_us;
        self
    }

    fn build(self) -> TopicState {
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
fn learned(memory: f64) -> TopicState {
    LearnedSpec::new(memory).build()
}

/// A frontier topic that failed its lesson at `failed_at`.
fn failed_lesson(failed_at_us: i64) -> TopicState {
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
fn states_of(pairs: Vec<(&str, TopicState)>) -> BTreeMap<String, TopicState> {
    pairs
        .into_iter()
        .map(|(id, state)| (id.to_owned(), state))
        .collect()
}

/// The id list of a slice of ids.
fn ids(values: &[&str]) -> Vec<String> {
    values.iter().map(|id| (*id).to_owned()).collect()
}

/// An id set from a slice of ids.
fn id_set(values: &[&str]) -> BTreeSet<String> {
    values.iter().map(|id| (*id).to_owned()).collect()
}

/// The 1.0 `seeded_rng(seed)` stand-in. It is 2.0's own generator (trap T11).
fn sampler(seed: u64) -> SeededSampler {
    SeededSampler::new(seed)
}

/// A quiz state that suppresses the quiz, the way the 1.0 tests pass
/// `QuizState(last_at=T.date())`.
fn quiz_quiet() -> QuizState {
    QuizState {
        last_at: Some(NaiveDate::from_ymd_opt(2026, 7, 14).unwrap()),
        xp_since: 0,
        retake_pending: false,
    }
}

/// The topic ids of the tasks of a plan, in serve order.
fn plan_topics(plan: &SessionPlan) -> Vec<String> {
    plan.tasks
        .iter()
        .filter_map(|task| task.topic.clone())
        .collect()
}

/// The task kinds of a plan, in serve order.
fn plan_kinds(plan: &SessionPlan) -> Vec<&'static str> {
    plan.tasks
        .iter()
        .map(|task| task.task_type.as_str())
        .collect()
}

// --------------------------------------------------------------------------- //
// due_reviews (test_selector.py:135-157)
// --------------------------------------------------------------------------- //

#[test]
fn due_reviews_restricts_to_review_history() {
    let graph = graph_of(
        vec![
            topic("root").build(),
            topic("a").prereqs(&[("root", 1.0, true)]).build(),
            topic("b").prereqs(&[("root", 1.0, true)]).build(),
        ],
        &[],
    );
    let states = states_of(vec![
        (
            "root",
            LearnedSpec::new(0.4).status(TopicStatus::Floor).build(),
        ),
        ("a", LearnedSpec::new(0.4).build()),
        (
            "b",
            TopicState {
                status: TopicStatus::Untouched,
                rep_num: 3.0,
                ..TopicState::default()
            },
        ),
    ]);
    // `tests/test_selector.py:151`.
    assert_eq!(
        due_reviews(&states, &graph, &cfg(), T_US, &BTreeSet::new()),
        ids(&["a"])
    );
}

#[test]
fn due_reviews_excludes_not_due() {
    let graph = graph_of(vec![topic("a").build()], &[]);
    // `tests/test_selector.py:156-157`.
    assert_eq!(
        due_reviews(
            &states_of(vec![("a", learned(0.7))]),
            &graph,
            &cfg(),
            T_US,
            &BTreeSet::new()
        ),
        Vec::<String>::new()
    );
    assert_eq!(
        due_reviews(
            &states_of(vec![("a", learned(0.5))]),
            &graph,
            &cfg(),
            T_US,
            &BTreeSet::new()
        ),
        ids(&["a"])
    );
}

// --------------------------------------------------------------------------- //
// compress (test_selector.py:165-190)
// --------------------------------------------------------------------------- //

#[test]
fn compress_free_knockout_by_frontier_lesson() {
    let graph = graph_of(
        vec![
            topic("child").build(),
            topic("parent").prereqs(&[("child", 0.9, true)]).build(),
        ],
        &[],
    );
    let states = states_of(vec![("child", learned(0.5))]);
    let comp = compress(&ids(&["child"]), &states, &graph, &cfg(), T_US, None);
    // `tests/test_selector.py:171-172`.
    assert_eq!(comp.surviving, Vec::<String>::new());
    assert_eq!(comp.knockouts.get("parent"), Some(&ids(&["child"])));
}

#[test]
fn compress_due_topic_dominoes_another() {
    let graph = graph_of(
        vec![
            topic("add").build(),
            topic("sub").prereqs(&[("add", 0.8, true)]).build(),
        ],
        &[],
    );
    let states = states_of(vec![("add", learned(0.5)), ("sub", learned(0.5))]);
    let comp = compress(&ids(&["add", "sub"]), &states, &graph, &cfg(), T_US, None);
    // `tests/test_selector.py:181-182`.
    assert_eq!(comp.surviving, ids(&["sub"]));
    assert_eq!(comp.knockouts.get("sub"), Some(&ids(&["add"])));
}

#[test]
fn compress_independent_reviews_all_survive() {
    let mut topics = vec![topic("root").build()];
    for index in 0..4 {
        topics.push(
            topic(&format!("r{index}"))
                .prereqs(&[("root", 0.3, false)])
                .build(),
        );
    }
    let graph = graph_of(topics, &[]);
    let mut pairs = vec![("root".to_owned(), learned(0.9))];
    for index in 0..4 {
        pairs.push((format!("r{index}"), learned(0.5)));
    }
    let states: BTreeMap<String, TopicState> = pairs.into_iter().collect();
    let due = ids(&["r0", "r1", "r2", "r3"]);
    let comp = compress(&due, &states, &graph, &cfg(), T_US, None);
    // `tests/test_selector.py:189-190`.
    assert_eq!(comp.surviving, due);
    assert_eq!(comp.knockouts, BTreeMap::new());
}

#[test]
fn blocked_lesson_does_not_absorb_a_due_review() {
    let graph = graph_of(
        vec![
            topic("child").build(),
            topic("lesson").prereqs(&[("child", 0.9, true)]).build(),
        ],
        &[],
    );
    let failed_at = T_US - days(1) / 2;
    let states = states_of(vec![
        ("child", learned(0.5)),
        ("lesson", failed_lesson(failed_at)),
    ]);
    let comp = compress(&ids(&["child"]), &states, &graph, &cfg(), T_US, None);
    // `tests/test_selector.py:441`.
    assert_eq!(comp.surviving, ids(&["child"]));

    let quiz = quiz_quiet();
    let plan = compose_session(
        &states,
        &graph,
        &cfg(),
        T_US,
        &mut sampler(1),
        &SessionContext::default().with_quiz_state(Some(&quiz)),
    );
    assert!(plan.tasks.iter().any(|task| {
        task.topic.as_deref() == Some("child") && task.task_type == TaskType::Review
    }));
}

// --------------------------------------------------------------------------- //
// The compression property (test_selector.py:200-222)
// --------------------------------------------------------------------------- //

/// A SplitMix64 generator, so the property runs on 200 REPRODUCIBLE states.
struct Rng {
    state: u64,
}

impl Rng {
    const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1_u64 << 53) as f64
    }

    fn flip(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }
}

fn curriculum_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../curriculum")
}

fn real_curriculum() -> Curriculum {
    load_curriculum(&curriculum_root())
        .expect("the curriculum tree loads")
        .0
}

#[test]
fn property_knockout_leaves_no_due_topic_uncovered() {
    let graph = real_curriculum();
    let cfg = cfg();
    // A bounded slice of the real graph, so one example stays cheap while the
    // encompassing weights stay real.
    let pool: Vec<String> = graph
        .topics()
        .iter()
        .take(60)
        .map(|topic| topic.id.as_str().to_owned())
        .collect();
    let mut rng = Rng::new(20_260_826);
    for _ in 0..200 {
        let mut states: BTreeMap<String, TopicState> = BTreeMap::new();
        for tid in &pool {
            if rng.flip() {
                states.insert(tid.clone(), learned(rng.unit()));
            }
        }
        let due: BTreeSet<String> = due_reviews(&states, &graph, &cfg, T_US, &BTreeSet::new())
            .into_iter()
            .collect();
        let comp = compress(
            &due.iter().cloned().collect::<Vec<String>>(),
            &states,
            &graph,
            &cfg,
            T_US,
            None,
        );
        let surviving: BTreeSet<String> = comp.surviving.iter().cloned().collect();
        let knocked: BTreeSet<String> = comp
            .knockouts
            .values()
            .flat_map(|list| list.iter().cloned())
            .collect();
        let union: BTreeSet<String> = surviving.union(&knocked).cloned().collect();
        assert_eq!(union, due, "surviving union knocked must equal due");
        assert!(
            surviving.is_disjoint(&knocked),
            "surviving and knocked must be disjoint"
        );
        assert!(surviving.is_subset(&due), "surviving must be due");
        assert!(knocked.is_subset(&due), "knocked must be due");
    }
}

// --------------------------------------------------------------------------- //
// Throttle, importance, review mix (test_selector.py:242-286)
// --------------------------------------------------------------------------- //

#[test]
fn backlog_yields_at_least_one_lesson_per_three_reviews() {
    let mut topics = vec![topic("root").build()];
    for index in 0..9 {
        topics.push(
            topic(&format!("r{index}"))
                .prereqs(&[("root", 0.3, false)])
                .build(),
        );
    }
    for index in 0..5 {
        topics.push(
            topic(&format!("l{index}"))
                .prereqs(&[("root", 0.3, false)])
                .build(),
        );
    }
    let graph = graph_of(topics, &[]);
    let mut states = states_of(vec![("root", learned(0.9))]);
    for index in 0..9 {
        states.insert(format!("r{index}"), learned(0.5));
    }
    let cfg = cfg();
    let quiz = quiz_quiet();
    let plan = compose_session(
        &states,
        &graph,
        &cfg,
        T_US,
        &mut sampler(1),
        &SessionContext::default().with_quiz_state(Some(&quiz)),
    );
    let kinds: Vec<&str> = plan_kinds(&plan)
        .into_iter()
        .filter(|kind| *kind == "review" || *kind == "lesson")
        .collect();
    let mut run = 0_i64;
    let mut maxrun = 0_i64;
    for kind in &kinds {
        run = if *kind == "review" { run + 1 } else { 0 };
        maxrun = maxrun.max(run);
    }
    // `tests/test_selector.py:253-256`.
    assert!(maxrun <= cfg.selector.max_reviews_per_lesson);
    assert!(plan.constraints.throttle_ok);
    assert!(plan.constraints.lesson_ratio_ok);
    assert!(kinds.iter().filter(|kind| **kind == "lesson").count() >= 3);
}

#[test]
fn order_lessons_prefers_knockout_mass() {
    let graph = graph_of(
        vec![
            topic("root").build(),
            topic("child").prereqs(&[("root", 0.3, false)]).build(),
            topic("la").prereqs(&[("child", 0.9, true)]).build(),
            topic("lb").prereqs(&[("root", 0.9, true)]).build(),
        ],
        &[],
    );
    let scope = course_scope(&graph, None);
    let ordered = order_lessons(&ids(&["lb", "la"]), &graph, &id_set(&["child"]), &scope);
    // `tests/test_selector.py:275`.
    assert_eq!(ordered.first().map(String::as_str), Some("la"));
}

#[test]
fn review_mix_spans_kps_and_component_skills() {
    let graph = graph_of(
        vec![
            topic("multiplication").build(),
            topic("equivalent-fractions").build(),
            topic("adding-fractions")
                .prereqs(&[("equivalent-fractions", 0.8, true)])
                .kps(vec![kp("kp1", &["multiplication"]), kp("kp2", &[])])
                .build(),
        ],
        &[],
    );
    let mix = review_mix(&graph, "adding-fractions");
    // `tests/test_selector.py:286`.
    assert_eq!(mix.get(..2), Some(ids(&["kp1", "kp2"]).as_slice()));
    assert!(mix.contains(&"component:equivalent-fractions".to_owned()));
    assert!(mix.contains(&"component:multiplication".to_owned()));
}

// --------------------------------------------------------------------------- //
// Quiz strata and cadence (test_selector.py:296-327, 656-739)
// --------------------------------------------------------------------------- //

#[test]
fn quiz_composer_strata_and_time_budget() {
    let recent_ids = ["counting", "addition", "subtraction", "multiplication"];
    let mid_ids = ["division", "place-value"];
    let old_ids = ["fraction-basics", "equivalent-fractions"];
    let all: Vec<&str> = recent_ids
        .iter()
        .chain(mid_ids.iter())
        .chain(old_ids.iter())
        .copied()
        .collect();
    let graph = graph_of(
        all.iter()
            .map(|id| topic(id).build())
            .collect::<Vec<Topic>>(),
        &[],
    );
    let states: BTreeMap<String, TopicState> = all
        .iter()
        .map(|id| ((*id).to_owned(), learned(0.8)))
        .collect();
    let mut learned_at: BTreeMap<String, i64> = BTreeMap::new();
    for id in recent_ids {
        learned_at.insert(id.to_owned(), T_US - days(5));
    }
    for id in mid_ids {
        learned_at.insert(id.to_owned(), T_US - days(30));
    }
    for id in old_ids {
        learned_at.insert(id.to_owned(), T_US - days(60));
    }
    let cfg = cfg();
    let plan = quiz_composer(
        &states,
        &graph,
        &cfg,
        T_US,
        &mut sampler(7),
        Some(&learned_at),
    );
    assert_eq!(
        i64::try_from(plan.questions.len()).unwrap(),
        cfg.quiz.questions
    );
    let mut by_stratum: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    for question in &plan.questions {
        by_stratum
            .entry(question.stratum)
            .or_default()
            .insert(question.topic.clone());
        // The authored expected time is 30 s, so the budget is 45 s.
        assert_eq!(question.time_budget_secs, 45);
    }
    assert_eq!(by_stratum.get("recent").map(BTreeSet::len), Some(4));
    assert_eq!(by_stratum.get("mid").map(BTreeSet::len), Some(2));
    assert_eq!(by_stratum.get("old").map(BTreeSet::len), Some(2));
    assert!(
        by_stratum
            .get("recent")
            .unwrap()
            .is_subset(&id_set(&recent_ids))
    );
    assert!(by_stratum.get("mid").unwrap().is_subset(&id_set(&mid_ids)));
    assert!(by_stratum.get("old").unwrap().is_subset(&id_set(&old_ids)));
}

#[test]
fn quiz_sampling_is_seed_deterministic() {
    let all: Vec<String> = (0..10).map(|index| format!("t{index}")).collect();
    let graph = graph_of(all.iter().map(|id| topic(id).build()).collect(), &[]);
    let states: BTreeMap<String, TopicState> =
        all.iter().map(|id| (id.clone(), learned(0.8))).collect();
    let cfg = cfg();
    let first = quiz_composer(&states, &graph, &cfg, T_US, &mut sampler(42), None);
    let second = quiz_composer(&states, &graph, &cfg, T_US, &mut sampler(42), None);
    // `tests/test_selector.py:322-327`, with 2.0's own generator (trap T11).
    assert_eq!(first.topics(), second.topics());
    assert_eq!(first.questions.len(), 8);
}

#[test]
fn failed_quiz_retake_respects_delay() {
    let all: Vec<String> = (0..10).map(|index| format!("t{index}")).collect();
    let graph = graph_of(all.iter().map(|id| topic(id).build()).collect(), &[]);
    let states: BTreeMap<String, TopicState> =
        all.iter().map(|id| (id.clone(), learned(0.8))).collect();
    let cfg = cfg();
    let failed_day = NaiveDate::from_ymd_opt(2026, 7, 14).unwrap();
    let failed = QuizState {
        last_at: Some(failed_day),
        xp_since: 0,
        retake_pending: true,
    };
    // `tests/test_selector.py:656-669`.
    assert!(!quiz_is_due(
        Some(&failed),
        &states,
        &graph,
        &cfg,
        T_US,
        None
    ));
    assert!(!quiz_is_due(
        Some(&failed),
        &states,
        &graph,
        &cfg,
        T_US + 6 * 3_600_000_000,
        None
    ));
    assert!(quiz_is_due(
        Some(&failed),
        &states,
        &graph,
        &cfg,
        T_US + days(QUIZ_RETAKE_DELAY_DAYS) + 3_600_000_000,
        None
    ));
    let ok = QuizState {
        last_at: Some(failed_day),
        xp_since: 0,
        retake_pending: false,
    };
    assert!(!quiz_is_due(
        Some(&ok),
        &states,
        &graph,
        &cfg,
        T_US + days(QUIZ_RETAKE_DELAY_DAYS) + 3_600_000_000,
        None
    ));
}

#[test]
fn quiz_difficulty_adapts_up_within_a_bounded_band() {
    // `tests/test_selector.py:693-698`.
    let base = quiz_difficulty_target(0);
    let once = quiz_difficulty_target(1);
    let twice = quiz_difficulty_target(2);
    assert_eq!(base, DIFFICULTY_TARGET);
    assert_eq!(base, "80-85% expected accuracy");
    assert_eq!(once, "75-80% expected accuracy");
    assert_eq!(twice, "70-75% expected accuracy");
    assert_eq!(quiz_difficulty_target(99), twice);
    assert_eq!(quiz_difficulty_target(-5), base);
}

#[test]
fn quiz_cadence_uses_activity_days() {
    let all: Vec<String> = (0..10).map(|index| format!("t{index}")).collect();
    let graph = graph_of(all.iter().map(|id| topic(id).build()).collect(), &[]);
    let states: BTreeMap<String, TopicState> =
        all.iter().map(|id| (id.clone(), learned(0.8))).collect();
    let cfg = cfg();
    let last = NaiveDate::from_ymd_opt(2026, 7, 1).unwrap();
    let quiz = QuizState {
        last_at: Some(last),
        xp_since: 0,
        retake_pending: false,
    };
    let now = common::noon_us(2026, 7, 30);
    // `tests/test_selector.py:722-739`.
    assert!(quiz_is_due(Some(&quiz), &states, &graph, &cfg, now, None));
    let few = [
        NaiveDate::from_ymd_opt(2026, 7, 5).unwrap(),
        NaiveDate::from_ymd_opt(2026, 7, 12).unwrap(),
        NaiveDate::from_ymd_opt(2026, 7, 20).unwrap(),
    ];
    assert!(!quiz_is_due(
        Some(&quiz),
        &states,
        &graph,
        &cfg,
        now,
        Some(&few)
    ));
    let enough: Vec<NaiveDate> = (2..2 + u32::try_from(cfg.quiz.cadence_days).unwrap())
        .map(|day| NaiveDate::from_ymd_opt(2026, 7, day).unwrap())
        .collect();
    assert_eq!(i64::try_from(enough.len()).unwrap(), cfg.quiz.cadence_days);
    assert!(quiz_is_due(
        Some(&quiz),
        &states,
        &graph,
        &cfg,
        now,
        Some(&enough)
    ));
    let repeats = vec![NaiveDate::from_ymd_opt(2026, 7, 5).unwrap(); 10];
    assert!(!quiz_is_due(
        Some(&quiz),
        &states,
        &graph,
        &cfg,
        now,
        Some(&repeats)
    ));
    let mut out_of_window = vec![last];
    for day in 1..12 {
        out_of_window.push(NaiveDate::from_ymd_opt(2026, 8, day).unwrap());
    }
    assert!(!quiz_is_due(
        Some(&quiz),
        &states,
        &graph,
        &cfg,
        now,
        Some(&out_of_window)
    ));
}

// --------------------------------------------------------------------------- //
// Module interleaving (test_selector.py:335-353)
// --------------------------------------------------------------------------- //

#[test]
fn no_two_consecutive_lessons_from_the_same_module() {
    let graph = graph_of(
        vec![
            topic("root").build(),
            topic("a1").build(),
            topic("a2").build(),
            topic("z1").build(),
        ],
        &[
            ("root", "Numbers"),
            ("a1", "Numbers"),
            ("a2", "Numbers"),
            ("z1", "Fractions"),
        ],
    );
    let mut states = states_of(vec![(
        "root",
        LearnedSpec::new(0.9).status(TopicStatus::Floor).build(),
    )]);
    for tid in ["a1", "a2", "z1"] {
        states.insert(tid.to_owned(), TopicState::default());
    }
    let quiz = quiz_quiet();
    let plan = compose_session(
        &states,
        &graph,
        &cfg(),
        T_US,
        &mut sampler(1),
        &SessionContext::default().with_quiz_state(Some(&quiz)),
    );
    let modules: Vec<&str> = plan
        .tasks
        .iter()
        .filter(|task| task.task_type == TaskType::Lesson)
        .filter_map(|task| task.topic.as_deref())
        .map(|tid| graph.module_of(graph.idx_of(tid).unwrap()))
        .collect();
    // `tests/test_selector.py:352-353`. The two Numbers lessons rank ahead of the
    // Fractions one, so a selector that ignored the last module used would serve
    // them back to back.
    assert_eq!(modules, vec!["Numbers", "Fractions", "Numbers"]);
}

#[test]
fn arrange_lessons_alternates_modules() {
    let graph = graph_of(
        vec![
            topic("na").build(),
            topic("nb").build(),
            topic("nc").build(),
            topic("fa").build(),
        ],
        &[
            ("na", "Numbers"),
            ("nb", "Numbers"),
            ("nc", "Numbers"),
            ("fa", "Fractions"),
        ],
    );
    // Three Numbers lessons and one Fractions lesson: the biggest module leads,
    // then the other module breaks the run.
    assert_eq!(
        arrange_lessons(&ids(&["na", "nb", "nc", "fa"]), &graph),
        ids(&["na", "fa", "nb", "nc"])
    );
}

#[test]
fn interleave_forces_a_lesson_after_three_reviews() {
    let cfg = cfg();
    let seq = interleave(
        &ids(&["r0", "r1", "r2", "r3", "r4"]),
        &ids(&["l0", "l1"]),
        &cfg,
    );
    let order: Vec<String> = seq.iter().map(|(_, tid)| tid.clone()).collect();
    assert_eq!(order, ids(&["r0", "r1", "r2", "l0", "r3", "r4", "l1"]));
}

// --------------------------------------------------------------------------- //
// Course complete and frontier blocked (test_selector.py:361-424)
// --------------------------------------------------------------------------- //

#[test]
fn course_complete_gives_an_empty_plan_and_the_flag() {
    let all: Vec<String> = (0..12).map(|index| format!("t{index:02}")).collect();
    let graph = graph_of(all.iter().map(|id| topic(id).build()).collect(), &[]);
    let states: BTreeMap<String, TopicState> = all
        .iter()
        .map(|id| (id.clone(), LearnedSpec::new(0.95).ability(0.99).build()))
        .collect();
    let quiz = quiz_quiet();
    let plan = compose_session(
        &states,
        &graph,
        &cfg(),
        T_US,
        &mut sampler(1),
        &SessionContext::default()
            .with_course(Some("c"))
            .with_quiz_state(Some(&quiz)),
    );
    // `tests/test_selector.py:389-398`.
    assert!(plan.course_complete);
    assert_eq!(plan.tasks, Vec::<Task>::new());
}

#[test]
fn frontier_blocked_serves_nearly_due_and_reports_until() {
    let graph = graph_of(
        vec![
            topic("base").build(),
            topic("lesson").prereqs(&[("base", 0.9, true)]).build(),
        ],
        &[],
    );
    let failed_at = T_US - days(1) / 2;
    let states = states_of(vec![
        ("base", learned(0.55)),
        ("lesson", failed_lesson(failed_at)),
    ]);
    let quiz = quiz_quiet();
    let plan = compose_session(
        &states,
        &graph,
        &cfg(),
        T_US,
        &mut sampler(1),
        &SessionContext::default().with_quiz_state(Some(&quiz)),
    );
    // `tests/test_selector.py:406-424`.
    assert_eq!(
        plan.frontier_blocked_until,
        Some(Timestamp::from_micros(failed_at + DAY_US))
    );
    assert!(!plan.course_complete);
    assert!(
        plan.tasks
            .iter()
            .all(|task| task.task_type != TaskType::Lesson)
    );
    assert_eq!(plan_topics(&plan), ids(&["base"]));
    let base = plan.tasks.first().unwrap();
    // `tests/test_selector.py:479-486`: the typed fact, not the prose.
    assert!(base.nearly_due);
    assert!(!base.is_remediation);
    assert!(base.why.contains("nearly-due review"));
}

// --------------------------------------------------------------------------- //
// Open-plan idempotency (test_selector.py:361-380, 488-501)
// --------------------------------------------------------------------------- //

#[test]
fn open_plan_reserves_minus_completed_keeping_ids() {
    let graph = graph_of(
        vec![
            topic("root").build(),
            topic("r0").prereqs(&[("root", 0.3, false)]).build(),
            topic("r1").prereqs(&[("root", 0.3, false)]).build(),
            topic("l0").prereqs(&[("root", 0.3, false)]).build(),
        ],
        &[],
    );
    let states = states_of(vec![
        ("root", learned(0.9)),
        ("r0", learned(0.5)),
        ("r1", learned(0.5)),
    ]);
    let quiz = quiz_quiet();
    let base = SessionContext::default()
        .with_session_id("s1")
        .with_quiz_state(Some(&quiz));
    let plan = compose_session(&states, &graph, &cfg(), T_US, &mut sampler(1), &base);
    let served: BTreeMap<String, String> = plan
        .tasks
        .iter()
        .filter_map(|task| task.topic.clone().map(|tid| (tid, task.task_id.clone())))
        .collect();
    // `tests/test_selector.py:370`.
    assert_eq!(
        served.keys().cloned().collect::<BTreeSet<String>>(),
        id_set(&["r0", "r1", "l0"])
    );
    assert_eq!(served.get("r0").map(String::as_str), Some("s1-review-r0"));
    assert_eq!(served.get("l0").map(String::as_str), Some("s1-lesson-l0"));

    let mut updated = states.clone();
    updated.insert("r0".to_owned(), learned(0.9));
    updated.insert("l0".to_owned(), learned(0.9));
    let reserved = compose_session(
        &updated,
        &graph,
        &cfg(),
        T_US,
        &mut sampler(1),
        &base.with_open_plan(Some(&plan)),
    );
    // `tests/test_selector.py:376-380`.
    assert_eq!(plan_topics(&reserved), ids(&["r1"]));
    assert_eq!(
        reserved.tasks.first().map(|task| task.task_id.as_str()),
        served.get("r1").map(String::as_str)
    );
    assert_eq!(reserved.session, plan.session);
}

#[test]
fn nearly_due_review_survives_the_reserve() {
    let graph = graph_of(
        vec![
            topic("base").build(),
            topic("lesson").prereqs(&[("base", 0.9, true)]).build(),
        ],
        &[],
    );
    let failed_at = T_US - days(1) / 2;
    let states = states_of(vec![
        ("base", learned(0.55)),
        ("lesson", failed_lesson(failed_at)),
    ]);
    let quiz = quiz_quiet();
    let base = SessionContext::default()
        .with_session_id("s1")
        .with_quiz_state(Some(&quiz));
    let plan = compose_session(&states, &graph, &cfg(), T_US, &mut sampler(1), &base);
    let original = plan.tasks.first().unwrap().task_id.clone();
    let reserved = compose_session(
        &states,
        &graph,
        &cfg(),
        T_US,
        &mut sampler(1),
        &base.with_open_plan(Some(&plan)),
    );
    // `tests/test_selector.py:496-501`.
    assert_eq!(plan_topics(&reserved), ids(&["base"]));
    assert_eq!(
        reserved.tasks.first().map(|task| task.task_id.as_str()),
        Some(original.as_str())
    );
    assert!(reserved.tasks.first().unwrap().nearly_due);
}

// --------------------------------------------------------------------------- //
// Remediation first (test_selector.py:552-622)
// --------------------------------------------------------------------------- //

#[test]
fn remediation_is_served_first_as_a_review() {
    let graph = graph_of(
        vec![
            topic("prereq").build(),
            topic("target").prereqs(&[("prereq", 0.9, true)]).build(),
            topic("r0").prereqs(&[("prereq", 0.3, false)]).build(),
        ],
        &[],
    );
    let states = states_of(vec![
        ("prereq", learned(0.9)),
        ("target", learned(0.9)),
        ("r0", learned(0.5)),
    ]);
    let pending = vec![remediation_for_quiz_miss("target").unwrap()];
    let quiz = quiz_quiet();
    let plan = compose_session(
        &states,
        &graph,
        &cfg(),
        T_US,
        &mut sampler(1),
        &SessionContext::default()
            .with_pending_remediation(&pending)
            .with_quiz_state(Some(&quiz)),
    );
    // `tests/test_selector.py:566-573`.
    let first = plan.tasks.first().unwrap();
    assert_eq!(first.topic.as_deref(), Some("target"));
    assert_eq!(first.task_type, TaskType::Review);
    assert!(first.why.contains("remediation (quiz_miss)"));
    assert_eq!(REMEDIATION_QUIZ_MISS, "quiz_miss");
    assert!(first.is_remediation);
    assert!(!first.nearly_due);
}

#[test]
fn remediation_supersedes_the_same_topic_due_review() {
    let graph = graph_of(vec![topic("dup").build(), topic("other").build()], &[]);
    let states = states_of(vec![("dup", learned(0.5)), ("other", learned(0.5))]);
    assert_eq!(
        due_reviews(&states, &graph, &cfg(), T_US, &BTreeSet::new()),
        ids(&["dup", "other"])
    );
    let pending = vec![remediation_for_quiz_miss("dup").unwrap()];
    let quiz = quiz_quiet();
    let plan = compose_session(
        &states,
        &graph,
        &cfg(),
        T_US,
        &mut sampler(1),
        &SessionContext::default()
            .with_pending_remediation(&pending)
            .with_quiz_state(Some(&quiz)),
    );
    // `tests/test_selector.py:588-600`: `dup` is served ONCE, as the remediation.
    assert_eq!(plan_topics(&plan), ids(&["dup", "other"]));
    let dup = plan.tasks.first().unwrap();
    assert!(dup.is_remediation);
    let other = plan.tasks.get(1).unwrap();
    assert!(other.why.contains("due review"));
    assert!(!other.is_remediation);
    assert!(!other.nearly_due);
}

#[test]
fn unmastered_remediation_target_becomes_a_lesson() {
    let graph = graph_of(
        vec![
            topic("prereq").build(),
            topic("topic").prereqs(&[("prereq", 0.9, true)]).build(),
        ],
        &[],
    );
    let states = states_of(vec![("prereq", learned(0.9))]);
    let pending = vec![PendingRemediation {
        kind: REMEDIATION_QUIZ_MISS.to_owned(),
        targets: vec![cadus_core::event::Slug::new("topic").unwrap()],
    }];
    let quiz = quiz_quiet();
    let plan = compose_session(
        &states,
        &graph,
        &cfg(),
        T_US,
        &mut sampler(1),
        &SessionContext::default()
            .with_pending_remediation(&pending)
            .with_quiz_state(Some(&quiz)),
    );
    // `tests/test_selector.py:620-622`.
    let first = plan.tasks.first().unwrap();
    assert_eq!(first.topic.as_deref(), Some("topic"));
    assert_eq!(first.task_type, TaskType::Lesson);
    assert!(first.is_remediation);
}

#[test]
fn repeat_fail_remediation_targets_key_prereqs() {
    let graph = graph_of(
        vec![
            topic("equivalent-fractions").build(),
            topic("adding-fractions")
                .kps(vec![kp("kp1", &["equivalent-fractions"])])
                .build(),
        ],
        &[],
    );
    let rem = remediation_for_repeat_fail("adding-fractions", "kp1", &graph);
    // `tests/test_selector.py:610-613`.
    assert_eq!(rem.kind, "repeat_fail");
    assert_eq!(
        rem.targets
            .iter()
            .map(|slug| slug.as_str().to_owned())
            .collect::<Vec<String>>(),
        ids(&["equivalent-fractions"])
    );
}

// --------------------------------------------------------------------------- //
// Drills (test_selector.py:629-640)
// --------------------------------------------------------------------------- //

#[test]
fn schedule_drills_respects_mastery_and_cadence() {
    let graph = graph_of(
        vec![
            topic("d").drill(true).build(),
            topic("nd").drill(false).build(),
        ],
        &[],
    );
    let states = states_of(vec![
        ("d", LearnedSpec::new(0.9).ability(0.8).build()),
        ("nd", LearnedSpec::new(0.9).ability(0.8).build()),
    ]);
    // `tests/test_selector.py:635`.
    assert_eq!(schedule_drills(&states, &graph, T_US, None), ids(&["d"]));
    // `tests/test_selector.py:637`.
    let at_bar = states_of(vec![("d", LearnedSpec::new(0.9).ability(0.96).build())]);
    assert_eq!(
        schedule_drills(&at_bar, &graph, T_US, None),
        Vec::<String>::new()
    );
    // `tests/test_selector.py:640`.
    let mut recent: BTreeMap<String, i64> = BTreeMap::new();
    recent.insert("d".to_owned(), T_US - days(1));
    assert_eq!(
        schedule_drills(&states, &graph, T_US, Some(&recent)),
        Vec::<String>::new()
    );
}

// --------------------------------------------------------------------------- //
// The multi-step integration task (selector.py:1044-1105)
// --------------------------------------------------------------------------- //

#[test]
fn multistep_cadence_is_consumable() {
    // `selector.py:1044-1052`: fires while `n_closed < n // 4`.
    assert!(!multistep_is_due(0, 0));
    assert!(!multistep_is_due(3, 0));
    assert!(multistep_is_due(4, 0));
    assert!(!multistep_is_due(4, 1));
    assert!(multistep_is_due(8, 1));
    assert!(!multistep_is_due(8, 2));
}

#[test]
fn multistep_components_are_dependency_ordered_and_capped() {
    let graph = graph_of(
        vec![
            topic("a").build(),
            topic("b").prereqs(&[("a", 0.5, false)]).build(),
            topic("c").prereqs(&[("b", 0.5, false)]).build(),
            topic("d").prereqs(&[("c", 0.5, false)]).build(),
            topic("e").prereqs(&[("d", 0.5, false)]).build(),
        ],
        &[],
    );
    // Roots lead, and the list is capped at 4 parts.
    assert_eq!(
        multistep_components(&ids(&["e", "d", "c", "b", "a"]), &graph),
        ids(&["a", "b", "c", "d"])
    );
}

#[test]
fn multistep_task_absorbs_its_component_reviews() {
    let mut topics = vec![topic("root").build()];
    for index in 0..6 {
        topics.push(
            topic(&format!("r{index}"))
                .prereqs(&[("root", 0.3, false)])
                .build(),
        );
    }
    let graph = graph_of(topics, &[]);
    let mut states = states_of(vec![("root", learned(0.9))]);
    for index in 0..6 {
        states.insert(format!("r{index}"), learned(0.5));
    }
    let quiz = quiz_quiet();
    let plan = compose_session(
        &states,
        &graph,
        &cfg(),
        T_US,
        &mut sampler(1),
        &SessionContext::default()
            .with_session_id("s1")
            .with_quiz_state(Some(&quiz)),
    );
    let multistep = plan
        .tasks
        .iter()
        .find(|task| task.task_type == TaskType::MultiStep)
        .expect("the cadence fires with 7 reviewable topics");
    // The id spells the wire form of the task type (`_assign_ids`).
    assert_eq!(multistep.task_id, "s1-multi-step");
    assert_eq!(multistep.component_topics.len(), 4);
    assert!(multistep.topic.is_none());
    // A component is never also served as a standalone review.
    for component in &multistep.component_topics {
        assert!(
            !plan.tasks.iter().any(|task| {
                task.task_type == TaskType::Review && task.topic.as_ref() == Some(component)
            }),
            "component {component} is served twice"
        );
    }
}

// --------------------------------------------------------------------------- //
// Cross-course gap fill (tests/test_gap_fill.py)
// --------------------------------------------------------------------------- //

/// The 1.0 `_topic` of `tests/test_gap_fill.py:53-66`: every prerequisite is a
/// key edge at weight 1.0.
fn gap_topic(id: &str, prereqs: &[&str]) -> Topic {
    let edges: Vec<(&str, f64, bool)> = prereqs.iter().map(|id| (*id, 1.0, true)).collect();
    topic(id).prereqs(&edges).build()
}

/// The 1.0 three-course ladder of `tests/test_gap_fill.py:100-115`.
fn ladder() -> Curriculum {
    let courses = vec![
        Course {
            id: Slug::new("low").unwrap(),
            name: "low".to_owned(),
            order: 1,
            mastery_floor: vec![Slug::new("low-a").unwrap()],
            mastery_floor_course: None,
        },
        Course {
            id: Slug::new("mid").unwrap(),
            name: "mid".to_owned(),
            order: 2,
            mastery_floor: Vec::new(),
            mastery_floor_course: None,
        },
        Course {
            id: Slug::new("top").unwrap(),
            name: "top".to_owned(),
            order: 3,
            mastery_floor: Vec::new(),
            mastery_floor_course: None,
        },
    ];
    let per_course: Vec<(&str, Vec<Topic>)> = vec![
        (
            "low",
            vec![gap_topic("low-a", &[]), gap_topic("low-b", &["low-a"])],
        ),
        (
            "mid",
            vec![
                gap_topic("mid-a", &["low-a"]),
                gap_topic("mid-b", &["low-b"]),
                gap_topic("mid-free", &[]),
            ],
        ),
        ("top", vec![gap_topic("top-a", &["mid-a"])]),
    ];
    let mut units = Vec::new();
    let mut first_load_index = 0;
    for (course, topics) in per_course {
        let count = topics.len();
        units.push(RawUnit {
            course_id: course.to_owned(),
            file_name: format!("{course}-u.yaml"),
            unit: Unit {
                unit: format!("{course}-u"),
                course: Slug::new(course).unwrap(),
                module: format!("{course}-M"),
                topics,
            },
            first_load_index,
        });
        first_load_index += count;
    }
    Curriculum::build(RawCurriculum {
        catalog: Catalog { courses },
        units,
    })
    .unwrap()
}

/// The 1.0 `_floor(graph, course)` helper: the mastery floor of a course, all in
/// `floor` status.
fn floor_states(graph: &Curriculum, course: &str) -> BTreeMap<String, TopicState> {
    graph
        .mastery_floor(course)
        .unwrap_or_default()
        .into_iter()
        .map(|idx| {
            (
                graph.id_of(idx).to_owned(),
                TopicState {
                    status: TopicStatus::Floor,
                    ..TopicState::default()
                },
            )
        })
        .collect()
}

#[test]
fn gap_course_is_lazy_and_descends_one_level() {
    let graph = ladder();
    let states = floor_states(&graph, "top");
    // `tests/test_gap_fill.py:140`.
    assert_eq!(
        gap_course_for(&states, &graph, &cfg(), T_US, Some("top"), None),
        Some("mid".to_owned())
    );
    // `tests/test_gap_fill.py:147`: every topic mastered, so no gap.
    let complete: BTreeMap<String, TopicState> = graph
        .topics()
        .iter()
        .map(|topic| {
            (
                topic.id.as_str().to_owned(),
                TopicState {
                    status: TopicStatus::Floor,
                    ..TopicState::default()
                },
            )
        })
        .collect();
    assert_eq!(
        gap_course_for(&complete, &graph, &cfg(), T_US, Some("top"), None),
        None
    );
}

#[test]
fn chain_restricts_to_blocking_topics_only() {
    let graph = ladder();
    let states = floor_states(&graph, "top");
    // `tests/test_gap_fill.py:165-166`: `mid-b` blocks nothing `top` needs.
    let chain = gap_fill_chain_for_stack(&states, &graph, &ids(&["top", "mid"]), None)
        .expect("the stack is switched down");
    assert_eq!(chain.to_id_set(&graph), id_set(&["mid-a"]));
    // `tests/test_gap_fill.py:158`.
    assert!(gap_fill_chain_for_stack(&states, &graph, &ids(&["top"]), None).is_none());
    // `tests/test_gap_fill.py:174-176`.
    let wide = gap_fill_chain_for_stack(&states, &graph, &ids(&["top", "mid", "low"]), None)
        .expect("the stack is switched down");
    assert!(wide.contains_id(&graph, "low-a"));
    assert!(
        blocking_gap_ancestors(&states, &graph, Some("top"), None).contains_id(&graph, "low-a")
    );
}

#[test]
fn descent_reaches_the_course_holding_the_real_blocker() {
    let graph = ladder();
    let states = floor_states(&graph, "top");
    // `tests/test_gap_fill.py:202`.
    assert_eq!(
        resolve_gap_fill_stack(&states, &graph, &cfg(), T_US, Some("top")),
        ids(&["top", "mid", "low"])
    );
    // `tests/test_gap_fill.py:211`.
    let stack = resolve_gap_fill_stack(&states, &graph, &cfg(), T_US, Some("top"));
    assert_eq!(
        serveable_gap_frontier(&states, &graph, &stack, None).to_id_set(&graph),
        id_set(&["low-a"])
    );
}

#[test]
fn descent_stops_once_the_tip_can_serve() {
    let graph = ladder();
    let mut states = floor_states(&graph, "top");
    states.insert(
        "low-a".to_owned(),
        TopicState {
            status: TopicStatus::Floor,
            ..TopicState::default()
        },
    );
    // `tests/test_gap_fill.py:220-221`.
    let stack = resolve_gap_fill_stack(&states, &graph, &cfg(), T_US, Some("top"));
    assert_eq!(stack, ids(&["top", "mid"]));
    assert_eq!(
        serveable_gap_frontier(&states, &graph, &stack, None).to_id_set(&graph),
        id_set(&["mid-a"])
    );
}

#[test]
fn stack_is_just_the_base_course_when_not_blocked() {
    let graph = ladder();
    let states = floor_states(&graph, "low");
    // `tests/test_gap_fill.py:227`.
    assert_eq!(
        resolve_gap_fill_stack(&states, &graph, &cfg(), T_US, Some("low")),
        ids(&["low"])
    );
}

#[test]
fn gap_fill_terminates_and_clears_the_whole_course() {
    let graph = ladder();
    let mut states = floor_states(&graph, "top");
    for _ in 0..20 {
        let stack = resolve_gap_fill_stack(&states, &graph, &cfg(), T_US, Some("top"));
        let serveable = serveable_gap_frontier(&states, &graph, &stack, None).to_id_set(&graph);
        if serveable.is_empty() {
            break;
        }
        for tid in serveable {
            states.insert(
                tid,
                TopicState {
                    status: TopicStatus::Floor,
                    ..TopicState::default()
                },
            );
        }
    }
    let mastered = mastered_set(&states, &graph);
    // `tests/test_gap_fill.py:243-248`.
    assert!(mastered.contains_id(&graph, "top-a"));
    assert!(!mastered.contains_id(&graph, "mid-b"));
    assert!(!mastered.contains_id(&graph, "mid-free"));
}

// --------------------------------------------------------------------------- //
// Retry delay and nearly-due ordering
// --------------------------------------------------------------------------- //

#[test]
fn in_retry_delay_needs_a_failed_kp_and_a_stamp() {
    let cfg = cfg();
    let failed_at = T_US - days(1) / 2;
    assert!(in_retry_delay(&failed_lesson(failed_at), &cfg, T_US));
    assert!(!in_retry_delay(
        &failed_lesson(failed_at),
        &cfg,
        failed_at + DAY_US
    ));
    // A stamp with no failed knowledge point never blocks.
    assert!(!in_retry_delay(&learned(0.5), &cfg, T_US));
    // A failed knowledge point with no stamp never blocks.
    let mut kp_progress = BTreeMap::new();
    kp_progress.insert("kp1".to_owned(), KpProgress::FailedTwice);
    let unstamped = TopicState {
        kp_progress,
        ..TopicState::default()
    };
    assert!(!in_retry_delay(&unstamped, &cfg, T_US));
}

#[test]
fn nearly_due_is_ordered_soonest_due_first() {
    let graph = graph_of(
        vec![topic("a").build(), topic("b").build(), topic("c").build()],
        &[],
    );
    // Memory 0.51 is nearer its due date than 0.59, so it comes first.
    let states = states_of(vec![
        ("a", learned(0.59)),
        ("b", learned(0.51)),
        ("c", learned(0.9)),
    ]);
    assert_eq!(nearly_due(&states, &graph, &cfg(), T_US), ids(&["b", "a"]));
}

// --------------------------------------------------------------------------- //
// L1 — compose_session on the full curriculum (REQUIREMENTS L1)
// --------------------------------------------------------------------------- //

/// The learner state of the L1 benchmark: 300 learned topics of the real
/// curriculum, a third of them due.
fn bench_states(graph: &Curriculum) -> BTreeMap<String, TopicState> {
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

#[test]
fn compose_session_runs_on_the_full_curriculum() {
    let graph = real_curriculum();
    assert_eq!(graph.topic_count(), 1090);
    let states = bench_states(&graph);
    let course = graph
        .topics()
        .first()
        .map(|topic| {
            graph
                .course_of(graph.idx_of(topic.id.as_str()).unwrap())
                .to_owned()
        })
        .unwrap();
    let quiz = quiz_quiet();
    let plan = compose_session(
        &states,
        &graph,
        &cfg(),
        T_US,
        &mut sampler(1),
        &SessionContext::default()
            .with_course(Some(&course))
            .with_quiz_state(Some(&quiz)),
    );
    assert!(!plan.tasks.is_empty(), "the plan serves work");
    // Every served task carries a content-stable id.
    for task in &plan.tasks {
        assert!(task.task_id.starts_with("s-"), "id {}", task.task_id);
    }
}

/// L1: composing a session over the full curriculum stays under 5 ms.
///
/// The budget is a RELEASE number, so the test measures only when
/// `CADUS_RELEASE_BENCH` is set. Run it with:
///
/// ```sh
/// CADUS_RELEASE_BENCH=1 cargo test --release -p cadus-core --test selector -- l1_
/// ```
#[test]
fn l1_compose_session_under_5_ms() {
    if std::env::var_os("CADUS_RELEASE_BENCH").is_none() {
        return;
    }
    if cfg!(debug_assertions) {
        panic!("CADUS_RELEASE_BENCH needs a release build: add --release");
    }
    let graph = real_curriculum();
    let states = bench_states(&graph);
    let quiz = quiz_quiet();
    // No course scope: the frontier, the compression, and the lesson ordering
    // all run over the whole 1,090-topic graph, the heaviest shape L1 has.
    let ctx = SessionContext::default().with_quiz_state(Some(&quiz));
    let cfg = cfg();
    // Warm up, then take the best of 20 runs.
    for _ in 0..5 {
        let _ = compose_session(&states, &graph, &cfg, T_US, &mut sampler(1), &ctx);
    }
    let mut best = std::time::Duration::from_secs(1);
    for _ in 0..20 {
        let started = std::time::Instant::now();
        let plan = compose_session(&states, &graph, &cfg, T_US, &mut sampler(1), &ctx);
        let elapsed = started.elapsed();
        assert!(!plan.tasks.is_empty());
        best = best.min(elapsed);
    }
    assert!(
        best < std::time::Duration::from_millis(5),
        "L1: compose_session took {best:?}, budget 5 ms"
    );
    println!("L1 compose_session: {best:?} (budget 5 ms)");
}

// --------------------------------------------------------------------------- //
// Importance, budgets, and decay
// --------------------------------------------------------------------------- //

#[test]
fn importance_adds_the_core_bonus_and_the_dependent_count() {
    let graph = graph_of(
        vec![
            topic("leaf-core").core(true).build(),
            topic("leaf-plain").core(false).build(),
            topic("parent")
                .core(false)
                .prereqs(&[("leaf-plain", 0.5, false)])
                .build(),
        ],
        &[],
    );
    let scope = course_scope(&graph, None);
    let empty: BTreeSet<String> = BTreeSet::new();
    // `selector.py:605-614`: mass 0 + dependents 0 + core bonus 0.5.
    assert!((importance("leaf-core", &graph, &empty, &scope) - 0.5).abs() < 1e-12);
    // Not core, but one in-course dependent: 0 + 1 + 0.
    assert!((importance("leaf-plain", &graph, &empty, &scope) - 1.0).abs() < 1e-12);
    // Not core, no dependent, no review target.
    assert!((importance("parent", &graph, &empty, &scope) - 0.0).abs() < 1e-12);
}

#[test]
fn quiz_budget_is_the_expected_time_times_one_and_a_half() {
    let graph = graph_of(
        vec![
            topic("fast").expected(30).build(),
            topic("slow").expected(60).build(),
            // 45 * 1.5 == 67.5, which rounds half to EVEN: 68.
            topic("half").expected(45).build(),
        ],
        &[],
    );
    // `selector.py:676-678`, with the banker's rounding of trap T3.
    assert_eq!(quiz_budget(&graph, "fast"), 45);
    assert_eq!(quiz_budget(&graph, "slow"), 90);
    assert_eq!(quiz_budget(&graph, "half"), 68);
}

#[test]
fn decay_moves_a_topic_through_the_review_bands() {
    let graph = graph_of(vec![topic("a").build()], &[]);
    // Memory base 1.0, interval 10 days: memory halves every 10 days.
    let at = |days_ago: i64| {
        states_of(vec![(
            "a",
            LearnedSpec::new(1.0).t0(T_US - days(days_ago)).build(),
        )])
    };
    let cfg = cfg();
    let empty = BTreeSet::new();
    // 0 days: memory 1.0, on schedule.
    assert_eq!(
        due_reviews(&at(0), &graph, &cfg, T_US, &empty),
        Vec::<String>::new()
    );
    assert_eq!(nearly_due(&at(0), &graph, &cfg, T_US), Vec::<String>::new());
    // 8 days: memory 0.574, nearly due.
    assert_eq!(nearly_due(&at(8), &graph, &cfg, T_US), ids(&["a"]));
    // 10 days: memory 0.5, due.
    assert_eq!(
        due_reviews(&at(10), &graph, &cfg, T_US, &empty),
        ids(&["a"])
    );
}

// --------------------------------------------------------------------------- //
// compress — differential check against a literal transcription of 1.0
// --------------------------------------------------------------------------- //

/// `_covers(candidate, pool)` of `selector.py:500-504`, transcribed literally.
///
/// It asks the arena for `W(candidate -> due)` once per pair, the way 1.0 asks
/// the graph. It is deliberately the slow, obvious form.
fn naive_covers(
    candidate: &str,
    pool: &BTreeSet<String>,
    graph: &Curriculum,
    cfg: &Config,
) -> BTreeSet<String> {
    pool.iter()
        .filter(|due| {
            due.as_str() != candidate
                && graph.encompassing_weight_by_id(candidate, due) >= cfg.fire.knockout_weight
        })
        .cloned()
        .collect()
}

/// `compress` of `selector.py:506-590`, transcribed literally.
fn naive_compress(
    due: &[String],
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    cfg: &Config,
    t_us: i64,
) -> (Vec<String>, BTreeMap<String, Vec<String>>) {
    let due_set: BTreeSet<String> = due
        .iter()
        .filter(|id| graph.idx_of(id).is_some())
        .cloned()
        .collect();
    if due_set.is_empty() {
        return (Vec::new(), BTreeMap::new());
    }
    let mastered = mastered_set(states, graph);
    let mut free: BTreeSet<String> = frontier(graph, &mastered)
        .sorted_ids(graph)
        .into_iter()
        .filter(|id| !in_retry_delay(states.get(*id).unwrap_or(&TopicState::default()), cfg, t_us))
        .map(ToOwned::to_owned)
        .collect();
    free.extend(nearly_due(states, graph, cfg, t_us));
    let free_candidates: Vec<String> = free
        .into_iter()
        .filter(|id| !due_set.contains(id))
        .collect();

    let mut knockouts: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut uncovered = due_set;
    loop {
        if uncovered.is_empty() {
            break;
        }
        let mut best: Option<String> = None;
        let mut best_cov: BTreeSet<String> = BTreeSet::new();
        for candidate in &free_candidates {
            let cov = naive_covers(candidate, &uncovered, graph, cfg);
            if cov.len() > best_cov.len() {
                best = Some(candidate.clone());
                best_cov = cov;
            }
        }
        let Some(winner) = best else { break };
        if best_cov.is_empty() {
            break;
        }
        knockouts.insert(winner, best_cov.iter().cloned().collect());
        uncovered.retain(|id| !best_cov.contains(id));
    }

    let mut surviving: Vec<String> = Vec::new();
    let mut remaining = uncovered;
    while !remaining.is_empty() {
        let mut best: Option<String> = None;
        let mut best_cov: BTreeSet<String> = BTreeSet::new();
        for candidate in &remaining {
            let mut cov = naive_covers(candidate, &remaining, graph, cfg);
            cov.insert(candidate.clone());
            if cov.len() > best_cov.len() {
                best = Some(candidate.clone());
                best_cov = cov;
            }
        }
        let Some(winner) = best else { break };
        let others: Vec<String> = best_cov
            .iter()
            .filter(|id| *id != &winner)
            .cloned()
            .collect();
        if !others.is_empty() {
            knockouts.insert(winner.clone(), others);
        }
        surviving.push(winner);
        remaining.retain(|id| !best_cov.contains(id));
    }
    surviving.sort_unstable();
    (surviving, knockouts)
}

#[test]
fn compress_matches_the_literal_1_0_transcription() {
    let graph = real_curriculum();
    let cfg = cfg();
    let pool: Vec<String> = graph
        .topics()
        .iter()
        .take(60)
        .map(|topic| topic.id.as_str().to_owned())
        .collect();
    let mut rng = Rng::new(4_242_026);
    for example in 0..60 {
        let mut states: BTreeMap<String, TopicState> = BTreeMap::new();
        for tid in &pool {
            if rng.flip() {
                states.insert(tid.clone(), learned(rng.unit()));
            }
        }
        let due = due_reviews(&states, &graph, &cfg, T_US, &BTreeSet::new());
        let fast = compress(&due, &states, &graph, &cfg, T_US, None);
        let (surviving, knockouts) = naive_compress(&due, &states, &graph, &cfg, T_US);
        assert_eq!(fast.surviving, surviving, "example {example}: surviving");
        assert_eq!(fast.knockouts, knockouts, "example {example}: knockouts");
    }
}

// --------------------------------------------------------------------------- //
// The selector boundaries, read AT the threshold
// --------------------------------------------------------------------------- //
//
// Every expected value below came from the live 1.0 selector on the same input,
// through `scripts/oracle/dump_selector_boundaries_1_0.py`. M3 review round 1,
// findings #8, #9, and #10.

/// 3.49 days in microseconds: one step INSIDE the drill cadence window.
const DRILL_GAP_INSIDE_US: i64 = 301_536_000_000;

/// 3.5 days in microseconds: the drill cadence window itself.
const DRILL_GAP_AT_WINDOW_US: i64 = 302_400_000_000;

/// 3.51 days in microseconds: one step OUTSIDE the drill cadence window.
const DRILL_GAP_OUTSIDE_US: i64 = 303_264_000_000;

#[test]
fn schedule_drills_brackets_the_automaticity_bar() {
    // `selector.py:113` holds `DRILL_MASTERY_ABILITY = 0.95` and `selector.py:866`
    // drops a topic whose `ability >= DRILL_MASTERY_ABILITY`. 1.0 on this graph:
    // ability 0.949 -> ['d'], 0.95 -> [], 0.951 -> [] (finding #9).
    assert!((DRILL_MASTERY_ABILITY - 0.95).abs() < f64::EPSILON);
    let graph = graph_of(vec![topic("d").drill(true).build()], &[]);

    let below = states_of(vec![("d", LearnedSpec::new(0.9).ability(0.949).build())]);
    assert_eq!(schedule_drills(&below, &graph, T_US, None), ids(&["d"]));

    let at_bar = states_of(vec![("d", LearnedSpec::new(0.9).ability(0.95).build())]);
    assert_eq!(
        schedule_drills(&at_bar, &graph, T_US, None),
        Vec::<String>::new()
    );

    let above = states_of(vec![("d", LearnedSpec::new(0.9).ability(0.951).build())]);
    assert_eq!(
        schedule_drills(&above, &graph, T_US, None),
        Vec::<String>::new()
    );
}

#[test]
fn schedule_drills_brackets_the_cadence_window() {
    // `selector.py:116` holds `DRILL_INTERVAL_DAYS = 3.5` and `selector.py:868`
    // skips a topic while `t - last_drill_at < timedelta(days=3.5)`. 1.0 on this
    // graph: a gap of 3.49 days -> [], 3.5 days -> ['d'], 3.51 days -> ['d'].
    // The gap of exactly 3.5 days is the one the strict `<` decides (finding #9).
    assert!((DRILL_INTERVAL_DAYS - 3.5).abs() < f64::EPSILON);
    let graph = graph_of(vec![topic("d").drill(true).build()], &[]);
    let states = states_of(vec![("d", LearnedSpec::new(0.9).ability(0.8).build())]);

    let mut inside: BTreeMap<String, i64> = BTreeMap::new();
    inside.insert("d".to_owned(), T_US - DRILL_GAP_INSIDE_US);
    assert_eq!(
        schedule_drills(&states, &graph, T_US, Some(&inside)),
        Vec::<String>::new()
    );

    let mut at_window: BTreeMap<String, i64> = BTreeMap::new();
    at_window.insert("d".to_owned(), T_US - DRILL_GAP_AT_WINDOW_US);
    assert_eq!(
        schedule_drills(&states, &graph, T_US, Some(&at_window)),
        ids(&["d"])
    );

    let mut outside: BTreeMap<String, i64> = BTreeMap::new();
    outside.insert("d".to_owned(), T_US - DRILL_GAP_OUTSIDE_US);
    assert_eq!(
        schedule_drills(&states, &graph, T_US, Some(&outside)),
        ids(&["d"])
    );
}

#[test]
fn the_quiz_recency_window_is_read_at_its_boundary() {
    // `selector.py:84` holds `QUIZ_RECENT_DAYS = 14` and `selector.py:711` is
    // `(t - learned_at[tid]) <= timedelta(days=QUIZ_RECENT_DAYS)`, so a topic
    // learned exactly 14 days ago is still recent. Three learned topics, aged 13,
    // 14, and 15 days: 1.0 puts q13 and q14 in `recent` and q15 in `mid`, for seed
    // 7 and for seed 42 (finding #10).
    assert_eq!(QUIZ_RECENT_DAYS, 14);
    let quiz_ids = ["q13", "q14", "q15"];
    let graph = graph_of(quiz_ids.iter().map(|id| topic(id).build()).collect(), &[]);
    let states: BTreeMap<String, TopicState> = quiz_ids
        .iter()
        .map(|id| ((*id).to_owned(), learned(0.9)))
        .collect();
    let mut learned_at: BTreeMap<String, i64> = BTreeMap::new();
    learned_at.insert("q13".to_owned(), T_US - days(13));
    learned_at.insert("q14".to_owned(), T_US - days(14));
    learned_at.insert("q15".to_owned(), T_US - days(15));

    let cfg = cfg();
    for seed in [7_u64, 42] {
        let plan = quiz_composer(
            &states,
            &graph,
            &cfg,
            T_US,
            &mut sampler(seed),
            Some(&learned_at),
        );
        let mut rows: Vec<(String, &str, i64)> = plan
            .questions
            .iter()
            .map(|question| {
                (
                    question.topic.clone(),
                    question.stratum,
                    question.time_budget_secs,
                )
            })
            .collect();
        rows.sort();
        assert_eq!(
            rows,
            vec![
                ("q13".to_owned(), "recent", 45),
                ("q14".to_owned(), "recent", 45),
                ("q15".to_owned(), "mid", 45),
            ],
            "seed {seed}"
        );
    }
}

#[test]
fn the_lesson_knockout_mass_is_a_compensated_sum() {
    // Trap T1 at `selector.py:602`: the knockout mass is a CPython `sum()` over the
    // review targets. Ten targets at weight 0.1 each total exactly 1.0 in CPython
    // 3.12 and later, where a naive left-to-right add gives 0.9999999999999999.
    // The topic is not core and has no dependent, so the importance IS the mass:
    // the live 1.0 `importance` on this graph prints 1.0 (finding #8).
    let leaves: Vec<String> = (0..10).map(|index| format!("leaf-{index}")).collect();
    let edges: Vec<(&str, f64, bool)> = leaves
        .iter()
        .map(|id| (id.as_str(), 0.1_f64, false))
        .collect();
    let mut topics: Vec<Topic> = leaves.iter().map(|id| topic(id).build()).collect();
    topics.push(topic("lesson-topic").core(false).prereqs(&edges).build());
    let graph = graph_of(topics, &[]);

    let targets: BTreeSet<String> = leaves.iter().cloned().collect();
    let scope = course_scope(&graph, None);
    let mass = importance("lesson-topic", &graph, &targets, &scope);

    // The comparison is EXACT: the naive total differs from 1.0 by one unit in the
    // last place, which is below `f64::EPSILON`.
    assert_eq!(
        mass.to_bits(),
        1.0_f64.to_bits(),
        "the knockout mass is {mass:?}, so the total is not compensated"
    );
}
