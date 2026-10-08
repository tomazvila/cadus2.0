//! Property contracts: the learner gives an object, the checker tests a named
//! predicate on it (example-generation tasks).
//!
//! The authored answer is one valid example, never a key to match: any learner
//! answer with the property is correct. Every predicate uses exact integer or
//! rational arithmetic (D6) and a bounded amount of work (L2):
//!
//! - divisor counting runs trial division up to the square root of a number at
//!   most [`MAX_DIVISOR_COUNT_INPUT`];
//! - the primality test is Miller-Rabin with the first thirteen prime bases,
//!   which is a proof (not a probability) for every number below
//!   [`MAX_PRIME_INPUT`]; a larger number gives no verdict.

use std::collections::BTreeMap;

use num_bigint::{BigInt, BigUint, Sign};
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};
use serde::{Deserialize, Serialize};

use super::{Canon, Undecidable, canonical_form};
use crate::answer::{Atom, Outcome, Poly, Verdict};

/// The largest number whose positive divisors the checker counts (10^12).
const MAX_DIVISOR_COUNT_INPUT: u64 = 1_000_000_000_000;

/// The bound below which the thirteen Miller-Rabin bases prove primality.
///
/// The first thirteen primes as bases decide every number below
/// 3,317,044,064,679,887,385,961,981 (Sorenson and Webster, 2015).
const MAX_PRIME_INPUT: u128 = 3_317_044_064_679_887_385_961_981;

/// The Miller-Rabin bases.
const BASES: [u32; 13] = [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41];

/// The largest divisor count an author can ask for.
const MAX_DIVISOR_COUNT: i64 = 64;

/// The largest absolute value of a counterexample variable.
const MAX_COUNTEREXAMPLE_INPUT: i64 = 1_000_000;

/// The largest degree of a counterexample expression.
const MAX_DEGREE: i64 = 4;

/// How far the near-miss list of [`near_misses`] walks from the example.
const NEAR_MISS_STEPS: i64 = 40;

/// The refusal of a learner number past the checked range.
pub(crate) const TOO_LARGE: &str = "a number too large for this property check";

/// The named predicates of the `property` contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PropertyCheck {
    /// A positive integer with exactly `n` positive divisors.
    DivisorCount,
    /// A prime number.
    Prime,
    /// A composite number: an integer greater than 1 that is not prime.
    Composite,
    /// An integer multiple of `k`.
    MultipleOf,
    /// A positive divisor of `k`.
    DivisorOf,
    /// An integer whose only common positive divisor with `k` is 1.
    CoprimeTo,
    /// An integer from `min` through `max`.
    IntegerInRange,
    /// A rational number strictly between `low` and `high`.
    Between,
    /// An integer `n >= min` for which `expr` is not prime: a counterexample to
    /// "`expr` is prime for every integer `n >= min`".
    PrimeCounterexample,
}

impl PropertyCheck {
    /// The argument names this check takes.
    const fn allowed(self) -> &'static [&'static str] {
        match self {
            Self::DivisorCount => &[
                "n",
                "min",
                "max",
                "multiple_of",
                "not_multiple_of",
                "coprime_to",
            ],
            Self::Prime | Self::Composite | Self::IntegerInRange => {
                &["min", "max", "multiple_of", "not_multiple_of", "coprime_to"]
            }
            // `multiple_of` is the check itself, so it takes no second one.
            Self::MultipleOf => &["k", "min", "max", "not_multiple_of", "coprime_to"],
            Self::DivisorOf | Self::CoprimeTo => &[
                "k",
                "min",
                "max",
                "multiple_of",
                "not_multiple_of",
                "coprime_to",
            ],
            Self::Between => &["low", "high"],
            Self::PrimeCounterexample => &["expr", "var", "min"],
        }
    }
}

/// One authored argument: an integer, or a text such as `2/5` or `n^2 + n + 41`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PropertyArg {
    /// A whole number written as a number.
    Integer(i64),
    /// A number or an expression written as a string.
    Text(String),
}

/// The arguments of one property contract, by name.
pub type PropertyArgs = BTreeMap<String, PropertyArg>;

