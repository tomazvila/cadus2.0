//! Serving and grading evidence for the independently audit-clean symbolic slice.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
use std::path::Path;

use cadus_core::answer::{AnswerContract, Outcome, check_contract};
use cadus_core::curriculum::{Curriculum, load_curriculum};
use cadus_core::readiness::ReadinessIndex;

fn curriculum() -> Curriculum {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../curriculum");
    load_curriculum(&root).unwrap().0
}

fn item<'a>(
    curriculum: &'a Curriculum,
    key: &str,
    index: usize,
) -> &'a cadus_core::curriculum::Exemplar {
    let (topic_id, kp_id) = key.split_once('/').unwrap();
    let topic = curriculum
        .topics()
        .iter()
        .find(|topic| topic.id.as_str() == topic_id)
        .unwrap();
    let kp = topic
        .knowledge_points
        .iter()
        .find(|kp| kp.id.as_str() == kp_id)
        .unwrap();
    &kp.exemplars[index]
}

#[test]
fn reviewed_kps_are_practicable_assessable_and_complete() {
    let curriculum = curriculum();
    let index = ReadinessIndex::build(&curriculum);
    let keys: Vec<String> =
        serde_json::from_str(include_str!("fixtures/symbolic_reviewed_kps.json")).unwrap();
    assert_eq!(keys.len(), 36);
    for key in &keys {
        let facts = index.get(key).unwrap_or_else(|| panic!("{key} absent"));
        assert!(
            facts.practice_exemplars() >= 3,
            "{key} has too little practice"
        );
        assert!(facts.held_out.is_some(), "{key} has no held-out item");
        for position in 0..facts.decidable.len() {
            let exemplar = item(&curriculum, key, position);
            assert!(
                exemplar
                    .solution_sketch
                    .as_deref()
                    .is_some_and(|text| !text.trim().is_empty()),
                "{key}[{position}] lacks a sketch"
            );
            if let Some(AnswerContract::Label { options }) = &exemplar.answer_contract {
                assert!(
                    options.len() >= 2,
                    "{key}[{position}] has a singleton label"
                );
            }
        }
    }
}

fn grade(curriculum: &Curriculum, key: &str, index: usize, learner: &str) -> Outcome {
    let exemplar = item(curriculum, key, index);
    check_contract(
        &exemplar.answer,
        learner,
        exemplar
            .answer_contract
            .clone()
            .expect("reviewed row needs a contract"),
    )
}

#[test]
fn interval_endpoints_are_graded_as_sets() {
    let curriculum = curriculum();
    for (kp, index, wrong) in [
        ("kp2", 0, "[2, ∞)"),
        ("kp2", 1, "(-∞, -1)"),
        ("kp2", 2, "x > 4"),
        ("kp2", 3, "x < 3"),
        ("kp3", 0, "(-∞, -2] ∪ (4, ∞)"),
        ("kp3", 1, "(-∞, -2) ∪ (3, ∞)"),
        ("kp3", 2, "x <= 1 or x >= 5"),
        ("kp3", 3, "(-∞, -5) ∪ (4, ∞)"),
    ] {
        let key = format!("interval-notation/{kp}");
        let exemplar = item(&curriculum, &key, index);
        assert!(matches!(
            exemplar.answer_contract,
            Some(AnswerContract::InequalityUnion)
        ));
        assert!(matches!(grade(&curriculum, &key, index, &exemplar.answer),
            Outcome::Decided(verdict) if verdict.correct));
        assert!(matches!(grade(&curriculum, &key, index, wrong),
            Outcome::Decided(verdict) if !verdict.correct));
    }
}

#[test]
fn interpretation_choices_are_visible_and_non_singleton() {
    let curriculum = curriculum();
    for key in [
        "interpreting-graphs-qualitatively/kp1",
        "interpreting-graphs-qualitatively/kp2",
        "interpreting-graphs-qualitatively/kp3",
        "interpreting-linear-models/kp1",
        "interpreting-linear-models/kp2",
    ] {
        for index in 0..4 {
            let exemplar = item(&curriculum, key, index);
            assert!(
                exemplar.problem.contains(" or "),
                "{key}[{index}] hides its choices"
            );
            let Some(AnswerContract::Label { options }) = &exemplar.answer_contract else {
                panic!("{key}[{index}] must be a label");
            };
            assert!(options.len() >= 2);
            assert!(
                options
                    .iter()
                    .flatten()
                    .any(|alias| alias == &exemplar.answer)
            );
        }
    }
}
