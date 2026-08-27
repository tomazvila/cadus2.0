//! The exact evaluator of `answer_expr` (A1, V2, D6, T1).
//!
//! The server computes the answer. No model computes it, at authoring time or at
//! serve time (T1), and no computer-algebra system computes it: `answer_expr`
//! parses once into the M2 [`Ast`], and this module evaluates that tree against a
//! bound tuple of exact rationals.
//!
//! # One grammar (`docs/plans/M4.md`, fixed decision)
//!
//! 1.0 substitutes the bound values into the source text and hands the string to
//! SymPy (`sympy_check.py:277-314`). Three defects follow from the textual step
//! alone: `a**2` with `a = -3` reads as `-3**2` unless every value is wrapped in
//! brackets; a choice value of `\times` breaks the replacement side of the
//! regular expression; and the result comes back as a SymPy string, so `a*1.5`
//! with `a = 2` answers `3.00000000000000` (spec section 3.4).
//!
//! 2.0 substitutes into a tree. A bound value becomes a literal node, so a sign
//! never re-associates, a backslash is never markup, and the value is an exact
//! rational at every step (D6).
//!
//! The answer expression reads inside the M2 grammar plus the evaluation-only
//! function set [`EVAL_FUNCTIONS`]. Those functions are ERASED before the answer
//! string exists: each one takes exact rationals and returns an exact value, and
//! the writer never puts the name back. The answer string therefore always lies
//! in the grammar the M2 checker reads, which is what makes the M5 grade path
//! deterministic (L2, A3, V2).
//!
//! # Every bound is deterministic
//!
//! [`MAX_VALUE_BITS`] bounds the width of every intermediate, [`MAX_FACTORIAL`]
//! bounds the factorial, and the parser already bounds the exponent at 1,000.
//! A value past a bound is an [`EvalError`], never a wrong answer (C4) and never
//! a panic.

use num_bigint::BigInt;

use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};

use crate::answer::ast::Ast;
use crate::answer::{Canon, Undecidable, canonical_form, parse_with_functions};

use super::domain::{Bindings, gcd_of, is_whole};

/// The evaluation-only functions and the argument count each one takes.
///
/// `abs` and `sqrt` are in the M2 grammar already. The other eight are not, and
/// [`parse_with_functions`] admits them for this one purpose. Every one of them
/// is erased before the answer string exists.
pub const EVAL_FUNCTIONS: [(&str, usize); 10] = [
    ("abs", 1),
    ("sqrt", 1),
    ("gcd", 2),
    ("lcm", 2),
    ("floor", 1),
    ("ceiling", 1),
    ("min", 2),
    ("max", 2),
    ("factorial", 1),
    ("binomial", 2),
];

/// The function names [`parse_with_functions`] admits beyond the M2 grammar.
pub const EXTRA_FUNCTIONS: [&str; 8] = [
    "gcd",
    "lcm",
    "floor",
    "ceiling",
    "min",
    "max",
    "factorial",
    "binomial",
];

/// The largest bit width of a numerator or a denominator of an intermediate.
///
/// 4,096 bits is about 1,233 decimal digits. It is the bound the M2
/// canonicalizer already holds, so a value this evaluator builds always fits the
/// checker that reads it back.
pub const MAX_VALUE_BITS: u64 = 4_096;

/// The largest count of multiplications [`binomial`] runs.
///
/// The width bound stops a large coefficient on its own, and this bound stops a
/// long walk that never builds a wide number, such as `binomial(10**9, 10**9-2)`.
pub const MAX_BINOMIAL_STEPS: u32 = 4_096;

/// The largest argument [`factorial`] takes.
///
/// 200! has 375 decimal digits, which is far inside [`MAX_VALUE_BITS`]. A school
/// problem never needs a larger one, and the bound keeps the serve path (L1) off
/// a number the machine spends a second on.
pub const MAX_FACTORIAL: u32 = 200;

