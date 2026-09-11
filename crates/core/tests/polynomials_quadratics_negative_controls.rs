//! Adversarial semantic negative controls for `07-polynomials-quadratics.yaml`.
//!
//! P2.4/P2.6 close on evidence that the grammar can DECIDE an authored
//! answer. That is not evidence the checker actually tells a wrong answer
//! from a right one. Each case here takes one real authored exemplar,
//! submits a plausible WRONG answer a student who made one specific,
//! named mistake would give, and asserts the checker marks it incorrect
//! (or, for the complex-root case, asserts it is never silently accepted
//! as correct) — never that the string merely fails to parse.
#![allow(clippy::unwrap_used, clippy::panic)]
use std::path::Path;

use cadus_core::answer::{Outcome, Verdict, check, check_contract};
use cadus_core::curriculum::{Curriculum, load_curriculum};

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The exemplar at `(topic_id, kp_id, index)`, and its topic's `answer_kind`.
fn exemplar<'c>(
    curriculum: &'c Curriculum,
    topic_id: &str,
    kp_id: &str,
    index: usize,
) -> (
    &'c cadus_core::curriculum::Exemplar,
    cadus_core::curriculum::AnswerKind,
) {
    let topic = curriculum
        .topics()
        .iter()
        .find(|t| t.id.as_str() == topic_id)
        .unwrap_or_else(|| panic!("missing topic {topic_id}"));
    let kp = topic
        .knowledge_points
        .iter()
        .find(|kp| kp.id.as_str() == kp_id)
        .unwrap_or_else(|| panic!("missing kp {topic_id}/{kp_id}"));
    (&kp.exemplars[index], topic.answer_kind)
}

/// Grade `wrong` against the exemplar's own authored answer and policy,
/// the same dispatch `crates/web/src/grade/verdict.rs::grade_item` uses:
/// an explicit contract wins, otherwise the topic's `answer_kind` decides.
fn grade(
    exemplar: &cadus_core::curriculum::Exemplar,
    kind: cadus_core::curriculum::AnswerKind,
    wrong: &str,
) -> Outcome {
    match &exemplar.answer_contract {
        Some(contract) => check_contract(&exemplar.answer, wrong, contract.clone()),
        None => check(&exemplar.answer, wrong, kind),
    }
}

fn assert_marked_incorrect(outcome: Outcome, case: &str) {
    match outcome {
        Outcome::Decided(Verdict { correct: false, .. }) => {}
        other => panic!("{case}: expected a decided-incorrect verdict, got {other:?}"),
    }
}

#[test]
fn polynomial_arithmetic_rejects_a_forgotten_constant_term() {
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    // "Add (2x + 9) + (5x - 3)." = 7x + 6; a learner who adds the x-terms
    // but drops the constants entirely answers "7x".
    let (item, kind) = exemplar(&curriculum, "adding-polynomials", "kp1", 0);
    assert_eq!(item.answer, "7x + 6");
    assert_marked_incorrect(grade(item, kind, "7x"), "dropped constant");
}

#[test]
fn factoring_rejects_a_wrong_factor_pair_by_full_expansion() {
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    // "Factor x^2 + 7x + 12." = (x+3)(x+4); (x+2)(x+6) also has product 12
    // but sums to 8, not 7 — the checker must EXPAND both sides, not just
    // pattern-match parenthesized factors.
    let (item, kind) = exemplar(&curriculum, "factoring-monic-trinomials", "kp1", 0);
    assert_eq!(item.answer, "(x + 3)(x + 4)");
    assert_marked_incorrect(
        grade(item, kind, "(x + 2)(x + 6)"),
        "wrong factor pair, same product",
    );
}

#[test]
fn zero_product_property_rejects_a_sign_flipped_root() {
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    // "Solve (x - 3)(x + 5) = 0." roots are 3 and -5; a learner who reads
    // the sign of the second factor wrong answers "3 or 5".
    let (item, kind) = exemplar(&curriculum, "zero-product-property", "kp1", 0);
    assert_eq!(item.answer, "x = 3 or x = -5");
    assert_marked_incorrect(grade(item, kind, "x = 3 or x = 5"), "sign-flipped root");
}

