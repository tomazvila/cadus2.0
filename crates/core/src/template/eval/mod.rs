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

mod builtin;
mod exact;
mod write;

use builtin::call;
use exact::{
    Fold, assignment, bound_value, chain, decimal_value, divide, evaluate_all, fold,
    fraction_literal, inequality, interval, literal, mixed_literal, negate, raise, root,
};
use num_rational::BigRational;

use crate::answer::ast::Ast;
use crate::answer::{Canon, Undecidable, canonical_form, parse_with_functions};

use super::domain::Bindings;

pub use write::write;

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
        Ast::Decimal { mantissa, scale } => literal(decimal_value(mantissa, *scale)),
        Ast::Fraction {
            numerator,
            denominator,
        } => fraction_literal(numerator, denominator),
        Ast::Mixed {
            whole,
            numerator,
            denominator,
        } => mixed_literal(whole, numerator, denominator),
        Ast::Var(name) => bound_value(name, bindings),
        Ast::Const(constant) => Ok(Ast::Const(*constant)),
        Ast::Neg(inner) => negate(inner, bindings),
        Ast::Add(items) => fold(items, bindings, Fold::Add),
        Ast::Mul(items) => fold(items, bindings, Fold::Mul),
        Ast::Div(left, right) => divide(left, right, bindings),
        Ast::Pow(base, exponent) => raise(base, *exponent, bindings),
        Ast::Sqrt(inner) => root(inner, bindings),
        Ast::Func(name, args) => call(name, args, bindings),
        Ast::Tuple(items) => evaluate_all(items, bindings).map(Ast::Tuple),
        Ast::Set(items) => evaluate_all(items, bindings).map(Ast::Set),
        Ast::List(items) => evaluate_all(items, bindings).map(Ast::List),
        Ast::Interval {
            lo,
            hi,
            lo_closed,
            hi_closed,
        } => interval(lo, hi, *lo_closed, *hi_closed, bindings),
        Ast::Ineq { var, op, bound } => inequality(var, *op, bound, bindings),
        Ast::Assign { var, value } => assignment(var, value, bindings),
        Ast::Chain {
            lo,
            lo_closed,
            var,
            hi_closed,
            hi,
        } => chain(lo, *lo_closed, var, *hi_closed, hi, bindings),
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