/// An answer expression the evaluator refuses.
///
/// Every variant is a defect of the template document or of a drawn tuple. The
/// gate of U2 turns it into a rejection message, and the instantiator drops the
/// instance instead of serving it (C4, V2).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EvalError {
    /// `answer_expr` left the grammar.
    #[error("answer_expr is outside the decidable grammar: {0}")]
    Grammar(#[from] Undecidable),
    /// A named parameter is bound to a text choice, which has no value.
    #[error("answer_expr reads {name:?}, which is bound to the text {text:?} and not to a number")]
    NotNumeric {
        /// The parameter name.
        name: String,
        /// The text the parameter is bound to.
        text: String,
    },
    /// An evaluation-only function got an argument that is not an exact number.
    #[error("{func} needs exact numbers, and an argument is not one")]
    NotNumber {
        /// The function name.
        func: &'static str,
    },
    /// An evaluation-only function got an argument that is not a whole number.
    #[error("{func} needs whole numbers, and an argument is not one")]
    NotWhole {
        /// The function name.
        func: &'static str,
    },
    /// A function call has the wrong count of arguments.
    #[error("{func} takes {want} argument(s), and the call has {given}")]
    Arity {
        /// The function name.
        func: String,
        /// The count the function takes.
        want: usize,
        /// The count the call writes.
        given: usize,
    },
    /// A division by zero.
    #[error("answer_expr divides by zero")]
    DivideByZero,
    /// A square root of a negative number. 1.0 answers `2*I` here (spec 3.4).
    #[error("answer_expr takes the square root of the negative number {value}")]
    NegativeRoot {
        /// The radicand.
        value: String,
    },
    /// A function got a whole number outside the range it reads.
    #[error("{func} takes a non-negative whole number, and it got {value}")]
    Domain {
        /// The function name.
        func: &'static str,
        /// The argument.
        value: String,
    },
    /// A factorial of a negative number or of a number past [`MAX_FACTORIAL`].
    #[error("factorial takes a whole number from 0 to {MAX_FACTORIAL}, and it got {value}")]
    FactorialRange {
        /// The argument.
        value: String,
    },
    /// A value grew past [`MAX_VALUE_BITS`].
    #[error("answer_expr builds a number wider than {MAX_VALUE_BITS} bits")]
    TooWide,
    /// The evaluated answer does not canonicalize (V2, the M5 grade path).
    #[error("the answer {text:?} does not canonicalize: {reason}")]
    NotCanonical {
        /// The answer string the writer produced.
        text: String,
        /// Why the checker refused it.
        reason: Undecidable,
    },
}

/// One computed answer: the string the learner is graded against, and its form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    /// The answer string, inside the M2 grammar.
    pub text: String,
    /// The canonical form of that string.
    pub canon: Canon,
}

/// Parse one `answer_expr` into the M2 AST, with the evaluation-only functions.
///
/// The parse runs ONCE, at authoring time. The serve path evaluates the tree it
/// produced; it never reads the source again (spec section 3.4).
///
/// # Errors
///
/// Returns [`Undecidable`] when the source leaves the grammar (V2).
pub fn parse_answer_expr(source: &str) -> Result<Ast, Undecidable> {
    parse_with_functions(source, &EXTRA_FUNCTIONS)
}

