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

use std::collections::{BTreeMap, BTreeSet};

use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};

use super::Undecidable;
use super::ast::{Ast, Const, IneqOp};

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
const TRIAL_DIVISION_LIMIT: u128 = 10_000;

/// Above this value a squarefree radicand needs a proof, not a trial division.
///
/// Every prime factor below [`TRIAL_DIVISION_LIMIT`] is already removed, so a
/// remainder below the square of that limit carries no square factor.
const SQUAREFREE_CERTAIN: u128 = TRIAL_DIVISION_LIMIT * TRIAL_DIVISION_LIMIT;

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

    /// Canonicalize one node by its kind.
    fn dispatch(&mut self, ast: &Ast) -> Result<Canon, Undecidable> {
        match ast {
            Ast::Integer(value) => self.rational(BigRational::from_integer(value.clone())),
            Ast::Decimal { mantissa, scale } => self.decimal(mantissa, *scale),
            Ast::Fraction {
                numerator,
                denominator,
            } => self.exact_fraction(numerator, denominator),
            Ast::Mixed {
                whole,
                numerator,
                denominator,
            } => self.mixed(whole, numerator, denominator),
            Ast::Var(name) => Ok(atom_value(Atom::Var(name.clone()))),
            Ast::Const(Const::Pi) => Ok(atom_value(Atom::Pi)),
            Ast::Const(Const::E) => Ok(atom_value(Atom::E)),
            // The root of the grammar. `\sqrt{a}`, `√a`, and `sqrt(a)` all build
            // this one node, so the three spellings take one path (FIXM2g).
            Ast::Sqrt(inner) => {
                let argument = self.node(inner)?;
                self.call("sqrt", vec![argument])
            }
            Ast::Pow(base, exponent) => {
                let base = self.node(base)?;
                self.power(&base, *exponent)
            }
            Ast::Neg(inner) => {
                let value = self.node(inner)?;
                let minus_one = Canon::Rational(-BigRational::one());
                self.multiply(&minus_one, &value)
            }
            Ast::Add(items) => {
                let mut total = Canon::Rational(BigRational::zero());
                for item in items {
                    let value = self.node(item)?;
                    total = self.add(&total, &value)?;
                }
                Ok(total)
            }
            Ast::Mul(items) => {
                let mut product = Canon::Rational(BigRational::one());
                for item in items {
                    let value = self.node(item)?;
                    product = self.multiply(&product, &value)?;
                }
                Ok(product)
            }
            // Division and the `\frac{a}{b}` node are one operation. FIXM2g adds
            // `Ast::Frac(a, b)`; that node takes this same arm, and the arm reads
            //
            //     Ast::Frac(numerator, denominator) => { … self.divide(…) }
            //
            // `Ast::Percent(p)` of the same unit is `p / 100`, so its arm divides
            // by the literal 100. Neither variant is in `answer/ast.rs` yet, so
            // neither arm compiles yet; `divide` is the one function both need.
            Ast::Div(dividend, divisor) => {
                let dividend = self.node(dividend)?;
                let divisor = self.node(divisor)?;
                self.divide(&dividend, &divisor)
            }
            Ast::Func(name, arguments) => {
                let values = self.items(arguments)?;
                self.call(name, values)
            }
            Ast::Tuple(items) => Ok(Canon::Tuple(self.items(items)?)),
            Ast::List(items) => Ok(Canon::List(self.items(items)?)),
            Ast::Set(items) => Ok(Canon::Set(self.items(items)?.into_iter().collect())),
            Ast::Assign { var, value } => {
                let value = self.node(value)?;
                Ok(Canon::Assign {
                    var: var.clone(),
                    value: Box::new(value),
                })
            }
            Ast::Interval {
                lo,
                hi,
                lo_closed,
                hi_closed,
            } => {
                let lo = self.node(lo)?;
                let hi = self.node(hi)?;
                Ok(Canon::Interval {
                    var: None,
                    lo: Some(Box::new(lo)),
                    lo_closed: *lo_closed,
                    hi: Some(Box::new(hi)),
                    hi_closed: *hi_closed,
                })
            }
            // The label is not part of the value, so the canonical form drops it.
            // `check` is the step that compares the two labels (FIXM2b, review
            // findings #2, #10, #16).
            Ast::Ineq { var, op, bound } => {
                let bound = Box::new(self.node(bound)?);
                Ok(match op {
                    IneqOp::Lt | IneqOp::Le => Canon::Interval {
                        var: Some(var.clone()),
                        lo: None,
                        lo_closed: false,
                        hi: Some(bound),
                        hi_closed: *op == IneqOp::Le,
                    },
                    IneqOp::Gt | IneqOp::Ge => Canon::Interval {
                        var: Some(var.clone()),
                        lo: Some(bound),
                        lo_closed: *op == IneqOp::Ge,
                        hi: None,
                        hi_closed: false,
                    },
                })
            }
            Ast::Chain {
                lo,
                lo_closed,
                var,
                hi_closed,
                hi,
            } => {
                let lo = self.node(lo)?;
                let hi = self.node(hi)?;
                Ok(Canon::Interval {
                    var: Some(var.clone()),
                    lo: Some(Box::new(lo)),
                    lo_closed: *lo_closed,
                    hi: Some(Box::new(hi)),
                    hi_closed: *hi_closed,
                })
            }
        }
    }

    /// Canonicalize every item of a collection.
    fn items(&mut self, items: &[Ast]) -> Result<Vec<Canon>, Undecidable> {
        let mut values = Vec::with_capacity(items.len());
        for item in items {
            values.push(self.node(item)?);
        }
        Ok(values)
    }

    /// Read an exact decimal as `mantissa / 10^scale`.
    fn decimal(&mut self, mantissa: &BigInt, scale: u32) -> Result<Canon, Undecidable> {
        if scale > MAX_SCALE {
            return Err(Undecidable::new("a decimal past the size bound"));
        }
        self.spend(1)?;
        let denominator = BigInt::from(10_u32).pow(scale);
        self.rational(BigRational::new(mantissa.clone(), denominator))
    }

    /// Read a mixed number `a b/c` as `sign(a) * (|a| + b/c)`.
    fn mixed(
        &mut self,
        whole: &BigInt,
        numerator: &BigInt,
        denominator: &BigInt,
    ) -> Result<Canon, Undecidable> {
        if denominator.is_zero() {
            return Err(Undecidable::new("a division by zero"));
        }
        self.spend(1)?;
        let magnitude = BigRational::from_integer(whole.abs())
            + BigRational::new(numerator.clone(), denominator.clone());
        let value = if whole.is_negative() {
            -magnitude
        } else {
            magnitude
        };
        self.rational(value)
    }

    /// Add two canonical values, over the common denominator.
    ///
    /// Two equal denominators stay one denominator, and two different ones give
    /// the product. The rule is what puts `2/x + 1/(x+1)` and `(3*x+2)/(x*(x+1))`
    /// into one form (M2 review 3, finding 13).
    fn add(&mut self, left: &Canon, right: &Canon) -> Result<Canon, Undecidable> {
        let left = self.frac_of(left)?;
        let right = self.frac_of(right)?;
        if left.den == right.den {
            let num = self.poly_add(&left.num, &right.num)?;
            return self.value(num, left.den);
        }
        let left_part = self.poly_mul(&left.num, &right.den)?;
        let right_part = self.poly_mul(&right.num, &left.den)?;
        let num = self.poly_add(&left_part, &right_part)?;
        let den = self.poly_mul(&left.den, &right.den)?;
        self.value(num, den)
    }

    /// Multiply two canonical values.
    fn multiply(&mut self, left: &Canon, right: &Canon) -> Result<Canon, Undecidable> {
        let left = self.frac_of(left)?;
        let right = self.frac_of(right)?;
        let num = self.poly_mul(&left.num, &right.num)?;
        let den = self.poly_mul(&left.den, &right.den)?;
        self.value(num, den)
    }

    /// Divide one canonical value by another.
    fn divide(&mut self, left: &Canon, right: &Canon) -> Result<Canon, Undecidable> {
        let left = self.frac_of(left)?;
        let right = self.frac_of(right)?;
        let num = self.poly_mul(&left.num, &right.den)?;
        let den = self.poly_mul(&left.den, &right.num)?;
        self.value(num, den)
    }

    /// Raise a canonical value to an integer power.
    fn power(&mut self, base: &Canon, exponent: i64) -> Result<Canon, Undecidable> {
        let base = self.frac_of(base)?;
        if base.num.is_empty() {
            return if exponent > 0 {
                Ok(Canon::Rational(BigRational::zero()))
            } else {
                Err(Undecidable::new("a zero base with a non-positive exponent"))
            };
        }
        if exponent == 0 {
            return Ok(Canon::Rational(BigRational::one()));
        }
        if exponent < 0 {
            let magnitude = exponent
                .checked_neg()
                .ok_or_else(|| Undecidable::new("an exponent past the size bound"))?;
            let num = self.poly_pow(&base.den, magnitude)?;
            let den = self.poly_pow(&base.num, magnitude)?;
            return self.value(num, den);
        }
        let num = self.poly_pow(&base.num, exponent)?;
        let den = self.poly_pow(&base.den, exponent)?;
        self.value(num, den)
    }

    /// Add two sums.
    fn poly_add(&mut self, left: &Poly, right: &Poly) -> Result<Poly, Undecidable> {
        self.rebuild(left)?;
        let mut sum = left.clone();
        for (monomial, coefficient) in right {
            self.spend(1)?;
            self.insert_term(&mut sum, monomial.clone(), coefficient.clone())?;
        }
        bound_terms(&sum)?;
        Ok(sum)
    }

    /// Multiply two sums, term by term.
    ///
    /// The charge is the count of terms of the one sum times the count of terms
    /// of the other, so the common denominators of the rational-function form
    /// stay inside the work bound.
    fn poly_mul(&mut self, left: &Poly, right: &Poly) -> Result<Poly, Undecidable> {
        // A factor of 1 is the common case of the form: every value whose
        // denominator is one term carries this exact sum. The short cut skips
        // the term-by-term product, and it still copies the whole sum, so it
        // charges the copy (M2 review 4, finding 2).
        if is_one(left) {
            self.rebuild(right)?;
            return Ok(right.clone());
        }
        if is_one(right) {
            self.rebuild(left)?;
            return Ok(left.clone());
        }
        self.spend(left.len().saturating_mul(right.len()))?;
        let mut product = Poly::new();
        for (left_monomial, left_coefficient) in left {
            for (right_monomial, right_coefficient) in right {
                let (monomial, coefficient) = self.multiply_terms(
                    left_monomial,
                    left_coefficient,
                    right_monomial,
                    right_coefficient,
                )?;
                self.insert_term(&mut product, monomial, coefficient)?;
                bound_terms(&product)?;
            }
        }
        Ok(product)
    }

    /// Raise one sum to a power of one or more.
    fn poly_pow(&mut self, base: &Poly, exponent: i64) -> Result<Poly, Undecidable> {
        if exponent <= 0 {
            return Ok(one_poly());
        }
        if let Some((monomial, coefficient)) = one_term(base) {
            self.spend(monomial.len().saturating_add(1))?;
            let (monomial, coefficient) = self.power_of_term(&monomial, &coefficient, exponent)?;
            return Ok(term(monomial, coefficient));
        }
        self.rebuild(base)?;
        let mut result = one_poly();
        let mut square = base.clone();
        let mut left = exponent;
        while left > 0 {
            if left % 2 == 1 {
                result = self.poly_mul(&result, &square)?;
            }
            left /= 2;
            if left > 0 {
                square = self.poly_mul(&square, &square)?;
            }
        }
        Ok(result)
    }

    /// Apply a whitelisted function to canonical arguments.
    fn call(&mut self, name: &str, arguments: Vec<Canon>) -> Result<Canon, Undecidable> {
        self.spend(1)?;
        // `ln` and `log` are one function: the natural logarithm. 1.0 makes `ln`
        // an alias of `log`, and the corpus authors both spellings on the topic
        // `change-of-base-formula`.
        let name = if name == "ln" { "log" } else { name };
        if arguments.len() == 1 {
            if name == "sqrt"
                && let Some(Canon::Rational(value)) = arguments.first()
            {
                let value = value.clone();
                return self.root_of_rational(&value, arguments);
            }
            if name == "exp"
                && let Some(argument) = arguments.first()
            {
                let argument = argument.clone();
                return self.exponential(&argument);
            }
        }
        Ok(atom_value(Atom::Call(name.to_string(), arguments)))
    }

    /// Read `exp(a)` into the canonical exponential.
    ///
    /// A whole `a` gives the atom `e` with exponent `a`, so `exp(2)` and `e**2`
    /// are one value. Every other `a` gives [`Atom::Exp`].
    fn exponential(&mut self, argument: &Canon) -> Result<Canon, Undecidable> {
        let mut monomial = Monomial::new();
        self.add_exp(&mut monomial, argument, 1)?;
        Ok(from_sum(term(monomial, BigRational::one())))
    }

    /// Reduce `sqrt(p/q)` into `sqrt(p*q)/q` (M2 review 3, findings 9 and 11).
    ///
    /// `p/q` is in lowest terms with a positive `q`, so `p*q` is a whole number
    /// and the identity is exact. `sqrt(1/2)` therefore becomes `sqrt(2)/2` and
    /// `sqrt(4/9)` becomes `2/3`, which are the two forms 1.0 answers True for.
    fn root_of_rational(
        &mut self,
        value: &BigRational,
        arguments: Vec<Canon>,
    ) -> Result<Canon, Undecidable> {
        if value.is_negative() {
            // The grammar holds real values only, so a negative radicand keeps its
            // function application and compares structurally.
            return Ok(atom_value(Atom::Call("sqrt".to_string(), arguments)));
        }
        if value.is_zero() {
            return Ok(Canon::Rational(BigRational::zero()));
        }
        let denominator = value.denom().clone();
        let radicand = value.numer() * &denominator;
        self.bounded_int(&radicand)?;
        let root = self.root_of_integer(&radicand, arguments)?;
        if denominator.is_one() {
            return Ok(root);
        }
        let scale = BigRational::new(BigInt::one(), denominator);
        let scale = Canon::Rational(self.bounded(scale)?);
        self.multiply(&root, &scale)
    }

    /// Reduce `sqrt(n)` for a whole number `n` into `outside * sqrt(radicand)`.
    fn root_of_integer(
        &mut self,
        value: &BigInt,
        arguments: Vec<Canon>,
    ) -> Result<Canon, Undecidable> {
        if value.is_negative() {
            return Ok(atom_value(Atom::Call("sqrt".to_string(), arguments)));
        }
        if value.is_zero() {
            return Ok(Canon::Rational(BigRational::zero()));
        }
        self.spend(1)?;
        let (outside, radicand) = extract_square(value)?;
        let mut monomial = Monomial::new();
        let mut coefficient = self.bounded(BigRational::from_integer(outside))?;
        if !radicand.is_one() {
            self.add_atom(&mut monomial, &mut coefficient, &Atom::Sqrt(radicand), 1)?;
        }
        Ok(from_sum(term(monomial, coefficient)))
    }

    /// Add one term into a sum, and drop a term whose coefficient cancels to zero.
    fn insert_term(
        &mut self,
        sum: &mut Poly,
        monomial: Monomial,
        coefficient: BigRational,
    ) -> Result<(), Undecidable> {
        if coefficient.is_zero() {
            return Ok(());
        }
        let total = match sum.get(&monomial) {
            Some(present) => present.clone() + coefficient,
            None => coefficient,
        };
        let total = self.bounded(total)?;
        if total.is_zero() {
            sum.remove(&monomial);
        } else {
            sum.insert(monomial, total);
        }
        Ok(())
    }

    /// Multiply two terms into one term.
    fn multiply_terms(
        &mut self,
        left_monomial: &Monomial,
        left_coefficient: &BigRational,
        right_monomial: &Monomial,
        right_coefficient: &BigRational,
    ) -> Result<(Monomial, BigRational), Undecidable> {
        let mut monomial = left_monomial.clone();
        let product = left_coefficient * right_coefficient;
        let mut coefficient = self.bounded(product)?;
        for (atom, exponent) in right_monomial {
            self.add_atom(&mut monomial, &mut coefficient, atom, *exponent)?;
        }
        Ok((monomial, coefficient))
    }

    /// Raise one term to an integer power.
    fn power_of_term(
        &mut self,
        monomial: &Monomial,
        coefficient: &BigRational,
        exponent: i64,
    ) -> Result<(Monomial, BigRational), Undecidable> {
        let mut out_monomial = Monomial::new();
        let mut out_coefficient = self.rational_power(coefficient, exponent)?;
        for (atom, atom_exponent) in monomial {
            let scaled = atom_exponent
                .checked_mul(exponent)
                .ok_or_else(|| Undecidable::new("an exponent past the size bound"))?;
            self.add_atom(&mut out_monomial, &mut out_coefficient, atom, scaled)?;
        }
        Ok((out_monomial, out_coefficient))
    }

    /// Multiply one atom power into a monomial, and move every square into the
    /// coefficient.
    ///
    /// A monomial holds at most one [`Atom::Sqrt`], and that atom always has
    /// exponent 1. `sqrt(2)**3` moves a factor 2 out, and `sqrt(2)*sqrt(3)`
    /// becomes `sqrt(6)`. A monomial holds at most one [`Atom::Exp`] too, and a
    /// power of it moves into its argument.
    fn add_atom(
        &mut self,
        monomial: &mut Monomial,
        coefficient: &mut BigRational,
        atom: &Atom,
        exponent: i64,
    ) -> Result<(), Undecidable> {
        if exponent == 0 {
            return Ok(());
        }
        if let Atom::Exp(inner) = atom {
            let inner = inner.as_ref().clone();
            return self.add_exp(monomial, &inner, exponent);
        }
        let Atom::Sqrt(radicand) = atom else {
            return insert_atom(monomial, atom, exponent);
        };
        let (squares, rest) = exponent.div_mod_floor(&2);
        let factor = self.int_power(radicand, squares)?;
        *coefficient = self.bounded(&*coefficient * factor)?;
        if rest == 0 {
            return Ok(());
        }
        let present = monomial.iter().find_map(|(key, _)| match key {
            Atom::Sqrt(value) => Some(value.clone()),
            _ => None,
        });
        let Some(present) = present else {
            monomial.insert(Atom::Sqrt(radicand.clone()), 1);
            return Ok(());
        };
        monomial.remove(&Atom::Sqrt(present.clone()));
        let (outside, merged) = extract_square(&(present * radicand))?;
        *coefficient = self.bounded(&*coefficient * BigRational::from_integer(outside))?;
        if !merged.is_one() {
            monomial.insert(Atom::Sqrt(merged), 1);
        }
        Ok(())
    }

    /// Multiply `e**(exponent * inner)` into a monomial.
    ///
    /// The exponent law lives here: a power of the atom scales the argument, and
    /// two exponentials add their arguments. A whole argument gives the atom
    /// [`Atom::E`] instead, so `exp(x)*exp(-x)` is 1 and `exp(x)*exp(2-x)` is
    /// `e**2`.
    ///
    /// The INTEGER PART of the rational constant term gives [`Atom::E`] too, so
    /// `e**(x+2)` and `e**2 * e**x` are one value, and `e**(5/2)` and
    /// `e**2 * e**(1/2)` are one value (M2 review 3, finding 12). The integer
    /// part is the floor, not the truncation: the constant that stays behind is
    /// then always in the half-open range 0 to 1, so `e**(-5/2)`,
    /// `e**-3 * e**(1/2)`, and `e**-2 * e**(-1/2)` are one value too.
    fn add_exp(
        &mut self,
        monomial: &mut Monomial,
        inner: &Canon,
        exponent: i64,
    ) -> Result<(), Undecidable> {
        if exponent == 0 {
            return Ok(());
        }
        let scaled = if exponent == 1 {
            inner.clone()
        } else {
            let factor = Canon::Rational(BigRational::from_integer(BigInt::from(exponent)));
            self.multiply(&factor, inner)?
        };
        let present = monomial.iter().find_map(|(key, _)| match key {
            Atom::Exp(value) => Some(value.as_ref().clone()),
            _ => None,
        });
        let total = match present {
            Some(value) => {
                monomial.remove(&Atom::Exp(Box::new(value.clone())));
                self.add(&value, &scaled)?
            }
            None => scaled,
        };
        // A collection carries no arithmetic, and a quotient carries no constant
        // term of its own, so both keep the whole argument.
        let Ok(argument) = self.frac_of(&total) else {
            monomial.insert(Atom::Exp(Box::new(total)), 1);
            return Ok(());
        };
        if !is_one(&argument.den) {
            monomial.insert(Atom::Exp(Box::new(total)), 1);
            return Ok(());
        }
        let mut argument = argument.num;
        let constant = Monomial::new();
        if let Some(value) = argument.get(&constant).cloned() {
            let whole = value.floor().to_integer();
            if let Some(step) = whole.to_i64() {
                let rest = self.bounded(value - BigRational::from_integer(whole))?;
                if rest.is_zero() {
                    argument.remove(&constant);
                } else {
                    argument.insert(constant, rest);
                }
                if step != 0 {
                    insert_atom(monomial, &Atom::E, step)?;
                }
            }
        }
        if !argument.is_empty() {
            monomial.insert(Atom::Exp(Box::new(from_sum(argument))), 1);
        }
        Ok(())
    }

    /// Raise an integer to an integer power, as an exact rational.
    fn int_power(&mut self, base: &BigInt, exponent: i64) -> Result<BigRational, Undecidable> {
        if exponent == 0 {
            return Ok(BigRational::one());
        }
        if base.is_zero() {
            return if exponent > 0 {
                Ok(BigRational::zero())
            } else {
                Err(Undecidable::new("a division by zero"))
            };
        }
        let magnitude = exponent
            .checked_abs()
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(|| Undecidable::new("an exponent past the size bound"))?;
        if base.bits().saturating_mul(u64::from(magnitude)) > MAX_BITS {
            return Err(Undecidable::new("a number past the size bound"));
        }
        let power = base.pow(magnitude);
        let value = if exponent > 0 {
            BigRational::from_integer(power)
        } else {
            BigRational::new(BigInt::one(), power)
        };
        self.bounded(value)
    }

    /// Raise a rational to an integer power, as an exact rational.
    fn rational_power(
        &mut self,
        value: &BigRational,
        exponent: i64,
    ) -> Result<BigRational, Undecidable> {
        if value.is_zero() {
            return if exponent > 0 {
                Ok(BigRational::zero())
            } else {
                Err(Undecidable::new("a division by zero"))
            };
        }
        let numerator = self.int_power(value.numer(), exponent)?;
        let denominator = self.int_power(value.denom(), exponent)?;
        self.bounded(numerator / denominator)
    }

    /// Split a sum into its rational content and its primitive part.
    ///
    /// The primitive part has coprime integer coefficients, and its greatest
    /// monomial carries a positive coefficient. The content holds the sign.
    /// `2*x + 2` therefore becomes `2` and `x + 1`, so `1/(x+1)` and `2/(2*x+2)`
    /// are one value.
    ///
    /// The fold runs the size bound and the width charge after every step. The
    /// least common multiple of many coprime denominators grows fast, and an
    /// unbounded fold cost one check 8.4 s of CPU (M2 review 1, finding 5). The
    /// width charge on the coefficients already empties the budget before a
    /// bomb of that shape reaches this function, so the two bounds hold the same
    /// case twice.
    fn content_normalize(&mut self, sum: &Poly) -> Result<(BigRational, Poly), Undecidable> {
        let mut numerator_gcd = BigInt::zero();
        let mut denominator_lcm = BigInt::one();
        for coefficient in sum.values() {
            self.spend(1)?;
            numerator_gcd = numerator_gcd.gcd(coefficient.numer());
            denominator_lcm = denominator_lcm.lcm(coefficient.denom());
            self.bounded_int(&numerator_gcd)?;
            self.bounded_int(&denominator_lcm)?;
        }
        if numerator_gcd.is_zero() {
            return Err(Undecidable::new("a division by zero"));
        }
        let leading_is_negative = sum
            .iter()
            .next_back()
            .is_some_and(|(_, coefficient)| coefficient.is_negative());
        let magnitude = BigRational::new(numerator_gcd, denominator_lcm);
        let signed = if leading_is_negative {
            -magnitude
        } else {
            magnitude
        };
        let content = self.bounded(signed)?;
        let divisor = reciprocal_of(&content)?;
        let mut primitive = Poly::new();
        for (monomial, coefficient) in sum {
            let scaled = self.bounded(coefficient * &divisor)?;
            if !scaled.is_zero() {
                primitive.insert(monomial.clone(), scaled);
            }
        }
        Ok((content, primitive))
    }

    /// Build the canonical value of one numerator over one denominator.
    ///
    /// The rules of [`Work::quotient`] read every term of the two sums, and the
    /// demotion of [`from_frac`] reads every term of the numerator again, so the
    /// step charges both sums (M2 review 4, finding 2).
    fn value(&mut self, num: Poly, den: Poly) -> Result<Canon, Undecidable> {
        self.rebuild(&num)?;
        self.rebuild(&den)?;
        let quotient = self.quotient(num, den)?;
        Ok(from_frac(quotient))
    }

    /// Put one numerator over one denominator into the rational-function form.
    ///
    /// The rules of the module header run here, in order: a denominator of one
    /// term folds into the numerator as negative exponents; every negative
    /// exponent of a true quotient clears into the denominator; and the
    /// denominator is content- and sign-normalized with the scale moved into the
    /// numerator. The step cancels no polynomial common factor.
    ///
    /// # Errors
    ///
    /// Returns [`Undecidable`] for a zero denominator, and for a value past the
    /// work, term, or size bound.
    fn quotient(&mut self, num: Poly, den: Poly) -> Result<Frac, Undecidable> {
        if den.is_empty() {
            return Err(Undecidable::new("a division by zero"));
        }
        if num.is_empty() {
            return Ok(Frac {
                num,
                den: one_poly(),
            });
        }
        if is_one(&den) {
            return Ok(Frac { num, den });
        }
        // Rule 3. A true quotient carries no common monomial factor and no
        // negative exponent. `1/x * 1/(x+h)` therefore reaches the form of
        // `1/(x*(x+h))` (M2 review 3, finding 10). The step covers rule 2 as
        // well: a denominator of one term divides itself away, and the fold
        // below finishes it.
        let common = common_monomial(&num, &den);
        let (num, den) = if common.is_empty() {
            (num, den)
        } else {
            self.spend(common.len())?;
            let (monomial, coefficient) = self.power_of_term(&common, &BigRational::one(), -1)?;
            let factor = term(monomial, coefficient);
            let num = self.poly_mul(&num, &factor)?;
            let den = self.poly_mul(&den, &factor)?;
            (num, den)
        };
        if one_term(&den).is_some() {
            // Rule 2. A denominator of one term is negative exponents of the
            // numerator, so `1/x` stays the monomial `x**-1`. A product of two
            // sums of two terms or more reaches this line too, when two atoms
            // merge: `(sqrt(2)+1)*(sqrt(2)-1)` is 1.
            return self.fold_monomial_denominator(num, &den);
        }
        // Rule 4. The denominator holds coprime integer coefficients and a
        // positive greatest monomial; the scale moves into the numerator.
        let (content, primitive) = self.content_normalize(&den)?;
        let scale = reciprocal_of(&content)?;
        let scale = self.bounded(scale)?;
        let num = self.poly_mul(&num, &term(Monomial::new(), scale))?;
        if num.is_empty() {
            return Ok(Frac {
                num,
                den: one_poly(),
            });
        }
        // Rule 5. A numerator that is a rational multiple of the denominator is
        // that rational: `(x+1)/(x+1)` is 1 and `(2*x+2)/(x+1)` is 2. The test
        // compares the two monomial keys first and the two coefficient ratios
        // after, so it costs no division of one polynomial by another.
        if let Some(scalar) = self.scalar_ratio(&num, &primitive)? {
            return Ok(Frac {
                num: term(Monomial::new(), scalar),
                den: one_poly(),
            });
        }
        Ok(Frac {
            num,
            den: primitive,
        })
    }

    /// Read the rational `r` of `num = r * den`, when there is one.
    ///
    /// The two sums must hold the same monomials, and every coefficient of the
    /// one must be the same multiple of the coefficient of the other. The test
    /// runs no polynomial division: it is the numerator half of the content
    /// normalization of rule 4.
    fn scalar_ratio(&mut self, num: &Poly, den: &Poly) -> Result<Option<BigRational>, Undecidable> {
        if num.len() != den.len() || !num.keys().eq(den.keys()) {
            return Ok(None);
        }
        let mut ratio: Option<BigRational> = None;
        for (left, right) in num.values().zip(den.values()) {
            self.spend(1)?;
            match &ratio {
                None => ratio = Some(self.bounded(left / right)?),
                Some(present) => {
                    let scaled = self.bounded(present * right)?;
                    if &scaled != left {
                        return Ok(None);
                    }
                }
            }
        }
        Ok(ratio)
    }

    /// Fold a denominator of one term into the numerator as negative exponents.
    ///
    /// Rule 2 of the module header. `1/x` therefore stays the monomial `x**-1`,
    /// and `1/sqrt(2)` stays `sqrt(2)/2`, because the reciprocal of the term goes
    /// through the same atom laws as every other product.
    fn fold_monomial_denominator(&mut self, num: Poly, den: &Poly) -> Result<Frac, Undecidable> {
        let Some((monomial, coefficient)) = one_term(den) else {
            return Err(Undecidable::new("a division by zero"));
        };
        self.spend(monomial.len().saturating_add(1))?;
        let (monomial, coefficient) = self.power_of_term(&monomial, &coefficient, -1)?;
        let num = self.poly_mul(&num, &term(monomial, coefficient))?;
        Ok(Frac {
            num,
            den: one_poly(),
        })
    }

    /// Promote a canonical form back into the internal quotient of two sums.
    ///
    /// # Errors
    ///
    /// Returns [`Undecidable`] for a collection: a tuple, a set, a list, a range,
    /// and a labeled value carry no arithmetic.
    fn frac_of(&mut self, value: &Canon) -> Result<Frac, Undecidable> {
        let num = match value {
            Canon::Rational(number) => term(Monomial::new(), number.clone()),
            Canon::Radical(parts) => {
                // The promotion touches every part, so it charges every part
                // (M2 review 4, finding 2).
                self.spend(parts.len())?;
                let mut sum = Poly::new();
                for (basis, coefficient) in parts {
                    let mut monomial = Monomial::new();
                    if !basis.radicand.is_one() {
                        monomial.insert(Atom::Sqrt(basis.radicand.clone()), 1);
                    }
                    if basis.pi != 0 {
                        monomial.insert(Atom::Pi, basis.pi);
                    }
                    if basis.e != 0 {
                        monomial.insert(Atom::E, basis.e);
                    }
                    self.insert_term(&mut sum, monomial, coefficient.clone())?;
                }
                sum
            }
            Canon::Poly(parts) => {
                self.rebuild(parts)?;
                parts.clone()
            }
            Canon::Value { num, den } => {
                self.rebuild(num)?;
                self.rebuild(den)?;
                return Ok(Frac {
                    num: num.clone(),
                    den: den.clone(),
                });
            }
            Canon::Func(name, arguments) => {
                let mut monomial = Monomial::new();
                monomial.insert(Atom::Call(name.clone(), arguments.clone()), 1);
                term(monomial, BigRational::one())
            }
            Canon::Tuple(_) | Canon::Set(_) | Canon::List(_) | Canon::Interval { .. } => {
                return Err(Undecidable::new("arithmetic on a collection"));
            }
            Canon::Assign { .. } => {
                return Err(Undecidable::new("arithmetic on a labeled value"));
            }
        };
        Ok(Frac {
            num,
            den: one_poly(),
        })
    }
}

