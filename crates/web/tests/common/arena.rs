//! The fixture curriculum of the task route tests, and the literals of the
//! lesson it serves.
//!
//! Every value here is an INPUT of a test. The expected values stay literals
//! of the test file that reads them (HANDOVER section 3).

use cadus_core::curriculum::{
    AnswerKind, Catalog, Course, Curriculum, Exemplar, KnowledgePoint, RawCurriculum, RawUnit,
    Slug, Topic, Unit,
};

/// The Unix microsecond instant of 2026-01-01T00:00:00Z.
pub const BASE_US: i64 = 1_767_225_600_000_000;

/// The session id every seeded log opens.
pub const SESSION: &str = "s_2026-01-01a";

/// The task id of the `addition` lesson (`assign_ids`: `{session}-{type}-{topic}`).
pub const LESSON: &str = "s_2026-01-01a-lesson-addition";

/// The serving key of the first knowledge point of `addition`.
pub const KEY: &str = "addition/kp1";

/// The statement of the served problem.
pub const PROBLEM_TEXT: &str = "Compute 8 + 5.5.";

/// The authored answer of the served problem.
pub const EXPECTED_ANSWER: &str = "13.5";

/// The authored solution sketch of the served problem.
pub const SOLUTION: &str = "Add the parts to reach 13.5.";

/// The `problem_id` every seeded state hands the client.
pub const PROBLEM_ID: &str = "p0000000000000000000000000000001";

/// The topic's authored solve time. The timing cap is ten times this.
pub const EXPECTED_TIME_SECS: i64 = 30;

/// One knowledge point with its authored exemplars and no key prerequisite.
pub fn kp(id: &str, exemplars: Vec<Exemplar>) -> KnowledgePoint {
    KnowledgePoint {
        id: Slug::new(id).unwrap(),
        name: format!("The {id} point"),
        key_prerequisites: Vec::new(),
        exemplars,
        constraints: None,
        finite_objective_domain: None,
        visuals: Vec::new(),
    }
}

/// One exemplar whose sketch names its answer.
pub fn exemplar(problem: &str, answer: &str) -> Exemplar {
    exemplar_with_solution(
        problem,
        answer,
        &format!("Add the parts to reach {answer}."),
    )
}

/// One exemplar with the solution its author wrote.
pub fn exemplar_with_solution(problem: &str, answer: &str, solution: &str) -> Exemplar {
    Exemplar {
        answer_contract: None,
        problem: problem.to_string(),
        answer: answer.to_string(),
        solution_sketch: Some(solution.to_string()),
    }
}

/// One numeric core topic with no prerequisite.
pub fn topic(id: &str, points: Vec<KnowledgePoint>) -> Topic {
    Topic {
        id: Slug::new(id).unwrap(),
        name: format!("The {id} topic"),
        core: true,
        difficulty: 0.3,
        drill: false,
        answer_kind: AnswerKind::Numeric,
        expected_time_secs: EXPECTED_TIME_SECS,
        prerequisites: Vec::new(),
        encompassings_extra: Vec::new(),
        knowledge_points: points,
        diagnostic_exemplar: None,
        anki_seeds: Vec::new(),
    }
}

/// The catalog of one course, `c1` ("Foundations").
pub fn one_course_catalog() -> Catalog {
    Catalog {
        courses: vec![Course {
            id: Slug::new("c1").unwrap(),
            name: "Foundations".to_string(),
            order: 0,
            mastery_floor: Vec::new(),
            mastery_floor_course: None,
        }],
    }
}

/// One unit file of `course`: the module `name`, with `topics`, loaded at
/// `first_load_index`.
pub fn unit(course: &str, name: &str, topics: Vec<Topic>, first_load_index: usize) -> RawUnit {
    RawUnit {
        course_id: course.to_string(),
        file_name: format!("{first_load_index:02}-{name}.yaml"),
        unit: Unit {
            unit: name.to_string(),
            course: Slug::new(course).unwrap(),
            module: name.to_string(),
            topics,
        },
        first_load_index,
    }
}

/// One course, one module `M1`, and `topics` in that module.
pub fn one_unit_curriculum(topics: Vec<Topic>) -> Curriculum {
    Curriculum::build(RawCurriculum {
        catalog: one_course_catalog(),
        units: vec![unit("c1", "M1", topics, 0)],
    })
    .unwrap()
}

/// The two-topic fixture of the grade and diagnosis tests.
///
/// `addition` authors two knowledge points, and `kp1` names `key_prereqs`.
/// `subtraction` authors one knowledge point with no exemplar.
pub fn addition_curriculum(key_prereqs: Vec<Slug>) -> Curriculum {
    let mut first = kp("kp1", vec![exemplar(PROBLEM_TEXT, EXPECTED_ANSWER)]);
    first.key_prerequisites = key_prereqs;
    one_unit_curriculum(vec![
        topic(
            "addition",
            vec![
                first,
                kp("kp2", vec![exemplar("Compute 40 + 2.5.", "42.5")]),
            ],
        ),
        topic("subtraction", vec![kp("kp1", vec![])]),
    ])
}