/// The optional filters every integer check can carry: inclusive bounds, a
/// divisor the answer must have, one it must avoid, and a number it must share
/// no factor with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Bounds {
    min: Option<BigInt>,
    max: Option<BigInt>,
    multiple_of: Option<BigInt>,
    not_multiple_of: Option<BigInt>,
    coprime_to: Option<BigInt>,
}

impl Bounds {
    fn read(args: &PropertyArgs) -> Result<Self, Undecidable> {
        let bounds = Self {
            min: optional_integer(args, "min")?,
            max: optional_integer(args, "max")?,
            multiple_of: optional_integer(args, "multiple_of")?,
            not_multiple_of: optional_integer(args, "not_multiple_of")?,
            coprime_to: optional_integer(args, "coprime_to")?,
        };
        if let (Some(min), Some(max)) = (&bounds.min, &bounds.max)
            && min > max
        {
            return Err(Undecidable::new(
                "property args: `min` must not exceed `max`",
            ));
        }
        if bounds
            .not_multiple_of
            .as_ref()
            .is_some_and(|m| m.abs() <= BigInt::one())
            || bounds.coprime_to.as_ref().is_some_and(Zero::is_zero)
            || bounds.multiple_of.as_ref().is_some_and(Zero::is_zero)
        {
            return Err(Undecidable::new(
                "property args: `not_multiple_of` must be at least 2 in size, `multiple_of` and \
                 `coprime_to` nonzero",
            ));
        }
        Ok(bounds)
    }

    fn holds(&self, value: &BigInt) -> bool {
        self.min.as_ref().is_none_or(|min| value >= min)
            && self.max.as_ref().is_none_or(|max| value <= max)
            && self
                .multiple_of
                .as_ref()
                .is_none_or(|m| (value % m).is_zero())
            && self
                .not_multiple_of
                .as_ref()
                .is_none_or(|m| !(value % m).is_zero())
            && self
                .coprime_to
                .as_ref()
                .is_none_or(|k| value.gcd(k).is_one())
    }

    /// The filters in plain words, for [`describe`]: empty, or ` (a; b)`.
    fn words(&self) -> String {
        let mut clauses = Vec::new();
        match (&self.min, &self.max) {
            (Some(min), Some(max)) => clauses.push(format!("from {min} through {max}")),
            (Some(min), None) => clauses.push(format!("at least {min}")),
            (None, Some(max)) => clauses.push(format!("at most {max}")),
            (None, None) => {}
        }
        if let Some(m) = &self.multiple_of {
            clauses.push(format!("a multiple of {m}"));
        }
        if let Some(m) = &self.not_multiple_of {
            clauses.push(format!("not a multiple of {m}"));
        }
        if let Some(k) = &self.coprime_to {
            clauses.push(format!("sharing no factor greater than 1 with {k}"));
        }
        if clauses.is_empty() {
            String::new()
        } else {
            format!(" ({})", clauses.join("; "))
        }
    }
}

/// A validated predicate.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Predicate {
    DivisorCount(u64, Bounds),
    Prime(Bounds),
    Composite(Bounds),
    MultipleOf(BigInt, Bounds),
    DivisorOf(BigInt, Bounds),
    CoprimeTo(BigInt, Bounds),
    IntegerInRange(Bounds),
    Between(BigRational, BigRational),
    PrimeCounterexample {
        var: String,
        poly: Poly,
        min: Option<BigInt>,
    },
}

/// Validate the arguments of one property contract.
///
/// # Errors
///
/// Returns [`Undecidable`] with a reason that names the bad argument.
pub(super) fn validate(check: PropertyCheck, args: &PropertyArgs) -> Result<(), Undecidable> {
    predicate(check, args).map(|_| ())
}

