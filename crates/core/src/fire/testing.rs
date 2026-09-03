//! Builders the unit tests of the engine, the fold, the selector and the
//! diagnostic share. Every function is straight-line code, so a unit test that
//! calls it covers it in full.

use crate::curriculum::load::{RawCurriculum, RawUnit};
use crate::curriculum::model::{
    Catalog, Course, Exemplar, KnowledgePoint, PrereqEdge, Slug, Topic, Unit,
};
use crate::curriculum::{AnswerKind, Curriculum};
use crate::event::{Timestamp, TopicStatus};
use crate::learner::TopicState;

/// `T` of the 1.0 FIRe tests, 2026-07-14 12:00 UTC, in microseconds.
pub(crate) const T_US: i64 = 1_784_030_400_000_000;

/// One day, in microseconds.
pub(crate) const DAY_US: i64 = 86_400_000_000;

/// A slug from a test id.
pub(crate) fn slug(id: &str) -> Slug {
    Slug::new(id).expect("a test id is a legal slug")
}

/// One knowledge point with its key prerequisites.
pub(crate) fn knowledge_point(id: &str, keys: &[&str]) -> KnowledgePoint {
    KnowledgePoint {
        id: slug(id),
        name: id.to_owned(),
        key_prerequisites: keys.iter().map(|key| slug(key)).collect(),
        exemplars: Vec::new(),
        constraints: None,
    }
}

/// A core topic of difficulty 0.3 with one knowledge point `kp1`, one
/// diagnostic exemplar, and the prerequisite edges `(id, weight, key)`.
pub(crate) fn topic(id: &str, prereqs: &[(&str, f64, bool)]) -> Topic {
    Topic {
        id: slug(id),
        name: id.to_owned(),
        core: true,
        difficulty: 0.3,
        drill: false,
        answer_kind: AnswerKind::Numeric,
        expected_time_secs: 30,
        prerequisites: prereqs
            .iter()
            .map(|&(pid, weight, key)| PrereqEdge {
                id: slug(pid),
                weight,
                key,
            })
            .collect(),
        encompassings_extra: Vec::new(),
        knowledge_points: vec![knowledge_point("kp1", &[])],
        diagnostic_exemplar: Some(Exemplar {
            problem: format!("probe {id}"),
            answer: "7".to_owned(),
            solution_sketch: None,
        }),
        anki_seeds: Vec::new(),
    }
}

/// One course `c` of one module `M` over `topics`, with no mastery floor.
pub(crate) fn graph(topics: Vec<Topic>) -> Curriculum {
    ladder(&[("c", &[], topics)])
}

/// Several courses in catalog order: `(course, mastery floor, topics)`. Course
/// `n` has order `n`, and its one module is `{course}-M`.
pub(crate) fn ladder(courses: &[(&str, &[&str], Vec<Topic>)]) -> Curriculum {
    let mut catalog = Vec::new();
    let mut units = Vec::new();
    let mut first_load_index = 0;
    for (order, (course, floor, topics)) in courses.iter().enumerate() {
        catalog.push(Course {
            id: slug(course),
            name: (*course).to_owned(),
            order: i64::try_from(order).expect("a small count") + 1,
            mastery_floor: floor.iter().map(|id| slug(id)).collect(),
            mastery_floor_course: None,
        });
        units.push(RawUnit {
            course_id: (*course).to_owned(),
            file_name: format!("{course}-u.yaml"),
            unit: Unit {
                unit: format!("{course}-u"),
                course: slug(course),
                module: format!("{course}-M"),
                topics: topics.clone(),
            },
            first_load_index,
        });
        first_load_index += topics.len();
    }
    Curriculum::build(RawCurriculum {
        catalog: Catalog { courses: catalog },
        units,
    })
    .expect("a test curriculum builds")
}

/// A `learning` state at `t0 == T_US` whose memory reads `memory` at `T_US`:
/// `repNum` 3, speed 1, interval 10 days, ability 0.5.
pub(crate) fn learned(memory: f64) -> TopicState {
    TopicState {
        status: TopicStatus::Learning,
        rep_num: 3.0,
        memory_base: memory,
        t0: Some(Timestamp::from_micros(T_US)),
        interval_days: 10.0,
        ability: 0.5,
        speed: 1.0,
        ..TopicState::default()
    }
}
