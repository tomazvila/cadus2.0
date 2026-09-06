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
mod structured;
mod write;

use builtin::call;
use exact::{
    Fold, assignment, bound_value, chain, decimal_value, divide, evaluate_all, fold,
    fraction_literal, inequality, interval, literal, mixed_literal, negate, raise, root,
};
use num_rational::BigRational;

use crate::answer::ast::Ast;
use crate::answer::{
    AnswerContract, AnswerPart, Canon, Undecidable, canonical_form, parse_with_functions,
};

use super::domain::Bindings;
use structured::{label_answer, list_answer};

pub use write::write;

/// The evaluation-only functions and the argument count each one takes.
///
/// `abs` and `sqrt` are in the M2 grammar already. The others are not, and
/// [`parse_with_functions`] admits them for this one purpose and erases them
/// before the answer string exists.
pub const EVAL_FUNCTIONS: [(&str, usize); 24] = [
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
    ("signcase", 2),
    ("powerform", 2),
    ("excludepoint", 2),
    ("lowerbound", 2),
    ("upperbound", 2),
    ("quotientremainder", 2),
    ("divisibilitylabel", 2),
    ("equalitylabel", 2),
    ("linearclass", 2),
    ("primeclass", 1),
    ("factorlist", 1),
    ("firstmultiples", 2),
    ("primefactors", 1),
    ("repeatedfactors", 2),
];

/// The function names [`parse_with_functions`] admits beyond the M2 grammar.
pub const EXTRA_FUNCTIONS: [&str; 23] = [
    "gcd",
    "lcm",
    "floor",
    "ceiling",
    "min",
    "max",
    "factorial",
    "binomial",
    "multipart",
    "signcase",
    "powerform",
    "excludepoint",
    "lowerbound",
    "upperbound",
    "quotientremainder",
    "divisibilitylabel",
    "equalitylabel",
    "linearclass",
    "primeclass",
    "factorlist",
    "firstmultiples",
    "primefactors",
    "repeatedfactors",
];

/// The largest bit width of a numerator or a denominator of an intermediate.
///
/// 4,096 bits is about 1,233 decimal digits. It is the bound the M2
/// canonicalizer already holds, so a value this evaluator builds always fits the
/// checker that reads it back.
pub const MAX_VALUE_BITS: u64 = 4_096;

/// The largest count of multiplications [`binomial`] runs.
///
/// The walk runs `min(k, n - k)` steps for `binomial(n, k)`, and the smallest
/// coefficient of `s` steps is `binomial(2*s, s)`, which has about `2*s - 6`
/// bits. So a walk of 2,049 to 2,051 steps builds a value inside
/// [`MAX_VALUE_BITS`], and this bound refuses it before the width bound reads
/// it. A longer walk always ends at the width bound, and this bound stops it at
/// its first step.
pub const MAX_BINOMIAL_STEPS: u32 = 2_048;

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
    /// `signcase` did not receive its bounded three-branch representation.
    #[error("signcase needs [negative, zero, positive] as its second argument")]
    SignCaseShape,
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
        Ast::Neg(inner) | Ast::Sqrt(inner) => unary(ast, inner, bindings),
        Ast::Add(items) => fold(items, bindings, Fold::Add),
        Ast::Mul(items) => fold(items, bindings, Fold::Mul),
        Ast::Div(left, right) => divide(left, right, bindings),
        Ast::Pow(base, exponent) => raise(base, *exponent, bindings),
        Ast::RationalPow { base: inner, .. } | Ast::Quantity { value: inner, .. } => {
            Ok(rebuilt(ast, evaluate(inner, bindings)?))
        }
        Ast::Func(name, args) => call(name, args, bindings),
        Ast::Tuple(items) | Ast::Set(items) | Ast::List(items) => collection(ast, items, bindings),
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

/// Rebuild a rational power or a quantity around its evaluated child.
fn rebuilt(ast: &Ast, child: Ast) -> Ast {
    match ast {
        Ast::RationalPow {
            numerator,
            denominator,
            ..
        } => Ast::RationalPow {
            base: Box::new(child),
            numerator: *numerator,
            denominator: *denominator,
        },
        Ast::Quantity { unit, .. } => Ast::Quantity {
            value: Box::new(child),
            unit,
        },
        // The caller passes one of the two nodes above; every other node keeps
        // its evaluated child as the value.
        _ => child,
    }
}

/// Evaluate a negation or a root, by the kind of the node.
fn unary(ast: &Ast, inner: &Ast, bindings: &Bindings) -> Result<Ast, EvalError> {
    if matches!(ast, Ast::Neg(_)) {
        negate(inner, bindings)
    } else {
        root(inner, bindings)
    }
}

/// Evaluate the items of a tuple, a set, or a list, and rebuild the node.
fn collection(ast: &Ast, items: &[Ast], bindings: &Bindings) -> Result<Ast, EvalError> {
    let values = evaluate_all(items, bindings)?;
    Ok(match ast {
        Ast::Tuple(_) => Ast::Tuple(values),
        Ast::Set(_) => Ast::Set(values),
        _ => Ast::List(values),
    })
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
    let text = write::write_tree(&value);
    match canonical_form(&text) {
        Ok(canon) => Ok(Answer { text, canon }),
        Err(reason) => Err(EvalError::NotCanonical { text, reason }),
    }
}

