//! Builders the FIRe and XP tests share: in-memory curricula, learned states,
//! and the tolerance the 1.0 `pytest.approx` assertions become.
//!
//! The 1.0 tests build their graphs from pydantic models with no file on disk
//! (`tests/test_fire.py:68-123`). These builders do the same through
//! [`RawCurriculum`], so the tests stay in one file and read no fixture tree.

#![allow(dead_code)]
#![allow(clippy::unwrap_used)]

use chrono::NaiveDate;
use chrono_tz::Tz;

use cadus_core::curriculum::load::{RawCurriculum, RawUnit};
use cadus_core::curriculum::model::{
    Catalog, Course, KnowledgePoint, PrereqEdge, Slug, Topic, Unit,
};
use cadus_core::curriculum::{AnswerKind, Curriculum};
use cadus_core::event::{Timestamp, TopicStatus};
use cadus_core::learner::TopicState;
use cadus_core::numeric::{from_naive_utc, resolve_timezone};

/// `T = datetime(2026, 7, 14, 12, 0, 0)` of the 1.0 FIRe tests, as UTC
/// microseconds. A naive 1.0 timestamp is UTC (trap T8).
pub const T_US: i64 = 1_784_030_400_000_000;

/// `T = datetime(2026, 7, 28, 12, 0, 0)` of the 1.0 ability-seeding tests.
pub const T_SEED_US: i64 = 1_785_240_000_000_000;

/// One day, in microseconds.
pub const DAY_US: i64 = 86_400_000_000;

/// `days` days, in microseconds.
#[must_use]
pub fn days(count: i64) -> i64 {
    count * DAY_US
}

/// The tolerance the 1.0 `pytest.approx` assertions become in the port:
/// `|a - b| <= 1e-6 * max(1, |b|)`.
#[must_use]
pub fn approx(actual: f64, expected: f64) -> bool {
    (actual - expected).abs() <= 1e-6 * expected.abs().max(1.0)
}

/// Assert that `actual` is within the port's `approx` tolerance of `expected`.
#[track_caller]
pub fn assert_approx(actual: f64, expected: f64, what: &str) {
    assert!(
        approx(actual, expected),
        "{what}: got {actual:?}, want approximately {expected:?}"
    );
}

/// The 1.0 `_topic` builder (`tests/test_fire.py:68-83`).
///
/// `prereqs` is `(id, weight, key)`, and `extra` is `(id, weight)` for
/// `encompassings_extra`. The default difficulty is 0.3, as in 1.0.
#[must_use]
pub fn topic(
    id: &str,
    prereqs: &[(&str, f64, bool)],
    difficulty: f64,
    extra: &[(&str, f64)],
) -> Topic {
    Topic {
        id: Slug::new(id).unwrap(),
        name: id.to_owned(),
        core: true,
        difficulty,
        drill: false,
        answer_kind: AnswerKind::Numeric,
        expected_time_secs: 30,
        prerequisites: prereqs
            .iter()
            .map(|&(pid, weight, key)| PrereqEdge {
                id: Slug::new(pid).unwrap(),
                weight,
                key,
            })
            .collect(),
        encompassings_extra: extra
            .iter()
            .map(|&(eid, weight)| PrereqEdge {
                id: Slug::new(eid).unwrap(),
                weight,
                key: false,
            })
            .collect(),
        knowledge_points: Vec::new(),
        diagnostic_exemplar: None,
        anki_seeds: Vec::new(),
    }
}

/// The same as [`topic`], with the 1.0 default difficulty of 0.3.
#[must_use]
pub fn plain_topic(id: &str, prereqs: &[(&str, f64, bool)]) -> Topic {
    topic(id, prereqs, 0.3, &[])
}

/// One knowledge point with its key prerequisites.
#[must_use]
pub fn knowledge_point(id: &str, key_prerequisites: &[&str]) -> KnowledgePoint {
    KnowledgePoint {
        id: Slug::new(id).unwrap(),
        name: id.to_owned(),
        key_prerequisites: key_prerequisites
            .iter()
            .map(|key| Slug::new(key).unwrap())
            .collect(),
        exemplars: Vec::new(),
        constraints: None,
    }
}

/// The 1.0 `_graph` builder (`tests/test_fire.py:86-89`): one unit `u` of module
/// `M` in course `c`.
#[must_use]
pub fn graph(topics: Vec<Topic>) -> Curriculum {
    graph_of_units(&[("M", topics)], "c")
}

/// A curriculum of several unit files, all inside one course.
///
/// Each entry is `(module, topics)`. The load order is the entry order, then the
/// topic order inside each entry, which is the 1.0 `sorted(glob)` order once the
/// caller lists the files in name order (trap T18).
#[must_use]
pub fn graph_of_units(units: &[(&str, Vec<Topic>)], course_id: &str) -> Curriculum {
    let catalog = Catalog {
        courses: vec![Course {
            id: Slug::new(course_id).unwrap(),
            name: course_id.to_owned(),
            order: 0,
            mastery_floor: Vec::new(),
            mastery_floor_course: None,
        }],
    };
    let mut raw_units = Vec::new();
    let mut first_load_index = 0;
    for (index, (module, topics)) in units.iter().enumerate() {
        let count = topics.len();
        raw_units.push(RawUnit {
            course_id: course_id.to_owned(),
            file_name: format!("{index:02}-{module}.yaml"),
            unit: Unit {
                unit: (*module).to_owned(),
                course: Slug::new(course_id).unwrap(),
                module: (*module).to_owned(),
                topics: topics.clone(),
            },
            first_load_index,
        });
        first_load_index += count;
    }
    Curriculum::build(RawCurriculum {
        catalog,
        units: raw_units,
    })
    .unwrap()
}

