//! FIX-D6 acceptance: the exact-rounding reading of a learner decimal.
//!
//! The ruling is `docs/DECISIONS.md` row `D6-dec`. A learner decimal is CORRECT
//! when it equals the exact expected value rounded half-to-even to the count of
//! decimal digits the learner typed, and the verdict carries the `notation` tag.
//! Every other decimal stays WRONG. The arithmetic is exact rational arithmetic,
//! so no float enters the decision (D6).
//!
//! 1.0 accepted the same pairs on a float rung at a 1e-6 relative tolerance
//! (`docs/reference/checker-1.0-spec.md` section 3.2). That rung accepted
//! `0.333333` for `1/3` and refused `0.33333`, which is a tolerance and not a
//! rounding. 2.0 is stricter and decidable: the rounding must be exact.
//!
//! No expected value in this file comes from the code under test. Every decimal
//! is the half-to-even rounding that Python `decimal` prints at 60 digits of
//! precision.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use cadus_core::answer::check::notation_note;

mod common;

use common::answer::*;

// ---------------------------------------------------------------------------
// A rational expected value
// ---------------------------------------------------------------------------

#[test]
fn the_exact_rounding_of_a_rational_carries_the_notation_tag() {
    // The ruling names these three. 1.0 accepted the first two on its 1e-6 rung
    // and refused the third; 2.0 accepts all three, because each one is the
    // half-to-even rounding of 1/3 to the digits the learner typed.
    assert_eq!(check("1/3", "0.3333333333", N), rounded());
    assert_eq!(check("1/3", "0.333333", N), rounded());
    assert_eq!(check("1/3", "0.33333", N), rounded());
    // The `significant_decimal` shapes of the generated parity set.
    assert_eq!(check("5/12", "0.4166666667", N), rounded());
    assert_eq!(check("-5/6", "-0.8333333333", E), rounded());
    // The 1.0 boundary case. `2/3` and `0.667` are 3.3e-4 apart, so 1.0 refused
    // the pair at both of its tolerances; `0.667` IS the rounding of 2/3 to
    // three digits, so 2.0 accepts it.
    assert_eq!(check("2/3", "0.667", N), rounded());
}

#[test]
fn a_decimal_that_is_not_the_rounding_stays_wrong() {
    // The rounding of 1/3 to four digits is 0.3333, so 0.3334 is a different
    // answer. The gap to the exact value is smaller than the gap 1.0 allowed at
    // its 1e-6 rung for a value of this size, and the answer is still wrong: the
    // rule is a rounding, not a tolerance.
    assert_eq!(check("1/3", "0.3334", N), decided(false, false));
    assert_eq!(check("2/3", "0.666", N), decided(false, false));
    assert_eq!(check("5/12", "0.4166666666", N), decided(false, false));
}

#[test]
fn a_negative_value_rounds_the_same_way() {
    // The rounding of -1/3 to two digits is -0.33, and the rounding of -1/3 to
    // one digit is -0.3.
    assert_eq!(check("-1/3", "-0.33", N), rounded());
    assert_eq!(check("-1/3", "-0.3", N), rounded());
    assert_eq!(check("-1/3", "-0.34", N), decided(false, false));
    // The sign belongs to the value. Plus one third is not minus one third.
    assert_eq!(check("-1/3", "0.33", N), decided(false, false));
}

#[test]
fn a_tie_goes_to_the_even_last_digit() {
    // 1/8 is 0.125 exactly, so the rounding to two digits is a tie. Half-to-even
    // picks 0.12, because 12 is even. Half-up would pick 0.13.
    assert_eq!(check("1/8", "0.12", N), rounded());
    assert_eq!(check("1/8", "0.13", N), decided(false, false));
    // 3/8 is 0.375 exactly. Half-to-even picks 0.38, because 37 is odd.
    assert_eq!(check("3/8", "0.38", N), rounded());
    assert_eq!(check("3/8", "0.37", N), decided(false, false));
    // The negative tie. -0.125 rounds to -0.12.
    assert_eq!(check("-1/8", "-0.12", N), rounded());
    assert_eq!(check("-1/8", "-0.13", N), decided(false, false));
}