/// Evaluate the answer tree against a bound tuple.
///
/// The result holds no evaluation-only function: every one of them is computed
/// exactly and erased. A parameter the tuple does not bind stays a free variable,
/// which is how an `expression` answer keeps its unknown.
///
/// # Errors
///
/// Returns [`EvalError`] for a text binding inside the expression, a function
/// argument outside its domain, a division by zero, and a value past a bound.
pub fn evaluate(ast: &Ast, bindings: &Bindings) -> Result<Ast, EvalError> {
    match ast {
        Ast::Integer(value) => literal(BigRational::from(value.clone())),
        Ast::Decimal { mantissa, scale } => literal(BigRational::new(
            mantissa.clone(),
            BigInt::from(10u8).pow(*scale),
        )),
        Ast::Fraction {
            numerator,
            denominator,
        } => {
            if denominator.is_zero() {
                return Err(EvalError::DivideByZero);
            }
            literal(BigRational::new(numerator.clone(), denominator.clone()))
        }
        Ast::Mixed {
            whole,
            numerator,
            denominator,
        } => {
            if denominator.is_zero() {
                return Err(EvalError::DivideByZero);
            }
            literal(
                BigRational::from(whole.clone())
                    + BigRational::new(numerator.clone(), denominator.clone()),
            )
        }
        Ast::Var(name) => match bindings.get(name) {
            None => Ok(Ast::Var(name.clone())),
            Some(value) => match value.as_rational() {
                Some(number) => literal(number.clone()),
                None => Err(EvalError::NotNumeric {
                    name: name.clone(),
                    text: value.canonical_string(),
                }),
            },
        },
        Ast::Const(constant) => Ok(Ast::Const(*constant)),
        Ast::Neg(inner) => {
            let value = evaluate(inner, bindings)?;
            match as_rational(&value) {
                Some(number) => literal(-number),
                None => Ok(Ast::Neg(Box::new(value))),
            }
        }
        Ast::Add(items) => fold(items, bindings, Fold::Add),
        Ast::Mul(items) => fold(items, bindings, Fold::Mul),
        Ast::Div(left, right) => {
            let dividend = evaluate(left, bindings)?;
            let divisor = evaluate(right, bindings)?;
            match (as_rational(&dividend), as_rational(&divisor)) {
                (Some(_), Some(second)) if second.is_zero() => Err(EvalError::DivideByZero),
                (Some(first), Some(second)) => literal(first / second),
                _ => Ok(Ast::Div(Box::new(dividend), Box::new(divisor))),
            }
        }
        Ast::Pow(base, exponent) => {
            let value = evaluate(base, bindings)?;
            match as_rational(&value) {
                Some(number) => literal(power(&number, *exponent)?),
                None => Ok(Ast::Pow(Box::new(value), *exponent)),
            }
        }
        Ast::Sqrt(inner) => {
            let value = evaluate(inner, bindings)?;
            match as_rational(&value) {
                Some(number) => square_root(&number),
                None => Ok(Ast::Sqrt(Box::new(value))),
            }
        }
        Ast::Func(name, args) => call(name, args, bindings),
        Ast::Tuple(items) => Ok(Ast::Tuple(evaluate_all(items, bindings)?)),
        Ast::Set(items) => Ok(Ast::Set(evaluate_all(items, bindings)?)),
        Ast::List(items) => Ok(Ast::List(evaluate_all(items, bindings)?)),
        Ast::Interval {
            lo,
            hi,
            lo_closed,
            hi_closed,
        } => Ok(Ast::Interval {
            lo: Box::new(evaluate(lo, bindings)?),
            hi: Box::new(evaluate(hi, bindings)?),
            lo_closed: *lo_closed,
            hi_closed: *hi_closed,
        }),
        Ast::Ineq { var, op, bound } => Ok(Ast::Ineq {
            var: var.clone(),
            op: *op,
            bound: Box::new(evaluate(bound, bindings)?),
        }),
        Ast::Assign { var, value } => Ok(Ast::Assign {
            var: var.clone(),
            value: Box::new(evaluate(value, bindings)?),
        }),
        Ast::Chain {
            lo,
            lo_closed,
            var,
            hi_closed,
            hi,
        } => Ok(Ast::Chain {
            lo: Box::new(evaluate(lo, bindings)?),
            lo_closed: *lo_closed,
            var: var.clone(),
            hi_closed: *hi_closed,
            hi: Box::new(evaluate(hi, bindings)?),
        }),
    }
}

/// Compute the answer of one bound tuple, as a string and as a canonical form.
///
/// The string is the expected answer a pool row carries, and the canonical form
/// is the proof that the M5 grade path decides it (V2, A3).
///
/// # Errors
///
/// Returns [`EvalError`] for every case [`evaluate`] refuses, and
/// [`EvalError::NotCanonical`] when the written answer leaves the M2 grammar.
pub fn answer(ast: &Ast, bindings: &Bindings) -> Result<Answer, EvalError> {
    let value = evaluate(ast, bindings)?;
    let text = write(&value)?;
    match canonical_form(&text) {
        Ok(canon) => Ok(Answer { text, canon }),
        Err(reason) => Err(EvalError::NotCanonical { text, reason }),
    }
}

/// Write an evaluated tree as an answer string inside the M2 grammar.
///
/// The writer brackets by precedence, so `2*(x+1)` keeps its brackets and `2*x`
/// takes none. A whole rational writes its digits, so the 1.0 float trap
/// `3.00000000000000` has no spelling here (spec section 8, trap 3).
///
/// # Errors
///
/// Returns [`EvalError`] when the tree still holds an evaluation-only function,
/// which happens only when an argument of one was not an exact number.
pub fn write(value: &Ast) -> Result<String, EvalError> {
    let mut out = String::new();
    write_at(value, Prec::Lowest, &mut out)?;
    Ok(out)
}

