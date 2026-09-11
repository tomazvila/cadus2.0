//! The evaluation-only functions: each one takes exact rationals and is erased.

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{Signed, ToPrimitive, Zero};

use super::exact::{as_rational, evaluate_all, literal, square_root};
use super::{EvalError, MAX_BINOMIAL_STEPS, MAX_FACTORIAL, MAX_VALUE_BITS, evaluate};
use crate::answer::ast::Ast;
use crate::template::domain::{Bindings, gcd_of, is_whole};

/// One evaluation-only function of one argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unary {
    /// The magnitude.
    Abs,
    /// The exact square root, or the root node.
    Sqrt,
    /// The greatest whole number at or below the value.
    Floor,
    /// The least whole number at or above the value.
    Ceiling,
    /// The factorial of a whole number.
    Factorial,
}

impl Unary {
    /// The name of the function, as the answer expression writes it.
    const fn name(self) -> &'static str {
        match self {
            Self::Abs => "abs",
            Self::Sqrt => "sqrt",
            Self::Floor => "floor",
            Self::Ceiling => "ceiling",
            Self::Factorial => "factorial",
        }
    }

    /// Apply the function to one exact rational.
    fn apply(self, first: &BigRational) -> Result<Ast, EvalError> {
        match self {
            Self::Abs => literal(first.abs()),
            Self::Sqrt => square_root(first),
            Self::Floor => literal(BigRational::from(first.floor().to_integer())),
            Self::Ceiling => literal(BigRational::from(first.ceil().to_integer())),
            Self::Factorial => literal(BigRational::from(factorial(first)?)),
        }
    }
}

/// One evaluation-only function of two arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Binary {
    /// The greatest common divisor of two whole numbers.
    Gcd,
    /// The least common multiple of two whole numbers.
    Lcm,
    /// The smaller of two values.
    Min,
    /// The larger of two values.
    Max,
    /// The binomial coefficient of two whole numbers.
    Binomial,
}

impl Binary {
    /// The name of the function, as the answer expression writes it.
    const fn name(self) -> &'static str {
        match self {
            Self::Gcd => "gcd",
            Self::Lcm => "lcm",
            Self::Min => "min",
            Self::Max => "max",
            Self::Binomial => "binomial",
        }
    }

    /// Apply the function to two exact rationals.
    fn apply(self, first: &BigRational, second: &BigRational) -> Result<Ast, EvalError> {
        match self {
            Self::Min => literal(first.min(second).clone()),
            Self::Max => literal(first.max(second).clone()),
            Self::Gcd => {
                let left = whole(self.name(), first)?;
                let right = whole(self.name(), second)?;
                literal(BigRational::from(gcd_of(&left, &right)))
            }
            Self::Lcm => {
                let left = whole(self.name(), first)?;
                let right = whole(self.name(), second)?;
                literal(BigRational::from(lcm_of(&left, &right)))
            }
            Self::Binomial => {
                let top = whole(self.name(), first)?;
                let bottom = whole(self.name(), second)?;
                literal(BigRational::from(binomial(&top, &bottom)?))
            }
        }
    }
}

/// One evaluation-only function, by the count of arguments it takes.
#[derive(Debug, Clone, Copy)]
enum Builtin {
    /// A function of one argument.
    Unary(Unary),
    /// A function of two arguments.
    Binary(Binary),
}

/// The evaluation-only function a name denotes, when it denotes one.
///
/// The table is [`super::EVAL_FUNCTIONS`], except for the structured
/// `signcase` call handled directly by [`call`].
fn builtin(name: &str) -> Option<Builtin> {
    match name {
        "abs" => Some(Builtin::Unary(Unary::Abs)),
        "sqrt" => Some(Builtin::Unary(Unary::Sqrt)),
        "floor" => Some(Builtin::Unary(Unary::Floor)),
        "ceiling" => Some(Builtin::Unary(Unary::Ceiling)),
        "factorial" => Some(Builtin::Unary(Unary::Factorial)),
        "gcd" => Some(Builtin::Binary(Binary::Gcd)),
        "lcm" => Some(Builtin::Binary(Binary::Lcm)),
        "min" => Some(Builtin::Binary(Binary::Min)),
        "max" => Some(Builtin::Binary(Binary::Max)),
        "binomial" => Some(Builtin::Binary(Binary::Binomial)),
        _ => None,
    }
}