fn predicate(check: PropertyCheck, args: &PropertyArgs) -> Result<Predicate, Undecidable> {
    if args
        .keys()
        .any(|name| name != "unit" && !check.allowed().contains(&name.as_str()))
    {
        return Err(Undecidable::new(
            "property args: an argument this property check does not take",
        ));
    }
    if let Some(unit) = args.get("unit")
        && !matches!(unit, PropertyArg::Text(text)
            if matches!(canonical_form(&format!("1 {text}")), Ok(Canon::Quantity { .. })))
    {
        return Err(Undecidable::new(
            "property args: `unit` must name a unit, such as \"m\"",
        ));
    }
    let predicate = match check {
        PropertyCheck::DivisorCount => {
            let n = required_integer(args, "n", "property divisor_count needs `n`")?;
            let n = n
                .to_i64()
                .filter(|n| (1..=MAX_DIVISOR_COUNT).contains(n))
                .and_then(|n| u64::try_from(n).ok())
                .ok_or(Undecidable::new(
                    "property divisor_count needs `n` from 1 through 64",
                ))?;
            Predicate::DivisorCount(n, Bounds::read(args)?)
        }
        PropertyCheck::Prime => Predicate::Prime(Bounds::read(args)?),
        PropertyCheck::Composite => Predicate::Composite(Bounds::read(args)?),
        PropertyCheck::MultipleOf => {
            let k = required_integer(args, "k", "property multiple_of needs `k`")?;
            if k.is_zero() {
                return Err(Undecidable::new("property multiple_of needs a nonzero `k`"));
            }
            Predicate::MultipleOf(k, Bounds::read(args)?)
        }
        PropertyCheck::DivisorOf => {
            let k = required_integer(args, "k", "property divisor_of needs `k`")?;
            if k.is_zero() {
                return Err(Undecidable::new("property divisor_of needs a nonzero `k`"));
            }
            Predicate::DivisorOf(k, Bounds::read(args)?)
        }
        PropertyCheck::CoprimeTo => {
            let k = required_integer(args, "k", "property coprime_to needs `k`")?;
            if k.is_zero() {
                return Err(Undecidable::new("property coprime_to needs a nonzero `k`"));
            }
            Predicate::CoprimeTo(k, Bounds::read(args)?)
        }
        PropertyCheck::IntegerInRange => {
            let bounds = Bounds::read(args)?;
            if bounds.min.is_none() && bounds.max.is_none() {
                return Err(Undecidable::new(
                    "property integer_in_range needs `min`, `max`, or both",
                ));
            }
            Predicate::IntegerInRange(bounds)
        }
        PropertyCheck::Between => {
            let low = required_rational(args, "low")?;
            let high = required_rational(args, "high")?;
            if low >= high {
                return Err(Undecidable::new(
                    "property between needs `low` less than `high`",
                ));
            }
            Predicate::Between(low, high)
        }
        PropertyCheck::PrimeCounterexample => counterexample(args)?,
    };
    Ok(predicate)
}

fn counterexample(args: &PropertyArgs) -> Result<Predicate, Undecidable> {
    let var = match args.get("var") {
        None => "n".to_owned(),
        Some(PropertyArg::Text(name))
            if name.len() == 1 && name.chars().all(|c| c.is_ascii_lowercase()) =>
        {
            name.clone()
        }
        Some(_) => {
            return Err(Undecidable::new(
                "property prime_counterexample needs `var` to be one lowercase letter",
            ));
        }
    };
    let Some(PropertyArg::Text(expr)) = args.get("expr") else {
        return Err(Undecidable::new(
            "property prime_counterexample needs `expr`, a polynomial written as a string",
        ));
    };
    let refusal = Undecidable::new(
        "property prime_counterexample needs `expr` to be a polynomial of degree 1 to 4 in `var` \
         with rational coefficients",
    );
    let poly = match canonical_form(expr) {
        Ok(Canon::Poly(poly)) => poly,
        _ => return Err(refusal),
    };
    let mut degree = 0;
    for monomial in poly.keys() {
        for (atom, exponent) in monomial {
            if *atom != Atom::Var(var.clone()) || !(1..=MAX_DEGREE).contains(exponent) {
                return Err(refusal);
            }
            degree = degree.max(*exponent);
        }
    }
    if degree == 0 {
        return Err(refusal);
    }
    Ok(Predicate::PrimeCounterexample {
        var,
        poly,
        min: optional_integer(args, "min")?,
    })
}

fn required_integer(
    args: &PropertyArgs,
    name: &str,
    missing: &'static str,
) -> Result<BigInt, Undecidable> {
    optional_integer(args, name)?.ok_or(Undecidable::new(missing))
}

fn optional_integer(args: &PropertyArgs, name: &str) -> Result<Option<BigInt>, Undecidable> {
    match args.get(name) {
        None => Ok(None),
        Some(PropertyArg::Integer(value)) => Ok(Some(BigInt::from(*value))),
        Some(PropertyArg::Text(text)) => {
            text.trim().parse::<BigInt>().map(Some).map_err(|_| {
                Undecidable::new("property args: an integer argument is not an integer")
            })
        }
    }
}

