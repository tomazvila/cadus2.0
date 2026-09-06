//! Certified inverse-tangent rounding. Every endpoint is an exact rational.
//!
//! atan's alternating series encloses its value after 64 terms for |x| <= 1/2.
//! Reciprocal and pi/4 reductions cover every finite rational argument. Machin's
//! identity supplies a separate pi enclosure. We emit a decimal only when both
//! degree endpoints round to the same half-even value; ambiguity fails closed.
use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational as Q;
use num_traits::{One, Signed, Zero};

use super::{Answer, EvalError, answer, contracted};
use crate::answer::{AnswerContract, Canon, Undecidable, ast::Ast};
use crate::template::domain::Bindings;

type Interval = (Q, Q);
const TERMS: u32 = 64;
// Bounds every series operand and its powers, independently of template domains.
const INPUT_BITS: u64 = 16;

pub(super) fn degrees(
    args: &[Ast],
    bindings: &Bindings,
    contract: Option<&AnswerContract>,
) -> Result<Answer, EvalError> {
    let Some(contract @ AnswerContract::Approx { decimals }) = contract else {
        return Err(refuse(
            "atandeg requires a decimal-place approximate contract",
        ));
    };
    contract.validate()?;
    let [arg] = args else {
        return Err(refuse("atandeg requires one ratio"));
    };
    let value = answer(arg, bindings)?;
    let Canon::Rational(ratio) = value.canon else {
        return Err(refuse("atandeg requires a rational ratio"));
    };
    if ratio.numer().bits() > INPUT_BITS || ratio.denom().bits() > INPUT_BITS {
        return Err(refuse(
            "atandeg ratio exceeds its 16-bit numerator/denominator bound",
        ));
    }
    let pi = pi_interval();
    let radians = positive_atan(&ratio.abs(), &pi);
    let mut degrees = (
        radians.0 * Q::from_integer(180.into()) / &pi.1,
        radians.1 * Q::from_integer(180.into()) / &pi.0,
    );
    if ratio.is_negative() {
        degrees = (-degrees.1, -degrees.0);
    }
    let scale = BigInt::from(10).pow(u32::from(*decimals));
    let lo = half_even(&(degrees.0 * Q::from_integer(scale.clone())));
    let hi = half_even(&(degrees.1 * Q::from_integer(scale)));
    if lo != hi {
        return Err(refuse("atandeg enclosure crosses a rounding boundary"));
    }
    let text = decimal(&lo, *decimals);
    let text = if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.')
    } else {
        &text
    };
    contracted(text.to_owned(), contract)
}

fn positive_atan(x: &Q, pi: &Interval) -> Interval {
    if x > &Q::one() {
        let reciprocal = positive_atan(&x.recip(), pi);
        return (
            &pi.0 / Q::from_integer(2.into()) - reciprocal.1,
            &pi.1 / Q::from_integer(2.into()) - reciprocal.0,
        );
    }
    if x > &Q::new(1.into(), 2.into()) {
        // (1-x)/(1+x) is in [0,1/3), and atan(x)=pi/4-atan((1-x)/(1+x)).
        let reduced = series(&((Q::one() - x) / (Q::one() + x)), TERMS);
        return (
            &pi.0 / Q::from_integer(4.into()) - reduced.1,
            &pi.1 / Q::from_integer(4.into()) - reduced.0,
        );
    }
    series(x, TERMS)
}

fn series(x: &Q, terms: u32) -> Interval {
    let square = x * x;
    let mut power = x.clone();
    let mut sum = Q::zero();
    for k in 0..terms {
        let term = &power / Q::from_integer((2 * k + 1).into());
        if k % 2 == 0 {
            sum += term;
        } else {
            sum -= term;
        }
        power *= &square;
    }
    let upper = &sum + power / Q::from_integer((2 * terms + 1).into());
    (sum, upper)
}

fn pi_interval() -> Interval {
    let fifth = series(&Q::new(1.into(), 5.into()), 32);
    let small = series(&Q::new(1.into(), 239.into()), 12);
    let sixteen = Q::from_integer(16.into());
    let four = Q::from_integer(4.into());
    (
        &sixteen * fifth.0 - &four * small.1,
        sixteen * fifth.1 - four * small.0,
    )
}

fn half_even(x: &Q) -> BigInt {
    let (floor, remainder) = x.numer().div_mod_floor(x.denom());
    let twice = remainder * 2;
    if twice > *x.denom() || (twice == *x.denom() && floor.is_odd()) {
        floor + 1
    } else {
        floor
    }
}

fn decimal(scaled: &BigInt, decimals: u8) -> String {
    if decimals == 0 {
        return scaled.to_string();
    }
    let width = usize::from(decimals) + 1;
    let digits = format!("{:0>width$}", scaled.abs().to_string());
    let split = digits.len() - usize::from(decimals);
    format!(
        "{}{}.{}",
        if scaled.is_negative() { "-" } else { "" },
        &digits[..split],
        &digits[split..]
    )
}

fn refuse(reason: &'static str) -> EvalError {
    EvalError::Grammar(Undecidable::new(reason))
}