/// Compute an answer under a reviewed structured policy.
///
/// Ordinary policies keep the mathematical evaluator. A label may select one
/// text-valued parameter directly. A flat multipart policy uses
/// `multipart(part_1, part_2)`, with arguments in the policy's part order; a
/// label part may likewise be a text-valued parameter. The contract validates
/// the final text and supplies its canonical form before anything can be stored.
pub fn answer_for_contract(
    ast: &Ast,
    bindings: &Bindings,
    contract: Option<&AnswerContract>,
) -> Result<Answer, EvalError> {
    if matches!(ast, Ast::Func(name, _) if name == "powerform") {
        return structured::power_form(ast, bindings, contract);
    }
    match contract {
        Some(contract @ AnswerContract::Label { .. }) => label_answer(ast, bindings, contract),
        Some(contract @ AnswerContract::Unit { unit, .. }) => {
            unit_answer(ast, bindings, contract, unit)
        }
        Some(AnswerContract::Multipart { parts }) => multipart_answer(ast, bindings, parts),
        Some(contract @ AnswerContract::List { .. }) => list_answer(ast, bindings, contract),
        Some(contract @ AnswerContract::ReducedRatio) => {
            reduced_ratio_answer(ast, bindings, contract)
        }
        Some(contract @ AnswerContract::InequalityUnion) => {
            inequality_union_answer(ast, bindings, contract)
        }
        Some(contract @ AnswerContract::QuotientRemainder { .. }) => {
            quotient_remainder_answer(ast, bindings, contract)
        }
        _ => answer(ast, bindings),
    }
}

fn quotient_remainder_answer(
    ast: &Ast,
    bindings: &Bindings,
    contract: &AnswerContract,
) -> Result<Answer, EvalError> {
    let Ast::Func(name, args) = ast else {
        return answer(ast, bindings);
    };
    if name != "quotientremainder" || args.len() != 2 {
        return answer(ast, bindings);
    }
    let quotient = answer(&args[0], bindings)?.text;
    let remainder = answer(&args[1], bindings)?.text;
    contracted(format!("{quotient} R{remainder}"), contract)
}

fn inequality_union_answer(
    ast: &Ast,
    bindings: &Bindings,
    contract: &AnswerContract,
) -> Result<Answer, EvalError> {
    let Ast::Func(name, args) = ast else {
        return answer(ast, bindings);
    };
    if args.len() != 2 || !matches!(name.as_str(), "excludepoint" | "lowerbound" | "upperbound") {
        return answer(ast, bindings);
    }
    let Some(variable) = text_binding(&args[0], bindings) else {
        return answer(ast, bindings);
    };
    let bound = answer(&args[1], bindings)?.text;
    let text = match name.as_str() {
        "excludepoint" => format!("{variable} < {bound} or {variable} > {bound}"),
        "lowerbound" => format!("{variable} >= {bound}"),
        "upperbound" => format!("{variable} <= {bound}"),
        _ => unreachable!(),
    };
    contracted(text, contract)
}

fn reduced_ratio_answer(
    ast: &Ast,
    bindings: &Bindings,
    contract: &AnswerContract,
) -> Result<Answer, EvalError> {
    let value = answer(ast, bindings)?;
    let Canon::Rational(ratio) = &value.canon else {
        return contracted(value.text, contract);
    };
    contracted(format!("{}:{}", ratio.numer(), ratio.denom()), contract)
}

fn unit_answer(
    ast: &Ast,
    bindings: &Bindings,
    contract: &AnswerContract,
    unit: &str,
) -> Result<Answer, EvalError> {
    let value = answer(ast, bindings)?;
    if matches!(value.canon, Canon::Quantity { .. }) {
        return contracted(value.text, contract);
    }
    contracted(format!("{} {unit}", value.text), contract)
}

fn multipart_answer(
    ast: &Ast,
    bindings: &Bindings,
    parts: &[AnswerPart],
) -> Result<Answer, EvalError> {
    let Ast::Func(name, args) = ast else {
        return answer(ast, bindings);
    };
    if name != "multipart" || args.len() != parts.len() {
        return answer(ast, bindings);
    }
    let fields = parts
        .iter()
        .zip(args)
        .map(|(part, arg)| part_text(part, arg, bindings))
        .collect::<Result<Vec<_>, _>>()?;
    contracted(
        fields.join("; "),
        &AnswerContract::Multipart {
            parts: parts.to_vec(),
        },
    )
}

fn part_text(part: &AnswerPart, ast: &Ast, bindings: &Bindings) -> Result<String, EvalError> {
    let value = answer_for_contract(ast, bindings, Some(&part.contract))?.text;
    Ok(format!("{} = {value}", part.name))
}

fn text_binding(ast: &Ast, bindings: &Bindings) -> Option<String> {
    let Ast::Var(name) = ast else {
        return None;
    };
    bindings
        .get(name)
        .filter(|value| value.as_rational().is_none())
        .map(|value| value.canonical_string())
}

fn contracted(text: String, contract: &AnswerContract) -> Result<Answer, EvalError> {
    contract
        .validate_expected(&text)
        .map(|canon| Answer {
            text: text.clone(),
            canon,
        })
        .map_err(|reason| EvalError::NotCanonical { text, reason })
}
