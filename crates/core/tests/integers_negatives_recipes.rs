//! `apply_integers_negatives_recipes.py` raised every one of
//! `02-integers-negatives.yaml`'s 46 knowledge points to at least four
//! decidable exemplars, each with a per-KP recipe encoding that KP's own
//! authored constraint (never the coarse operator-only generator this
//! curriculum's `foundations_drafts.py`/`apply_foundations_held_out_exemplars.py`
//! stay fail-closed on). This test checks the same curriculum-level
//! consequence the sibling `arithmetic_core_recipes.rs`/`radical_core_recipes.rs`
//! tests check, against the real curriculum and the real [`ReadinessIndex`]:
//! `scripts/authoring/test_apply_integers_negatives_recipes.py` is the
//! semantic table proving each new exemplar's answer obeys its own
//! independently re-derived arithmetic.
//!
//! The second test below is this unit's adversarial negative control: for
//! one exemplar per answer SHAPE this file introduces or repairs (a bare
//! signed integer, a fraction, a `label` relation symbol, a `label`
//! true/false judgment, a `label` sign-word judgment, and an
//! `ascending_chain`), a deliberately wrong learner answer must grade
//! incorrect or undecidable — never silently accepted. This is what proves
//! the checker discriminates, not merely that a string parses.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use cadus_core::answer::{AnswerContract, Outcome, check, check_contract};
use cadus_core::curriculum::{AnswerKind, load_curriculum};
use cadus_core::readiness::ReadinessIndex;

#[test]
fn every_recipe_kp_is_practicable_assessable_and_has_solutions() {
    let curriculum_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("curriculum");
    let (curriculum, findings) = load_curriculum(&curriculum_dir).unwrap();
    assert!(findings.is_empty(), "{findings:#?}");
    let readiness = ReadinessIndex::build(&curriculum);
    let keys: Vec<String> =
        serde_json::from_str(include_str!("fixtures/integers_negatives_recipes_kps.json")).unwrap();
    assert_eq!(keys.len(), 46);

    for key in keys {
        assert!(
            readiness.get(&key).is_some(),
            "{key}: absent from curriculum"
        );
        let facts = readiness.get(&key).unwrap();
        assert!(
            facts.decidable.len() >= 4,
            "{key}: {} decidable exemplars",
            facts.decidable.len()
        );
        assert!(facts.held_out.is_some(), "{key}: no held-out exemplar");
        assert!(
            facts.practice_exemplars() >= 3,
            "{key}: {} practice exemplars",
            facts.practice_exemplars()
        );
        assert!(facts.solutions, "{key}: solutions blocker remains");
    }
}

fn correct_contract(expected: &str, learner: &str, json: &str) -> bool {
    let contract: AnswerContract = serde_json::from_str(json).expect("valid contract");
    matches!(check_contract(expected, learner, contract), Outcome::Decided(verdict) if verdict.correct)
}

fn incorrect_contract(expected: &str, learner: &str, json: &str) -> bool {
    let contract: AnswerContract = serde_json::from_str(json).expect("valid contract");
    matches!(check_contract(expected, learner, contract), Outcome::Decided(v) if !v.correct)
}

#[test]
fn adversarial_negative_controls_reject_a_wrong_answer_per_shape() {
    // Bare signed integer (no contract): `adding-integers/kp1`'s new
    // both-positive exemplar, `Compute $8 + 5$.` -> `13`.
    assert!(matches!(
        check("13", "13", AnswerKind::Numeric),
        Outcome::Decided(v) if v.correct
    ));
    assert!(matches!(
        check("13", "-13", AnswerKind::Numeric),
        Outcome::Decided(v) if !v.correct
    ));
    assert!(matches!(
        check("13", "12", AnswerKind::Numeric),
        Outcome::Decided(v) if !v.correct
    ));

    // A signed fraction (no contract): `adding-subtracting-negative-fractions/kp1`'s
    // new both-negative exemplar, `-5/12 + (-1/4)` -> `-2/3`.
    assert!(matches!(
        check("-2/3", "-2/3", AnswerKind::Expression),
        Outcome::Decided(v) if v.correct
    ));
    assert!(matches!(
        check("-2/3", "2/3", AnswerKind::Expression),
        Outcome::Decided(v) if !v.correct
    ));

    // `label` relation symbol: `comparing-integers/kp1`'s repaired `<`/`>`
    // fill-in-the-blank shape.
    let lt_gt = r#"{"kind":"label","options":[["<"],[">"]]}"#;
    assert!(correct_contract("<", "<", lt_gt));
    assert!(!correct_contract("<", ">", lt_gt));
    assert!(incorrect_contract("<", "less than", lt_gt));

    // `label` true/false: `comparing-integers/kp2`'s new true-judgment
    // exemplar, `True or false: $-3 < 2$.` -> `true`.
    let true_false = r#"{"kind":"label","options":[["true"],["false"]]}"#;
    assert!(correct_contract("true", "true", true_false));
    assert!(!correct_contract("true", "false", true_false));

    // `label` sign word: `integer-multiplication-division/kp3`'s repaired
    // "positive or negative" judgment.
    let pos_neg = r#"{"kind":"label","options":[["positive"],["negative"]]}"#;
    assert!(correct_contract("negative", "negative", pos_neg));
    assert!(!correct_contract("negative", "positive", pos_neg));

    // `ascending_chain`: `comparing-integers/kp2`'s repaired 3-term chain and
    // new all-negative chain.
    let chain = r#"{"kind":"ascending_chain"}"#;
    assert!(correct_contract("-4 < -1 < 3", "-4 < -1 < 3", chain));
    assert!(!correct_contract("-4 < -1 < 3", "-4 < -1 < 2", chain));
    assert!(!correct_contract("-4 < -1 < 3", "3 < -1 < -4", chain));
    assert!(correct_contract("-8 < -5 < -2", "-8<-5<-2", chain));
    assert!(!correct_contract("-8 < -5 < -2", "-2 < -5 < -8", chain));

    // The two contract fixes read the exemplar's OWN authored answer as the
    // "expected" side (not a value this test invents), proving the repair
    // itself is not just decidable but discriminating.
    assert!(!correct_contract("<", "-8 > -6", lt_gt));
}
