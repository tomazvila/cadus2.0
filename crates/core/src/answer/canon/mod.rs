//! Canonical forms and exact arithmetic (V1, D6).
//!
//! Every value of the grammar reads into one canonical form. Two answers are the
//! same answer when their canonical forms are equal, so equality is a structural
//! comparison and never a search (V1: no heuristic simplification).
//!
//! # The one internal form
//!
//! Arithmetic runs on a rational function: one numerator sum over one
//! denominator sum. A sum is a map of terms, a term is a rational coefficient and
//! a monomial, and a monomial maps an [`Atom`] to an integer exponent. The atoms
//! are a square root of a squarefree integer, `pi`, `e`, an exponential with an
//! argument that is not a whole number, a variable, and a function call. One form
//! therefore holds a rational, a radical, a Laurent polynomial, a function
//! application, and a quotient of two polynomials together.
//!
//! [`Canon`] is the outside view of that form. `from_frac` demotes a quotient to
//! the narrowest variant that holds it, and the demotion is total and
//! deterministic, so two equal values always produce the same [`Canon`].
//!
//! # The rational-function form (M2 review 3, findings 9 to 13)
//!
//! [`Canon::Value`] holds the numerator and the denominator of one value. The
//! form obeys six rules, and the six rules together give one spelling per
//! value:
//!
//! 1. Both sides are expanded sums. No factored form survives.
//! 2. A denominator of one term folds into the numerator as negative exponents,
//!    so `1/x` is the monomial `x**-1` and never a quotient.
//! 3. A quotient with a denominator of two terms or more carries no common
//!    monomial factor and no negative exponent: the numerator and the
//!    denominator both divide by the greatest monomial that divides every term
//!    of the two. `1/x * 1/(x+h)` therefore becomes `1/(x**2+x*h)`, which is the
//!    form of `1/(x*(x+h))`, and `(6*s**3-17*s**2+15*s)/(s**4-3*s**3)` becomes
//!    `(6*s**2-17*s+15)/(s**3-3*s**2)`. The step takes a MONOMIAL content, the
//!    way rule 4 takes a rational content. It takes no polynomial factor.
//!    A square root and an exponential never carry a negative exponent, so for
//!    those two atoms the denominator alone gives the content:
//!    `1/(sqrt(2)*(x+1))` and `1/sqrt(2) * 1/(x+1)` are one value.
//! 4. The denominator is content- and sign-normalized: its coefficients are
//!    coprime integers and its greatest monomial carries a positive sign. The
//!    rational scale moves into the numerator, so `1/(x+1)` and `2/(2*x+2)` are
//!    one value.
//! 5. A numerator that is a rational multiple of the denominator is that
//!    rational: `(x+1)/(x+1)` is 1 and `(2*x+2)/(x+1)` is 2.
//! 6. A sum of two quotients goes over the product of the two denominators, and
//!    two equal denominators stay one denominator.
//!
//! The form cancels no polynomial common factor: a greatest common divisor of
//! two polynomials is beyond this unit. `(x**2-1)/(x-1)` and `x+1` therefore stay
//! two values, and `1/(x+1) + 1/(x+1)**2` and `(x+2)/(x+1)**2` stay two values.
//! Both narrowings are documented, and `answer_divergence.rs` pins the first one
//! with the 1.0 verdict.
//!
//! # What the form decides, and what it refuses
//!
//! - A decimal becomes an exact rational: `0.7` is `7/10`, never a float (D6).
//! - `sqrt(8)` becomes `2*sqrt(2)` and `sqrt(4)` becomes `2`. A rational radicand
//!   reduces too, through `sqrt(p/q) = sqrt(p*q)/q`, so `sqrt(1/2)` is
//!   `sqrt(2)/2` and `sqrt(4/9)` is `2/3`. A radicand that is not a rational
//!   number stays a function application.
//! - `exp(k)` for an integer `k` becomes the atom `e` with exponent `k`, so
//!   `e**2` and `exp(2)` are one value. The INTEGER PART of the rational constant
//!   term of the argument becomes the atom `e` too, so `e**(x+2)` and
//!   `e**2 * e**x` are one value, and `e**(5/2)` and `e**2 * e**(1/2)` are one
//!   value. The integer part is the floor, so the constant that stays behind is
//!   always in the half-open range 0 to 1 and a negative exponent has one
//!   spelling as well. The rest of the argument becomes [`Atom::Exp`], which
//!   obeys the exponent law: `e**(-x)` and `1/e**x` are one value, and `e**x`
//!   and `e**(2*x)` are two values.
//! - `ln` and `log` are one function, the natural logarithm, as they are in 1.0.
//! - `sin(x)**2 + cos(x)**2` and `1` are different values. That is the documented
//!   narrowing of 1.0 (V1).
//!
//! # Bounds
//!
//! The work bound, the term bound, and the size bound below are deterministic. An
//! answer that goes past one of them is [`Undecidable`]; it never becomes a wrong
//! verdict (C4). No step reads a clock and no step runs a search, so the cost of a
//! check depends on the input only (L2).
//!
//! The work bound charges the width of a number as well as the count of terms.
//! One operation on a wide rational costs about the square of one operation on a
//! machine-word rational, so a budget that counts term operations alone bounds
//! the wrong quantity: 2,000 term operations on 4,096-bit coefficients cost
//! 155 ms in a release build (M2 review 1, finding 11). Every rational the
//! arithmetic builds goes through [`Work::bounded`], and every intermediate of
//! the content normalization goes through [`Work::bounded_int`], so no operation
//! and no fold runs outside the budget.
//!
//! The products of the rational-function form are inside the same budget. Every
//! product of two sums charges the count of terms of the one sum times the count
//! of terms of the other, and every sum it builds goes through the term bound, so
//! a common denominator that grows costs the budget and then stops.
//!
//! The budget charges the REBUILD of a sum as well, at one step per term. A sum
//! is a map of terms, and an add, a multiply, a promotion, and a demotion each
//! copy or walk the whole map. A budget that charges the count of operations
//! alone therefore bounds the wrong quantity a second time: 846 factors of `1`
//! beside a 280-term sum spent 846 steps of the budget and 378 ms of a release
//! build, which is 1.26 times the whole 300 ms of L2 for one grade (M2 review 4,
//! finding 2). [`Work::rebuild`] is the charge, and every clone and every walk
//! of a whole sum runs through it.

