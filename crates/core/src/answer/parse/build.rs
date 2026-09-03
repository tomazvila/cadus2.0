//! The node builders and the small readers the productions share.

use num_bigint::BigInt;
use num_traits::{Signed, Zero};

use super::{FUNCTIONS, FractionPart, GREEK_VARIABLES, MAX_RUN_LETTERS, RUN_LETTERS};
use crate::answer::Undecidable;
use crate::answer::ast::{Ast, IneqOp};
use crate::answer::lexer::{Tok, Token};

/// Whether a token is the single letter `x` or `X` that stands for times.
pub(super) fn is_times_letter(kind: &Tok) -> bool {
    match kind {
        Tok::Ident(name) => name == "x" || name == "X",
        _ => false,
    }
}

/// Whether `name` is a name a value label may carry.
///
/// One letter, or a spelled Greek name. A function name is never a label, and
/// `extra` holds the function names of [`parse_with_functions`].
pub(super) fn is_variable_name(name: &str, extra: &[&str]) -> bool {
    if FUNCTIONS.contains(&name) || extra.contains(&name) {
        return false;
    }
    GREEK_VARIABLES.contains(&name) || name.chars().count() == 1
}

/// Split a multi-letter run into its single-letter variables, or refuse it.
///
/// The run splits only when every letter is a [`RUN_LETTERS`] letter, the letters
/// differ from each other, the run is not a function name of this parse (`extra`
/// holds the extra names, so `gcd` stays one function and never becomes the
/// product `g*c*d`), and the run is at most [`MAX_RUN_LETTERS`] long. A
/// repeated letter is a spelling, not a product: a variable times itself is
/// written as a power. Everything else — a function name, a Greek name, a
/// differential (`dx`), a word (`yes`), a label (`HT`), an upper-case run
/// (`DNE`) — stays undecidable (C4).
pub(super) fn letter_run(name: &str, extra: &[&str]) -> Option<Vec<char>> {
    let letters: Vec<char> = name.chars().collect();
    if letters.len() < 2 || letters.len() > MAX_RUN_LETTERS {
        return None;
    }
    if FUNCTIONS.contains(&name)
        || extra.contains(&name)
        || GREEK_VARIABLES.contains(&name)
        || name == "pi"
    {
        return None;
    }
    for (index, letter) in letters.iter().enumerate() {
        if !RUN_LETTERS.contains(letter) {
            return None;
        }
        if letters.get(index + 1..)?.contains(letter) {
            return None;
        }
    }
    Some(letters)
}

/// Read the fraction part as a proper fraction, or refuse it.
///
/// The checks are the same for every mixed-number spelling: two plain digit
/// runs, and `0 < b < c`. The digit-run spelling refuses a three-digit numerator
/// as well, because a three-digit run after a space is a thousands group.
pub(super) fn proper_fraction_part(part: &FractionPart) -> Option<(BigInt, BigInt)> {
    let (numerator, denominator) = part.digits.as_ref()?;
    if !is_plain_digit_run(numerator) || !is_plain_digit_run(denominator) {
        return None;
    }
    if part.digit_run && numerator.chars().count() == 3 {
        return None;
    }
    let numerator = numerator.parse::<BigInt>().ok()?;
    let denominator = denominator.parse::<BigInt>().ok()?;
    if numerator.is_zero() || numerator >= denominator {
        return None;
    }
    Some((numerator, denominator))
}

