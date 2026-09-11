//! The answer-shape classifier of the Foundations inventory (unit f1-inventory).
//!
//! The classifier reads the AUTHORED answer text and names one shape. It runs
//! BEFORE the grammar, so it names a shape for an answer that the grammar
//! refuses. The rules form one cascade, and the first rule that matches wins.
//! Every rule reads the text only; no rule reads the topic or the problem.
//!
//! The order of the cascade is:
//!
//! 1. A notation marker the shape list does not name: `≈` (an approximation) and
//!    `a:b` (a ratio) become [`Shape::Other`].
//! 2. A structure: a quotient with a remainder, a relation, a bracket interval,
//!    a coordinate pair, a set.
//! 3. A numeric literal: an integer, a decimal, a fraction, a mixed number.
//! 4. A plain answer: a value with a unit, an ordered list, prose.
//! 5. A symbolic answer: a rational exponent, a radical, an expression.
//!
//! An ordered list stands before prose, because `and` and `,` separate the
//! members of a list and are not words of the answer. A list stands before a
//! radical, because the contract of `5, sqrt(30), 6` is the contract of a list.

#![allow(dead_code)]

/// The shape of one authored answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Shape {
    /// A whole number, with an optional minus sign.
    Integer,
    /// A finite decimal.
    Decimal,
    /// One integer over one integer.
    Fraction,
    /// A whole part and a fraction part, separated by a space.
    MixedNumber,
    /// A root, written `√a` or `sqrt(a)`.
    Radical,
    /// A power whose exponent is a fraction.
    RationalExponent,
    /// An algebraic expression.
    Expression,
    /// An equation or an inequality.
    EquationOrInequality,
    /// A range in bracket notation.
    Interval,
    /// A number with a unit, a degree sign, a percent sign, or a currency sign.
    ValueWithUnit,
    /// A quotient and a remainder, written `q Rr` or `q remainder r`.
    QuotientRemainder,
    /// A point, written as a parenthesized tuple.
    Coordinates,
    /// Two or more values in one order.
    OrderedList,
    /// A set in brace notation.
    Set,
    /// Words.
    Prose,
    /// A notation the list above does not name.
    Other,
}

/// Every shape, in declaration order.
pub const SHAPES: [Shape; 16] = [
    Shape::Integer,
    Shape::Decimal,
    Shape::Fraction,
    Shape::MixedNumber,
    Shape::Radical,
    Shape::RationalExponent,
    Shape::Expression,
    Shape::EquationOrInequality,
    Shape::Interval,
    Shape::ValueWithUnit,
    Shape::QuotientRemainder,
    Shape::Coordinates,
    Shape::OrderedList,
    Shape::Set,
    Shape::Prose,
    Shape::Other,
];

impl Shape {
    /// The wire value of the shape.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Integer => "integer",
            Self::Decimal => "decimal",
            Self::Fraction => "fraction",
            Self::MixedNumber => "mixed_number",
            Self::Radical => "radical",
            Self::RationalExponent => "rational_exponent",
            Self::Expression => "expression",
            Self::EquationOrInequality => "equation_or_inequality",
            Self::Interval => "interval",
            Self::ValueWithUnit => "value_with_unit",
            Self::QuotientRemainder => "quotient_remainder",
            Self::Coordinates => "coordinates",
            Self::OrderedList => "ordered_list",
            Self::Set => "set",
            Self::Prose => "prose",
            Self::Other => "other",
        }
    }
}

/// The unit words of the Foundations answers. A unit follows a number and one
/// space. `°`, `%`, and a currency sign carry no space and have their own rule.
pub const UNITS: [&str; 30] = [
    "mm", "cm", "m", "km", "g", "kg", "mg", "L", "mL", "s", "sec", "second", "seconds", "min",
    "minute", "minutes", "h", "hour", "hours", "day", "days", "week", "weeks", "month", "months",
    "year", "years", "km/h", "m/s", "mph",
];

/// The names the answer grammar reads as a function or a constant. A run of
/// letters from this list is not a word of prose.
const MATH_NAMES: [&str; 12] = [
    "sqrt", "sin", "cos", "tan", "sec", "csc", "cot", "log", "ln", "pi", "exp", "frac",
];

/// The two-letter words of prose. Every other run of two letters is a product of
/// two single-letter variables, such as the `xy` of `6xy^3`.
const SHORT_WORDS: [&str; 12] = [
    "or", "no", "of", "at", "in", "it", "is", "to", "so", "ii", "up", "an",
];

/// Name the shape of one authored answer.
#[must_use]
pub fn shape_of(text: &str) -> Shape {
    let trimmed = text.trim();
    marker_shape(trimmed)
        .or_else(|| structured_shape(trimmed))
        .or_else(|| literal_shape(trimmed))
        .or_else(|| plain_shape(trimmed))
        .unwrap_or_else(|| symbolic_shape(trimmed))
}

/// Rule 1: a notation marker that no other shape names.
fn marker_shape(text: &str) -> Option<Shape> {
    (text.contains('≈') || is_ratio(text)).then_some(Shape::Other)
}

/// Rule 2: the structure of the answer.
fn structured_shape(text: &str) -> Option<Shape> {
    if is_quotient_remainder(text) {
        return Some(Shape::QuotientRemainder);
    }
    if has_relation(text) {
        return Some(Shape::EquationOrInequality);
    }
    if is_interval(text) {
        return Some(Shape::Interval);
    }
    if is_coordinates(text) {
        return Some(Shape::Coordinates);
    }
    (text.starts_with('{') && text.ends_with('}')).then_some(Shape::Set)
}

