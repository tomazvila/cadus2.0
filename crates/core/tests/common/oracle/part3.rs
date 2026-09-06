//! Part 3 of the helpers of the `answer_oracle` tests.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    unused_imports
)]

use super::*;

// ---------------------------------------------------------------------------
// The generators of spec section 9.3
// ---------------------------------------------------------------------------
/// What a real learner means by the variant (spec section 9.3).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Intent {
    /// The variant is the same answer, written another way (V4).
    Same,
    /// The variant is a different answer.
    Different,
    /// The variant is the same value under the period-grouping reading (V4).
    Notation,
}

impl Intent {
    /// The name of the intent, for the report.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Same => "same",
            Self::Different => "different",
            Self::Notation => "notation",
        }
    }
}

/// One learner-notation generator.
pub struct Generator {
    /// The name of the generator, used in the report and in the pair dump.
    pub name: &'static str,
    /// What the learner means by the variant.
    pub intent: Intent,
    /// Build the learner string, or refuse the answer.
    pub make: fn(&Row) -> Option<String>,
}

/// A variant that changed nothing is no variant.
pub fn changed(row: &Row, candidate: String) -> Option<String> {
    (candidate != row.answer).then_some(candidate)
}

/// The same rule for a generator that rewrites the parser source.
///
/// A source-based generator starts from the normalized source, so it must
/// compare against that source. Comparing against the raw answer would let it
/// emit the plain rewritten source and claim the rewrite as its own variant.
pub fn changed_source(row: &Row, candidate: String) -> Option<String> {
    (candidate != row.source).then_some(candidate)
}

/// The same rule for a generator that rewrites the printed tree.
pub fn changed_printed(row: &Row, candidate: String) -> Option<String> {
    (candidate != row.printed).then_some(candidate)
}

pub fn generate_identity(row: &Row) -> Option<String> {
    Some(row.answer.clone())
}

pub fn generate_padding(row: &Row) -> Option<String> {
    Some(format!("  {}  ", row.answer))
}

/// The characters the internal-space family puts a space around.
///
/// A run of them stays one token, so `**` stays `**`, `<=` stays `<=`, and
/// `x^-2` stays `x ^- 2`. Splitting a run would build an answer no learner types
/// and would ask the oracle a question about the split, not about the space.
pub const SPACED_OPERATORS: [char; 11] = ['+', '-', '*', '/', '^', '(', ')', ',', '=', '<', '>'];

/// Space out every operator of the answer (spec section 9.3, "internal space
/// collapse").
///
/// The learner spaces the answer out and the checker collapses the spaces again
/// (V4): `1/2` becomes `1 / 2`, `1+2x` becomes `1 + 2 x`, `(4, 17)` becomes
/// `( 4 , 17 )`, and `x^2` becomes `x ^ 2`. The expected verdict is True.
///
/// The family is the one spec section 9.3 names and `GENERATORS` omitted, so the
/// 100% class-3 agreement of review round 1 measured the generators that were
/// written and not the parity of the checker (M2 review 2, finding 16).
pub fn generate_internal_spaces(row: &Row) -> Option<String> {
    // A `$…$` wrapper keeps its two ends glued to the answer. A learner spaces
    // the maths out, never the wrapper.
    let trimmed = row.answer.trim();
    let wrapped = trimmed.len() >= 2 && trimmed.starts_with('$') && trimmed.ends_with('$');
    let body = if wrapped {
        trimmed.get(1..trimmed.len().checked_sub(1)?)?
    } else {
        trimmed
    };
    let spaced = space_out(body)?;
    let candidate = if wrapped {
        format!("${spaced}$")
    } else {
        spaced
    };
    changed(row, candidate)
}