fn required_rational(args: &PropertyArgs, name: &str) -> Result<BigRational, Undecidable> {
    let refusal = Undecidable::new(
        "property between needs `low` and `high`, each an exact number such as 2/5 or 0.4",
    );
    let value = match args.get(name) {
        None => return Err(refusal),
        Some(PropertyArg::Integer(value)) => return Ok(BigRational::from_integer((*value).into())),
        Some(PropertyArg::Text(text)) => canonical_form(text).map_err(|_| refusal)?,
    };
    match value {
        Canon::Rational(value) => Ok(value),
        _ => Err(refusal),
    }
}

/// Validate the stored example: it must have the property.
pub(super) fn expected(
    check: PropertyCheck,
    args: &PropertyArgs,
    text: &str,
) -> Result<Canon, Undecidable> {
    let predicate = predicate(check, args)?;
    let value = canonical_form(text)?;
    match decide(&predicate, &value)? {
        true => Ok(value),
        false => Err(Undecidable::new(
            "the stored example does not have the property its contract checks",
        )),
    }
}

/// Grade one learner answer against the property.
///
/// An answer outside the grammar keeps its refusal (ungraded, as for every
/// other contract). A readable answer of the wrong type is a decided miss.
pub(super) fn grade(check: PropertyCheck, args: &PropertyArgs, learner: &str) -> Outcome {
    let predicate = match predicate(check, args) {
        Ok(predicate) => predicate,
        Err(reason) => return Outcome::Undecidable(reason),
    };
    let value = match canonical_form(learner) {
        Ok(value) => value,
        Err(reason) => return Outcome::Undecidable(reason),
    };
    let value = match in_authored_unit(args, value) {
        Ok(value) => value,
        Err(reason) => return Outcome::Undecidable(reason),
    };
    match decide(&predicate, &value) {
        Ok(correct) => Outcome::Decided(Verdict {
            correct,
            notation: false,
        }),
        Err(reason) => Outcome::Undecidable(reason),
    }
}

/// The number of a learner answer that carries a unit.
///
/// The optional argument `unit` names the unit of the item (`"m"`): a learner
/// answer in that unit, or in an equal unit of the same kind, reads as its
/// number in the authored unit. Without `unit` the contract does not know which
/// unit the item asks for, so a learner answer with a unit gives no verdict.
fn in_authored_unit(args: &PropertyArgs, value: Canon) -> Result<Canon, Undecidable> {
    let Canon::Quantity {
        quantity,
        value: base,
    } = &value
    else {
        return Ok(value);
    };
    let Some(PropertyArg::Text(unit)) = args.get("unit") else {
        return Err(Undecidable::new(
            "this item asks for a number; write the answer without a unit",
        ));
    };
    let refusal = || Undecidable::new("the unit of the answer does not match the item");
    match canonical_form(&format!("1 {unit}"))? {
        Canon::Quantity {
            quantity: authored,
            value: one,
        } if authored == *quantity => match (base.as_ref(), one.as_ref()) {
            (Canon::Rational(base), Canon::Rational(one)) if !one.is_zero() => {
                Ok(Canon::Rational(base / one))
            }
            _ => Err(refusal()),
        },
        _ => Err(refusal()),
    }
}