/// Build the canonical value of one atom with exponent 1 and coefficient 1.
fn atom_value(atom: Atom) -> Canon {
    let mut monomial = Monomial::new();
    monomial.insert(atom, 1);
    from_sum(term(monomial, BigRational::one()))
}

/// Multiply a plain atom power into a monomial, and drop a zero exponent.
///
/// The atom is a plain one: a variable, a function call, `pi`, or `e`. The two
/// atoms that carry a law of their own ([`Atom::Sqrt`] and [`Atom::Exp`]) go
/// through [`Work::add_atom`] instead.
fn insert_atom(monomial: &mut Monomial, atom: &Atom, exponent: i64) -> Result<(), Undecidable> {
    let previous = monomial.get(atom).copied().unwrap_or(0);
    let total = previous
        .checked_add(exponent)
        .ok_or_else(|| Undecidable::new("an exponent past the size bound"))?;
    if total == 0 {
        monomial.remove(atom);
    } else {
        monomial.insert(atom.clone(), total);
    }
    Ok(())
}

/// Build a one-term sum, or the empty sum when the coefficient is zero.
fn term(monomial: Monomial, coefficient: BigRational) -> Poly {
    let mut sum = Poly::new();
    if !coefficient.is_zero() {
        sum.insert(monomial, coefficient);
    }
    sum
}

