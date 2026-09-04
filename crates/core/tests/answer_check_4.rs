//! Part 4 of the `answer_check` tests. The header of `answer_check_1.rs` names the sources.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use common::check::*;

#[test]
fn the_corpus_canonicalization_holds_the_l2_budget() {
    // The self-check above stops on the string rung, so it never reaches the
    // arithmetic. This test drives the whole path and asserts the same budget.
    //
    // Measured after the rational-function form of M2 review 3 (release build,
    // this machine): 11.3 ms for the 3,492 answers, and 129.9 µs for the worst
    // one, `24(2x + 1)^2 (1 + (2x + 1)^3)^3`. The worst rational-function PAIR
    // is `(3x + 8)/((x - 4)(x + 4))` against `5/(2(x - 4)) + 1/(2(x + 4))` at
    // 47.2 µs, so the new products cost about one third of the worst answer.
    let corpus = corpus();
    let mut worst = Duration::ZERO;
    let mut worst_answer = "";
    let start = Instant::now();
    for row in &corpus {
        let one = Instant::now();
        let _ = canonical_form(&row.answer);
        let elapsed = one.elapsed();
        if elapsed > worst {
            worst = elapsed;
            worst_answer = &row.answer;
        }
    }
    let total = start.elapsed();
    let corpus_budget = corpus_budget();
    assert!(
        total < corpus_budget,
        "the 3,492 canonicalizations took {total:?}, and the budget is {corpus_budget:?}"
    );
    let one_check_budget = one_check_budget();
    assert!(
        worst < one_check_budget,
        "the longest canonicalization took {worst:?} on {worst_answer:?}, \
         and the budget is {one_check_budget:?}"
    );
}

#[test]
fn the_reciprocal_bomb_of_review_1_is_refused_inside_the_budget() {
    // M2 review 1, findings 5 and 11. The content normalization folded an
    // unbounded least common multiple, and this one answer cost 8.4 s of CPU in
    // a release build. The fold now runs the size bound and the width charge
    // after every step.
    let bomb = reciprocal_bomb(143);
    assert_eq!(bomb.chars().count(), 2_072, "the reviewer's answer length");
    let start = Instant::now();
    let outcome = check("1", &bomb, E);
    let elapsed = start.elapsed();
    assert!(
        matches!(outcome, Outcome::Undecidable(_)),
        "the bomb gave {outcome:?}"
    );
    let one_check_budget = one_check_budget();
    assert!(
        elapsed < one_check_budget,
        "the 2,072-character bomb took {elapsed:?}, and the budget is {one_check_budget:?}"
    );
}

#[test]
fn the_sum_rebuild_bomb_of_review_4_is_refused_inside_the_budget() {
    // M2 review 4, finding 2. The work bound charges one step per term the
    // arithmetic touches now, so the rebuild of a wide sum costs what it is
    // worth. The reviewer's pair cost 378 ms in a release build, which is 1.26
    // times the whole 300 ms of L2 for one deterministic grade.
    let expected = sum_rebuild_bomb(846);
    assert_eq!(
        expected.chars().count(),
        3_267,
        "the reviewer's first answer length"
    );
    let learner = expected.replacen('(', "(0+", 1);
    assert_eq!(
        learner.chars().count(),
        3_269,
        "the reviewer's second answer length"
    );
    let budget = bomb_budget();
    let start = Instant::now();
    let outcome = check(&expected, &learner, E);
    let elapsed = start.elapsed();
    match &outcome {
        Outcome::Undecidable(refused) => {
            assert_eq!(refused.reason, "the answer goes past the work bound");
        }
        other => panic!("the bomb gave {other:?}"),
    }
    assert!(
        elapsed < budget,
        "the 3,267-character pair took {elapsed:?}, and the budget is {budget:?}"
    );
    // The learner side alone reaches the same cost with a short authored answer,
    // and the learner writes that side.
    let learner_bomb = sum_rebuild_bomb(1_212);
    assert_eq!(
        learner_bomb.chars().count(),
        3_999,
        "the learner-side answer length"
    );
    let start = Instant::now();
    let outcome = check("42", &learner_bomb, N);
    let elapsed = start.elapsed();
    match &outcome {
        Outcome::Undecidable(refused) => {
            assert_eq!(refused.reason, "the answer goes past the work bound");
        }
        other => panic!("the learner-side bomb gave {other:?}"),
    }
    assert!(
        elapsed < budget,
        "the 3,999-character learner answer took {elapsed:?}, and the budget is {budget:?}"
    );
}