/// Put one space around every operator of one answer body.
pub fn space_out(text: &str) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut index = 0;
    while let Some(ch) = chars.get(index).copied() {
        if SPACED_OPERATORS.contains(&ch) {
            let start = index;
            while matches!(chars.get(index), Some(c) if SPACED_OPERATORS.contains(c)) {
                index += 1;
            }
            push_spaced(&mut out, chars.get(start..index)?);
            continue;
        }
        // A digit that a letter follows is the implicit product `2x` of the spec
        // example `1 + 2 x`.
        if ch.is_ascii_digit()
            && matches!(chars.get(index + 1), Some(next) if next.is_ascii_alphabetic())
        {
            out.push(ch);
            out.push(' ');
            index += 1;
            continue;
        }
        out.push(ch);
        index += 1;
    }
    Some(out.split_whitespace().collect::<Vec<&str>>().join(" "))
}

/// Put one space in front of a token and one space after it.
pub fn push_spaced(out: &mut String, token: &[char]) {
    out.push(' ');
    out.extend(token);
    out.push(' ');
}

pub fn generate_trailing_period(row: &Row) -> Option<String> {
    Some(format!("{}.", row.answer))
}

pub fn generate_dollar_wrapped(row: &Row) -> Option<String> {
    if row.answer.trim().starts_with('$') {
        return None;
    }
    Some(format!("${}$", row.answer))
}

pub fn generate_comma_space_removed(row: &Row) -> Option<String> {
    changed(row, row.answer.replace(", ", ","))
}

pub fn generate_plus_spaced(row: &Row) -> Option<String> {
    if !row.answer.contains('+') {
        return None;
    }
    changed(row, row.answer.replace('+', " + "))
}

pub fn generate_comma_thousands(row: &Row) -> Option<String> {
    thousands_grouped(row, ",")
}

pub fn generate_space_thousands(row: &Row) -> Option<String> {
    thousands_grouped(row, " ")
}

pub fn generate_nbsp_thousands(row: &Row) -> Option<String> {
    thousands_grouped(row, "\u{00a0}")
}

pub fn generate_narrow_space_thousands(row: &Row) -> Option<String> {
    thousands_grouped(row, "\u{202f}")
}

pub fn generate_thin_space_thousands(row: &Row) -> Option<String> {
    thousands_grouped(row, "\u{2009}")
}

pub fn generate_figure_space_thousands(row: &Row) -> Option<String> {
    thousands_grouped(row, "\u{2007}")
}

pub fn generate_dot_thousands(row: &Row) -> Option<String> {
    thousands_grouped(row, ".")
}

pub fn generate_equivalent_fraction(row: &Row) -> Option<String> {
    let (numerator, denominator) = fraction_parts(&row.printed)?;
    let factor = 2 + (row.printed.len() % 8) as i128;
    let numerator = numerator.checked_mul(factor)?;
    let denominator = denominator.checked_mul(factor)?;
    Some(format!("{numerator}/{denominator}"))
}

pub fn generate_fraction_to_decimal(row: &Row) -> Option<String> {
    let (numerator, denominator) = fraction_parts(&row.printed)?;
    exact_decimal(numerator, denominator)
}

pub fn generate_decimal_to_fraction(row: &Row) -> Option<String> {
    let (negative, whole, fraction) = decimal_parts(&row.printed)?;
    if fraction.len() > 12 {
        return None;
    }
    let mantissa: i128 = format!("{whole}{fraction}").parse().ok()?;
    let denominator = 10_i128.checked_pow(u32::try_from(fraction.len()).ok()?)?;
    let sign = if negative { "-" } else { "" };
    Some(format!("{sign}{mantissa}/{denominator}"))
}

pub fn generate_trailing_zero(row: &Row) -> Option<String> {
    if decimal_parts(&row.printed).is_some() {
        return Some(format!("{}0", row.printed));
    }
    let (_, digits) = integer_digits(&row.printed)?;
    if digits.len() > 15 {
        return None;
    }
    Some(format!("{}.0", row.printed))
}

pub fn generate_star_power(row: &Row) -> Option<String> {
    if !row.answer.contains('^') {
        return None;
    }
    changed(row, row.answer.replace('^', "**"))
}

pub fn generate_caret_power(row: &Row) -> Option<String> {
    if !row.answer.contains("**") {
        return None;
    }
    changed(row, row.answer.replace("**", "^"))
}