/// Build the sum that holds the number 1.
fn one_poly() -> Poly {
    term(Monomial::new(), BigRational::one())
}

/// Whether a sum is the number 1.
fn is_one(sum: &Poly) -> bool {
    match single_term(sum) {
        Some((monomial, coefficient)) => monomial.is_empty() && coefficient.is_one(),
        None => false,
    }
}

/// Build the greatest monomial that divides every term of two sums.
///
/// The monomial is the monomial content of the quotient, and it carries each
/// atom to the LEAST power any term of the two sums gives it. A term that holds
/// no such atom gives it the power 0, so the result carries a positive power
/// only when every term of both sums holds that atom.
///
/// The monomial content is the reason the form is canonical. Rule 3 without it
/// clears the negative exponents but keeps a common factor: `4/s - 5/s**2 +
/// 2/(s-3)` reaches the denominator `s**3-3*s**2` and the same value written
/// `(2*s**3 + 4*s**2*(s-3) - 5*s*(s-3))/(s**3*(s-3))` reaches `s**4-3*s**3`. The
/// content takes the factor `s` out of the second one and the two meet.
///
/// The step takes a MONOMIAL content only. It runs no polynomial division and it
/// takes no polynomial factor, so `(x**2-1)/(x-1)` keeps its denominator.
fn common_monomial(num: &Poly, den: &Poly) -> Monomial {
    let mut common = least_of([num, den]);
    // [`Atom::Sqrt`] and [`Atom::Exp`] never carry a negative exponent:
    // `1/sqrt(2)` is `sqrt(2)/2` and `1/e**x` is `e**(-x)`. A numerator therefore
    // cannot hold one of them back, and the content of the denominator alone is
    // the right content for those two atoms. Without this line
    // `1/(sqrt(2)*(x+1))` and `1/sqrt(2) * 1/(x+1)` are two forms of one value.
    for (atom, exponent) in least_of([den]) {
        if exponent > 0 && matches!(atom, Atom::Sqrt(_) | Atom::Exp(_)) {
            common.insert(atom, exponent);
        }
    }
    common
}

