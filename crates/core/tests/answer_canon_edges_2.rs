//! Part 2 of the canonicalizer edge tests: the exact width of each size bound,
//! the depth bound on a wide tree, and the scalar ratio of two sums with a
//! content. Every expected value is a literal or one exact big integer.

#![allow(clippy::panic)]

use num_bigint::BigInt;
use num_rational::BigRational;

use cadus_core::answer::canon::canon_with_budget;
use cadus_core::answer::{Ast, Canon, Undecidable, canonical_form};

/// A budget no test below spends.
const WIDE_BUDGET: usize = 1_000_000;

/// The whole number `2^exponent`.
fn two_to(exponent: usize) -> BigInt {
    BigInt::from(1) << exponent
}

/// The canonical form of one whole number.
fn whole(value: BigInt) -> Result<Canon, Undecidable> {
    Ok(Canon::Rational(BigRational::from_integer(value)))
}

/// The refusal of one answer.
fn refusal(text: &str) -> &'static str {
    match canonical_form(text) {
        Ok(value) => panic!("{text:?} decided {value:?}"),
        Err(reason) => reason.reason,
    }
}

#[test]
fn a_number_at_the_size_bound_meets_the_work_bound_and_one_past_it_meets_the_size_bound() {
    // 2^4095 holds exactly 4,096 bits, which the size bound admits, so the
    // width charge of 64 words refuses it. 2^4096 holds one bit more, and the
    // size bound refuses it before any charge.
    let at_bound = two_to(4_095).to_string();
    assert_eq!(refusal(&at_bound), "the answer goes past the work bound");
    let past_bound = two_to(4_096).to_string();
    assert_eq!(refusal(&past_bound), "a number past the size bound");
}

#[test]
fn a_radicand_at_the_size_bound_is_admitted_and_one_past_it_is_refused() {
    // (2^2048 - 1)^2 holds exactly 4,096 bits, so the integer bound admits it
    // and the root is exact.
    let root = two_to(2_048) - 1;
    let square = Ast::Sqrt(Box::new(Ast::Integer(&root * &root)));
    assert_eq!(canon_with_budget(&square, WIDE_BUDGET), whole(root));
    // The radicand of sqrt(p/q) is p*q. 2^2100 over 3^1330 builds a radicand
    // of 4,208 bits, past the bound, while each side alone is inside it.
    let wide = Ast::Sqrt(Box::new(Ast::Fraction {
        numerator: two_to(2_100),
        denominator: BigInt::from(3).pow(1_330),
    }));
    assert_eq!(
        canon_with_budget(&wide, WIDE_BUDGET),
        Err(Undecidable::new("a number past the size bound"))
    );
}

#[test]
fn a_power_whose_width_estimate_meets_the_size_bound_is_computed() {
    // The estimate is the bit count of the base times the exponent: 2 * 2048
    // is exactly the bound, so the power is built; 2 * 2049 is past it. The
    // grammar caps a literal exponent at 1,000, so the tree is built by hand.
    let power = |exponent: i64| Ast::Pow(Box::new(Ast::Integer(BigInt::from(2))), exponent);
    assert_eq!(
        canon_with_budget(&power(2_048), WIDE_BUDGET),
        whole(two_to(2_048))
    );
    assert_eq!(
        canon_with_budget(&power(2_049), WIDE_BUDGET),
        Err(Undecidable::new("a number past the size bound"))
    );
}

#[test]
fn a_wide_flat_sum_stays_inside_the_depth_bound() {
    // 150 terms is more than the depth bound of 128 levels, and a flat sum is
    // two levels deep, so the sum is a value and not a refusal.
    let sum: String = ["1"; 150].join("+");
    assert_eq!(canonical_form(&sum), whole(BigInt::from(150)));
}

#[test]
fn a_numerator_that_is_a_rational_multiple_of_the_denominator_is_that_rational() {
    // The content of 6x + 9 is 3, so the primitive denominator is 2x + 3 and
    // the numerator scales to (4/3)x + 2. Every coefficient of the numerator
    // is 2/3 of the one under it.
    assert_eq!(
        canonical_form("(4x+6)/(6x+9)"),
        Ok(Canon::Rational(BigRational::new(
            BigInt::from(2),
            BigInt::from(3)
        )))
    );
}