/// Rule 3: a bare numeric literal.
fn literal_shape(text: &str) -> Option<Shape> {
    if is_integer(text) {
        return Some(Shape::Integer);
    }
    if is_decimal(text) {
        return Some(Shape::Decimal);
    }
    if is_fraction(text) {
        return Some(Shape::Fraction);
    }
    is_mixed_number(text).then_some(Shape::MixedNumber)
}

/// Rule 4: a value with a unit, a list, or prose.
fn plain_shape(text: &str) -> Option<Shape> {
    if is_value_with_unit(text) {
        return Some(Shape::ValueWithUnit);
    }
    if is_ordered_list(text) {
        return Some(Shape::OrderedList);
    }
    has_word(text).then_some(Shape::Prose)
}

/// Rule 5: a symbolic answer.
fn symbolic_shape(text: &str) -> Shape {
    if text.is_empty() {
        return Shape::Other;
    }
    if has_rational_exponent(text) {
        return Shape::RationalExponent;
    }
    if text.contains('√') || text.contains("sqrt") {
        return Shape::Radical;
    }
    Shape::Expression
}

/// Whether the text is one unsigned integer, a colon, and one unsigned integer.
fn is_ratio(text: &str) -> bool {
    text.split_once(':')
        .is_some_and(|(left, right)| is_digits(left) && is_digits(right))
}

/// Whether the text is a quotient with a remainder, in either spelling.
fn is_quotient_remainder(text: &str) -> bool {
    if let Some((quotient, rest)) = text.split_once(" remainder ") {
        return !quotient.trim().is_empty() && is_digits(rest.trim());
    }
    text.split_once('R')
        .is_some_and(|(quotient, rest)| is_integer(quotient.trim()) && is_digits(rest.trim()))
}

/// Whether the text carries a relation sign.
fn has_relation(text: &str) -> bool {
    text.contains(['=', '<', '>', '≤', '≥'])
}

/// Whether the text is a range in bracket notation.
///
/// A square bracket, an infinity sign, or a union sign separates a range from a
/// coordinate pair: `(2, 5)` is a point, and `(2, ∞)` is a range.
fn is_interval(text: &str) -> bool {
    let bracketed = text.starts_with(['(', '[']) && text.ends_with([')', ']']);
    bracketed
        && text.contains(',')
        && (text.contains('[') || text.contains(']') || text.contains(['∞', '∪']))
}

/// Whether the text is one parenthesized tuple of two or three members.
fn is_coordinates(text: &str) -> bool {
    let Some(inner) = text
        .strip_prefix('(')
        .and_then(|rest| rest.strip_suffix(')'))
    else {
        return false;
    };
    if inner.contains(['(', ')']) {
        return false;
    }
    let members: Vec<&str> = inner.split(',').collect();
    (2..=3).contains(&members.len()) && members.iter().all(|member| is_value(member))
}

/// Whether the text is a number with a unit, a sign, or a currency mark.
fn is_value_with_unit(text: &str) -> bool {
    if let Some(rest) = text.strip_prefix(['€', '$', '£']) {
        return is_number(rest);
    }
    if let Some(rest) = text.strip_suffix('°').or_else(|| text.strip_suffix('%')) {
        return is_number(rest.trim_end());
    }
    text.rsplit_once(' ')
        .is_some_and(|(value, unit)| UNITS.contains(&unit) && is_number(value))
}

/// Whether a power of the text carries a fraction as its exponent.
fn has_rational_exponent(text: &str) -> bool {
    ["^(", "^{"].iter().any(|open| {
        text.split_once(open).is_some_and(|(_, rest)| {
            let end = rest.find([')', '}']).unwrap_or(rest.len());
            rest.get(..end).is_some_and(|inner| inner.contains('/'))
        })
    })
}

/// Whether the text is two or more values in one order.
fn is_ordered_list(text: &str) -> bool {
    [", ", " and ", "; "].iter().any(|separator| {
        let members: Vec<&str> = text.split(separator).collect();
        members.len() > 1 && members.iter().all(|member| is_value(member))
    })
}

/// Whether the text is one value and carries no word of prose.
fn is_value(text: &str) -> bool {
    !text.trim().is_empty() && !has_word(text)
}

/// Whether the text holds a run of two letters or more that names no function.
fn has_word(text: &str) -> bool {
    text.split(|c: char| !c.is_alphabetic())
        .any(|word| is_word(&word.to_lowercase()))
}

/// Whether one run of letters, already in lower case, is a word of prose.
fn is_word(word: &str) -> bool {
    let letters = word.chars().count();
    if letters < 2 || MATH_NAMES.contains(&word) {
        return false;
    }
    letters > 2 || SHORT_WORDS.contains(&word)
}

/// Whether the text is one or more ASCII digits.
fn is_digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

/// Whether the text is a whole number with an optional minus sign.
fn is_integer(text: &str) -> bool {
    is_digits(text.strip_prefix('-').unwrap_or(text))
}

/// Whether the text is a finite decimal.
fn is_decimal(text: &str) -> bool {
    text.strip_prefix('-')
        .unwrap_or(text)
        .split_once('.')
        .is_some_and(|(whole, part)| is_digits(whole) && is_digits(part))
}

/// Whether the text is one integer over one integer.
fn is_fraction(text: &str) -> bool {
    text.split_once('/')
        .is_some_and(|(top, bottom)| is_integer(top) && is_integer(bottom))
}

/// Whether the text is a whole part and a fraction part.
fn is_mixed_number(text: &str) -> bool {
    text.split_once(' ')
        .is_some_and(|(whole, part)| is_integer(whole) && is_fraction(part))
}

/// Whether the text is any numeric literal the rules above name.
fn is_number(text: &str) -> bool {
    is_integer(text) || is_decimal(text) || is_fraction(text) || is_mixed_number(text)
}