/// Put a `*` between every pair of tokens that only touch.
pub fn generate_explicit_multiplication(row: &Row) -> Option<String> {
    let chars: Vec<char> = row.source.chars().collect();
    let mut out = String::new();
    for (index, ch) in chars.iter().enumerate() {
        if index > 0 {
            let previous = chars.get(index - 1).copied().unwrap_or(' ');
            let joins = (previous.is_ascii_digit() && ch.is_ascii_alphabetic())
                || (previous == ')' && (ch.is_alphanumeric() || *ch == '('));
            if joins {
                out.push('*');
            }
        }
        out.push(*ch);
    }
    changed_source(row, out)
}

/// Drop the `*` between a number and a name, the way a learner writes it.
///
/// The family reads the printed tree, so it reaches `2*x` whatever the author
/// wrote. A `*` that follows a divisor keeps its star: `4/3*sin(x)` and
/// `4/3sin(x)` are two readings of one string, and the family asks about the
/// juxtaposition and not about the precedence of an implicit product.
pub fn generate_implicit_multiplication(row: &Row) -> Option<String> {
    let chars: Vec<char> = row.printed.chars().collect();
    let mut out = String::new();
    for (index, ch) in chars.iter().enumerate() {
        if *ch == '*' {
            let previous = chars.get(index.wrapping_sub(1)).copied().unwrap_or(' ');
            let next = chars.get(index + 1).copied().unwrap_or(' ');
            if previous.is_ascii_digit()
                && next.is_ascii_alphabetic()
                && !follows_a_divisor(&chars, index)
            {
                continue;
            }
        }
        out.push(*ch);
    }
    changed_printed(row, out)
}

/// Whether the number that ends at `index` is the divisor of a `/`.
pub fn follows_a_divisor(chars: &[char], index: usize) -> bool {
    last_before(chars, index, |ch| ch.is_ascii_digit() || ch == '.') == Some('/')
}

pub fn generate_unicode_to_ascii(row: &Row) -> Option<String> {
    let mut out = radical_to_call(&row.answer);
    for (glyph, ascii) in UNICODE_TO_ASCII {
        out = out.replace(glyph, ascii);
    }
    changed(row, out)
}

pub fn generate_ascii_to_unicode(row: &Row) -> Option<String> {
    // A backslash keeps its LaTeX word together, and `\pi` must not become
    // `\\π`. No learner types that.
    if row.answer.contains('\\') {
        return None;
    }
    let mut out = replace_word(&row.answer, "pi", "π");
    out = replace_word(&out, "theta", "θ");
    out = out.replace("<=", "≤").replace(">=", "≥");
    out = out.replace("sqrt(", "√(");
    changed(row, out)
}

pub fn generate_sum_reorder(row: &Row) -> Option<String> {
    let parts = top_level_split(&row.printed, '+')?;
    if parts.len() < 2 {
        return None;
    }
    let mut rotated = parts.clone();
    rotated.rotate_left(1);
    changed(row, rotated.join("+"))
}

pub fn generate_set_reordered(row: &Row) -> Option<String> {
    let mut items = bracket_items(&row.printed, '{', '}')?;
    if items.len() < 2 {
        return None;
    }
    items.reverse();
    changed(row, format!("{{{}}}", items.join(", ")))
}

/// Write the answer in upper case (spec section 9.3, "case flip").
///
/// The expected verdict is True on both sides, and both checkers reach it on
/// their string rung: 1.0 `_normalize` casefolds (`sympy_check.py:44`), and 2.0
/// builds the same casefolded `string_key` (`answer::normalize`, spec section
/// 2.1). Neither parser casefolds, so the family measures the rung and not the
/// grammar: `SQRT(2)` never reaches a `sqrt` call, and `2*X` never reaches the
/// variable `x`.
pub fn generate_case_flip(row: &Row) -> Option<String> {
    changed(row, row.answer.to_ascii_uppercase())
}

/// Write the answer as a decimal of ten significant digits (spec section 9.3,
/// "decimal to >= 8 significant digits").
///
/// The family applies to a rational with no exact decimal and to a radical. 1.0
/// grades the pair True on a float rung; 2.0 holds exact values only, so it
/// grades the pair False (D6). The divergence is the documented class "no float
/// tolerance rung (D6)", and [`the_1_0_float_rung_closes_the_gap`] names it.
pub fn generate_significant_decimal(row: &Row) -> Option<String> {
    if !takes_a_rounded_decimal(row.canon.as_ref()?) {
        return None;
    }
    let value = numeric_value(&row.printed)?;
    changed_printed(row, ten_significant_digits(value)?)
}

