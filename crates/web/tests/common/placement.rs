//! The fixture curriculum of the placement diagnostic: one course of three
//! topics in a chain, each one with a diagnostic exemplar.

use cadus_core::config::Config;
use cadus_core::curriculum::model::{Exemplar, KnowledgePoint, PrereqEdge};
use cadus_core::curriculum::{
    AnswerKind, Catalog, Course, Curriculum, RawCurriculum, RawUnit, Slug, Topic, Unit,
};
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::state::Content;
use cadus_web::{AppState, create_app};

pub use super::prelude::*;
use super::*;

/// The answer every exemplar of the fixture expects.
pub const ANSWER: &str = "7";

/// The solution sketch of every exemplar. No reply may carry it.
pub const SKETCH: &str = "add the two numbers and read the total";

/// The course of the fixture.
pub const COURSE: &str = "c1";

// --------------------------------------------------------------------------- //
// The fixture
// --------------------------------------------------------------------------- //

/// One topic with a diagnostic exemplar and a numeric answer kind.
pub fn topic(id: &str, prereq: Option<&str>) -> Topic {
    Topic {
        id: Slug::new(id).unwrap(),
        name: id.to_owned(),
        core: true,
        difficulty: 0.3,
        drill: false,
        answer_kind: AnswerKind::Numeric,
        expected_time_secs: 30,
        prerequisites: prereq
            .map(|parent| {
                vec![PrereqEdge {
                    id: Slug::new(parent).unwrap(),
                    weight: 1.0,
                    key: true,
                }]
            })
            .unwrap_or_default(),
        encompassings_extra: Vec::new(),
        knowledge_points: vec![KnowledgePoint {
            id: Slug::new("kp1").unwrap(),
            name: "kp1".to_owned(),
            key_prerequisites: Vec::new(),
            exemplars: Vec::new(),
            constraints: None,
            visuals: Vec::new(),
        }],
        diagnostic_exemplar: Some(Exemplar {
            answer_contract: None,
            problem: format!("probe for {id}: what is 3 + 4?"),
            answer: ANSWER.to_owned(),
            solution_sketch: Some(SKETCH.to_owned()),
        }),
        anki_seeds: Vec::new(),
    }
}

/// One course of three topics in a chain.
pub fn graph() -> Curriculum {
    let catalog = Catalog {
        courses: vec![Course {
            id: Slug::new(COURSE).unwrap(),
            name: "Foundations".to_owned(),
            order: 0,
            mastery_floor: Vec::new(),
            mastery_floor_course: None,
        }],
    };
    Curriculum::build(RawCurriculum {
        catalog,
        units: vec![RawUnit {
            course_id: COURSE.to_owned(),
            file_name: "00-M1.yaml".to_owned(),
            unit: Unit {
                unit: "M1".to_owned(),
                course: Slug::new(COURSE).unwrap(),
                module: "M1".to_owned(),
                topics: vec![
                    topic("addition", None),
                    topic("subtraction", Some("addition")),
                    topic("word-problems", Some("subtraction")),
                ],
            },
            first_load_index: 0,
        }],
    })
    .unwrap()
}

/// The router of a test, with the fixture curriculum loaded.
pub fn app(db: &TestDb) -> Router {
    create_app(
        AppState::new(Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS))
            .with_content(Arc::new(super::open_content(graph()))),
    )
}
