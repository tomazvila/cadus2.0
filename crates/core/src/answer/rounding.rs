//! The exact-rounding reading of a learner decimal (D6, ruling `D6-dec`).
//!
//! A learner who types `0.3333333333` for `1/3` writes the authored value to the
//! digits the learner typed. 1.0 accepted that pair on a float rung with a 1e-6
//! relative tolerance (`docs/reference/checker-1.0-spec.md` section 3.2). The
//! same rung accepted `0.333333` and refused `0.33333`, so the verdict came from
//! the size of the number and not from the digits the learner wrote.
//!
//! 2.0 keeps no tolerance. The decimal is CORRECT when it is the exact rounding
//! of the exact value, half-to-even, to the count of digits the learner typed,
//! and WRONG otherwise. Every step here is exact integer and rational
//! arithmetic, so no float enters the decision (D6) and the rule is decidable.
//!
//! # The two expected forms
//!
//! - [`Canon::Rational`]: one multiplication and one floor division decide it.
//!   The rounding is exact, ties included.
//! - [`Canon::Radical`]: the value is a rational combination of square roots of
//!   positive integers. The module brackets each root between two exact
//!   rationals and refines the bracket until the interval of `value - decimal`
//!   lies inside or outside the half-unit bound. A sum of roots of different
//!   squarefree integers is irrational, so the interval never straddles the
//!   bound forever and the refinement ends.
//!
//! Every other canonical form takes no rounding decision here.
//!
//! # The bounds
//!
//! The refinement is a latency bound and not a taste (L2). [`REFINEMENTS`] holds
//! the precision of each round, and [`MAX_ROOT_TERMS`] holds the count of roots
//! one decision reads. A pair that leaves either bound is [`Rounding::Refused`],
//! which the checker reports as [`super::Undecidable`]: the checker never
//! guesses a verdict (V2).

use std::collections::BTreeMap;

use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{One, Signed, Zero};

use super::bracket;
use super::canon::{Atom, Basis, Canon, Poly};

/// The largest count of root terms one rounding decision reads.
///
/// An answer of the corpus holds four terms at most. A wider combination is a
/// crafted input, and the decision refuses it instead of spending the 300 ms of
/// L2 on square roots (L2).
const MAX_ROOT_TERMS: usize = 16;

/// The precision of each refinement round, in bits.
///
/// The first round decides every ordinary answer: four digits of a square root
/// need about 14 bits. The last round holds 8,192 bits, about 2,466 decimal
/// digits, which brackets the widest coefficient the canonicalizer builds
/// (`MAX_BITS`, 4,096 bits) against the longest decimal it reads (`MAX_SCALE`,
/// 1,000 digits). A pair that no round decides is refused.
const REFINEMENTS: [usize; 4] = [128, 512, 2_048, 8_192];

/// The answer of the rounding rule for one pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rounding {
    /// The learner decimal is the exact rounding of the expected value.
    Same,
    /// The learner decimal is a decimal, and it is not that rounding.
    Different,
    /// The expected value takes no rounding decision, so another rung owns the
    /// pair.
    NotANumber,
    /// The rule applies and the decision is not exact, so the checker refuses it.
    Refused(&'static str),
}

/// Whether `learner` is `expected` rounded half-to-even to `scale` digits.
///
/// `learner` is the exact value of the decimal the learner typed, and `scale` is
/// the count of digits after the point, which is always one or more.
#[must_use]
pub fn rounds_to(expected: &Canon, learner: &BigRational, scale: u32) -> Rounding {
    rounds_to_in(expected, learner, scale, false)
}

/// The rounding rule of an approximate contract.
///
/// The contract asks for a number to some digits, so the value may also hold
/// `pi`, `e`, a logarithm, or a root of index 3 or more. The bracket module
/// encloses each of them between two exact rationals. [`rounds_to`] leaves those
/// values without a verdict, because the checker has no contract there to ask.
#[must_use]
pub(crate) fn rounds_to_constant(expected: &Canon, learner: &BigRational, scale: u32) -> Rounding {
    rounds_to_in(expected, learner, scale, true)
}