/// Whether the value takes a rounded decimal: it is a number, and it is not the
/// decimal it writes.
///
/// The predicate reads the CANONICAL FORM, and no longer the normalized source.
/// FIXM2g made every construct a lexer token, so the source of `√(2 + √3)/2`
/// keeps its `√` and the old `source.contains("sqrt(")` test went silently false
/// (M2 review 3, the FIXM2i ruling). The canonical form carries the value, so
/// the question the family asks is the question the predicate asks.
///
/// A rational with a terminating decimal is the decimal it writes, so it takes
/// no rounded decimal. Every other exact number — a repeating rational, a
/// radical, `pi`, `e` — does. A value with a free symbol is no number at all.
pub fn takes_a_rounded_decimal(canon: &Canon) -> bool {
    match canon {
        Canon::Rational(value) => !terminates_exactly(value),
        // A radical map holds a root, a `pi`, or an `e` against a rational
        // coefficient. The rational alone never builds this variant.
        Canon::Radical(_) => true,
        // A symbol-free product of roots, such as `sqrt(2 + sqrt(3))`. A radicand
        // that is not a rational stays an `Atom::Call` of the name `sqrt`.
        Canon::Poly(poly) => an_irrational_number(poly),
        _ => false,
    }
}

/// Whether a polynomial is one irrational number with no free symbol.
pub fn an_irrational_number(poly: &cadus_core::answer::Poly) -> bool {
    let mut irrational = false;
    for (atom, _) in poly.keys().flatten() {
        match atom {
            Atom::Var(_) => return false,
            Atom::Call(name, _) if name != "sqrt" => return false,
            Atom::Root(base, _) if holds_a_variable(base) => return false,
            _ => irrational = true,
        }
    }
    irrational
}

/// Whether a canonical form holds a variable at any depth.
fn holds_a_variable(canon: &Canon) -> bool {
    let mut names = BTreeSet::new();
    collect_variable_names(canon, &mut names);
    !names.is_empty()
}

/// Whether the decimal expansion of an exact rational ends.
pub fn terminates_exactly(value: &num_rational::BigRational) -> bool {
    let mut rest = value.denom().clone();
    let two = num_bigint::BigInt::from(2);
    let five = num_bigint::BigInt::from(5);
    while (&rest % &two).is_zero() {
        rest /= &two;
    }
    while (&rest % &five).is_zero() {
        rest /= &five;
    }
    rest.is_one()
}

/// Write `value` with ten significant digits.
///
/// The count is ten and not eight, so the pair stays inside the 1e-6 tolerance
/// of the 1.0 `evalf` rung for every magnitude the corpus holds.
///
/// The place count comes from the decimal text of the value, and not from
/// `log10`: a library logarithm is not correctly rounded, and one unit in the
/// last place at a power of ten moves the digit count and with it the generated
/// pair. Rust formats a `f64` in Rust and not in the C library, so the text is
/// the same on every build box and the generated set stays reproducible.
pub fn ten_significant_digits(value: f64) -> Option<String> {
    if !value.is_finite() || value == 0.0 {
        return None;
    }
    let text = format!("{:.30}", value.abs());
    let (whole, fraction) = text.split_once('.')?;
    let places = if whole == "0" {
        // The value is under one. Every leading zero of the fraction takes one
        // more place, so ten significant digits still follow it.
        let zeros = fraction.chars().take_while(|c| *c == '0').count();
        10_i64.checked_add(i64::try_from(zeros).ok()?)?
    } else {
        // The value is at least one, and its whole part already carries digits.
        9_i64
            .checked_sub(i64::try_from(whole.len()).ok()?)?
            .checked_add(1)?
    };
    let places = usize::try_from(places).ok()?;
    if places > 30 {
        return None;
    }
    Some(format!("{value:.places$}"))
}