/// Test one canonical value against one predicate.
fn decide(predicate: &Predicate, value: &Canon) -> Result<bool, Undecidable> {
    let value = unlabeled(value);
    if let Predicate::Between(low, high) = predicate {
        return Ok(matches!(value, Canon::Rational(q) if low < q && q < high));
    }
    let Some(n) = integer(value) else {
        return Ok(false);
    };
    match predicate {
        Predicate::DivisorCount(count, bounds) => {
            if !n.is_positive() || !bounds.holds(&n) {
                return Ok(false);
            }
            let small = n
                .to_u64()
                .filter(|n| *n <= MAX_DIVISOR_COUNT_INPUT)
                .ok_or(Undecidable::new(TOO_LARGE))?;
            Ok(divisor_count(small) == *count)
        }
        Predicate::Prime(bounds) => Ok(bounds.holds(&n) && is_prime(&n)?),
        Predicate::Composite(bounds) => Ok(bounds.holds(&n) && n > BigInt::one() && !is_prime(&n)?),
        Predicate::MultipleOf(k, bounds) => Ok(bounds.holds(&n) && (&n % k).is_zero()),
        Predicate::DivisorOf(k, bounds) => {
            Ok(n.is_positive() && bounds.holds(&n) && (k % &n).is_zero())
        }
        Predicate::CoprimeTo(k, bounds) => Ok(bounds.holds(&n) && n.gcd(k).is_one()),
        Predicate::IntegerInRange(bounds) => Ok(bounds.holds(&n)),
        Predicate::PrimeCounterexample { var, poly, min } => {
            if min.as_ref().is_some_and(|min| &n < min) {
                return Ok(false);
            }
            if n.abs() > BigInt::from(MAX_COUNTEREXAMPLE_INPUT) {
                return Err(Undecidable::new(TOO_LARGE));
            }
            let value = evaluate(poly, var, &n);
            if !value.is_integer() {
                return Ok(true);
            }
            Ok(!is_prime(&value.to_integer())?)
        }
        Predicate::Between(..) => unreachable!("decided above"),
    }
}

/// The value of an answer without a leading `x =` label.
fn unlabeled(value: &Canon) -> &Canon {
    let mut node = value;
    while let Canon::Assign { value, .. } = node {
        node = value;
    }
    node
}

fn integer(value: &Canon) -> Option<BigInt> {
    match value {
        Canon::Rational(q) if q.is_integer() => Some(q.to_integer()),
        _ => None,
    }
}

/// Evaluate a validated polynomial in one variable at an integer, exactly.
fn evaluate(poly: &Poly, var: &str, at: &BigInt) -> BigRational {
    let at = BigRational::from_integer(at.clone());
    let mut total = BigRational::zero();
    for (monomial, coefficient) in poly {
        let mut term = coefficient.clone();
        for (atom, exponent) in monomial {
            if *atom == Atom::Var(var.to_owned()) {
                let power = u32::try_from(*exponent).unwrap_or(0);
                term *= num_traits::pow(at.clone(), usize::try_from(power).unwrap_or(0));
            }
        }
        total += term;
    }
    total
}

/// Count the positive divisors of `n` by trial division up to its square root.
fn divisor_count(n: u64) -> u64 {
    let mut count = 0;
    let mut d = 1_u64;
    while d.saturating_mul(d) <= n {
        if n.is_multiple_of(d) {
            count += if d * d == n { 1 } else { 2 };
        }
        d += 1;
    }
    count
}

/// Whether `n` is prime, proved by Miller-Rabin below [`MAX_PRIME_INPUT`].
fn is_prime(n: &BigInt) -> Result<bool, Undecidable> {
    if n.sign() != Sign::Plus {
        return Ok(false);
    }
    let small = n.to_u128().filter(|n| *n < MAX_PRIME_INPUT);
    let Some(small) = small else {
        return Err(Undecidable::new(TOO_LARGE));
    };
    if small < 2 {
        return Ok(false);
    }
    for base in BASES {
        let base = u128::from(base);
        if small == base {
            return Ok(true);
        }
        if small % base == 0 {
            return Ok(false);
        }
    }
    let n = BigUint::from(small);
    let one = BigUint::one();
    let minus_one = &n - &one;
    let mut d = minus_one.clone();
    let mut s = 0_u32;
    while d.is_even() {
        d >>= 1_u32;
        s += 1;
    }
    'bases: for base in BASES {
        let mut x = BigUint::from(base).modpow(&d, &n);
        if x == one || x == minus_one {
            continue;
        }
        for _ in 1..s {
            x = (&x * &x) % &n;
            if x == minus_one {
                continue 'bases;
            }
        }
        return Ok(false);
    }
    Ok(true)
}