fn rounds_to_in(expected: &Canon, learner: &BigRational, scale: u32, constants: bool) -> Rounding {
    match expected {
        Canon::Rational(value) => rational_rounds_to(value, learner, scale),
        Canon::Radical(parts) => radical_rounds_to(parts, learner, scale, constants),
        // A number with a root of index 3 or more holds no exact bound in the
        // plain rule, so the pair gets no verdict (V2).
        Canon::Poly(sum) if constants && holds_a_number_only(sum) => {
            poly_rounds_to(sum, learner, scale)
        }
        Canon::Poly(sum) if holds_a_root_only(sum) => {
            Rounding::Refused("a rounding of a higher root is not decidable")
        }
        Canon::Func(name, arguments) if constants && is_logarithm(name, arguments) => {
            decide(learner, scale, |bits| {
                atom_interval(&Atom::Call(name.clone(), arguments.clone()), bits)
            })
        }
        _ => Rounding::NotANumber,
    }
}

/// Whether every atom of a sum is a root of a number, a square root, `pi`, or `e`.
fn holds_a_root_only(sum: &Poly) -> bool {
    sum.keys().flatten().all(|(atom, _)| match atom {
        Atom::Root(base, _) => matches!(base.as_ref(), Canon::Rational(_)),
        Atom::Sqrt(_) | Atom::Pi | Atom::E => true,
        Atom::Exp(_) | Atom::Var(_) | Atom::Call(_, _) => false,
    })
}

/// Whether every atom of a sum is a number the bracket module can enclose: a
/// root of a number, a square root, `pi`, `e`, or the logarithm of a whole number.
fn holds_a_number_only(sum: &Poly) -> bool {
    sum.keys().flatten().all(|(atom, _)| match atom {
        Atom::Root(base, _) => matches!(base.as_ref(), Canon::Rational(_)),
        Atom::Call(name, arguments) => is_logarithm(name, arguments),
        Atom::Sqrt(_) | Atom::Pi | Atom::E => true,
        Atom::Exp(_) | Atom::Var(_) => false,
    })
}

/// Whether a call is the logarithm of one rational.
fn is_logarithm(name: &str, arguments: &[Canon]) -> bool {
    name == "ln" && matches!(arguments, [Canon::Rational(_)])
}

/// Whether the rounding rule decides a value: a number built from roots, `pi`,
/// `e`, and logarithms of whole numbers.
#[must_use]
pub(crate) fn is_rounding_number(value: &Canon) -> bool {
    match value {
        Canon::Rational(_) | Canon::Radical(_) => true,
        Canon::Poly(sum) => holds_a_number_only(sum),
        Canon::Func(name, arguments) => is_logarithm(name, arguments),
        _ => false,
    }
}

/// Decide the rounding of a sum of products of constants.
fn poly_rounds_to(sum: &Poly, learner: &BigRational, scale: u32) -> Rounding {
    if sum.len() > MAX_ROOT_TERMS {
        return Rounding::Refused("the value holds more roots than the rounding bound");
    }
    decide(learner, scale, |bits| {
        let mut total = (BigRational::zero(), BigRational::zero());
        for (monomial, coefficient) in sum {
            let mut product = (BigRational::one(), BigRational::one());
            for (atom, exponent) in monomial {
                let base = atom_interval(atom, bits)?;
                product = bracket::times(&product, &bracket::power(&base, *exponent)?);
            }
            total = bracket::plus(&total, &bracket::scale(&product, coefficient));
        }
        Some(total)
    })
}

/// The bracket of one positive atom.
fn atom_interval(atom: &Atom, bits: usize) -> Option<(BigRational, BigRational)> {
    match atom {
        Atom::Pi => Some(bracket::pi(bits)),
        Atom::E => Some(bracket::e(bits)),
        Atom::Sqrt(radicand) => {
            bracket::root(&BigRational::from_integer(radicand.clone()), 2, bits)
        }
        Atom::Root(base, index) => match base.as_ref() {
            Canon::Rational(value) if value.is_positive() => bracket::root(value, *index, bits),
            _ => None,
        },
        Atom::Call(name, arguments) if name == "ln" => match arguments.as_slice() {
            [Canon::Rational(value)] if value.is_integer() => bracket::ln(value.numer(), bits),
            _ => None,
        },
        _ => None,
    }
}