mod arith;
mod quotient;
mod read;
mod sum;

use std::collections::{BTreeMap, BTreeSet};

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::Zero;

use super::Undecidable;
use super::ast::Ast;

/// The largest count of terms one canonical sum holds.
const MAX_TERMS: usize = 512;

/// The largest count of coefficient operations one canonicalization spends.
///
/// The number is a latency bound, not a taste. The budget pays for the count of
/// coefficient operations and for the width of every number those operations
/// build (see [`BITS_PER_STEP`]), so the worst answer inside the budget stays
/// well inside the 300 ms of L2. `(x+1)**200` goes past the budget and is
/// [`Undecidable`]; 1.0 spends 276 ms of the same budget on that one answer
/// (spec section 3.2), which is the concrete case for V1.
const MAX_STEPS: usize = 2_000;

/// The largest bit width of a numerator or a denominator.
///
/// 4,096 bits is about 1,233 decimal digits. A learner answer never needs one,
/// and a bigger number turns a multiplication into a latency problem. The width
/// charge of [`Work::spend_width`] refuses a number of about 2,880 bits earlier
/// still, because one operation of that width already costs the whole budget.
const MAX_BITS: u64 = 4_096;

/// The count of bits in one unit of width, for the width charge.
///
/// One machine word is the unit. A number under one word costs no extra unit, so
/// an ordinary answer spends what it spent before this rule. The charge grows
/// with the square of the count of words, so the budget bounds bit operations
/// and not term operations alone (L2). See [`Work::spend_width`].
const BITS_PER_STEP: u64 = 64;

/// The largest nesting the canonicalizer descends into.
const MAX_DEPTH: usize = 128;

/// The largest scale of an exact decimal, in digits after the point.
///
/// The denominator of the exact rational is `10^scale`, so the scale must stay
/// inside [`MAX_BITS`].
const MAX_SCALE: u32 = 1_000;

/// The largest trial divisor the radical factoring tries.
///
/// Trial division stops early when the next divisor is past the square root of
/// the remainder. A remainder that stops the division that way holds no prime
/// factor below its own square root, so it is 1 or a prime, and it needs no
/// further proof. A remainder that outlasts every divisor is at least the
/// square of this limit, and it needs the perfect-square test.
const TRIAL_DIVISION_LIMIT: u128 = 10_000;