/// The bracketing levels of the writer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Prec {
    /// A whole answer, a bracketed group, or a collection member.
    Lowest,
    /// An operand of a sum.
    Sum,
    /// An operand of a product or a quotient.
    Product,
    /// A place where a power stands with no brackets: an operand of a product,
    /// the divisor of a quotient, and the operand of a minus sign.
    Power,
    /// The base of a power, which reads no operator of its own.
    ///
    /// The level exists because `**` groups to the right: the base of a power
    /// must be atomic, or `Pow(Pow(x, 2), 3)` writes `x**2**3`, which the M2
    /// parser refuses as a tower of powers (M4 review 1, finding 17).
    Atom,
}

/// Which variadic fold one node takes.
#[derive(Debug, Clone, Copy)]
enum Fold {
    /// A sum.
    Add,
    /// A product.
    Mul,
}

/// Evaluate a variadic node, folding the exact operands into one rational.
///
/// Every exact operand folds into one number, and every symbolic operand stays.
/// A node with no symbolic operand left is one literal, which is the whole point:
/// a numeric answer reduces to a number and never to a sum of numbers.
fn fold(items: &[Ast], bindings: &Bindings, kind: Fold) -> Result<Ast, EvalError> {
    let identity = match kind {
        Fold::Add => BigRational::from(BigInt::from(0u8)),
        Fold::Mul => BigRational::from(BigInt::from(1u8)),
    };
    let mut number = identity.clone();
    let mut rest: Vec<Ast> = Vec::new();
    for item in items {
        let value = evaluate(item, bindings)?;
        match as_rational(&value) {
            Some(operand) => {
                match kind {
                    Fold::Add => number += operand,
                    Fold::Mul => number *= operand,
                }
                width_ok(&number)?;
            }
            None => rest.push(value),
        }
    }
    if matches!(kind, Fold::Mul) && number.is_zero() {
        return literal(number);
    }
    let redundant = match kind {
        Fold::Add => number.is_zero(),
        Fold::Mul => number.is_one(),
    };
    if !redundant {
        rest.insert(0, rational_node(&number));
    }
    match rest.pop() {
        None => literal(identity),
        Some(last) if rest.is_empty() => Ok(last),
        Some(last) => {
            rest.push(last);
            Ok(match kind {
                Fold::Add => Ast::Add(rest),
                Fold::Mul => Ast::Mul(rest),
            })
        }
    }
}

/// Evaluate every member of a collection.
fn evaluate_all(items: &[Ast], bindings: &Bindings) -> Result<Vec<Ast>, EvalError> {
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        out.push(evaluate(item, bindings)?);
    }
    Ok(out)
}

/// Evaluate one function call, erasing it when its arguments are exact.
fn call(name: &str, args: &[Ast], bindings: &Bindings) -> Result<Ast, EvalError> {
    let values = evaluate_all(args, bindings)?;
    let Some((func, want)) = EVAL_FUNCTIONS
        .iter()
        .find(|(candidate, _)| *candidate == name)
    else {
        // A function of the M2 grammar that this module does not evaluate, such
        // as `sin` or `ln`. It stays in the tree, and the M2 canonicalizer reads
        // it. Its arguments are evaluated already.
        return Ok(Ast::Func(name.to_string(), values));
    };
    if values.len() != *want {
        return Err(EvalError::Arity {
            func: (*func).to_string(),
            want: *want,
            given: values.len(),
        });
    }
    let numbers: Option<Vec<BigRational>> = values.iter().map(as_rational).collect();
    let Some(numbers) = numbers else {
        // `abs` is in the M2 grammar, so a symbolic argument keeps the call.
        // Every other name of the set must reduce, or the answer string would
        // carry a name the checker cannot read (V2).
        if *func == "abs" {
            return Ok(Ast::Func("abs".to_string(), values));
        }
        return Err(EvalError::NotNumber { func });
    };
    exact_call(func, &numbers)
}

