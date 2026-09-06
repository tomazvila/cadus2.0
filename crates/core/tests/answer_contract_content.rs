//! Contracts reach curriculum validation and template instances (D-F1, C5).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use cadus_core::answer::{AnswerContract, Outcome, check_contract};
use cadus_core::curriculum::{AnswerKind, lint_curriculum, load_raw_curriculum};
use cadus_core::template::{Compiled, GateSpec, gate, rng_from_seed};
use common::gate::{body_with, doc_of, exemplars};
use common::paths::curriculum_root;
use common::scratch::ScratchTree;

#[test]
fn a_multi_step_template_uses_its_reviewed_contract() {
    let body = body_with(&[
        ("answer_kind", r#""multi-step""#),
        ("answer_contract", r#"{"kind":"exact"}"#),
    ]);
    let doc = doc_of(&body);
    let items = exemplars(&["49", "81"]);
    let spec = GateSpec {
        answer_kind: AnswerKind::MultiStep,
        exemplars: &items,
    };
    gate(&doc, &spec).expect("the complete template gate accepts this representation");
    let instance = Compiled::new(&doc)
        .unwrap()
        .draw(&mut rng_from_seed(2))
        .unwrap();
    assert_eq!(instance.answer_contract, Some(AnswerContract::Exact));
    assert!(
        matches!(check_contract(&instance.answer, &instance.answer, instance.answer_contract.unwrap()), Outcome::Decided(v) if v.correct)
    );
    let mut missing = doc.clone();
    missing.answer_contract = None;
    assert!(gate(&missing, &spec).is_err());
    missing.answer_contract = Some(AnswerContract::None);
    assert!(gate(&missing, &spec).is_err());
}

#[test]
fn the_nineteen_inventory_topics_have_explicit_usable_exact_items() {
    let (raw, findings) = load_raw_curriculum(&curriculum_root()).unwrap();
    assert!(findings.is_empty(), "{findings:?}");
    let mut topics = 0;
    let mut items = 0;
    for entry in raw.topics() {
        let annotated: Vec<_> = entry
            .topic
            .knowledge_points
            .iter()
            .flat_map(|kp| &kp.exemplars)
            .filter(|item| item.answer_contract == Some(AnswerContract::Exact))
            .collect();
        if annotated.is_empty() {
            continue;
        }
        if entry.topic.answer_kind != AnswerKind::MultiStep {
            continue;
        }
        topics += 1;
        for item in annotated {
            assert_eq!(item.answer_contract, Some(AnswerContract::Exact));
            item.canonical_answer().unwrap();
            assert!(
                matches!(check_contract(&item.answer, &item.answer, AnswerContract::Exact), Outcome::Decided(v) if v.correct)
            );
            items += 1;
        }
    }
    assert_eq!(topics, 19);
    assert_eq!(items, 108);
    assert!(lint_curriculum(&curriculum_root()).is_empty());
}

#[test]
fn the_loader_and_lint_refuse_invalid_contracts_and_expected_values() {
    let tree = ScratchTree::new("answer-contract");
    tree.courses(&["demo"]);
    for (contract, expected, code) in [
        ("{kind: approx, decimals: 19}", "1/3", "schema"),
        ("{kind: exact, tolerance: 1}", "1", "schema"),
        ("{kind: exact}", "garbage", "answer_contract"),
        ("{kind: approx, decimals: 2}", "x", "answer_contract"),
    ] {
        tree.unit("demo/01-unit.yaml", "demo", &[("item", &format!(
            "    knowledge_points:\n      - id: kp1\n        name: Point\n        exemplars:\n          - problem: Question\n            answer: '{expected}'\n            answer_contract: {contract}\n"
        ))]);
        let findings = lint_curriculum(tree.root());
        assert!(
            findings.iter().any(|finding| finding.code == code),
            "{findings:?}"
        );
    }
}
