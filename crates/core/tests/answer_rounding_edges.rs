//! The refusal sites of the exact-rounding rung (D6-dec, V2).
//!
//! `rounds_to` takes any canonical form, so this file hands it the forms the
//! canonicalizer never builds: a negative radicand, a constant basis, more
//! roots than the bound, and a coefficient wider than every refinement.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::collections::BTreeMap;

use cadus_core::answer::{Basis, Canon, Outcome, Rounding, Verdict, check, rounds_to};
use cadus_core::curriculum::AnswerKind;
use num_bigint::BigInt;
use num_rational::BigRational;

/// A radical of one basis per radicand, every coefficient one.
fn radical(radicands: &[i64]) -> Canon {
    let parts: BTreeMap<Basis, BigRational> = radicands
        .iter()
        .map(|radicand| {
            (
                Basis {
                    radicand: BigInt::from(*radicand),
                    pi: 0,
                    e: 0,
                },
                BigRational::from_integer(BigInt::from(1)),
            )
        })
        .collect();
    Canon::Radical(parts)
}

/// The rational `numerator / denominator`.
fn ratio(numerator: i64, denominator: i64) -> BigRational {
    BigRational::new(BigInt::from(numerator), BigInt::from(denominator))
}

#[test]
fn a_labeled_expected_value_takes_the_rounding_rung() {
    assert_eq!(
        check("x = 1/3", "0.333", AnswerKind::Numeric),
        Outcome::Decided(Verdict {
            correct: true,
            notation: true,
        })
    );
    assert_eq!(
        check("x = 1/3", "-0.333", AnswerKind::Numeric),
        Outcome::Decided(Verdict {
            correct: false,
            notation: false,
        })
    );
}

#[test]
fn more_roots_than_the_bound_are_refused() {
    let seventeen = radical(&[
        2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59,
    ]);
    assert_eq!(
        rounds_to(&seventeen, &ratio(1, 10), 1),
        Rounding::Refused("the value holds more roots than the rounding bound")
    );
    let sixteen = radical(&[2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53]);
    assert_eq!(rounds_to(&sixteen, &ratio(1, 10), 1), Rounding::Different);
}

#[test]
fn a_negative_radicand_and_a_constant_basis_are_not_decidable() {
    assert_eq!(
        rounds_to(&radical(&[-2]), &ratio(14, 10), 1),
        Rounding::Refused("a rounding of a negative radicand is not decidable")
    );
    let mut parts = BTreeMap::new();
    parts.insert(
        Basis {
            radicand: BigInt::from(1),
            pi: 1,
            e: 0,
        },
        BigRational::from_integer(BigInt::from(1)),
    );
    assert_eq!(
        rounds_to(&Canon::Radical(parts), &ratio(31, 10), 1),
        Rounding::Refused("a rounding of a constant is not decidable")
    );
}

#[test]
fn a_coefficient_wider_than_every_refinement_is_refused() {
    // 2^20000 * sqrt(2) against the whole number nearest to it: every bracket
    // of the four rounds is wider than the half unit, so no round decides.
    let mut parts = BTreeMap::new();
    parts.insert(
        Basis {
            radicand: BigInt::from(2),
            pi: 0,
            e: 0,
        },
        BigRational::from_integer(BigInt::from(2).pow(20_000)),
    );
    let nearest = (BigInt::from(2) << 40_000_usize).sqrt();
    assert_eq!(
        rounds_to(
            &Canon::Radical(parts),
            &BigRational::from_integer(nearest),
            1
        ),
        Rounding::Refused("the rounding needs a finer bound than the checker builds")
    );
}

/// The rational combination `coefficient * sqrt(2)`.
fn sqrt_two_times(coefficient: BigRational) -> Canon {
    let mut parts = BTreeMap::new();
    parts.insert(
        Basis {
            radicand: BigInt::from(2),
            pi: 0,
            e: 0,
        },
        coefficient,
    );
    Canon::Radical(parts)
}

/// The lower end `F` of the bracket of `sqrt(2)` at the finest refinement,
/// 8,192 bits, and the unit `2^8192` of that bracket: `F/unit <= sqrt(2) <
/// (F + 1)/unit`.
fn finest_bracket() -> (BigInt, BigInt) {
    let bits = 8_192_usize;
    let unit = BigInt::from(1) << bits;
    let floor = (BigInt::from(2) << (bits * 2)).sqrt();
    (floor, unit)
}

/// The rational `numerator / denominator` of two big integers.
fn big_ratio(numerator: BigInt, denominator: BigInt) -> BigRational {
    BigRational::new(numerator, denominator)
}

/// The rounding of `coefficient * sqrt(2)` against the learner value 1.0.
fn against_one(coefficient: BigRational) -> Rounding {
    rounds_to(
        &sqrt_two_times(coefficient),
        &BigRational::from_integer(BigInt::from(1)),
        1,
    )
}

/// The refusal of a pair that no round decides.
const NO_ROUND: Rounding =
    Rounding::Refused("the rounding needs a finer bound than the checker builds");

#[test]
fn a_bracket_end_on_the_half_unit_bound_gives_no_verdict() {
    // The learner value is 1.0 and the half unit is 1/20. Each coefficient
    // puts one end of the finest bracket exactly on a bound, where the strict
    // comparison holds nothing, so every round refuses.
    let (floor, unit) = finest_bracket();
    let on_bound = |numerator: i64, end: &BigInt| {
        big_ratio(BigInt::from(numerator) * &unit, BigInt::from(20) * end)
    };
    // The upper end is 1 + 1/20: no `Same`.
    assert_eq!(against_one(on_bound(21, &(&floor + 1))), NO_ROUND);
    // The lower end is 1 - 1/20: no `Same`.
    assert_eq!(against_one(on_bound(19, &floor)), NO_ROUND);
    // The lower end is 1 + 1/20: no `Different`.
    assert_eq!(against_one(on_bound(21, &floor)), NO_ROUND);
    // The upper end is 1 - 1/20: no `Different`.
    assert_eq!(against_one(on_bound(19, &(&floor + 1))), NO_ROUND);
}

#[test]
fn a_bracket_as_wide_as_the_half_unit_gives_no_verdict() {
    // The coefficient unit/20 makes the finest bracket exactly one half unit
    // wide, and the learner value sits on its upper end: the value is under
    // the bound above and on the bound below, which decides nothing.
    let (floor, unit) = finest_bracket();
    let coefficient = big_ratio(unit, BigInt::from(20));
    let learner = big_ratio(&floor + 1, BigInt::from(20));
    assert_eq!(
        rounds_to(&sqrt_two_times(coefficient), &learner, 1),
        NO_ROUND
    );
}

#[test]
fn a_negative_root_keeps_its_sign_and_its_size_in_the_bracket() {
    // -sqrt(2) is -1.414...: the sign-flipped decimal, the decimal of the
    // wrong size, and the decimal of half the size are all wrong, and only the
    // rounding of the value itself is the notation verdict.
    let wrong = Outcome::Decided(Verdict {
        correct: false,
        notation: false,
    });
    assert_eq!(check("-sqrt(2)", "1.41", AnswerKind::Numeric), wrong);
    assert_eq!(check("-sqrt(2)", "0.41", AnswerKind::Numeric), wrong);
    assert_eq!(check("-sqrt(2)", "-0.71", AnswerKind::Numeric), wrong);
    assert_eq!(
        check("-sqrt(2)", "-1.41", AnswerKind::Numeric),
        Outcome::Decided(Verdict {
            correct: true,
            notation: true,
        })
    );
}