/// One factor of a canonical monomial.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Atom {
    /// The square root of a squarefree integer greater than 1.
    Sqrt(BigInt),
    /// The ratio of a circle's circumference to its diameter.
    Pi,
    /// Euler's number.
    E,
    /// `e` raised to a value that is not a whole number.
    ///
    /// The atom always carries exponent 1 in a monomial. A power of the atom
    /// moves into the argument, so `e**(-x)` and `1/e**x` are one value and
    /// `(e**x)**3` is `e**(3*x)`.
    Exp(Box<Canon>),
    /// A variable of the answer.
    Var(String),
    /// An application of a whitelisted function to canonical arguments.
    Call(String, Vec<Canon>),
}

/// A product of atoms with integer exponents. No exponent is zero.
pub type Monomial = BTreeMap<Atom, i64>;

/// A sparse multivariate polynomial: monomials with rational coefficients.
///
/// The exponents are signed and the atoms carry radicals and function calls, so
/// the type also holds `1/x`, `sqrt(x)`, and `sin(x)/2`. No coefficient is zero.
pub type Poly = BTreeMap<Monomial, BigRational>;

/// The basis of one term of a [`Canon::Radical`].
///
/// The basis is a squarefree integer under a root, times a power of `pi`, times a
/// power of `e`. A radicand of 1 is the rational part of the combination.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Basis {
    /// The squarefree radicand. 1 means the term carries no root.
    pub radicand: BigInt,
    /// The exponent of `pi`.
    pub pi: i64,
    /// The exponent of `e`.
    pub e: i64,
}

/// The canonical form of one answer.
///
/// The variants run from the narrowest form to the widest. The canonicalizer
/// always picks the narrowest one that holds the value, so equality of two answers
/// is equality of two `Canon` values.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Canon {
    /// An exact rational.
    Rational(BigRational),
    /// A rational combination of roots and constants, keyed by basis.
    Radical(BTreeMap<Basis, BigRational>),
    /// A sparse multivariate polynomial.
    Poly(Poly),
    /// A quotient of two expanded sums, with a denominator of two terms or more.
    ///
    /// The variant is the rational-function form of the module header. A
    /// denominator of one term never reaches it: that denominator is negative
    /// exponents of the numerator, and the value is a [`Canon::Poly`].
    Value {
        /// The numerator. Never the empty sum, and never a negative exponent.
        num: Poly,
        /// The denominator. Two terms or more, content- and sign-normalized.
        den: Poly,
    },
    /// One application of a whitelisted function, with canonical arguments.
    Func(String, Vec<Canon>),
    /// An ordered tuple.
    Tuple(Vec<Canon>),
    /// An unordered set. Repeated members collapse into one member.
    Set(BTreeSet<Canon>),
    /// An ordered list.
    List(Vec<Canon>),
    /// A range of values, from a bracket pair or from an inequality.
    ///
    /// A bracket interval carries no variable. A chained inequality carries the
    /// variable it constrains, so `-1 <= x <= 3` and `[-1, 3]` stay different
    /// answers: one is a predicate over `x` and the other is a set of numbers.
    Interval {
        /// The constrained variable, for an inequality.
        var: Option<String>,
        /// The lower end. `None` means the range has no lower end.
        lo: Option<Box<Canon>>,
        /// True when the lower end belongs to the range. False when `lo` is `None`.
        lo_closed: bool,
        /// The upper end. `None` means the range has no upper end.
        hi: Option<Box<Canon>>,
        /// True when the upper end belongs to the range. False when `hi` is `None`.
        hi_closed: bool,
    },
    /// A value with the label of the unknown it answers for.
    ///
    /// A leading `x =` on an answer builds this variant. The canonical form keeps
    /// the label and the value apart, so `x = 4` and `y = 4` are never one value;
    /// `check` owns the rule that compares two labels.
    Assign {
        /// The labeled variable, as the answer spells it.
        var: String,
        /// The labeled value.
        value: Box<Canon>,
    },
}

/// One value as a numerator over a denominator, inside the arithmetic.
///
/// The six rules of the module header hold after [`Work::quotient`] builds it.
struct Frac {
    /// The numerator sum.
    num: Poly,
    /// The denominator sum.
    den: Poly,
}

