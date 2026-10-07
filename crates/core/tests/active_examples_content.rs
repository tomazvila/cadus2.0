//! Step 5a content: the authored active worked examples of the tree.
//!
//! The loader already refuses a malformed block. This file pins what the
//! loader cannot see: each try-first answer grades correct under its own
//! contract, a plausible miss grades incorrect, and the ten authored topics
//! carry their blocks (the owner's next eighty lessons).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_core::answer::{Outcome, check_contract};
use common::paths::tree;

/// The owner's next eighty Foundations lessons, each authored with a step check
/// on every knowledge point.
const AUTHORED: [&str; 80] = [
    "divisibility-rules",
    "prime-composite-numbers",
    "prime-factorization",
    "translating-phrases-to-expressions",
    "translating-sentences-to-equations",
    "basic-absolute-value-equations",
    "equation-word-problems",
    "absolute-value-equations",
    "consecutive-integer-problems",
    "equations-special-cases",
    "least-common-multiple",
    "adding-subtracting-fractions",
    "understanding-ratios",
    "unit-rates",
    "ratios-proportions",
    "improper-fractions-mixed-numbers",
    "mixed-numbers",
    "decimal-multiplication-powers-of-ten",
    "percent-conversions",
    "percent-of-a-number",
    "percentages",
    "fractions-on-number-line",
    "percent-finding-the-whole",
    "comparing-ordering-fractions",
    "dividing-mixed-numbers",
    "gcf-lcm",
    "equations-with-fractions",
    "equations-with-decimals",
    "equivalent-expressions",
    "fraction-word-problems",
    "comparing-ordering-whole-numbers",
    "comparing-ordering-decimals",
    "rounding-whole-numbers",
    "percent-applications",
    "rounding-estimation",
    "factoring-linear-expressions",
    "ratio-tables-equivalent-ratios",
    "perfect-square-roots",
    "square-roots",
    "exponent-product-rule",
    "exponent-quotient-rule",
    "exponent-product-quotient-rules",
    "simplifying-radicals",
    "power-of-a-power-rule",
    "power-rule-exponents",
    "zero-exponent-rule",
    "negative-zero-exponents",
    "pythagorean-theorem",
    "cube-roots",
    "adding-subtracting-radicals",
    "scientific-notation-conversion",
    "dividing-radicals",
    "radical-exponent-conversion",
    "radical-operations",
    "scientific-notation",
    "rational-exponents",
    "rationalizing-denominators",
    "simplifying-radicals-variables",
    "polynomial-basics",
    "multiplying-monomials-polynomials",
    "adding-polynomials",
    "multiplying-binomials",
    "polynomial-addition-subtraction",
    "polynomial-multiplication",
    "gcf-of-monomials",
    "factoring-gcf",
    "factoring-monic-trinomials",
    "difference-of-squares",
    "factoring-by-grouping",
    "factoring-trinomials",
    "zero-product-property",
    "quadratic-equations-factoring",
    "special-products",
    "perfect-square-trinomials",
    "square-root-property",
    "completing-the-square",
    "evaluating-polynomials",
    "applying-the-quadratic-formula",
    "dividing-polynomials-by-monomials",
    "quadratic-formula",
];

/// A wrong answer of the same shape as `answer`.
fn plausible_miss(answer: &str) -> String {
    match answer {
        "yes" => "no".to_owned(),
        "0" => "1".to_owned(),
        "{-23,23}" => "{23}".to_owned(),
        other => format!("{other}+1"),
    }
}

#[test]
fn every_try_first_grades_its_own_answer_and_refuses_a_miss() {
    let graph = tree();
    let mut seen = 0;
    for topic in graph.topics() {
        for point in &topic.knowledge_points {
            let Some(first) = &point.try_first else {
                continue;
            };
            seen += 1;
            let key = format!("{}/{}", topic.id, point.id);
            assert!(
                matches!(
                    check_contract(&first.answer, &first.answer, first.answer_contract.clone()),
                    Outcome::Decided(verdict) if verdict.correct
                ),
                "{key}: the try-first answer does not grade correct"
            );
            let miss = plausible_miss(&first.answer);
            assert!(
                matches!(
                    check_contract(&first.answer, &miss, first.answer_contract.clone()),
                    Outcome::Decided(verdict) if !verdict.correct
                ),
                "{key}: the miss {miss:?} does not grade incorrect"
            );
        }
    }
    // 627 on 2026-10-07 after the courses rewrite (was 44).
    assert_eq!(seen, 627, "the authored try-first count moved");
}

#[test]
fn the_eighty_authored_topics_carry_a_step_check_on_every_point() {
    let graph = tree();
    for id in AUTHORED {
        let idx = graph
            .idx_of(id)
            .unwrap_or_else(|| panic!("{id} is in the tree"));
        for point in graph.knowledge_points(idx) {
            assert!(
                point.step_check.is_some(),
                "{id}/{} has no step check",
                point.id
            );
        }
    }
}

/// The most correct answers one option position may hold across the tree, in
/// percent.
const MAX_POSITION_SHARE_PERCENT: usize = 40;

#[test]
fn the_correct_option_position_is_spread_across_the_tree_and_each_topic() {
    let graph = tree();
    let mut per_position = [0_usize; cadus_core::curriculum::active::STEP_CHECK_MAX_OPTIONS];
    let mut total = 0;
    let mut same_everywhere = Vec::new();
    for topic in graph.topics() {
        let positions: Vec<usize> = topic
            .knowledge_points
            .iter()
            .filter_map(|point| point.step_check.as_ref())
            .map(|check| {
                check
                    .options
                    .iter()
                    .position(|option| check.is_correct(option))
                    .expect("the loader keeps the answer among the options")
            })
            .collect();
        for &position in &positions {
            per_position[position] += 1;
            total += 1;
        }
        if positions.len() >= 3 && positions.iter().all(|&position| position == positions[0]) {
            same_everywhere.push(format!("{} (option {})", topic.id, positions[0] + 1));
        }
    }
    assert!(total > 0, "the tree carries step checks");
    for (index, count) in per_position.iter().enumerate() {
        assert!(
            count * 100 <= total * MAX_POSITION_SHARE_PERCENT,
            "option {} holds {count} of {total} correct answers, more than \
             {MAX_POSITION_SHARE_PERCENT}%; move some answers to another position",
            index + 1
        );
    }
    assert!(
        same_everywhere.is_empty(),
        "these topics put the correct option at the same position in every step check: \
         {same_everywhere:?}"
    );
}