#[test]
fn discriminant_repeated_root_multiplicity_rejects_a_miscount() {
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    // "How many real solutions does x^2 - 6x + 9 = 0 have?" disc = 0, one
    // REPEATED root, so the multiplicity-aware answer is "1", not "2".
    let (item, kind) = exemplar(&curriculum, "discriminant", "kp2", 0);
    assert_eq!(item.answer, "1");
    assert_marked_incorrect(
        grade(item, kind, "2"),
        "repeated root miscounted as two distinct roots",
    );
}

#[test]
fn discriminant_rejects_the_b_squared_only_shortcut() {
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    // "Find the discriminant of x^2 + 3x + 2." = 9 - 8 = 1; a learner who
    // forgets the -4ac term answers the bare b^2 = 9.
    let (item, kind) = exemplar(&curriculum, "discriminant", "kp1", 0);
    assert_eq!(item.answer, "1");
    assert_marked_incorrect(grade(item, kind, "9"), "b^2 alone, -4ac dropped");
}

#[test]
fn completing_the_square_rejects_a_sign_error_on_both_roots() {
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    // "Solve x^2 + 6x + 5 = 0 by completing the square." roots -1, -5; the
    // sign-flipped pair "1 or 5" is a common completing-the-square mistake
    // (forgetting the vertex form subtracts, not adds, the shift).
    let (item, kind) = exemplar(&curriculum, "completing-the-square", "kp2", 0);
    assert_eq!(item.answer, "x = -1 or x = -5");
    assert_marked_incorrect(
        grade(item, kind, "x = 1 or x = 5"),
        "sign-flipped root pair",
    );
}

#[test]
fn vertex_reading_rejects_a_sign_error_on_k() {
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    // "Find the vertex of y = x^2 - 6x + 5." vertex (3, -4); a learner who
    // forgets the minus when evaluating y at the vertex answers (3, 4).
    let (item, kind) = exemplar(&curriculum, "quadratic-graphs-vertex", "kp1", 0);
    assert_eq!(item.answer, "(3, -4)");
    assert_marked_incorrect(grade(item, kind, "(3, 4)"), "sign-flipped y at the vertex");
}

#[test]
fn parabola_direction_label_rejects_the_opposite_direction() {
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    // "For y = -3(x - 4)^2 + 8..." a = -3 < 0, so
    // it opens downward; the opposite-direction label must be rejected by
    // the multipart contract's label sub-check, not just its exact half.
    let (item, kind) = exemplar(&curriculum, "parabola-vertex-form", "kp2", 0);
    assert_eq!(item.answer, "direction = downward; extreme_value = 8");
    assert_marked_incorrect(
        grade(item, kind, "direction = upward; extreme_value = 8"),
        "opposite opening direction",
    );
}

#[test]
fn negative_discriminant_count_never_accepts_two_as_correct() {
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    // Complex-answer policy: "How many real solutions does x^2 + 2x + 5 = 0
    // have?" has a negative discriminant (complex roots), and this
    // Foundations file's policy is to report the REAL-root count "0", never
    // a complex pair. A learner who counts the two complex roots as if they
    // were real answers "2"; that must never be marked correct.
    let (item, kind) = exemplar(&curriculum, "quadratic-formula", "kp3", 1);
    assert_eq!(item.answer, "0");
    assert!(!matches!(
        grade(item, kind, "2"),
        Outcome::Decided(Verdict { correct: true, .. })
    ));
}

#[test]
fn negative_discriminant_count_never_accepts_a_complex_number_as_correct() {
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    // The same item never accepts literal complex-number notation either;
    // this file authors no complex-number answers anywhere, and a learner
    // who writes the roots directly must not be marked correct by accident.
    let (item, kind) = exemplar(&curriculum, "quadratic-formula", "kp3", 1);
    assert!(!matches!(
        grade(item, kind, "x = -1 + 2i or x = -1 - 2i"),
        Outcome::Decided(Verdict { correct: true, .. })
    ));
}