#[test]
fn a_wide_sum_costs_its_terms_and_a_narrow_one_does_not() {
    // M2 review 4, finding 2. The charge is one step per term touched, so an
    // ordinary answer of a few terms keeps the cost it had, and a sum of
    // hundreds of terms pays for every rebuild. The two assertions below hold
    // the rule in both directions.
    //
    // A 20-term sum with one factor of `1` is inside the budget and decides.
    let sum = wide_sum(20);
    assert_eq!(
        check(&sum, &format!("{sum}*1"), E),
        decided(true, false),
        "one factor beside a 20-term sum"
    );
    // The same sum with 100 factors is past the budget.
    let wide = format!("{sum}{}", "*1".repeat(100));
    match check("42", &wide, N) {
        Outcome::Undecidable(refused) => {
            assert_eq!(refused.reason, "the answer goes past the work bound");
        }
        other => panic!("100 factors beside a 20-term sum gave {other:?}"),
    }
    // The rule holds for a polynomial sum too, and a polynomial takes another
    // path through the promotion than a sum of powers of `e` takes.
    let powers: Vec<String> = (2..22).map(|power| format!("x^{power}")).collect();
    let polynomial = format!("({})", powers.join("+"));
    assert_eq!(
        check(&polynomial, &format!("{polynomial}*1"), E),
        decided(true, false),
        "one factor beside a 20-term polynomial"
    );
    let wide_polynomial = format!("{polynomial}{}", "*1".repeat(100));
    match check("42", &wide_polynomial, N) {
        Outcome::Undecidable(refused) => {
            assert_eq!(refused.reason, "the answer goes past the work bound");
        }
        other => panic!("100 factors beside a 20-term polynomial gave {other:?}"),
    }
    // A two-term sum keeps its verdict through the same count of factors.
    let narrow = format!("(x+1){}", "*1".repeat(100));
    assert_eq!(check("x+1", &narrow, E), decided(true, false));
}

#[test]
fn a_wide_coefficient_costs_more_than_a_narrow_one() {
    // M2 review 1, finding 11. `MAX_STEPS` charged term operations only, so a
    // 20-character answer spent 155 ms in a release build on 4,096-bit
    // coefficients. The budget now charges the width of every number it builds.
    for bomb in [
        "((7/3)**23*x+y)**616",
        "(x+(7/3)**23)**512",
        "(2*x+3*y)**900",
    ] {
        let start = Instant::now();
        let outcome = check("1", bomb, E);
        let elapsed = start.elapsed();
        assert!(
            matches!(outcome, Outcome::Undecidable(_)),
            "{bomb:?} gave {outcome:?}"
        );
        // A refused bomb takes the bomb budget (M2 review 1 ruling: 50 ms in a
        // release build, 500 ms in a debug build), not the single-check budget: the
        // debug bound of 50 ms failed under load while other builds ran on the box.
        let bomb_budget = bomb_budget();
        assert!(
            elapsed < bomb_budget,
            "{bomb:?} took {elapsed:?}, and the budget is {bomb_budget:?}"
        );
    }
}

#[test]
fn the_checker_never_panics_on_random_input() {
    let mut rng = Rng(0x2026_0826_u64 | 1);
    let cases = checker_never_panics(&mut rng, fuzz_case);
    assert!(cases > 1_000, "the fuzz ran only {cases} cases");
}

#[test]
fn an_answer_past_the_input_cap_is_undecidable() {
    let long = "1".repeat(4_001);
    assert!(matches!(check("1", &long, N), Outcome::Undecidable(_)));
    assert!(matches!(check(&long, "1", N), Outcome::Undecidable(_)));
    let at_cap = "1".repeat(4_000);
    assert_eq!(check(&at_cap, &at_cap, N), decided(true, false));
}
