//! The pure readiness audit of D-F5 over a small curriculum fixture.
//!
//! Audit findings (h), (i) and (j): the store is empty on a fresh deployment,
//! most knowledge points hold two exemplars, and the service teaches nothing it
//! has no approved page for. These tests pin what the audit says about each.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_core::curriculum::{Curriculum, Exemplar, KnowledgePoint};
use cadus_core::instruction::{KIND_HINT_LADDER, KIND_TEACH};
use cadus_core::readiness::{
    Blocker, ContentIndex, EmptyContent, KIND_TEMPLATE, MapContent, ReadinessGate, ReadinessIndex,
    ReadinessReport, visual_needed,
};

use common::{graph, knowledge_point, plain_topic};

/// One exemplar with a problem, an answer, and a sketch when `sketch` is true.
fn exemplar(problem: &str, answer: &str, sketch: bool) -> Exemplar {
    Exemplar {
        answer_contract: None,
        problem: problem.to_owned(),
        answer: answer.to_owned(),
        solution_sketch: sketch.then(|| format!("work {problem}")),
    }
}

/// A knowledge point of `id` over the exemplars.
fn kp_with(id: &str, exemplars: Vec<Exemplar>) -> KnowledgePoint {
    KnowledgePoint {
        exemplars,
        ..knowledge_point(id, &[])
    }
}

/// The fixture tree.
///
/// - `add` — one knowledge point with four decidable exemplars, sketches on the
///   three that stay in practice.
/// - `pair` — one knowledge point with two exemplars, the Foundations shape of
///   audit finding (i).
/// - `mixed` — one refused answer, one repeated problem, and three usable
///   statements.
/// - `bar-graph` — the visual heuristic, and `pair` as its prerequisite.
fn tree() -> Curriculum {
    let mut add = plain_topic("add", &[]);
    add.knowledge_points = vec![kp_with(
        "kp1",
        vec![
            exemplar("1 + 1", "2", true),
            exemplar("2 + 2", "4", true),
            exemplar("3 + 3", "6", true),
            exemplar("4 + 4", "8", false),
        ],
    )];

    let mut pair = plain_topic("pair", &[]);
    pair.knowledge_points = vec![kp_with(
        "kp1",
        vec![exemplar("5 + 5", "10", true), exemplar("6 + 6", "12", true)],
    )];

    let mut mixed = plain_topic("mixed", &[]);
    mixed.knowledge_points = vec![kp_with(
        "kp1",
        vec![
            exemplar("7 + 7", "see the diagram", true),
            exemplar("8 + 8", "16", true),
            exemplar("8 + 8", "16", true),
            exemplar("9 + 9", "18", true),
            exemplar("10 + 10", "20", true),
        ],
    )];

    let mut graphing = plain_topic("bar-graph", &[("pair", 0.9, true)]);
    graphing.name = "Read a bar graph".to_owned();
    graphing.knowledge_points = vec![kp_with("kp1", vec![exemplar("read it", "3", true)])];

    graph(vec![add, pair, mixed, graphing])
}

/// A store that approved a teach page, a hint ladder, and `templates`
/// templates, for every serving key of the fixture.
fn stocked(templates: usize) -> MapContent {
    let mut content = MapContent::default();
    for key in ["add/kp1", "pair/kp1", "mixed/kp1", "bar-graph/kp1"] {
        content.insert(key, KIND_TEACH, 1);
        content.insert(key, KIND_HINT_LADDER, 1);
        content.insert(key, KIND_TEMPLATE, templates);
    }
    content
}

#[test]
fn a_stocked_knowledge_point_teaches_practices_and_assesses() {
    let index = ReadinessIndex::build(&tree());
    let set = index.resolve(&stocked(0));
    let add = set.get("add/kp1").expect("the fixture names it");
    assert!(add.teachable && add.hints && add.solutions);
    assert_eq!(add.decidable_exemplars, 4);
    // The last decidable exemplar is held out, so three stay in practice.
    assert_eq!(add.practice_items, 3);
    assert!(add.practicable && add.assessable);
    assert!(add.serves_lesson() && add.serves_review());
    assert!(add.blockers().is_empty());
    assert!(add.lesson_blockers().is_empty());
}