/// Build the monomial of the least power of each atom of a group of sums.
fn least_of<'a, I: IntoIterator<Item = &'a Poly>>(sums: I) -> Monomial {
    let mut least: Option<Monomial> = None;
    for sum in sums {
        for monomial in sum.keys() {
            least = Some(match least {
                None => monomial.clone(),
                Some(present) => least_powers(&present, monomial),
            });
        }
    }
    least.unwrap_or_default()
}

/// Build the monomial of the least power of each atom of two monomials.
///
/// An atom that one monomial does not hold has the power 0 there, so it survives
/// only with a negative power. A power of 0 leaves the result.
fn least_powers(left: &Monomial, right: &Monomial) -> Monomial {
    let mut least = Monomial::new();
    for (atom, exponent) in left {
        let other = right.get(atom).copied().unwrap_or(0);
        let value = (*exponent).min(other);
        if value != 0 {
            least.insert(atom.clone(), value);
        }
    }
    for (atom, exponent) in right {
        if left.contains_key(atom) {
            continue;
        }
        if *exponent < 0 {
            least.insert(atom.clone(), *exponent);
        }
    }
    least
}

/// Read the only term of a sum, when the sum has exactly one.
fn single_term(sum: &Poly) -> Option<(&Monomial, &BigRational)> {
    if sum.len() == 1 {
        sum.iter().next()
    } else {
        None
    }
}