/// Candidate wrong answers near the stored example, for the key self-checks.
///
/// The list puts the boundary values first (an endpoint of `between`, one
/// past a bound), then walks outward from an integer example one step at a
/// time. A tool takes the first candidate that does not grade correct: when
/// the predicate rejects none of them, it accepts almost anything and the item
/// asks nothing.
#[must_use]
pub fn near_misses(check: PropertyCheck, args: &PropertyArgs, example: &str) -> Vec<String> {
    let Ok(predicate) = predicate(check, args) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    match &predicate {
        Predicate::Between(low, high) => {
            out.push(high.to_string());
            out.push(low.to_string());
            out.push((high + (high - low)).to_string());
        }
        Predicate::DivisorCount(_, bounds)
        | Predicate::Prime(bounds)
        | Predicate::Composite(bounds)
        | Predicate::MultipleOf(_, bounds)
        | Predicate::DivisorOf(_, bounds)
        | Predicate::CoprimeTo(_, bounds)
        | Predicate::IntegerInRange(bounds) => {
            if let Some(max) = &bounds.max {
                out.push((max + BigInt::one()).to_string());
            }
            if let Some(min) = &bounds.min {
                out.push((min - BigInt::one()).to_string());
            }
        }
        Predicate::PrimeCounterexample { min, .. } => {
            if let Some(min) = min {
                out.push((min - BigInt::one()).to_string());
            }
        }
    }
    if let Ok(value) = canonical_form(example)
        && let Some(n) = integer(unlabeled(&value))
    {
        for step in 1..=NEAR_MISS_STEPS {
            out.push((&n + BigInt::from(step)).to_string());
            out.push((&n - BigInt::from(step)).to_string());
        }
    }
    out
}

/// A one-line description of the property, for the model prompts.
#[must_use]
pub fn describe(check: PropertyCheck, args: &PropertyArgs) -> String {
    let Ok(predicate) = predicate(check, args) else {
        return "a property the checker could not read".to_owned();
    };
    let integer = |base: String, bounds: &Bounds| format!("{base}{}", bounds.words());
    match &predicate {
        Predicate::DivisorCount(n, bounds) => integer(
            format!("a positive integer with exactly {n} positive divisors"),
            bounds,
        ),
        Predicate::Prime(bounds) => integer("a prime number".to_owned(), bounds),
        Predicate::Composite(bounds) => integer("a composite number".to_owned(), bounds),
        Predicate::MultipleOf(k, bounds) => integer(format!("a multiple of {k}"), bounds),
        Predicate::DivisorOf(k, bounds) => integer(format!("a positive divisor of {k}"), bounds),
        Predicate::CoprimeTo(k, bounds) => integer(
            format!("an integer sharing no factor greater than 1 with {k}"),
            bounds,
        ),
        Predicate::IntegerInRange(bounds) => integer("an integer".to_owned(), bounds),
        Predicate::Between(low, high) => {
            format!("a rational number strictly between {low} and {high}, in any exact form")
        }
        Predicate::PrimeCounterexample { var, min, .. } => {
            let expr = match args.get("expr") {
                Some(PropertyArg::Text(expr)) => expr.as_str(),
                _ => "the expression",
            };
            let floor = min
                .as_ref()
                .map(|min| format!(" with {var} >= {min}"))
                .unwrap_or_default();
            format!("an integer {var}{floor} for which {expr} is not prime")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn divisor_counts_match_a_direct_count() {
        for n in 1..=200_u64 {
            let direct = (1..=n).filter(|d| n.is_multiple_of(*d)).count() as u64;
            assert_eq!(divisor_count(n), direct, "{n}");
        }
    }

    #[test]
    fn the_primality_test_matches_trial_division() {
        let trial = |n: u64| {
            n >= 2
                && (2..n)
                    .take_while(|d| d * d <= n)
                    .all(|d| !n.is_multiple_of(d))
        };
        for n in 0..5_000_u64 {
            assert_eq!(is_prime(&BigInt::from(n)), Ok(trial(n)), "{n}");
        }
        // Strong pseudoprimes to several small bases.
        for n in [
            3_215_031_751_u64,
            2_152_302_898_747,
            3_474_749_660_383,
            341_550_071_728_321,
        ] {
            assert_eq!(is_prime(&BigInt::from(n)), Ok(false), "{n}");
        }
        assert_eq!(is_prime(&BigInt::from(1_000_000_007_u64)), Ok(true));
        assert_eq!(is_prime(&BigInt::from(-7)), Ok(false));
        assert!(is_prime(&BigInt::from(MAX_PRIME_INPUT)).is_err());
    }
}