#[test]
fn two_exemplars_neither_practice_nor_assess() {
    let index = ReadinessIndex::build(&tree());
    let set = index.resolve(&stocked(0));
    let pair = set.get("pair/kp1").expect("the fixture names it");
    assert_eq!(pair.decidable_exemplars, 2);
    assert_eq!(pair.practice_items, 2);
    assert!(!pair.practicable && !pair.assessable);
    assert_eq!(
        pair.lesson_blockers(),
        [Blocker::Practicable, Blocker::Assessable]
    );
    assert!(!pair.serves_lesson() && !pair.serves_review());

    // One approved template lifts the practice count to three; the held-out
    // item still needs a third decidable EXEMPLAR, so the lesson stays blocked.
    let stocked = index.resolve(&stocked(1));
    let pair = stocked.get("pair/kp1").expect("the fixture names it");
    assert!(pair.practicable && !pair.assessable);
    assert_eq!(pair.practice_items, 3);
    assert_eq!(pair.lesson_blockers(), [Blocker::Assessable]);
    assert!(pair.serves_review());
}

#[test]
fn a_refused_answer_and_a_repeated_problem_leave_the_decidable_list() {
    let index = ReadinessIndex::build(&tree());
    let facts = index.get("mixed/kp1").expect("the fixture names it");
    // Five authored exemplars: one answer the grammar refuses, one repeated
    // problem statement, three usable items.
    assert_eq!(facts.decidable, [1, 3, 4]);
    assert_eq!(facts.held_out, Some(4));
    let set = index.resolve(&stocked(1));
    let mixed = set.get("mixed/kp1").expect("the fixture names it");
    assert_eq!(mixed.decidable_exemplars, 3);
    assert_eq!(mixed.practice_items, 3);
    assert!(mixed.practicable && mixed.assessable && mixed.serves_lesson());
}

#[test]
fn an_empty_store_blocks_every_knowledge_point_on_the_teach_page() {
    let index = ReadinessIndex::build(&tree());
    let set = index.resolve(&EmptyContent);
    for readiness in set.all() {
        assert!(!readiness.teachable, "{}", readiness.kp_key);
        assert!(!readiness.hints, "{}", readiness.kp_key);
        assert!(!readiness.serves_lesson(), "{}", readiness.kp_key);
        assert!(
            readiness.blockers().contains(&Blocker::Teachable),
            "{}",
            readiness.kp_key
        );
    }
    assert_eq!(set.len(), 4);
    assert!(!set.is_empty());
    assert!(EmptyContent.approved_templates("add/kp1") == 0);
    assert!(!EmptyContent.has_approved("add/kp1", KIND_TEACH));
}

#[test]
fn a_practice_exemplar_without_a_sketch_blocks_the_solutions_condition() {
    let mut short = plain_topic("short", &[]);
    short.knowledge_points = vec![kp_with(
        "kp1",
        vec![
            exemplar("1 + 2", "3", true),
            exemplar("2 + 3", "5", false),
            exemplar("3 + 4", "7", true),
            exemplar("4 + 5", "9", false),
        ],
    )];
    let index = ReadinessIndex::build(&graph(vec![short]));
    let set = index.resolve(&EmptyContent);
    let readiness = set.get("short/kp1").expect("the fixture names it");
    // The held-out exemplar carries no sketch and that is no defect: the audit
    // reads the practice exemplars only. Index 1 is the one that fails.
    assert!(!readiness.solutions);
    assert!(readiness.blockers().contains(&Blocker::Solutions));
    assert!(!readiness.lesson_blockers().contains(&Blocker::Solutions));
}