/// Read one parsed answer into its canonical form.
///
/// # Errors
///
/// Returns [`Undecidable`] when the answer divides by zero, when it needs a
/// radicand the factoring cannot prove squarefree, or when it goes past the work,
/// term, size, or nesting bound of this module.
pub fn canon(ast: &Ast) -> Result<Canon, Undecidable> {
    let mut work = Work {
        steps: MAX_STEPS,
        depth: 0,
    };
    work.node(ast)
}

/// Read one parsed answer into its canonical form, with a given work budget.
///
/// `steps` replaces the default work bound. A test sweeps the budget from zero
/// upward to reach every refusal site of the arithmetic, so the fault path of
/// each bound has a test (V2, C4).
///
/// # Errors
///
/// Returns [`Undecidable`] for every case [`canon`] refuses, and for a budget
/// the answer spends before the canonical form is built.
pub fn canon_with_budget(ast: &Ast, steps: usize) -> Result<Canon, Undecidable> {
    let mut work = Work { steps, depth: 0 };
    work.node(ast)
}

/// The canonicalizer state: the work budget and the nesting depth.
struct Work {
    steps: usize,
    depth: usize,
}

impl Work {
    /// Take `count` units from the work budget.
    fn spend(&mut self, count: usize) -> Result<(), Undecidable> {
        match self.steps.checked_sub(count) {
            Some(left) => {
                self.steps = left;
                Ok(())
            }
            None => Err(Undecidable::new("the answer goes past the work bound")),
        }
    }

    /// Charge the width of one number to the work budget.
    ///
    /// The charge is the square of the width in machine words, because a
    /// multiplication and a greatest common divisor of two big integers cost
    /// about the square of the operand width. A number under one word costs
    /// nothing, so an ordinary answer spends what it spent before this rule. A
    /// number of about 2,880 bits costs the whole budget on its own, which is
    /// the correct price: one operation of that width is already a latency
    /// problem.
    fn spend_width(&mut self, bits: u64) -> Result<(), Undecidable> {
        let words = bits / BITS_PER_STEP;
        let units = usize::try_from(words.saturating_mul(words)).unwrap_or(usize::MAX);
        self.spend(units)
    }

    /// Charge one step for every term of a sum the arithmetic rebuilds.
    ///
    /// A sum is a map of terms, and an operation that clones it, walks it, or
    /// demotes it costs one step per term. The budget charged the COUNT of
    /// operations and not the COST of one, so 846 factors of `1` beside a
    /// 280-term sum cost 378 ms in a release build, which is 1.26 times the
    /// whole 300 ms of L2 for one grade (M2 review 4, finding 2).
    fn rebuild(&mut self, sum: &Poly) -> Result<(), Undecidable> {
        self.spend(sum.len())
    }

    /// Refuse a rational past the size bound, and charge its width.
    ///
    /// Every rational the arithmetic builds goes through this function, so no
    /// operand grows past the size bound and no operation runs outside the
    /// budget.
    fn bounded(&mut self, value: BigRational) -> Result<BigRational, Undecidable> {
        let bits = value.numer().bits().max(value.denom().bits());
        if bits > MAX_BITS {
            return Err(Undecidable::new("a number past the size bound"));
        }
        self.spend_width(bits)?;
        Ok(value)
    }

    /// Refuse an integer past the size bound, and charge its width.
    fn bounded_int(&mut self, value: &BigInt) -> Result<(), Undecidable> {
        let bits = value.bits();
        if bits > MAX_BITS {
            return Err(Undecidable::new("a number past the size bound"));
        }
        self.spend_width(bits)
    }

    /// Wrap a rational in its canonical form, after the size bound.
    fn rational(&mut self, value: BigRational) -> Result<Canon, Undecidable> {
        Ok(Canon::Rational(self.bounded(value)?))
    }

    /// Read a literal fraction, or refuse a zero denominator.
    fn exact_fraction(
        &mut self,
        numerator: &BigInt,
        denominator: &BigInt,
    ) -> Result<Canon, Undecidable> {
        if denominator.is_zero() {
            return Err(Undecidable::new("a division by zero"));
        }
        self.rational(BigRational::new(numerator.clone(), denominator.clone()))
    }

    /// Canonicalize one node, one level deeper.
    fn node(&mut self, ast: &Ast) -> Result<Canon, Undecidable> {
        self.spend(1)?;
        if self.depth >= MAX_DEPTH {
            return Err(Undecidable::new("the answer nests too deeply"));
        }
        self.depth += 1;
        let result = self.dispatch(ast);
        self.depth -= 1;
        result
    }
}