#[test]
fn the_decision_is_exact_and_never_a_float() {
    // Twenty digits of 1/3. A `f64` holds about 16 decimal digits, so a decision
    // that compares two floats calls this pair wrong: the nearest `f64` to 1/3
    // is 1.85e-17 away from the exact value, which is bigger than the 5e-21 the
    // twentieth digit allows. The exact arithmetic accepts it (D6).
    assert_eq!(check("1/3", "0.33333333333333333333", N), rounded());
    // The same test on the radical branch. The nearest `f64` to sqrt(2) is
    // 1.4142135623730951, which is not the twenty-digit rounding below.
    assert_eq!(check("sqrt(2)", "1.41421356237309504880", E), rounded());
    // One unit of the last place is still wrong.
    assert_eq!(
        check("1/3", "0.33333333333333333334", N),
        decided(false, false)
    );
}

// ---------------------------------------------------------------------------
// A radical expected value
// ---------------------------------------------------------------------------

#[test]
fn the_exact_rounding_of_a_radical_carries_the_notation_tag() {
    assert_eq!(check("sqrt(2)", "1.4142", E), rounded());
    assert_eq!(check("sqrt(2)", "1.4143", E), decided(false, false));
    // The ruling names this one: eight digits of 8*sqrt(2).
    assert_eq!(check("8\\sqrt{2}", "11.31370850", E), rounded());
    assert_eq!(check("8*sqrt(2)", "11.31370850", E), rounded());
    assert_eq!(check("8*sqrt(2)", "11.31370851", E), decided(false, false));
    // A radical over a divisor, and a sum of two roots.
    assert_eq!(check("2√3/3", "1.154700538", E), rounded());
    assert_eq!(check("sqrt(2)+sqrt(3)", "3.1462643699", E), rounded());
    assert_eq!(
        check("sqrt(2)+sqrt(3)", "3.1462643698", E),
        decided(false, false)
    );
}

#[test]
fn a_constant_has_no_rounding_verdict() {
    // `pi` and `e` are a `Canon::Radical` basis, and neither is a square root of
    // a positive rational: no exact rational bound of this module brackets them.
    // The ruling refuses the pair rather than guess a verdict (V2), so `3.14`
    // for `pi` is Undecidable and the caller grades it another way.
    assert_undecidable("pi", "3.14", N, "a rounding of a constant is not decidable");
    assert_undecidable(
        "pi",
        "3.14159",
        N,
        "a rounding of a constant is not decidable",
    );
    assert_undecidable(
        "e",
        "2.71828",
        N,
        "a rounding of a constant is not decidable",
    );
    assert_undecidable(
        "2*pi",
        "6.28319",
        E,
        "a rounding of a constant is not decidable",
    );
    // A learner who writes the constant itself is still decided, and correct.
    assert_eq!(check("pi", "\\pi", N), decided(true, false));
}

#[test]
fn a_nested_radical_keeps_the_wrong_verdict() {
    // `sqrt(2 + sqrt(3))` is a `Canon::Poly` over an `Atom::Call`, and not a
    // `Canon::Radical`: the radicand is not a rational. The D6-dec rule reads a
    // rational and a rational combination of square roots, so the pair falls
    // through to the wrong verdict, which is the verdict 2.0 gave before the
    // rule. The residue is recorded in `docs/reference/undecidable-answers.md`.
    assert_eq!(
        check("√(2 + √3)/2", "0.9659258263", E),
        decided(false, false)
    );
}

// ---------------------------------------------------------------------------
// What is not a decimal, and what the rule never touches
// ---------------------------------------------------------------------------

#[test]
fn an_exact_decimal_is_a_plain_correct_and_carries_no_tag() {
    // The rule sits AFTER the equality rung, so a decimal that IS the value is a
    // plain correct answer. `0.50` is one half, and not a rounding of one half,
    // so it takes no note about its form. The two pairs below are the pinned 1.0
    // parity of `crates/core/tests/answer_check.rs`, and the rule leaves them
    // where they were.
    assert_eq!(check("1/2", "0.5", N), decided(true, false));
    assert_eq!(check("1/2", "0.50", N), decided(true, false));
    assert_eq!(check("2", "2.0", N), decided(true, false));
    assert_eq!(check("1", "1.000", N), decided(true, false));
    assert_eq!(check("12", "12.0", N), decided(true, false));
}

