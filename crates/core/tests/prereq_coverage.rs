//! The prerequisite and diagnostic coverage audit over a fixture (unit f10).
//!
//! The real-curriculum run lives in `crates/worker/tests/prereq_coverage.rs`.
//! This file pins the RULES: which edge counts as dangling, which diagnostic
//! item counts as decidable, and what evidence an assumed-mastery topic needs.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod common;

use cadus_core::curriculum::{
    Catalog, Course, Curriculum, Exemplar, KnowledgePoint, PrereqEdge, RawCurriculum, RawUnit,
    Slug, Topic, Unit,
};
use cadus_core::readiness::{DiagnosticState, PrereqCoverage, ReadinessIndex};

/// One slug.
fn slug(id: &str) -> Slug {
    Slug::new(id).unwrap()
}

/// One knowledge point with `count` decidable exemplars.
fn kp(id: &str, count: usize) -> KnowledgePoint {
    KnowledgePoint {
        id: slug(id),
        name: id.to_owned(),
        key_prerequisites: Vec::new(),
        exemplars: (0..count)
            .map(|at| common::solved_exemplar(&format!("{id} item {at}"), &at.to_string()))
            .collect(),
        constraints: None,
        visuals: Vec::new(),
    }
}

/// One topic with its prerequisite ids, its knowledge points, and its
/// diagnostic item.
fn topic(
    id: &str,
    prereqs: &[&str],
    kps: Vec<KnowledgePoint>,
    diagnostic: Option<Exemplar>,
) -> Topic {
    let mut topic = common::plain_topic(id, &[]);
    topic.prerequisites = prereqs
        .iter()
        .map(|prereq| PrereqEdge {
            id: slug(prereq),
            weight: 1.0,
            key: false,
        })
        .collect();
    topic.knowledge_points = kps;
    topic.diagnostic_exemplar = diagnostic;
    topic
}

/// One course of `topics`, with `floor` seeded as mastered.
fn graph(topics: Vec<Topic>, floor: &[&str]) -> Curriculum {
    Curriculum::build(RawCurriculum {
        catalog: Catalog {
            courses: vec![Course {
                id: slug("c"),
                name: "c".to_owned(),
                order: 1,
                mastery_floor: floor.iter().map(|id| slug(id)).collect(),
                mastery_floor_course: None,
            }],
        },
        units: vec![RawUnit {
            course_id: "c".to_owned(),
            file_name: "00-M.yaml".to_owned(),
            unit: Unit {
                unit: "M".to_owned(),
                course: slug("c"),
                module: "M".to_owned(),
                topics,
            },
            first_load_index: 0,
        }],
    })
    .expect("the fixture builds")
}

/// The coverage of one fixture.
fn coverage_of(curriculum: &Curriculum) -> PrereqCoverage {
    PrereqCoverage::build(curriculum, &ReadinessIndex::build(curriculum))
}

/// A curriculum of three topics: `base` with practice, `thin` with two
/// exemplars, and `top` that needs both plus one topic the tree misses.
fn fixture(floor: &[&str]) -> Curriculum {
    graph(
        vec![
            topic(
                "base",
                &[],
                vec![kp("count", 4)],
                Some(common::solved_exemplar("What is 2 + 2?", "4")),
            ),
            topic("thin", &[], vec![kp("halve", 2)], None),
            topic(
                "top",
                &["base", "thin", "ghost"],
                vec![kp("apply", 4)],
                Some(common::solved_exemplar("Simplify.", "as far as it goes")),
            ),
        ],
        floor,
    )
}

#[test]
fn a_prerequisite_the_tree_misses_is_dangling_and_one_with_no_practice_is_unpracticable() {
    let curriculum = fixture(&[]);
    let coverage = coverage_of(&curriculum);
    let top = coverage
        .topics
        .iter()
        .find(|row| row.topic_id == "top")
        .unwrap();

    assert_eq!(top.prerequisites, 3);
    assert_eq!(top.dangling, vec!["ghost".to_owned()]);
    // `base` holds four exemplars: three stay in practice after the held-out
    // one. `thin` holds two, so it never reaches the practice minimum.
    assert_eq!(top.unpracticable, vec!["thin".to_owned()]);

    let counts = coverage.counts("c");
    assert_eq!(counts.topics, 3);
    assert_eq!(counts.practicable_topics, 2);
    assert_eq!(counts.prerequisites, 3);
    assert_eq!(counts.dangling, 1);
    assert_eq!(counts.unpracticable_edges, 1);
}

#[test]
fn the_diagnostic_state_reads_the_authored_item_and_the_answer_grammar() {
    let coverage = coverage_of(&fixture(&[]));
    let state = |id: &str| {
        coverage
            .topics
            .iter()
            .find(|row| row.topic_id == id)
            .unwrap()
            .diagnostic
    };
    assert_eq!(state("base"), DiagnosticState::Decidable);
    assert_eq!(state("thin"), DiagnosticState::Missing);
    // The grammar refuses "as far as it goes", so the probe cannot be graded.
    assert_eq!(state("top"), DiagnosticState::Undecidable);

    let counts = coverage.counts("c");
    assert_eq!(counts.diagnostic_decidable, 1);
    assert_eq!(counts.diagnostic_undecidable, 1);
    assert_eq!(counts.diagnostic_missing, 1);
}

#[test]
fn an_assumed_topic_needs_a_decidable_item_to_confirm_and_practice_to_remediate() {
    let coverage = coverage_of(&fixture(&["base", "thin"]));
    let row = |id: &str| {
        coverage
            .topics
            .iter()
            .find(|topic| topic.topic_id == id)
            .unwrap()
    };

    let base = row("base");
    assert!(base.assumed_mastery());
    assert_eq!(base.assumed_by, vec!["c".to_owned()]);
    assert!(base.floor_evidence().complete());

    // `thin` has no diagnostic item and no practice, so the course assumes it
    // with no evidence at all.
    let thin = row("thin");
    assert!(thin.assumed_mastery());
    assert!(!thin.floor_evidence().confirmable);
    assert!(!thin.floor_evidence().remediable);
    assert!(!thin.floor_evidence().complete());

    assert!(!row("top").assumed_mastery());
    assert!(row("top").assumed_by.is_empty());

    let counts = coverage.counts("c");
    assert_eq!(counts.assumed, 2);
    assert_eq!(counts.assumed_confirmable, 1);
    assert_eq!(counts.assumed_remediable, 1);
    assert_eq!(counts.assumed_without_evidence, 1);
}

#[test]
fn a_curriculum_that_seeds_no_floor_reports_no_assumption() {
    let coverage = coverage_of(&fixture(&[]));
    assert!(coverage.topics.iter().all(|row| !row.assumed_mastery()));
    let counts = coverage.counts("c");
    assert_eq!(counts.assumed, 0);
    assert_eq!(counts.assumed_without_evidence, 0);
    assert_eq!(coverage.courses(), vec!["c".to_owned()]);
    assert!(coverage.course("no-such-course").is_empty());
}