/// Apply one evaluation-only function to exact rationals.
fn exact_call(func: &'static str, args: &[BigRational]) -> Result<Ast, EvalError> {
    let first = args.first().ok_or(EvalError::Arity {
        func: func.to_string(),
        want: 1,
        given: 0,
    })?;
    match func {
        "abs" => literal(first.abs()),
        "sqrt" => square_root(first),
        "floor" => literal(BigRational::from(first.floor().to_integer())),
        "ceiling" => literal(BigRational::from(first.ceil().to_integer())),
        "factorial" => literal(BigRational::from(factorial(first)?)),
        _ => {
            let second = args.get(1).ok_or(EvalError::Arity {
                func: func.to_string(),
                want: 2,
                given: args.len(),
            })?;
            match func {
                "min" => literal(first.min(second).clone()),
                "max" => literal(first.max(second).clone()),
                "gcd" => {
                    let left = whole(func, first)?;
                    let right = whole(func, second)?;
                    literal(BigRational::from(gcd_of(&left, &right)))
                }
                "lcm" => {
                    let left = whole(func, first)?;
                    let right = whole(func, second)?;
                    let gcd = gcd_of(&left, &right);
                    if gcd.is_zero() {
                        return literal(BigRational::from(BigInt::from(0u8)));
                    }
                    let value = (&left * &right) / gcd;
                    literal(BigRational::from(if value.is_negative() {
                        -value
                    } else {
                        value
                    }))
                }
                "binomial" => {
                    let top = whole(func, first)?;
                    let bottom = whole(func, second)?;
                    literal(BigRational::from(binomial(&top, &bottom)?))
                }
                _ => Err(EvalError::NotNumber { func }),
            }
        }
    }
}

/// The factorial of a whole number from 0 to [`MAX_FACTORIAL`].
fn factorial(value: &BigRational) -> Result<BigInt, EvalError> {
    let refuse = || EvalError::FactorialRange {
        value: value.to_string(),
    };
    if !is_whole(value) || value.numer().is_negative() {
        return Err(refuse());
    }
    let count = value.numer().to_u32().ok_or_else(refuse)?;
    if count > MAX_FACTORIAL {
        return Err(refuse());
    }
    let mut product = BigInt::from(1u8);
    for step in 2..=count {
        product *= BigInt::from(step);
    }
    Ok(product)
}

/// The binomial coefficient of two whole numbers.
///
/// The count is zero when the lower number is negative or above the upper one,
/// which is the SymPy reading 1.0 uses. The product runs multiplicatively, so it
/// needs no factorial and it stays inside [`MAX_VALUE_BITS`].
fn binomial(top: &BigInt, bottom: &BigInt) -> Result<BigInt, EvalError> {
    if top.is_negative() {
        return Err(EvalError::Domain {
            func: "binomial",
            value: top.to_string(),
        });
    }
    if bottom.is_negative() || bottom > top {
        return Ok(BigInt::from(0u8));
    }
    let complement = top - bottom;
    let steps = if bottom < &complement {
        bottom
    } else {
        &complement
    };
    let steps = steps.to_u32().ok_or(EvalError::TooWide)?;
    if steps > MAX_BINOMIAL_STEPS {
        return Err(EvalError::TooWide);
    }
    let mut result = BigInt::from(1u8);
    for step in 0..steps {
        result *= top - BigInt::from(step);
        result /= BigInt::from(step + 1);
        if result.bits() > MAX_VALUE_BITS {
            return Err(EvalError::TooWide);
        }
    }
    Ok(result)
}

/// The exact square root of a non-negative rational, or the root node.
///
/// `sqrt(4)` is 2 and `sqrt(4/9)` is `2/3`. A radicand with no exact root stays
/// [`Ast::Sqrt`] over a literal, which is inside the M2 grammar: the
/// canonicalizer reduces `sqrt(8)` to `2*sqrt(2)` on its own.
fn square_root(value: &BigRational) -> Result<Ast, EvalError> {
    if value.numer().is_negative() {
        return Err(EvalError::NegativeRoot {
            value: value.to_string(),
        });
    }
    let numerator = value.numer().sqrt();
    let denominator = value.denom().sqrt();
    if &(&numerator * &numerator) == value.numer()
        && &(&denominator * &denominator) == value.denom()
    {
        return literal(BigRational::new(numerator, denominator));
    }
    Ok(Ast::Sqrt(Box::new(rational_node(value))))
}