/// Refine the bracket of a value until it decides the rounding.
///
/// `learner` is the exact decimal `d`. The rule is the definition of a rounding:
/// `d` is the value rounded to `scale` digits when `|value - d|` is under
/// `5 * 10^-(scale+1)`. The value is irrational here, so the bound itself is
/// never met and the two strict comparisons decide the pair.
fn decide(
    learner: &BigRational,
    scale: u32,
    bracket_at: impl Fn(usize) -> Option<(BigRational, BigRational)>,
) -> Rounding {
    // Half of the last digit the learner typed: 5 * 10^-(scale+1).
    let half = BigRational::new(BigInt::one(), power_of_ten(scale) * 2);
    for bits in REFINEMENTS {
        let Some((low, high)) = bracket_at(bits) else {
            return Rounding::Refused("the value is outside the brackets of the rounding rule");
        };
        let low = low - learner;
        let high = high - learner;
        if high < half && low > -half.clone() {
            return Rounding::Same;
        }
        if low > half || high < -half.clone() {
            return Rounding::Different;
        }
    }
    Rounding::Refused("the rounding needs a finer bound than the checker builds")
}

/// Decide the rounding of an exact rational.
fn rational_rounds_to(value: &BigRational, learner: &BigRational, scale: u32) -> Rounding {
    let power = power_of_ten(scale);
    let scaled = value * BigRational::from_integer(power.clone());
    let target = learner * BigRational::from_integer(power);
    if round_half_even(&scaled) == target.to_integer() {
        Rounding::Same
    } else {
        Rounding::Different
    }
}

/// Round an exact rational to the nearest whole number, ties to the even one.
///
/// The floor and the remainder come from one division of the numerator by the
/// denominator, and the denominator of a [`BigRational`] is always positive, so
/// the remainder is always in the half-open range 0 to the denominator.
fn round_half_even(value: &BigRational) -> BigInt {
    let (floor, remainder) = value.numer().div_mod_floor(value.denom());
    let twice = remainder * BigInt::from(2_u32);
    match twice.cmp(value.denom()) {
        std::cmp::Ordering::Less => floor,
        std::cmp::Ordering::Greater => floor + 1,
        std::cmp::Ordering::Equal => {
            if floor.is_even() {
                floor
            } else {
                floor + 1
            }
        }
    }
}

/// Decide the rounding of a rational combination of square roots.
///
/// `learner` is the exact decimal `d`, and the rule is the definition of a
/// rounding: `d` is the value rounded to `scale` digits when
/// `|value - d| < 5 * 10^-(scale+1)`, or the two are equal at that bound and the
/// half-to-even rule picks `d`. The value here is irrational — a rational
/// combination of square roots of different squarefree integers is never a
/// rational — so the equality case cannot happen and the two strict comparisons
/// decide the pair.
fn radical_rounds_to(
    parts: &BTreeMap<Basis, BigRational>,
    learner: &BigRational,
    scale: u32,
    constants: bool,
) -> Rounding {
    if parts.len() > MAX_ROOT_TERMS {
        return Rounding::Refused("the value holds more roots than the rounding bound");
    }
    for basis in parts.keys() {
        if !constants && (basis.pi != 0 || basis.e != 0) {
            // `pi` and `e` are the bases that are not a square root of a
            // positive rational. The plain rule has no bracket for them, so the
            // pair gets no verdict (V2).
            return Rounding::Refused("a rounding of a constant is not decidable");
        }
        if !basis.radicand.is_positive() {
            return Rounding::Refused("a rounding of a negative radicand is not decidable");
        }
    }
    decide(learner, scale, |bits| {
        let mut total = (BigRational::zero(), BigRational::zero());
        for (basis, coefficient) in parts {
            let mut product = (BigRational::one(), BigRational::one());
            if !basis.radicand.is_one() {
                let radicand = BigRational::from_integer(basis.radicand.clone());
                product = bracket::root(&radicand, 2, bits)?;
            }
            if basis.pi != 0 {
                product = bracket::times(&product, &bracket::power(&bracket::pi(bits), basis.pi)?);
            }
            if basis.e != 0 {
                product = bracket::times(&product, &bracket::power(&bracket::e(bits), basis.e)?);
            }
            total = bracket::plus(&total, &bracket::scale(&product, coefficient));
        }
        Some(total)
    })
}

/// Build `10^scale` as a big integer.
fn power_of_ten(scale: u32) -> BigInt {
    BigInt::from(10_u32).pow(scale)
}