/// Copy the only term of a sum, when the sum has exactly one.
///
/// The copy frees the borrow of the sum, so the caller keeps the work budget.
fn one_term(sum: &Poly) -> Option<(Monomial, BigRational)> {
    single_term(sum).map(|(monomial, coefficient)| (monomial.clone(), coefficient.clone()))
}

/// Refuse a sum that goes past the term bound.
fn bound_terms(sum: &Poly) -> Result<(), Undecidable> {
    if sum.len() > MAX_TERMS {
        return Err(Undecidable::new("the answer goes past the term bound"));
    }
    Ok(())
}

/// Build the reciprocal of a rational, or refuse zero.
fn reciprocal_of(value: &BigRational) -> Result<BigRational, Undecidable> {
    if value.is_zero() {
        return Err(Undecidable::new("a division by zero"));
    }
    Ok(value.recip())
}

/// Split a positive integer into `outside^2 * radicand` with a squarefree radicand.
///
/// # Errors
///
/// Returns [`Undecidable`] when trial division cannot prove the remainder
/// squarefree. A refusal is safe; an unreduced radical would compare wrong.
fn extract_square(value: &BigInt) -> Result<(BigInt, BigInt), Undecidable> {
    let Some(mut remainder) = value.to_u128() else {
        let root = value.sqrt();
        return if &(&root * &root) == value {
            Ok((root, BigInt::one()))
        } else {
            Err(Undecidable::new("a radicand past the factoring bound"))
        };
    };
    let mut outside: u128 = 1;
    let mut radicand: u128 = 1;
    let mut divisor: u128 = 2;
    let mut exhausted = false;
    while divisor <= TRIAL_DIVISION_LIMIT {
        if divisor > remainder / divisor {
            exhausted = true;
            break;
        }
        let mut multiplicity: u32 = 0;
        while remainder % divisor == 0 {
            remainder /= divisor;
            multiplicity += 1;
        }
        if multiplicity > 0 {
            outside = checked(
                divisor
                    .checked_pow(multiplicity / 2)
                    .and_then(|square| outside.checked_mul(square)),
            )?;
            if multiplicity % 2 == 1 {
                radicand = checked(radicand.checked_mul(divisor))?;
            }
        }
        divisor += 1;
    }
    if remainder > 1 {
        if exhausted || remainder < SQUAREFREE_CERTAIN {
            radicand = checked(radicand.checked_mul(remainder))?;
        } else {
            let root = remainder.isqrt();
            if root.saturating_mul(root) == remainder {
                outside = checked(outside.checked_mul(root))?;
            } else {
                return Err(Undecidable::new("a radicand past the factoring bound"));
            }
        }
    }
    Ok((BigInt::from(outside), BigInt::from(radicand)))
}