/// The p.364 graph of the 1.0 tests (`tests/test_fire.py:116-123`):
/// `two-digit-mult` encompasses `one-digit-mult` at `W = 0.8` (a key edge) and
/// `addition` at `W = 0.6`.
#[must_use]
pub fn p364_graph() -> Curriculum {
    graph(vec![
        plain_topic("addition", &[]),
        plain_topic("one-digit-mult", &[]),
        plain_topic(
            "two-digit-mult",
            &[("one-digit-mult", 0.8, true), ("addition", 0.6, false)],
        ),
    ])
}

/// The 1.0 `_learned` builder (`tests/test_fire.py:92-111`): a `learning` state
/// whose memory equals `memory` when it is read at its own `t0`.
#[derive(Debug, Clone, Copy)]
pub struct Learned {
    /// The `memoryBase` of the state.
    pub memory: f64,
    /// The `repNum` of the state.
    pub rep: f64,
    /// The `speed` of the state.
    pub speed: f64,
    /// The `interval_days` of the state.
    pub interval: f64,
    /// The `ability` of the state.
    pub ability: f64,
    /// The `t0` of the state, in UTC microseconds.
    pub t0_us: i64,
}

impl Learned {
    /// The 1.0 defaults: `rep 3.0`, `speed 1.0`, `interval 10.0`, `ability 0.5`,
    /// and `t0 == T`.
    #[must_use]
    pub fn new(memory: f64) -> Self {
        Self {
            memory,
            rep: 3.0,
            speed: 1.0,
            interval: 10.0,
            ability: 0.5,
            t0_us: T_US,
        }
    }

    /// Set `repNum`.
    #[must_use]
    pub fn rep(mut self, rep: f64) -> Self {
        self.rep = rep;
        self
    }

    /// Set `speed`.
    #[must_use]
    pub fn speed(mut self, speed: f64) -> Self {
        self.speed = speed;
        self
    }

    /// Set `interval_days`.
    #[must_use]
    pub fn interval(mut self, interval: f64) -> Self {
        self.interval = interval;
        self
    }

    /// Set `ability`.
    #[must_use]
    pub fn ability(mut self, ability: f64) -> Self {
        self.ability = ability;
        self
    }

    /// Set `t0`, in UTC microseconds.
    #[must_use]
    pub fn t0(mut self, t0_us: i64) -> Self {
        self.t0_us = t0_us;
        self
    }

    /// Build the state.
    #[must_use]
    pub fn state(self) -> TopicState {
        TopicState {
            status: TopicStatus::Learning,
            rep_num: self.rep,
            memory_base: self.memory,
            t0: Some(Timestamp::from_micros(self.t0_us)),
            interval_days: self.interval,
            ability: self.ability,
            speed: self.speed,
            ..TopicState::default()
        }
    }
}

/// The 1.0 `_learned(memory)` call with every default.
#[must_use]
pub fn learned(memory: f64) -> TopicState {
    Learned::new(memory).state()
}

/// The 1.0 `_noon(day)` helper (`tests/test_xp.py:125-126`): that calendar day at
/// 12:00 UTC, as microseconds.
#[must_use]
pub fn noon_us(year: i32, month: u32, day: u32) -> i64 {
    from_naive_utc(
        NaiveDate::from_ymd_opt(year, month, day)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap(),
    )
}

/// One calendar date.
#[must_use]
pub fn on(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).unwrap()
}

/// The UTC zone, resolved the way the fold resolves one.
#[must_use]
pub fn utc() -> Tz {
    resolve_timezone(Some("UTC")).unwrap()
}

/// The topic ids of the 1.0 `tests/fixtures/curriculum_mini` tree, by unit file.
///
/// The tree holds 12 topics of one course, `testcourse`, across two unit files.
/// The XP tests read the topic COUNT and the course membership only, so the
/// in-memory build below carries the same ids with no edges.
pub const MINI_NUMBERS: [&str; 6] = [
    "counting",
    "addition",
    "subtraction",
    "multiplication",
    "division",
    "place-value",
];

/// The topic ids of the second unit file of `curriculum_mini`.
pub const MINI_FRACTIONS: [&str; 6] = [
    "fraction-basics",
    "equivalent-fractions",
    "adding-fractions",
    "multiplying-fractions",
    "mixed-numbers",
    "fraction-word-problems",
];

/// The in-memory equivalent of the 1.0 `curriculum_mini` fixture: 12 topics of
/// the course `testcourse`, in the file order `00-numbers`, `01-fractions`.
#[must_use]
pub fn mini_curriculum() -> Curriculum {
    graph_of_units(
        &[
            (
                "numbers",
                MINI_NUMBERS.iter().map(|id| plain_topic(id, &[])).collect(),
            ),
            (
                "fractions",
                MINI_FRACTIONS
                    .iter()
                    .map(|id| plain_topic(id, &[]))
                    .collect(),
            ),
        ],
        "testcourse",
    )
}
