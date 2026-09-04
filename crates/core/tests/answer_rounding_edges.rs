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