#[test]
fn a_fraction_and_an_integer_are_not_decimals() {
    // The rule reads the LEARNER side, and only when the learner typed a decimal
    // literal. A fraction carries no digit count, so `333/1000` is not a
    // rounding of `1/3` and stays wrong.
    assert_eq!(check("1/3", "333/1000", N), decided(false, false));
    assert_eq!(check("1/3", "33/100", N), decided(false, false));
    // An integer carries no digit count either. `0` is not a rounding of 1/3.
    assert_eq!(check("1/3", "0", N), decided(false, false));
    assert_eq!(check("2/5", "0", N), decided(false, false));
    // A trailing period is normalization and not a scale (V4), so `2.` is the
    // integer 2. A scale of 0 is therefore never a decimal: were it one, the
    // rounding of 5/2 to zero digits would be the even 2 and this pair would be
    // correct.
    assert_eq!(check("5/2", "2.", N), decided(false, false));
    assert_eq!(check("2", "2.", N), decided(true, false));
}

#[test]
fn a_decimal_against_an_expression_stays_wrong() {
    // The expected side is a polynomial, an interval, a tuple, or a set. The rule
    // does not apply, and the pair keeps the verdict it had.
    assert_eq!(check("2*x", "0.5", E), decided(false, false));
    assert_eq!(check("x/3", "0.3333333333", E), decided(false, false));
    assert_eq!(check("(1, 2)", "1.5", E), decided(false, false));
}

#[test]
fn the_period_grouped_reading_still_wins_first() {
    // `7.329` is a period-grouped integer AND a decimal literal. The grouping
    // rung runs first and reads it as 7329, which is the 1.0 verdict and the
    // pinned behavior of spec section 2.4.
    assert_eq!(check("7329", "7.329", N), rounded());
    assert_eq!(check("2500", "2.500", N), rounded());
    assert_eq!(check("7239", "7.329", N), decided(false, false));
    // The 1000x hole of spec section 7.2 stays closed: `0.008` is not `8`, and
    // the rounding of 8 to three digits is 8.000 and not 0.008.
    assert_eq!(check("8", "0.008", N), decided(false, false));
}

#[test]
fn a_decimal_past_the_size_bound_stays_undecidable() {
    // The canonicalizer refuses a scale over 1,000 digits before the rounding
    // rule ever runs, so the answer has no verdict (V2) and no rounding is
    // attempted on it.
    let long = format!("0.{}", "3".repeat(1_001));
    assert_undecidable("1/3", &long, N, "a decimal past the size bound");
    // A long decimal INSIDE the bound is still decided. The work budget of the
    // canonicalizer charges the width of `10^scale`, so a decimal of 200 digits
    // is decided and a decimal of 1,000 digits leaves the budget: both refusals
    // are the canonicalizer's, and neither is a rounding.
    let inside = format!("0.{}", "3".repeat(200));
    assert_eq!(check("1/3", &inside, N), rounded());
    let costly = format!("0.{}", "3".repeat(1_000));
    assert_undecidable("1/3", &costly, N, "the answer goes past the work bound");
}

// ---------------------------------------------------------------------------
// The learner-facing note (spec section 7.1 idiom)
// ---------------------------------------------------------------------------

#[test]
fn the_note_names_the_exact_form() {
    assert_eq!(
        notation_note("1/3", "0.3333333333", N).as_deref(),
        Some(
            "Correct value. One note on form: the exact value is $1/3$; \
             $0.3333333333$ is a rounding of it."
        )
    );
    assert_eq!(
        notation_note("8\\sqrt{2}", "11.31370850", E).as_deref(),
        Some(
            "Correct value. One note on form: the exact value is $8\\sqrt{2}$; \
             $11.31370850$ is a rounding of it."
        )
    );
}

#[test]
fn the_period_grouped_note_keeps_the_1_0_wording() {
    // `deterministic_grade.py:82-91`, quoted in spec section 7.1. The note cites
    // the learner's own value and not a stock example.
    assert_eq!(
        notation_note("7329", "7.329", N).as_deref(),
        Some(
            "Correct value. One note on form: here a period is a decimal point, \
             so $7329$ is the way to write it — $7.329$ reads as a decimal."
        )
    );
}

#[test]
fn a_verdict_with_no_tag_has_no_note() {
    assert_eq!(notation_note("1/2", "0.5", N), None);
    assert_eq!(notation_note("1/3", "0.3334", N), None);
    assert_eq!(notation_note("pi", "3.14", N), None);
    assert_eq!(notation_note("12", "", N), None);
}