#[test]
fn a_prerequisite_with_no_practicable_point_blocks_its_dependents() {
    let index = ReadinessIndex::build(&tree());
    let set = index.resolve(&stocked(0));
    // `bar-graph` needs `pair`, and `pair` practices nothing.
    let graphing = set.get("bar-graph/kp1").expect("the fixture names it");
    assert!(!graphing.prerequisites_ok);
    assert!(graphing.blockers().contains(&Blocker::Prerequisites));
    // `add` has no prerequisite at all.
    let add = set.get("add/kp1").expect("the fixture names it");
    assert!(add.prerequisites_ok);
    assert_eq!(index.prerequisites("bar-graph"), ["pair"]);
    assert!(index.prerequisites("add").is_empty());

    // Three approved templates make `pair` practicable, and the dependent
    // clears.
    let stocked = index.resolve(&stocked(3));
    assert!(
        stocked
            .get("bar-graph/kp1")
            .expect("the fixture names it")
            .prerequisites_ok
    );
}

#[test]
fn the_visual_heuristic_reads_the_topic_text() {
    let index = ReadinessIndex::build(&tree());
    let set = index.resolve(&stocked(3));
    let graphing = set.get("bar-graph/kp1").expect("the fixture names it");
    assert!(graphing.visual_needed && !graphing.visual_present);
    assert!(graphing.blockers().contains(&Blocker::Visual));
    let add = set.get("add/kp1").expect("the fixture names it");
    assert!(!add.visual_needed);
    assert!(!add.blockers().contains(&Blocker::Visual));
    assert!(visual_needed("bar-graph"));
}

#[test]
fn the_gate_answers_the_selector_and_ignores_a_key_it_does_not_hold() {
    let index = ReadinessIndex::build(&tree());
    let set = index.resolve(&stocked(0));
    assert!(set.lesson_blockers("add", "kp1").is_empty());
    assert_eq!(
        set.lesson_blockers("pair", "kp1"),
        [Blocker::Practicable, Blocker::Assessable]
    );
    assert!(set.topic_practicable("add"));
    assert!(!set.topic_practicable("pair"));
    // A key of another tree blocks nothing.
    assert!(set.lesson_blockers("ghost", "kp1").is_empty());
    assert!(set.topic_practicable("ghost"));
}

#[test]
fn the_report_counts_the_course_the_topics_and_the_blockers() {
    let index = ReadinessIndex::build(&tree());
    let set = index.resolve(&stocked(0));
    let report = ReadinessReport::build(&index, &set, Some("c"));
    let course = report.course("c").expect("the fixture names one course");
    assert_eq!(course.topics, 4);
    assert_eq!(course.knowledge_points, 4);
    assert_eq!(course.ready, 1);
    assert_eq!(course.blocked, 3);
    assert_eq!(report.totals(), (1, 3));
    let histogram = report.histogram();
    assert_eq!(histogram.get(&Blocker::Practicable), Some(&3));
    assert_eq!(histogram.get(&Blocker::Assessable), Some(&2));
    assert_eq!(histogram.get(&Blocker::Visual), Some(&1));
    assert_eq!(histogram.get(&Blocker::Teachable), None);
    let topic = course
        .topic_reports
        .iter()
        .find(|topic| topic.topic_id == "pair")
        .expect("the fixture names it");
    assert_eq!(topic.course_id, "c");
    assert_eq!(topic.ready, 0);
    assert_eq!(topic.blocked, 1);
    assert_eq!(
        topic.blocker_list(),
        [Blocker::Practicable, Blocker::Assessable]
    );
    assert_eq!(topic.knowledge_points.len(), 1);
    // A course the tree does not hold reports nothing.
    assert!(
        ReadinessReport::build(&index, &set, Some("ghost"))
            .courses
            .is_empty()
    );
    assert_eq!(index.course_of("add"), "c");
    assert_eq!(index.course_of("ghost"), "");
}

#[test]
fn the_blocker_wire_values_round_trip() {
    let names: Vec<&str> = Blocker::every().iter().map(|one| one.as_str()).collect();
    assert_eq!(names, cadus_core::readiness::BLOCKERS);
    assert_eq!(Blocker::Teachable.to_string(), "teachable");
}