/// Read the digit run of one `\frac` brace body, when the body is exactly one.
///
/// The body reaches the parser as a token list, so whitespace inside the braces
/// changes nothing: `\frac{ 1 }{2}` and `\frac{1}{2}` hold the same one token.
/// Round 2 tested the brace text instead, and one space there turned the mixed
/// number `2\frac{ 1}{2}` into the product 1 (review round 3, findings #1, #2).
pub(super) fn digit_run_body(tokens: &[Token]) -> Option<String> {
    match tokens {
        [only] => match &only.kind {
            Tok::Num(text) => Some(text.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// Read the sign token and the magnitude of a whole-number literal.
///
/// The sign comes from the [`Ast::Neg`] node that the sign token built, and
/// never from the integer value. `-0` is the integer zero, so a rule that reads
/// the value alone drops the minus of `-0 1/2` and grades minus one half as plus
/// one half (review round 3, finding #7).
pub(super) fn signed_whole(node: &Ast) -> Option<(bool, BigInt)> {
    match node {
        Ast::Integer(value) => Some((value.is_negative(), value.abs())),
        Ast::Neg(inner) => signed_whole(inner).map(|(negative, magnitude)| (!negative, magnitude)),
        _ => None,
    }
}

/// Whether the node is a number literal, with or without a leading sign.
pub(super) fn is_numeric_literal(node: &Ast) -> bool {
    match node {
        Ast::Integer(_) | Ast::Decimal { .. } | Ast::Fraction { .. } | Ast::Mixed { .. } => true,
        Ast::Neg(inner) => is_numeric_literal(inner),
        _ => false,
    }
}

/// Whether the text is a digit run that carries no grouping and no leading zero.
fn is_plain_digit_run(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_digit() {
        return false;
    }
    if first == '0' && chars.clone().next().is_some() {
        return false;
    }
    chars.all(|c| c.is_ascii_digit())
}

/// Build an interval from the two ends of a mixed bracket pair.
pub(super) fn make_interval(
    mut items: Vec<Ast>,
    lo_closed: bool,
    hi_closed: bool,
) -> Result<Ast, Undecidable> {
    if items.len() != 2 {
        return Err(Undecidable::new("an interval that has no two ends"));
    }
    let hi = items
        .pop()
        .ok_or_else(|| Undecidable::new("an interval with no upper end"))?;
    let lo = items
        .pop()
        .ok_or_else(|| Undecidable::new("an interval with no lower end"))?;
    Ok(Ast::Interval {
        lo: Box::new(lo),
        hi: Box::new(hi),
        lo_closed,
        hi_closed,
    })
}

/// Build a simple inequality, with the variable moved to the left.
pub(super) fn simple_inequality(left: Ast, op: IneqOp, right: Ast) -> Result<Ast, Undecidable> {
    match (left, right) {
        (Ast::Var(_), Ast::Var(_)) => Err(Undecidable::new("an inequality between two variables")),
        (Ast::Var(var), bound) => Ok(Ast::Ineq {
            var,
            op,
            bound: Box::new(bound),
        }),
        (bound, Ast::Var(var)) => Ok(Ast::Ineq {
            var,
            op: op.flipped(),
            bound: Box::new(bound),
        }),
        _ => Err(Undecidable::new("an inequality with no bare variable")),
    }
}

/// Fold a one-element list into its element, and a longer one into `build`.
pub(super) fn collapse(mut parts: Vec<Ast>, build: fn(Vec<Ast>) -> Ast) -> Ast {
    if parts.len() == 1 {
        match parts.pop() {
            Some(single) => single,
            None => build(parts),
        }
    } else {
        build(parts)
    }
}

/// Build a quotient, folding an integer over an integer into a fraction.
pub(super) fn make_quotient(dividend: Ast, divisor: Ast) -> Result<Ast, Undecidable> {
    if let (Some(numerator), Some(denominator)) = (whole_number(&dividend), whole_number(&divisor))
    {
        if denominator.is_zero() {
            return Err(Undecidable::new("a fraction with a zero denominator"));
        }
        let negative = denominator.sign() == num_bigint::Sign::Minus;
        let (numerator, denominator) = if negative {
            (-numerator, -denominator)
        } else {
            (numerator, denominator)
        };
        return Ok(Ast::Fraction {
            numerator,
            denominator,
        });
    }
    if whole_number(&divisor).is_some_and(|value| value.is_zero()) {
        return Err(Undecidable::new("a quotient with a zero divisor"));
    }
    Ok(Ast::Div(Box::new(dividend), Box::new(divisor)))
}

/// Build a function application, with one node for every square root.
///
/// `sqrt(2)`, `\sqrt{2}` and `√2` are one value, so all three build
/// [`Ast::Sqrt`] and the canonicalizer meets one shape (review round 3, the
/// structural ruling).
pub(super) fn make_call(name: &str, mut args: Vec<Ast>) -> Ast {
    if name == "sqrt"
        && args.len() == 1
        && let Some(argument) = args.pop()
    {
        return Ast::Sqrt(Box::new(argument));
    }
    Ast::Func(name.to_string(), args)
}

/// Read the value of a node that is a signed integer literal, if it is one.
fn whole_number(node: &Ast) -> Option<BigInt> {
    match node {
        Ast::Integer(value) => Some(value.clone()),
        Ast::Neg(inner) => whole_number(inner).map(|value| -value),
        _ => None,
    }
}

/// Turn a number literal into an integer or an exact decimal.
pub(super) fn parse_number(text: &str) -> Result<Ast, Undecidable> {
    match text.split_once('.') {
        None => Ok(Ast::Integer(parse_integer(text)?)),
        Some((whole, fraction)) => {
            let scale = u32::try_from(fraction.chars().count())
                .map_err(|_| Undecidable::new("a decimal with too many digits"))?;
            let digits = format!("{whole}{fraction}");
            Ok(Ast::Decimal {
                mantissa: parse_integer(&digits)?,
                scale,
            })
        }
    }
}

/// Parse a run of decimal digits into a big integer.
fn parse_integer(text: &str) -> Result<BigInt, Undecidable> {
    if text.is_empty() {
        return Ok(BigInt::zero());
    }
    text.parse::<BigInt>()
        .map_err(|_| Undecidable::new("a number the reader cannot read"))
}