/// Raise an exact rational to a whole power, inside the width bound.
fn power(base: &BigRational, exponent: i64) -> Result<BigRational, EvalError> {
    if exponent == 0 {
        return Ok(BigRational::from(BigInt::from(1u8)));
    }
    if base.is_zero() && exponent < 0 {
        return Err(EvalError::DivideByZero);
    }
    let magnitude = exponent.unsigned_abs();
    let steps = u32::try_from(magnitude).map_err(|_| EvalError::TooWide)?;
    let mut result = BigRational::from(BigInt::from(1u8));
    for _ in 0..steps {
        result *= base;
        width_ok(&result)?;
    }
    if exponent < 0 {
        if result.is_zero() {
            return Err(EvalError::DivideByZero);
        }
        return Ok(result.recip());
    }
    Ok(result)
}

/// Read a whole number out of an exact rational, or refuse it.
fn whole(func: &'static str, value: &BigRational) -> Result<BigInt, EvalError> {
    if !is_whole(value) {
        return Err(EvalError::NotWhole { func });
    }
    Ok(value.numer().clone())
}

/// Build a literal node from an exact rational, inside the width bound.
fn literal(value: BigRational) -> Result<Ast, EvalError> {
    width_ok(&value)?;
    Ok(rational_node(&value))
}

/// Build the node that holds an exact rational.
fn rational_node(value: &BigRational) -> Ast {
    if is_whole(value) {
        return Ast::Integer(value.numer().clone());
    }
    Ast::Fraction {
        numerator: value.numer().clone(),
        denominator: value.denom().clone(),
    }
}

/// Read the exact rational a node holds, when it holds one.
fn as_rational(node: &Ast) -> Option<BigRational> {
    match node {
        Ast::Integer(value) => Some(BigRational::from(value.clone())),
        Ast::Fraction {
            numerator,
            denominator,
        } if !denominator.is_zero() => {
            Some(BigRational::new(numerator.clone(), denominator.clone()))
        }
        _ => None,
    }
}

/// Refuse a value wider than [`MAX_VALUE_BITS`] on either side.
fn width_ok(value: &BigRational) -> Result<(), EvalError> {
    if value.numer().bits().max(value.denom().bits()) > MAX_VALUE_BITS {
        return Err(EvalError::TooWide);
    }
    Ok(())
}

/// The bracketing level one node writes at when it stands alone.
///
/// A node whose level is BELOW the level its place needs takes brackets. A
/// leading minus sign is the case that matters: `Ast::Integer(-3)` writes `-3`,
/// and `-3**2` reads as `-(3**2)`, so a negative literal takes the level of a
/// sum and the power brackets it into `(-3)**2`.
///
/// A power writes an operator of its own, so it is not atomic and the base of a
/// power brackets it. Every self-delimiting node — a non-negative literal, a
/// name, a function call, a root, a collection — is atomic.
fn level(node: &Ast) -> Prec {
    match node {
        Ast::Ineq { .. } | Ast::Assign { .. } | Ast::Chain { .. } => Prec::Lowest,
        Ast::Add(_) | Ast::Neg(_) | Ast::Mixed { .. } => Prec::Sum,
        Ast::Integer(value) if value.is_negative() => Prec::Sum,
        Ast::Decimal { mantissa, .. } if mantissa.is_negative() => Prec::Sum,
        Ast::Fraction { numerator, .. } if numerator.is_negative() => Prec::Sum,
        Ast::Fraction { .. } | Ast::Mul(_) | Ast::Div(_, _) => Prec::Product,
        Ast::Pow(_, _) => Prec::Power,
        _ => Prec::Atom,
    }
}

/// Write one node at a bracketing level.
fn write_at(node: &Ast, need: Prec, out: &mut String) -> Result<(), EvalError> {
    let bracket = level(node) < need;
    if bracket {
        out.push('(');
    }
    write_bare(node, out)?;
    if bracket {
        out.push(')');
    }
    Ok(())
}