/// Evaluate one function call, erasing it when its arguments are exact.
pub(super) fn call(name: &str, args: &[Ast], bindings: &Bindings) -> Result<Ast, EvalError> {
    if name == "symbol" {
        return super::symbol::variable(args, bindings);
    }
    if name == "compounding" {
        return super::compounding::factor(args, bindings);
    }
    if name == "quarterextremum" {
        return super::finite_graph::extremum(args, bindings);
    }
    if name == "quartervalue" {
        return super::quarter_value::value(args, bindings);
    }
    if name == "signcase" {
        return sign_case(args, bindings);
    }
    let values = evaluate_all(args, bindings)?;
    match builtin(name) {
        // A function of the M2 grammar that this module does not evaluate, such
        // as `sin` or `ln`. It stays in the tree, and the M2 canonicalizer reads
        // it. Its arguments are evaluated already.
        None => Ok(Ast::Func(name.to_string(), values)),
        Some(Builtin::Unary(func)) => unary_call(func, values),
        Some(Builtin::Binary(func)) => binary_call(func, values),
    }
}

/// Select the negative, zero, or positive result from a three-item list.
fn sign_case(args: &[Ast], bindings: &Bindings) -> Result<Ast, EvalError> {
    let [selector, choices] = args else {
        return Err(arity("signcase", 2, args.len()));
    };
    let Ast::List(choices) = choices else {
        return Err(EvalError::SignCaseShape);
    };
    if choices.len() != 3 {
        return Err(EvalError::SignCaseShape);
    }
    let selector = evaluate(selector, bindings)?;
    let Some(selector) = as_rational(&selector) else {
        return Err(EvalError::NotNumber { func: "signcase" });
    };
    let index = if selector.is_negative() {
        0
    } else if selector.is_zero() {
        1
    } else {
        2
    };
    evaluate(&choices[index], bindings)
}

/// The error of a call with the wrong count of arguments.
fn arity(func: &str, want: usize, given: usize) -> EvalError {
    EvalError::Arity {
        func: func.to_string(),
        want,
        given,
    }
}

/// Apply a function of one argument to its evaluated argument list.
fn unary_call(func: Unary, values: Vec<Ast>) -> Result<Ast, EvalError> {
    let [value] = values.as_slice() else {
        return Err(arity(func.name(), 1, values.len()));
    };
    let Some(number) = as_rational(value) else {
        // `abs` is in the M2 grammar, so a symbolic argument keeps the call.
        // Every other name of the set must reduce, or the answer string would
        // carry a name the checker cannot read (V2).
        if func == Unary::Abs {
            return Ok(Ast::Func("abs".to_string(), values));
        }
        return Err(EvalError::NotNumber { func: func.name() });
    };
    func.apply(&number)
}

/// Apply a function of two arguments to its evaluated argument list.
fn binary_call(func: Binary, values: Vec<Ast>) -> Result<Ast, EvalError> {
    let [left, right] = values.as_slice() else {
        return Err(arity(func.name(), 2, values.len()));
    };
    let (Some(left), Some(right)) = (as_rational(left), as_rational(right)) else {
        return Err(EvalError::NotNumber { func: func.name() });
    };
    func.apply(&left, &right)
}

/// The least common multiple of two whole numbers, as a non-negative number.
///
/// Two zeros give zero, the way SymPy reads `lcm(0, 0)`.
fn lcm_of(left: &BigInt, right: &BigInt) -> BigInt {
    let gcd = gcd_of(left, right);
    if gcd.is_zero() {
        return BigInt::from(0u8);
    }
    let value = (left * right) / gcd;
    if value.is_negative() { -value } else { value }
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
    let steps = std::cmp::min(bottom, &complement)
        .to_u32()
        .ok_or(EvalError::TooWide)?;
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

/// Read a whole number out of an exact rational, or refuse it.
fn whole(func: &'static str, value: &BigRational) -> Result<BigInt, EvalError> {
    if !is_whole(value) {
        return Err(EvalError::NotWhole { func });
    }
    Ok(value.numer().clone())
}