/// Turn an overflowed unsigned product into a refusal.
fn checked(value: Option<u128>) -> Result<u128, Undecidable> {
    value.ok_or_else(|| Undecidable::new("a radicand past the factoring bound"))
}

/// Demote a quotient to the narrowest canonical variant that holds it.
///
/// The demotion is total and deterministic, which is what makes equality of two
/// canonical forms an equality of two values.
fn from_frac(quotient: Frac) -> Canon {
    if quotient.num.is_empty() {
        return Canon::Rational(BigRational::zero());
    }
    if is_one(&quotient.den) {
        return from_sum(quotient.num);
    }
    Canon::Value {
        num: quotient.num,
        den: quotient.den,
    }
}

/// Demote a sum of terms to the narrowest canonical variant that holds it.
fn from_sum(sum: Poly) -> Canon {
    if sum.is_empty() {
        return Canon::Rational(BigRational::zero());
    }
    if let Some((monomial, coefficient)) = single_term(&sum) {
        if monomial.is_empty() {
            return Canon::Rational(coefficient.clone());
        }
        if coefficient.is_one()
            && monomial.len() == 1
            && let Some((Atom::Call(name, arguments), 1)) = monomial.iter().next()
        {
            return Canon::Func(name.clone(), arguments.clone());
        }
    }
    match as_radical(&sum) {
        Some(parts) => Canon::Radical(parts),
        None => Canon::Poly(sum),
    }
}

/// Read a sum as a rational combination of roots and constants, if it is one.
fn as_radical(sum: &Poly) -> Option<BTreeMap<Basis, BigRational>> {
    let mut parts = BTreeMap::new();
    for (monomial, coefficient) in sum {
        let mut basis = Basis {
            radicand: BigInt::one(),
            pi: 0,
            e: 0,
        };
        for (atom, exponent) in monomial {
            match atom {
                // A monomial holds one root at most. A second one is not a basis.
                Atom::Sqrt(radicand) if *exponent == 1 && basis.radicand.is_one() => {
                    basis.radicand = radicand.clone();
                }
                Atom::Pi => basis.pi = *exponent,
                Atom::E => basis.e = *exponent,
                _ => return None,
            }
        }
        if parts.insert(basis, coefficient.clone()).is_some() {
            return None;
        }
    }
    Some(parts)
}