/// Write one node without its outer brackets.
fn write_bare(node: &Ast, out: &mut String) -> Result<(), EvalError> {
    match node {
        Ast::Integer(value) => out.push_str(&value.to_string()),
        Ast::Decimal { mantissa, scale } => {
            let value = BigRational::new(mantissa.clone(), BigInt::from(10u8).pow(*scale));
            write_bare(&rational_node(&value), out)?;
        }
        Ast::Fraction {
            numerator,
            denominator,
        } => {
            out.push_str(&numerator.to_string());
            out.push('/');
            out.push_str(&denominator.to_string());
        }
        Ast::Mixed {
            whole,
            numerator,
            denominator,
        } => {
            out.push_str(&whole.to_string());
            out.push(' ');
            out.push_str(&numerator.to_string());
            out.push('/');
            out.push_str(&denominator.to_string());
        }
        Ast::Var(name) => out.push_str(name),
        Ast::Const(constant) => out.push_str(constant.name()),
        Ast::Neg(inner) => {
            out.push('-');
            write_at(inner, Prec::Power, out)?;
        }
        Ast::Sqrt(inner) => {
            out.push_str("sqrt(");
            write_at(inner, Prec::Lowest, out)?;
            out.push(')');
        }
        Ast::Pow(base, exponent) => {
            write_at(base, Prec::Atom, out)?;
            out.push_str("**");
            if *exponent < 0 {
                out.push('(');
                out.push_str(&exponent.to_string());
                out.push(')');
            } else {
                out.push_str(&exponent.to_string());
            }
        }
        Ast::Add(items) => write_joined(items, " + ", Prec::Product, out)?,
        Ast::Mul(items) => write_joined(items, "*", Prec::Power, out)?,
        Ast::Div(left, right) => {
            write_at(left, Prec::Product, out)?;
            out.push('/');
            write_at(right, Prec::Power, out)?;
        }
        Ast::Func(name, args) => {
            if !is_writable_function(name) {
                return Err(EvalError::NotNumber {
                    func: "an evaluation-only function",
                });
            }
            out.push_str(name);
            out.push('(');
            write_joined(args, ", ", Prec::Lowest, out)?;
            out.push(')');
        }
        Ast::Tuple(items) => {
            out.push('(');
            write_joined(items, ", ", Prec::Lowest, out)?;
            out.push(')');
        }
        Ast::Set(items) => {
            out.push('{');
            write_joined(items, ", ", Prec::Lowest, out)?;
            out.push('}');
        }
        Ast::List(items) => {
            out.push('[');
            write_joined(items, ", ", Prec::Lowest, out)?;
            out.push(']');
        }
        Ast::Interval {
            lo,
            hi,
            lo_closed,
            hi_closed,
        } => {
            out.push(if *lo_closed { '[' } else { '(' });
            write_at(lo, Prec::Lowest, out)?;
            out.push_str(", ");
            write_at(hi, Prec::Lowest, out)?;
            out.push(if *hi_closed { ']' } else { ')' });
        }
        Ast::Ineq { var, op, bound } => {
            out.push_str(var);
            out.push(' ');
            out.push_str(op.symbol());
            out.push(' ');
            write_at(bound, Prec::Lowest, out)?;
        }
        Ast::Assign { var, value } => {
            out.push_str(var);
            out.push_str(" = ");
            write_at(value, Prec::Lowest, out)?;
        }
        Ast::Chain {
            lo,
            lo_closed,
            var,
            hi_closed,
            hi,
        } => {
            write_at(lo, Prec::Lowest, out)?;
            out.push_str(if *lo_closed { " <= " } else { " < " });
            out.push_str(var);
            out.push_str(if *hi_closed { " <= " } else { " < " });
            write_at(hi, Prec::Lowest, out)?;
        }
    }
    Ok(())
}

/// Write a list of nodes, separated by one string.
fn write_joined(
    items: &[Ast],
    separator: &str,
    need: Prec,
    out: &mut String,
) -> Result<(), EvalError> {
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            out.push_str(separator);
        }
        write_at(item, need, out)?;
    }
    Ok(())
}

/// Whether the writer may put a function name back into the answer string.
///
/// An evaluation-only name is never writable: it must be erased, or the answer
/// string would leave the grammar the M2 checker reads (V2). `abs` and `sqrt`
/// are in both sets, and the M2 grammar holds them, so they are writable.
fn is_writable_function(name: &str) -> bool {
    if name == "abs" || name == "sqrt" {
        return true;
    }
    !EXTRA_FUNCTIONS.contains(&name)
}
