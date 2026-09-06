//! The visual half of the readiness audit (unit f9).
//!
//! Before unit f9 `Readiness::visual_present` was hardcoded `false`. These tests
//! pin the real rule: an authored visual that passes its own check is a present
//! visual, and a visual the check refuses stays absent. A refused figure never
//! clears the blocker, so a broken picture never makes a knowledge point ready.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_core::curriculum::{Curriculum, Exemplar, KnowledgePoint, Topic};
use cadus_core::readiness::{Blocker, EmptyContent, ReadinessIndex};
use cadus_core::visual::{FractionFigure, NumberLineFigure, VisualSpec};

use common::{graph, knowledge_point, plain_topic};

/// One decidable exemplar with a solution sketch.
fn exemplar(problem: &str, answer: &str) -> Exemplar {
    Exemplar {
        problem: problem.to_owned(),
        answer_contract: None,
        answer: answer.to_owned(),
        solution_sketch: Some("Add the parts.".to_owned()),
    }
}

/// One knowledge point with three decidable exemplars and the given visuals.
fn kp_with_visuals(id: &str, visuals: Vec<VisualSpec>) -> KnowledgePoint {
    let mut kp = knowledge_point(id, &[]);
    kp.exemplars = vec![
        exemplar("1 + 1", "2"),
        exemplar("2 + 2", "4"),
        exemplar("3 + 3", "6"),
    ];
    kp.visuals = visuals;
    kp
}

/// One topic that holds the knowledge points.
fn topic_of(id: &str, kps: Vec<KnowledgePoint>) -> Topic {
    let mut topic = plain_topic(id, &[]);
    topic.knowledge_points = kps;
    topic
}

/// A valid number line from 0 to 5 with a point at 3.
fn good_line() -> VisualSpec {
    VisualSpec::NumberLine(NumberLineFigure::new(0_i64, 5_i64, 1_i64).with_point(3_i64, Some("x")))
}

/// A number line whose range does not ascend.
fn broken_line() -> VisualSpec {
    VisualSpec::NumberLine(NumberLineFigure::new(5_i64, 0_i64, 1_i64))
}

/// The readiness of one serving key over an empty store.
fn readiness_of(curriculum: &Curriculum, kp_key: &str) -> cadus_core::readiness::Readiness {
    ReadinessIndex::build(curriculum)
        .resolve(&EmptyContent)
        .get(kp_key)
        .cloned()
        .expect("the audit names every knowledge point")
}

#[test]
fn an_authored_visual_that_passes_its_check_is_a_present_visual() {
    let curriculum = graph(vec![topic_of(
        "add-whole-numbers",
        vec![kp_with_visuals("count-on", vec![good_line()])],
    )]);
    let readiness = readiness_of(&curriculum, "add-whole-numbers/count-on");
    assert!(readiness.visual_needed);
    assert!(readiness.visual_present);
    assert_eq!(readiness.broken_visuals, 0);
    assert!(!readiness.blockers().contains(&Blocker::Visual));
}

#[test]
fn an_authored_visual_that_fails_its_check_stays_absent_and_keeps_the_blocker() {
    let curriculum = graph(vec![topic_of(
        "add-whole-numbers",
        vec![kp_with_visuals("count-on", vec![broken_line()])],
    )]);
    let readiness = readiness_of(&curriculum, "add-whole-numbers/count-on");
    assert!(readiness.visual_needed);
    assert!(!readiness.visual_present);
    assert_eq!(readiness.broken_visuals, 1);
    assert!(readiness.blockers().contains(&Blocker::Visual));
}

#[test]
fn one_good_visual_beside_a_broken_one_clears_the_blocker_and_reports_the_break() {
    let curriculum = graph(vec![topic_of(
        "add-whole-numbers",
        vec![kp_with_visuals(
            "count-on",
            vec![
                broken_line(),
                good_line(),
                VisualSpec::Fraction(FractionFigure::bar(3, 4)),
            ],
        )],
    )]);
    let readiness = readiness_of(&curriculum, "add-whole-numbers/count-on");
    assert!(readiness.visual_present);
    assert_eq!(readiness.broken_visuals, 1);
    assert!(!readiness.blockers().contains(&Blocker::Visual));
}

#[test]
fn a_knowledge_point_with_no_authored_visual_and_plain_text_needs_none() {
    let curriculum = graph(vec![topic_of(
        "add-whole-numbers",
        vec![kp_with_visuals("count-on", Vec::new())],
    )]);
    let readiness = readiness_of(&curriculum, "add-whole-numbers/count-on");
    assert!(!readiness.visual_needed);
    assert!(!readiness.visual_present);
    assert_eq!(readiness.broken_visuals, 0);
    assert!(!readiness.blockers().contains(&Blocker::Visual));
}

#[test]
fn the_word_heuristic_still_reports_a_topic_that_names_a_picture_and_authors_none() {
    let curriculum = graph(vec![topic_of(
        "read-a-bar-graph",
        vec![kp_with_visuals("read-one-bar", Vec::new())],
    )]);
    let readiness = readiness_of(&curriculum, "read-a-bar-graph/read-one-bar");
    assert!(readiness.visual_needed);
    assert!(!readiness.visual_present);
    assert!(readiness.blockers().contains(&Blocker::Visual));
}

#[test]
fn the_audit_counts_the_visual_facts_of_every_knowledge_point_of_a_topic() {
    let curriculum = graph(vec![topic_of(
        "add-whole-numbers",
        vec![
            kp_with_visuals("count-on", vec![good_line()]),
            kp_with_visuals("count-back", vec![broken_line()]),
            kp_with_visuals("count-by-two", Vec::new()),
        ],
    )]);
    let index = ReadinessIndex::build(&curriculum);
    let facts: Vec<(usize, usize)> = index
        .topic("add-whole-numbers")
        .iter()
        .map(|kp| (kp.valid_visuals, kp.broken_visuals))
        .collect();
    assert_eq!(facts, vec![(1, 0), (0, 1), (0, 0)]);
}

#[test]
fn a_knowledge_point_reads_its_visuals_out_of_the_authored_yaml() {
    let text = "\
id: plot-a-point
name: Plot a point on a number line
visuals:
  - kind: number_line
    min: 0
    max: 1
    tick: 1/4
    caption: Plot three quarters
    points:
      - at: 3/4
        label: A
  - kind: fraction
    parts: 4
    shaded: 3
    shape: circle
";
    let kp: KnowledgePoint = serde_norway::from_str(text).unwrap();
    assert_eq!(kp.visuals.len(), 2);
    assert!(kp.visuals.iter().all(|visual| visual.validate().is_ok()));
    assert_eq!(kp.visuals[0].kind(), "number_line");
    assert_eq!(
        kp.visuals[0].text_equivalent(),
        "Plot three quarters. A number line from 0 to 1 with a tick every 1/4. \
         A filled point at 3/4, labeled A."
    );
    assert!(kp.visuals[1].text_equivalent().contains("4 equal sectors"));
}

#[test]
fn a_knowledge_point_with_no_visuals_key_loads_and_serializes_without_one() {
    let kp: KnowledgePoint = serde_norway::from_str("id: kp\nname: K\n").unwrap();
    assert!(kp.visuals.is_empty());
    let text = serde_norway::to_string(&kp).unwrap();
    assert!(!text.contains("visuals"));
}
