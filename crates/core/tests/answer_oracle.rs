//! U3 oracle parity: the 2.0 checker against the 1.0 checker (V3, R5, V2).
//!
//! The file holds the learner-notation generators of
//! `docs/reference/checker-1.0-spec.md` section 9.3. It applies every generator
//! to every corpus answer that the 2.0 grammar accepts, and it compares the 2.0
//! verdict with the recorded 1.0 verdict.
//!
//! # Where the 1.0 verdicts come from
//!
//! `crates/core/tests/fixtures/answers/oracle_verdicts_1_0.jsonl` holds one line
//! per generated pair, recorded once with the live 1.0 checker through
//! `scripts/oracle/check_1_0.py`. The test always compares against that file. To
//! record it again, or to prove the file still matches the live 1.0 checker:
//!
//! ```text
//! CADUS_ORACLE_DUMP=/tmp/pairs.jsonl cargo test -p cadus-core --test answer_oracle
//! CADUS_ORACLE_PYTHON=/home/deploy/dev/cadus/.venv/bin/python \
//!     cargo test -p cadus-core --test answer_oracle -- --ignored --nocapture
//! ```
//!
//! # The four divergence classes (spec section 9.3)
//!
//! 1. Either side leaves the decidable grammar, so 2.0 refuses a verdict (V2).
//!    Not a parity failure.
//! 2. The authored answer is prose, so the topic is mis-kinded. Not a parity
//!    failure; the list goes to `docs/reference/undecidable-answers.md`.
//! 3. Both sides are inside the grammar and the two verdicts agree, or they
//!    differ for no documented reason. A difference here is a 2.0 bug (R5).
//! 4. Both sides are inside the grammar, the two verdicts differ, and the
//!    difference is one of the documented 2.0 divergences.
//!
//! Every count in this file is a literal. No expected value is read back from
//! the code under test.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use cadus_core::answer::{Canon, Outcome, canonical_form, check, normalize, parse};
use cadus_core::curriculum::AnswerKind;

/// The largest number of pairs the test runs (the task bound of U3).
const PAIR_CAP: usize = 20_000;

/// The seed of the selection shuffle. A fixed seed makes the set reproducible.
const SHUFFLE_SEED: u64 = 0x2026_0826_5533_1177;

// ---------------------------------------------------------------------------
// The corpus
// ---------------------------------------------------------------------------

/// One line of `crates/core/tests/fixtures/answers/corpus_1_0.jsonl`.
#[derive(serde::Deserialize)]
struct CorpusLine {
    answer: String,
    answer_kind: String,
    shape: String,
}

/// One corpus answer that the 2.0 grammar accepts.
struct Row {
    /// The authored answer, verbatim.
    answer: String,
    /// The normalized parser source of that answer (V4).
    source: String,
    /// The shape bucket of spec section 5.
    shape: String,
    /// The authored answer kind.
    kind: AnswerKind,
}

/// The path of one test fixture.
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/answers")
        .join(name)
}

/// Read the corpus rows the grammar accepts, deduplicated by answer and kind.
///
/// The corpus repeats an answer string across topics (`6` occurs hundreds of
/// times). A repeat adds no pair, so the set is keyed by the answer text and the
/// answer kind.
fn in_grammar_rows() -> Vec<Row> {
    let path = fixture("corpus_1_0.jsonl");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
    let mut rows = Vec::new();
    for line in text.lines() {
        let parsed: CorpusLine =
            serde_json::from_str(line).unwrap_or_else(|e| panic!("row {line}: {e}"));
        let kind = match parsed.answer_kind.as_str() {
            "numeric" => AnswerKind::Numeric,
            "expression" => AnswerKind::Expression,
            other => panic!("the corpus holds only verifiable kinds, and this row says {other}"),
        };
        if !seen.insert((parsed.answer.clone(), parsed.answer_kind.clone())) {
            continue;
        }
        let source = normalize(&parsed.answer).source;
        if parse(&source).is_err() {
            continue;
        }
        rows.push(Row {
            answer: parsed.answer,
            source,
            shape: parsed.shape,
            kind,
        });
    }
    rows
}

// ---------------------------------------------------------------------------
// Small text tools the generators share
// ---------------------------------------------------------------------------

/// Split `text` on `separator` at bracket depth zero.
///
/// Returns `None` when the brackets do not balance or a part is blank.
fn top_level_split(text: &str, separator: char) -> Option<Vec<String>> {
    let mut depth = 0_i32;
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        match ch {
            '(' | '[' | '{' => {
                depth += 1;
                current.push(ch);
            }
            ')' | ']' | '}' => {
                depth -= 1;
                if depth < 0 {
                    return None;
                }
                current.push(ch);
            }
            _ if ch == separator && depth == 0 => {
                parts.push(std::mem::take(&mut current));
            }
            _ => current.push(ch),
        }
    }
    if depth != 0 {
        return None;
    }
    parts.push(current);
    if parts.iter().any(|part| part.trim().is_empty()) {
        return None;
    }
    Some(parts.iter().map(|part| part.trim().to_string()).collect())
}

/// Read the items of one bracketed collection.
fn bracket_items(source: &str, open: char, close: char) -> Option<Vec<String>> {
    let inner = source.strip_prefix(open)?.strip_suffix(close)?;
    top_level_split(inner, ',')
}

/// Read a plain integer source into its sign and its digits.
fn integer_digits(source: &str) -> Option<(bool, String)> {
    let (negative, rest) = match source.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, source),
    };
    if rest.is_empty() || !rest.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((negative, rest.to_string()))
}

/// Read a plain decimal source into its sign, its whole part, and its fraction.
fn decimal_parts(source: &str) -> Option<(bool, String, String)> {
    let (negative, rest) = match source.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, source),
    };
    let (whole, fraction) = rest.split_once('.')?;
    if fraction.is_empty()
        || !fraction.chars().all(|c| c.is_ascii_digit())
        || !whole.chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    Some((negative, whole.to_string(), fraction.to_string()))
}

/// Read a plain `a/b` source into its two integers.
fn fraction_parts(source: &str) -> Option<(i128, i128)> {
    let parts = top_level_split(source, '/')?;
    if parts.len() != 2 {
        return None;
    }
    let numerator: i128 = parts.first()?.parse().ok()?;
    let denominator: i128 = parts.get(1)?.parse().ok()?;
    if denominator == 0 {
        return None;
    }
    Some((numerator, denominator))
}

/// Group `digits` in threes from the right with `separator`.
fn group_digits(digits: &str, separator: &str) -> String {
    let chars: Vec<char> = digits.chars().collect();
    let mut out = String::new();
    for (index, ch) in chars.iter().enumerate() {
        if index > 0 && (chars.len() - index).is_multiple_of(3) {
            out.push_str(separator);
        }
        out.push(*ch);
    }
    out
}

/// Write one integer answer with `separator` between its thousands groups.
fn thousands_grouped(row: &Row, separator: &str) -> Option<String> {
    let (negative, digits) = integer_digits(&row.source)?;
    if digits.len() < 4 || digits.len() > 15 || digits.starts_with('0') {
        return None;
    }
    let body = group_digits(&digits, separator);
    Some(if negative { format!("-{body}") } else { body })
}

/// The greatest common divisor of two non-negative numbers.
fn gcd(a: i128, b: i128) -> i128 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// Write `numerator / denominator` as an exact decimal, when one exists.
fn exact_decimal(numerator: i128, denominator: i128) -> Option<String> {
    let divisor = gcd(numerator.abs(), denominator.abs());
    if divisor == 0 {
        return None;
    }
    let numerator = numerator / divisor;
    let denominator = denominator / divisor;
    let negative = (numerator < 0) != (denominator < 0);
    let numerator = numerator.abs();
    let mut rest = denominator.abs();
    let mut twos = 0_u32;
    let mut fives = 0_u32;
    while rest % 2 == 0 {
        rest /= 2;
        twos += 1;
    }
    while rest % 5 == 0 {
        rest /= 5;
        fives += 1;
    }
    if rest != 1 {
        return None;
    }
    let scale = twos.max(fives);
    if scale == 0 || scale > 12 {
        return None;
    }
    let factor = 10_i128.checked_pow(scale)?;
    let scaled = numerator.checked_mul(factor)? / denominator.abs();
    let mut digits = scaled.to_string();
    while digits.len() <= scale as usize {
        digits.insert(0, '0');
    }
    let (whole, fraction) = digits.split_at(digits.len() - scale as usize);
    let body = format!("{whole}.{fraction}");
    Some(if negative { format!("-{body}") } else { body })
}

/// The plain Unicode table of 1.0 `_UNICODE_SIMPLE`, plus the superscripts.
///
/// The table is a literal copy of `sympy_check.py:137-143`. The test owns it, so
/// a change in `answer::normalize` cannot quietly change the generated pairs.
const UNICODE_TO_ASCII: [(&str, &str); 30] = [
    ("π", "pi"),
    ("τ", "(2*pi)"),
    ("∞", "oo"),
    ("·", "*"),
    ("−", "-"),
    ("–", "-"),
    ("≤", "<="),
    ("≥", ">="),
    ("θ", "theta"),
    ("α", "alpha"),
    ("β", "beta"),
    ("λ", "lamda"),
    ("½", "(1/2)"),
    ("⅓", "(1/3)"),
    ("⅔", "(2/3)"),
    ("¼", "(1/4)"),
    ("¾", "(3/4)"),
    ("°", ""),
    ("×", "*"),
    ("÷", "/"),
    ("⁰", "^0"),
    ("¹", "^1"),
    ("²", "^2"),
    ("³", "^3"),
    ("⁴", "^4"),
    ("⁵", "^5"),
    ("⁶", "^6"),
    ("⁷", "^7"),
    ("⁸", "^8"),
    ("⁹", "^9"),
];

/// Rewrite every `√` of `text` into a `sqrt(…)` call.
fn radical_to_call(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars.get(index).copied().unwrap_or(' ');
        if ch != '√' {
            out.push(ch);
            index += 1;
            continue;
        }
        index += 1;
        while matches!(chars.get(index), Some(' ')) {
            index += 1;
        }
        if chars.get(index) == Some(&'(') {
            let mut depth = 0_i32;
            let start = index;
            while index < chars.len() {
                match chars.get(index) {
                    Some('(') => depth += 1,
                    Some(')') => depth -= 1,
                    _ => {}
                }
                index += 1;
                if depth == 0 {
                    break;
                }
            }
            let inner: String = chars.get(start..index).unwrap_or_default().iter().collect();
            out.push_str("sqrt");
            out.push_str(&inner);
        } else {
            let start = index;
            while matches!(chars.get(index), Some(c) if c.is_alphanumeric()) {
                index += 1;
            }
            let token: String = chars.get(start..index).unwrap_or_default().iter().collect();
            if token.is_empty() {
                out.push('√');
            } else {
                let _ = write!(out, "sqrt({token})");
            }
        }
    }
    out
}

/// Replace the whole word `from` with `to`.
fn replace_word(text: &str, from: &str, to: &str) -> String {
    let mut out = String::new();
    let mut run = String::new();
    for ch in text.chars() {
        if ch.is_ascii_alphabetic() {
            run.push(ch);
            continue;
        }
        if !run.is_empty() {
            out.push_str(if run == from { to } else { &run });
            run.clear();
        }
        out.push(ch);
    }
    if !run.is_empty() {
        out.push_str(if run == from { to } else { &run });
    }
    out
}

/// Bump the last decimal digit of `text` by one, modulo ten.
fn bump_last_digit(text: &str) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let position = chars.iter().rposition(char::is_ascii_digit)?;
    let digit = chars.get(position)?.to_digit(10)?;
    let mut out = chars;
    *out.get_mut(position)? = char::from_digit((digit + 1) % 10, 10)?;
    Some(out.into_iter().collect())
}

/// Bump the first number that follows `marker` by one.
fn bump_number_after(text: &str, marker: &str) -> Option<String> {
    let start = text.find(marker)? + marker.len();
    let rest = text.get(start..)?;
    let width = rest.chars().take_while(char::is_ascii_digit).count();
    if width == 0 {
        return None;
    }
    let number: i128 = rest.get(..width)?.parse().ok()?;
    let head = text.get(..start)?;
    let tail = rest.get(width..)?;
    Some(format!("{head}{}{tail}", number + 1))
}

// ---------------------------------------------------------------------------
// The generators of spec section 9.3
// ---------------------------------------------------------------------------

/// What a real learner means by the variant (spec section 9.3).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Intent {
    /// The variant is the same answer, written another way (V4).
    Same,
    /// The variant is a different answer.
    Different,
    /// The variant is the same value under the period-grouping reading (V4).
    Notation,
}

impl Intent {
    /// The name of the intent, for the report.
    const fn name(self) -> &'static str {
        match self {
            Self::Same => "same",
            Self::Different => "different",
            Self::Notation => "notation",
        }
    }
}

/// One learner-notation generator.
struct Generator {
    /// The name of the generator, used in the report and in the pair dump.
    name: &'static str,
    /// What the learner means by the variant.
    intent: Intent,
    /// Build the learner string, or refuse the answer.
    make: fn(&Row) -> Option<String>,
}

/// A variant that changed nothing is no variant.
fn changed(row: &Row, candidate: String) -> Option<String> {
    (candidate != row.answer).then_some(candidate)
}

/// The same rule for a generator that rewrites the parser source.
///
/// A source-based generator starts from the normalized source, so it must
/// compare against that source. Comparing against the raw answer would let it
/// emit the plain rewritten source and claim the rewrite as its own variant.
fn changed_source(row: &Row, candidate: String) -> Option<String> {
    (candidate != row.source).then_some(candidate)
}

fn generate_identity(row: &Row) -> Option<String> {
    Some(row.answer.clone())
}

fn generate_padding(row: &Row) -> Option<String> {
    Some(format!("  {}  ", row.answer))
}

fn generate_trailing_period(row: &Row) -> Option<String> {
    Some(format!("{}.", row.answer))
}

fn generate_dollar_wrapped(row: &Row) -> Option<String> {
    if row.answer.trim().starts_with('$') {
        return None;
    }
    Some(format!("${}$", row.answer))
}

fn generate_comma_space_removed(row: &Row) -> Option<String> {
    changed(row, row.answer.replace(", ", ","))
}

fn generate_plus_spaced(row: &Row) -> Option<String> {
    if !row.answer.contains('+') {
        return None;
    }
    changed(row, row.answer.replace('+', " + "))
}

fn generate_comma_thousands(row: &Row) -> Option<String> {
    thousands_grouped(row, ",")
}

fn generate_space_thousands(row: &Row) -> Option<String> {
    thousands_grouped(row, " ")
}

fn generate_nbsp_thousands(row: &Row) -> Option<String> {
    thousands_grouped(row, "\u{00a0}")
}

fn generate_narrow_space_thousands(row: &Row) -> Option<String> {
    thousands_grouped(row, "\u{202f}")
}

fn generate_thin_space_thousands(row: &Row) -> Option<String> {
    thousands_grouped(row, "\u{2009}")
}

fn generate_figure_space_thousands(row: &Row) -> Option<String> {
    thousands_grouped(row, "\u{2007}")
}

fn generate_dot_thousands(row: &Row) -> Option<String> {
    thousands_grouped(row, ".")
}

fn generate_equivalent_fraction(row: &Row) -> Option<String> {
    let (numerator, denominator) = fraction_parts(&row.source)?;
    let factor = 2 + (row.source.len() % 8) as i128;
    let numerator = numerator.checked_mul(factor)?;
    let denominator = denominator.checked_mul(factor)?;
    Some(format!("{numerator}/{denominator}"))
}

fn generate_fraction_to_decimal(row: &Row) -> Option<String> {
    let (numerator, denominator) = fraction_parts(&row.source)?;
    exact_decimal(numerator, denominator)
}

fn generate_decimal_to_fraction(row: &Row) -> Option<String> {
    let (negative, whole, fraction) = decimal_parts(&row.source)?;
    if fraction.len() > 12 {
        return None;
    }
    let mantissa: i128 = format!("{whole}{fraction}").parse().ok()?;
    let denominator = 10_i128.checked_pow(u32::try_from(fraction.len()).ok()?)?;
    let sign = if negative { "-" } else { "" };
    Some(format!("{sign}{mantissa}/{denominator}"))
}

fn generate_trailing_zero(row: &Row) -> Option<String> {
    if decimal_parts(&row.source).is_some() {
        return Some(format!("{}0", row.source));
    }
    let (_, digits) = integer_digits(&row.source)?;
    if digits.len() > 15 {
        return None;
    }
    Some(format!("{}.0", row.source))
}

fn generate_star_power(row: &Row) -> Option<String> {
    if !row.answer.contains('^') {
        return None;
    }
    changed(row, row.answer.replace('^', "**"))
}

fn generate_caret_power(row: &Row) -> Option<String> {
    if !row.answer.contains("**") {
        return None;
    }
    changed(row, row.answer.replace("**", "^"))
}

/// Put a `*` between every pair of tokens that only touch.
fn generate_explicit_multiplication(row: &Row) -> Option<String> {
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
fn generate_implicit_multiplication(row: &Row) -> Option<String> {
    let chars: Vec<char> = row.source.chars().collect();
    let mut out = String::new();
    for (index, ch) in chars.iter().enumerate() {
        if *ch == '*' {
            let previous = chars.get(index.wrapping_sub(1)).copied().unwrap_or(' ');
            let next = chars.get(index + 1).copied().unwrap_or(' ');
            if previous.is_ascii_digit() && next.is_ascii_alphabetic() {
                continue;
            }
        }
        out.push(*ch);
    }
    changed_source(row, out)
}

fn generate_unicode_to_ascii(row: &Row) -> Option<String> {
    let mut out = radical_to_call(&row.answer);
    for (glyph, ascii) in UNICODE_TO_ASCII {
        out = out.replace(glyph, ascii);
    }
    changed(row, out)
}

fn generate_ascii_to_unicode(row: &Row) -> Option<String> {
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

fn generate_sum_reorder(row: &Row) -> Option<String> {
    let parts = top_level_split(&row.source, '+')?;
    if parts.len() < 2 {
        return None;
    }
    let mut rotated = parts.clone();
    rotated.rotate_left(1);
    changed(row, rotated.join("+"))
}

fn generate_set_reordered(row: &Row) -> Option<String> {
    let mut items = bracket_items(&row.source, '{', '}')?;
    if items.len() < 2 {
        return None;
    }
    items.reverse();
    changed(row, format!("{{{}}}", items.join(", ")))
}

fn generate_last_digit_bumped(row: &Row) -> Option<String> {
    bump_last_digit(&row.source)
}

fn generate_sign_flipped(row: &Row) -> Option<String> {
    if row.source.chars().all(|c| c == '0' || c == '-') {
        return None;
    }
    match row.source.strip_prefix('-') {
        Some(rest) => Some(rest.to_string()),
        None => Some(format!("-{}", row.source)),
    }
}

fn generate_digit_transposition(row: &Row) -> Option<String> {
    let chars: Vec<char> = row.source.chars().collect();
    let mut position = None;
    for index in 0..chars.len().saturating_sub(1) {
        let left = chars.get(index).copied().unwrap_or(' ');
        let right = chars.get(index + 1).copied().unwrap_or(' ');
        if left.is_ascii_digit() && right.is_ascii_digit() && left != right {
            position = Some(index);
        }
    }
    let index = position?;
    let mut out = chars;
    out.swap(index, index + 1);
    Some(out.into_iter().collect())
}

fn generate_times_thousand(row: &Row) -> Option<String> {
    let (negative, digits) = integer_digits(&row.source)?;
    if digits.len() > 12 || digits == "0" {
        return None;
    }
    let sign = if negative { "-" } else { "" };
    Some(format!("{sign}{digits}000"))
}

fn generate_over_thousand(row: &Row) -> Option<String> {
    let (negative, digits) = integer_digits(&row.source)?;
    if digits.len() > 3 || digits == "0" {
        return None;
    }
    let sign = if negative { "-" } else { "" };
    Some(format!("{sign}0.{digits:0>3}"))
}

fn generate_coarse_decimal(row: &Row) -> Option<String> {
    let (numerator, denominator) = fraction_parts(&row.source)?;
    let scaled = numerator.checked_mul(1_000)?;
    if scaled % denominator == 0 {
        return None;
    }
    let rounded =
        (scaled * 2 + denominator.signum() * numerator.signum().abs()) / (denominator * 2);
    let negative = rounded < 0;
    let digits = format!("{:0>4}", rounded.abs());
    let (whole, fraction) = digits.split_at(digits.len() - 3);
    let sign = if negative { "-" } else { "" };
    Some(format!("{sign}{whole}.{fraction}"))
}

fn generate_tuple_swapped(row: &Row) -> Option<String> {
    let mut items = bracket_items(&row.source, '(', ')')?;
    if items.len() < 2 || items.first() == items.get(1) {
        return None;
    }
    items.swap(0, 1);
    Some(format!("({})", items.join(", ")))
}

fn generate_set_element_changed(row: &Row) -> Option<String> {
    let items = bracket_items(&row.source, '{', '}')?;
    let first = items.first()?;
    let bumped = bump_last_digit(first)?;
    if items.contains(&bumped) {
        return None;
    }
    let mut out = items.clone();
    *out.first_mut()? = bumped;
    Some(format!("{{{}}}", out.join(", ")))
}

fn generate_wrong_radicand(row: &Row) -> Option<String> {
    bump_number_after(&row.source, "sqrt(")
}

fn generate_wrong_exponent(row: &Row) -> Option<String> {
    bump_number_after(&row.source, "**")
}

fn generate_appended_junk(row: &Row) -> Option<String> {
    Some(format!("{}x", row.answer))
}

/// Every generator of spec section 9.3, in a fixed order.
///
/// The order fixes the pair order, so the generated set is reproducible.
const GENERATORS: [Generator; 33] = [
    Generator {
        name: "identity",
        intent: Intent::Same,
        make: generate_identity,
    },
    Generator {
        name: "whitespace_padding",
        intent: Intent::Same,
        make: generate_padding,
    },
    Generator {
        name: "trailing_period",
        intent: Intent::Same,
        make: generate_trailing_period,
    },
    Generator {
        name: "dollar_wrapped",
        intent: Intent::Same,
        make: generate_dollar_wrapped,
    },
    Generator {
        name: "comma_space_removed",
        intent: Intent::Same,
        make: generate_comma_space_removed,
    },
    Generator {
        name: "plus_spaced",
        intent: Intent::Same,
        make: generate_plus_spaced,
    },
    Generator {
        name: "comma_thousands",
        intent: Intent::Same,
        make: generate_comma_thousands,
    },
    Generator {
        name: "space_thousands",
        intent: Intent::Same,
        make: generate_space_thousands,
    },
    Generator {
        name: "nbsp_thousands",
        intent: Intent::Same,
        make: generate_nbsp_thousands,
    },
    Generator {
        name: "narrow_space_thousands",
        intent: Intent::Same,
        make: generate_narrow_space_thousands,
    },
    Generator {
        name: "thin_space_thousands",
        intent: Intent::Same,
        make: generate_thin_space_thousands,
    },
    Generator {
        name: "figure_space_thousands",
        intent: Intent::Same,
        make: generate_figure_space_thousands,
    },
    Generator {
        name: "dot_thousands",
        intent: Intent::Notation,
        make: generate_dot_thousands,
    },
    Generator {
        name: "equivalent_fraction",
        intent: Intent::Same,
        make: generate_equivalent_fraction,
    },
    Generator {
        name: "fraction_to_decimal",
        intent: Intent::Same,
        make: generate_fraction_to_decimal,
    },
    Generator {
        name: "decimal_to_fraction",
        intent: Intent::Same,
        make: generate_decimal_to_fraction,
    },
    Generator {
        name: "trailing_zero",
        intent: Intent::Same,
        make: generate_trailing_zero,
    },
    Generator {
        name: "star_power",
        intent: Intent::Same,
        make: generate_star_power,
    },
    Generator {
        name: "caret_power",
        intent: Intent::Same,
        make: generate_caret_power,
    },
    Generator {
        name: "explicit_multiplication",
        intent: Intent::Same,
        make: generate_explicit_multiplication,
    },
    Generator {
        name: "implicit_multiplication",
        intent: Intent::Same,
        make: generate_implicit_multiplication,
    },
    Generator {
        name: "unicode_to_ascii",
        intent: Intent::Same,
        make: generate_unicode_to_ascii,
    },
    Generator {
        name: "ascii_to_unicode",
        intent: Intent::Same,
        make: generate_ascii_to_unicode,
    },
    Generator {
        name: "sum_reorder",
        intent: Intent::Same,
        make: generate_sum_reorder,
    },
    Generator {
        name: "set_reordered",
        intent: Intent::Same,
        make: generate_set_reordered,
    },
    Generator {
        name: "last_digit_bumped",
        intent: Intent::Different,
        make: generate_last_digit_bumped,
    },
    Generator {
        name: "sign_flipped",
        intent: Intent::Different,
        make: generate_sign_flipped,
    },
    Generator {
        name: "digit_transposition",
        intent: Intent::Different,
        make: generate_digit_transposition,
    },
    Generator {
        name: "times_thousand",
        intent: Intent::Different,
        make: generate_times_thousand,
    },
    Generator {
        name: "over_thousand",
        intent: Intent::Different,
        make: generate_over_thousand,
    },
    Generator {
        name: "coarse_decimal",
        intent: Intent::Different,
        make: generate_coarse_decimal,
    },
    Generator {
        name: "tuple_swapped",
        intent: Intent::Different,
        make: generate_tuple_swapped,
    },
    Generator {
        name: "set_element_changed",
        intent: Intent::Different,
        make: generate_set_element_changed,
    },
];

/// The two generators that hunt a wrong radicand and a wrong exponent, and the
/// junk generator. They live apart from [`GENERATORS`] only because the array
/// length is a literal; the pair builder runs both arrays in order.
const MORE_GENERATORS: [Generator; 3] = [
    Generator {
        name: "wrong_radicand",
        intent: Intent::Different,
        make: generate_wrong_radicand,
    },
    Generator {
        name: "wrong_exponent",
        intent: Intent::Different,
        make: generate_wrong_exponent,
    },
    Generator {
        name: "appended_junk",
        intent: Intent::Different,
        make: generate_appended_junk,
    },
];

// ---------------------------------------------------------------------------
// The generated pair set
// ---------------------------------------------------------------------------

/// One generated pair.
#[derive(Clone)]
struct Pair {
    generator: &'static str,
    intent: Intent,
    expected: String,
    learner: String,
    kind: AnswerKind,
    shape: String,
}

/// A deterministic xorshift generator. The selection needs no crate for this.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut state = self.0;
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        self.0 = state;
        state
    }
}

/// Build the generated pair set.
///
/// The set is deduplicated by (expected, learner, kind), because the corpus
/// repeats an answer and two generators can meet on one variant. When the set is
/// larger than [`PAIR_CAP`], a seeded shuffle picks the survivors and the
/// survivors go back into generation order.
fn generated_pairs() -> Vec<Pair> {
    let rows = in_grammar_rows();
    let mut seen: BTreeSet<(String, String, &'static str)> = BTreeSet::new();
    let mut pairs: Vec<Pair> = Vec::new();
    for row in &rows {
        for generator in GENERATORS.iter().chain(MORE_GENERATORS.iter()) {
            let Some(learner) = (generator.make)(row) else {
                continue;
            };
            let key = (row.answer.clone(), learner.clone(), row.kind.as_str());
            if !seen.insert(key) {
                continue;
            }
            pairs.push(Pair {
                generator: generator.name,
                intent: generator.intent,
                expected: row.answer.clone(),
                learner,
                kind: row.kind,
                shape: row.shape.clone(),
            });
        }
    }
    if pairs.len() <= PAIR_CAP {
        return pairs;
    }
    let mut order: Vec<usize> = (0..pairs.len()).collect();
    let mut rng = Rng(SHUFFLE_SEED);
    for index in (1..order.len()).rev() {
        let swap = (rng.next() % (index as u64 + 1)) as usize;
        order.swap(index, swap);
    }
    order.truncate(PAIR_CAP);
    order.sort_unstable();
    order
        .into_iter()
        .filter_map(|index| pairs.get(index).cloned())
        .collect()
}

// ---------------------------------------------------------------------------
// The recorded 1.0 verdicts
// ---------------------------------------------------------------------------

/// One line of `oracle_verdicts_1_0.jsonl`.
#[derive(serde::Deserialize)]
struct OracleLine {
    expected: String,
    learner: String,
    kind: String,
    equivalent: Option<bool>,
    notation: Option<bool>,
    timeout: bool,
}

/// The 1.0 verdict on one pair.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct OracleVerdict {
    equivalent: bool,
    notation: bool,
}

/// The key of one pair in the verdict map.
type PairKey = (String, String, String);

/// Read the committed 1.0 verdicts, keyed by (expected, learner, kind).
fn committed_verdicts() -> BTreeMap<PairKey, Option<OracleVerdict>> {
    let path = fixture("oracle_verdicts_1_0.jsonl");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mut map = BTreeMap::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let row: OracleLine =
            serde_json::from_str(line).unwrap_or_else(|e| panic!("row {line}: {e}"));
        let verdict = if row.timeout {
            None
        } else {
            Some(OracleVerdict {
                equivalent: row.equivalent.unwrap_or_else(|| {
                    panic!("a recorded verdict with no timeout must carry `equivalent`: {line}")
                }),
                notation: row.notation.unwrap_or_else(|| {
                    panic!("a recorded verdict with no timeout must carry `notation`: {line}")
                }),
            })
        };
        map.insert((row.expected, row.learner, row.kind), verdict);
    }
    map
}

// ---------------------------------------------------------------------------
// Classification
// ---------------------------------------------------------------------------

/// The divergence class of one pair (spec section 9.3).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Class {
    /// Class 1 — outside the grammar, so 2.0 refuses a verdict (V2).
    OutsideGrammar,
    /// Class 2 — the authored answer is prose, so the topic is mis-kinded (V2).
    ProseExpected,
    /// Class 3 — both sides decidable; a difference here is a 2.0 bug (R5).
    Comparable,
    /// Class 4 — a documented, intentional 2.0 divergence.
    DocumentedDivergence,
    /// The 1.0 oracle timed out, so it has no opinion (spec section 9.1).
    OracleSilent,
}

impl Class {
    const fn name(self) -> &'static str {
        match self {
            Self::OutsideGrammar => "class 1 outside_grammar",
            Self::ProseExpected => "class 2 prose_expected",
            Self::Comparable => "class 3 comparable",
            Self::DocumentedDivergence => "class 4 documented_divergence",
            Self::OracleSilent => "oracle_silent",
        }
    }
}

/// The documented 2.0 divergences.
///
/// Every one of them is a decision of `docs/plans/M2.md` or a 1.0 defect that
/// `docs/reference/checker-1.0-spec.md` names, and every one of them has a pinned
/// test in `crates/core/tests/answer_divergence.rs`. A pair that leaves 1.0 for
/// any other reason stays in class 3 and fails the parity assertion.
const DOCUMENTED_REASONS: [&str; 9] = [
    // The four the M2 plan names. The first one is a 2.0 decision; the other
    // three are 1.0 defects that 2.0 refuses to reproduce.
    "no float tolerance rung (D6)",
    "a transcendental identity is not simplified (V1)",
    "prose is not a value (V2)",
    "a SymPy name is not a value (V2)",
    // Four more 1.0 defects that this generated set reaches. Every one of them
    // marks a correct learner WRONG in 1.0, and 2.0 decides it correctly.
    "the 1.0 exponent-tower guard refuses a legal power (spec 5.1)",
    "the 1.0 tokenizer reads a Python number literal (spec 3.1)",
    "the 1.0 radical rewrite misses a nested group (spec 2.2)",
    "a chained inequality raises inside 1.0 (spec 7.7)",
    "the 1.0 rewriter deletes a backslash and leaves a brace group (spec 7.7)",
];

/// Whether 1.0 refuses `source` for a tower of powers (1.0 `_POW_TOWER_RE`).
///
/// The predicate is a literal port of `sympy_check.py:215`,
/// `r"\*\*\s*[^*+\-/()\s]+\s*\*\*"`, applied to the string 1.0 evaluates. That
/// guard is the reason 1.0 refuses the legal answer `36x**2y**2`
/// (spec section 5.1).
fn nineteen_zero_reads_a_power_tower(text: &str) -> bool {
    let source = text.replace('^', "**");
    let chars: Vec<char> = source.chars().collect();
    let mut index = 0;
    while index + 1 < chars.len() {
        if chars.get(index) != Some(&'*') || chars.get(index + 1) != Some(&'*') {
            index += 1;
            continue;
        }
        let mut scan = index + 2;
        while matches!(chars.get(scan), Some(c) if c.is_whitespace()) {
            scan += 1;
        }
        let body_start = scan;
        while matches!(chars.get(scan), Some(c)
            if !matches!(c, '*' | '+' | '-' | '/' | '(' | ')') && !c.is_whitespace())
        {
            scan += 1;
        }
        if scan == body_start {
            index += 1;
            continue;
        }
        while matches!(chars.get(scan), Some(c) if c.is_whitespace()) {
            scan += 1;
        }
        if chars.get(scan) == Some(&'*') && chars.get(scan + 1) == Some(&'*') {
            return true;
        }
        index += 1;
    }
    false
}

/// Whether the Python tokenizer of 1.0 reads part of `text` as a number literal.
///
/// `parse_expr` runs the CPython tokenizer, so `2j` is the imaginary literal
/// `2*I` and `0x` is an invalid hexadecimal literal. Measured on 2026-08-26:
/// `_parse("3i - 2j")` gives `3*i - 2*I`, and `_parse("0x")` raises
/// `TokenError: invalid hexadecimal literal`. 2.0 runs no Python tokenizer, so
/// `2j` is `2*j` and `0x` is `0*x` (spec section 3.1).
fn nineteen_zero_reads_a_python_literal(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    for (index, ch) in chars.iter().enumerate() {
        if !ch.is_ascii_digit() {
            continue;
        }
        let next = chars.get(index + 1).copied().unwrap_or(' ');
        // An imaginary literal: a digit run that ends in `j`.
        if matches!(next, 'j' | 'J')
            && !matches!(chars.get(index + 2), Some(c) if c.is_alphanumeric() || *c == '_')
        {
            return true;
        }
        // A radix prefix: `0x`, `0o`, or `0b`, with no digit in front of it.
        if *ch == '0'
            && matches!(next, 'x' | 'X' | 'o' | 'O' | 'b' | 'B')
            && !matches!(chars.get(index.wrapping_sub(1)), Some(c) if c.is_ascii_digit())
        {
            return true;
        }
    }
    false
}

/// Whether `text` puts a Unicode radical over a group that nests parentheses.
///
/// 1.0 rewrites the radical with `re.sub(r"√\s*\(([^()]*)\)", ...)`
/// (`sympy_check.py:153`). The character class refuses a nested group, so
/// `√(1 + sin(x)^2)` keeps its `√` and reaches SymPy as a bare symbol
/// (spec section 2.2). 2.0 parses the radical with the grammar, so it nests.
fn a_unicode_radical_over_a_nested_group(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    for (index, ch) in chars.iter().enumerate() {
        if *ch != '√' {
            continue;
        }
        let mut scan = index + 1;
        while matches!(chars.get(scan), Some(' ')) {
            scan += 1;
        }
        if chars.get(scan) != Some(&'(') {
            continue;
        }
        let mut depth = 0_i32;
        while let Some(inner) = chars.get(scan) {
            match inner {
                '(' => {
                    depth += 1;
                    if depth > 1 {
                        return true;
                    }
                }
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            scan += 1;
        }
    }
    false
}

/// Whether 1.0 hands SymPy a LaTeX brace group.
///
/// `to_sympy_source` deletes every remaining backslash (`sympy_check.py:96`) and
/// keeps the braces, so `\sqrt{13}` reaches SymPy as `sqrt{13}` and `x^{2}` as
/// `x**{2}`. Both are syntax errors (spec sections 2.2 and 7.7). The predicate
/// looks for a `{` that a name, a caret, or a star touches; a set literal such as
/// `$\{1, 3, 5\}$` has a backslash in front of its brace and parses in 1.0.
fn nineteen_zero_leaves_a_brace_group(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    for (index, ch) in chars.iter().enumerate() {
        if *ch != '{' {
            continue;
        }
        let mut back = index;
        while back > 0 && matches!(chars.get(back - 1), Some(' ')) {
            back -= 1;
        }
        let previous = match back.checked_sub(1).and_then(|i| chars.get(i)) {
            Some(previous) => *previous,
            None => continue,
        };
        if previous.is_alphanumeric() || matches!(previous, '_' | '^' | '*') {
            return true;
        }
    }
    false
}

/// Whether `text` is a chained inequality: a variable between two bounds.
///
/// 1.0 evaluates `-1 <= x <= 3` as a Python `and` of two relationals, and
/// `Relational.__bool__` raises `TypeError` (spec sections 5.1 and 7.7). The
/// answer is therefore unreachable for the 1.0 checker. 2.0 reads it as a range.
fn a_chained_inequality(text: &str) -> bool {
    matches!(
        canonical_form(text),
        Ok(Canon::Interval {
            var: Some(_),
            lo: Some(_),
            hi: Some(_),
            ..
        })
    )
}

/// Whether both sides carry a value with no free symbol.
///
/// That is the precondition of the 1.0 float rung: `_sympy_equivalent` compares
/// `evalf()` results at a 1e-6 relative tolerance when neither side has a free
/// symbol (`sympy_check.py:345-352`), and `_numeric_equal` compares plain floats
/// at 1e-9 (`sympy_check.py:172-174`). 2.0 holds exact values only (D6), so two
/// near numbers are two different answers.
fn both_sides_are_numbers(pair: &Pair) -> bool {
    let numeric = |text: &str| {
        matches!(
            canonical_form(text),
            Ok(Canon::Rational(_) | Canon::Radical(_))
        )
    };
    numeric(&pair.expected) && numeric(&pair.learner)
}

/// Whether either side names a transcendental function.
fn names_a_transcendental(pair: &Pair) -> bool {
    const NAMES: [&str; 12] = [
        "sin", "cos", "tan", "sec", "csc", "cot", "sinh", "cosh", "tanh", "exp", "ln", "log",
    ];
    NAMES.iter().any(|name| {
        normalize(&pair.expected).source.contains(name)
            || normalize(&pair.learner).source.contains(name)
    })
}

/// Name the documented reason a pair diverges, when one covers it.
///
/// The order is fixed, so one pair gets one reason. A pair that no predicate
/// covers stays in class 3 and fails the parity assertion (R5).
fn documented_reason(
    pair: &Pair,
    rust_correct: bool,
    oracle: OracleVerdict,
) -> Option<&'static str> {
    if pair.shape == "prose_or_words" {
        return Some("prose is not a value (V2)");
    }
    // 2.0 says no where 1.0 said yes.
    if oracle.equivalent && !rust_correct {
        if both_sides_are_numbers(pair) {
            return Some("no float tolerance rung (D6)");
        }
        if names_a_transcendental(pair) {
            return Some("a transcendental identity is not simplified (V1)");
        }
        return None;
    }
    // 2.0 says yes where 1.0 said no, because a 1.0 stage refused the answer.
    if !oracle.equivalent && rust_correct {
        if nineteen_zero_reads_a_power_tower(&pair.expected)
            || nineteen_zero_reads_a_power_tower(&pair.learner)
        {
            return Some("the 1.0 exponent-tower guard refuses a legal power (spec 5.1)");
        }
        if nineteen_zero_reads_a_python_literal(&pair.expected)
            || nineteen_zero_reads_a_python_literal(&pair.learner)
        {
            return Some("the 1.0 tokenizer reads a Python number literal (spec 3.1)");
        }
        if a_unicode_radical_over_a_nested_group(&pair.expected)
            || a_unicode_radical_over_a_nested_group(&pair.learner)
        {
            return Some("the 1.0 radical rewrite misses a nested group (spec 2.2)");
        }
        if a_chained_inequality(&pair.expected) {
            return Some("a chained inequality raises inside 1.0 (spec 7.7)");
        }
        if nineteen_zero_leaves_a_brace_group(&pair.expected)
            || nineteen_zero_leaves_a_brace_group(&pair.learner)
        {
            return Some(
                "the 1.0 rewriter deletes a backslash and leaves a brace group (spec 7.7)",
            );
        }
    }
    None
}

/// One classified pair.
struct Classified {
    class: Class,
    reason: Option<&'static str>,
    agreed: bool,
}

/// Classify one pair against its recorded 1.0 verdict.
fn classify(pair: &Pair, oracle: Option<OracleVerdict>) -> Classified {
    let Some(oracle) = oracle else {
        return Classified {
            class: Class::OracleSilent,
            reason: None,
            agreed: true,
        };
    };
    if pair.shape == "prose_or_words" {
        return Classified {
            class: Class::ProseExpected,
            reason: Some("prose is not a value (V2)"),
            agreed: true,
        };
    }
    let outcome = check(&pair.expected, &pair.learner, pair.kind);
    let verdict = match outcome {
        Outcome::Undecidable(_) => {
            return Classified {
                class: Class::OutsideGrammar,
                reason: None,
                agreed: true,
            };
        }
        Outcome::Decided(verdict) => verdict,
    };
    let agreed = verdict.correct == oracle.equivalent && verdict.notation == oracle.notation;
    if agreed {
        return Classified {
            class: Class::Comparable,
            reason: None,
            agreed: true,
        };
    }
    match documented_reason(pair, verdict.correct, oracle) {
        Some(reason) => Classified {
            class: Class::DocumentedDivergence,
            reason: Some(reason),
            agreed: true,
        },
        None => Classified {
            class: Class::Comparable,
            reason: None,
            agreed: false,
        },
    }
}

// ---------------------------------------------------------------------------
// The report
// ---------------------------------------------------------------------------

/// The counts the acceptance check quotes.
struct Report {
    pairs: usize,
    per_generator: BTreeMap<&'static str, usize>,
    per_intent: BTreeMap<&'static str, usize>,
    per_class: BTreeMap<&'static str, usize>,
    per_reason: BTreeMap<&'static str, usize>,
    comparable: usize,
    comparable_agreed: usize,
    disagreements: Vec<String>,
}

/// Run every generated pair against its recorded verdict and collect the counts.
fn build_report(pairs: &[Pair], verdicts: &BTreeMap<PairKey, Option<OracleVerdict>>) -> Report {
    let mut report = Report {
        pairs: pairs.len(),
        per_generator: BTreeMap::new(),
        per_intent: BTreeMap::new(),
        per_class: BTreeMap::new(),
        per_reason: BTreeMap::new(),
        comparable: 0,
        comparable_agreed: 0,
        disagreements: Vec::new(),
    };
    for pair in pairs {
        *report.per_generator.entry(pair.generator).or_insert(0) += 1;
        *report.per_intent.entry(pair.intent.name()).or_insert(0) += 1;
        let key = (
            pair.expected.clone(),
            pair.learner.clone(),
            pair.kind.as_str().to_string(),
        );
        let oracle = verdicts.get(&key).copied().unwrap_or_else(|| {
            panic!(
                "the committed oracle file has no verdict for {:?} against {:?} on {}",
                pair.expected, pair.learner, pair.kind
            )
        });
        let classified = classify(pair, oracle);
        *report.per_class.entry(classified.class.name()).or_insert(0) += 1;
        if let Some(reason) = classified.reason {
            *report.per_reason.entry(reason).or_insert(0) += 1;
        }
        if classified.class == Class::Comparable {
            report.comparable += 1;
            if classified.agreed {
                report.comparable_agreed += 1;
            } else {
                let outcome = check(&pair.expected, &pair.learner, pair.kind);
                report.disagreements.push(format!(
                    "{}: {:?} against {:?} on {} -> 2.0 {:?}, 1.0 {:?}",
                    pair.generator, pair.expected, pair.learner, pair.kind, outcome, oracle
                ));
            }
        }
    }
    report
}

/// Print the report the acceptance check quotes.
fn print_report(report: &Report) {
    println!("pairs: {}", report.pairs);
    println!("-- per generator --");
    for (name, count) in &report.per_generator {
        println!("{name}: {count}");
    }
    println!("-- per intent --");
    for (name, count) in &report.per_intent {
        println!("{name}: {count}");
    }
    println!("-- per class --");
    for (name, count) in &report.per_class {
        println!("{name}: {count}");
    }
    println!("-- per documented reason --");
    for (name, count) in &report.per_reason {
        println!("{name}: {count}");
    }
    let percent = if report.comparable == 0 {
        100.0
    } else {
        100.0 * report.comparable_agreed as f64 / report.comparable as f64
    };
    println!(
        "class 3 agreement: {}/{} = {percent:.4}%",
        report.comparable_agreed, report.comparable
    );
    for line in report.disagreements.iter().take(40) {
        println!("DISAGREE {line}");
    }
}

// ---------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------

/// The literal size of the generated set.
const GENERATED_PAIRS: usize = 14_875;

/// The literal pair count of every generator, in name order.
const GENERATOR_COUNTS: [(&str, usize); 36] = [
    ("appended_junk", 1549),
    ("ascii_to_unicode", 70),
    ("caret_power", 0),
    ("coarse_decimal", 92),
    ("comma_space_removed", 197),
    ("comma_thousands", 44),
    ("decimal_to_fraction", 105),
    ("digit_transposition", 601),
    ("dollar_wrapped", 1291),
    ("dot_thousands", 44),
    ("equivalent_fraction", 165),
    ("explicit_multiplication", 399),
    ("figure_space_thousands", 44),
    ("fraction_to_decimal", 79),
    ("identity", 1549),
    ("implicit_multiplication", 59),
    ("last_digit_bumped", 1508),
    ("narrow_space_thousands", 44),
    ("nbsp_thousands", 44),
    ("over_thousand", 256),
    ("plus_spaced", 332),
    ("set_element_changed", 9),
    ("set_reordered", 11),
    ("sign_flipped", 1546),
    ("space_thousands", 44),
    ("star_power", 326),
    ("sum_reorder", 203),
    ("thin_space_thousands", 44),
    ("times_thousand", 300),
    ("trailing_period", 1549),
    ("trailing_zero", 408),
    ("tuple_swapped", 151),
    ("unicode_to_ascii", 60),
    ("whitespace_padding", 1549),
    ("wrong_exponent", 170),
    ("wrong_radicand", 33),
];

/// The literal pair count of every divergence class.
const CLASS_COUNTS: [(&str, usize); 5] = [
    ("class 1 outside_grammar", 935),
    ("class 2 prose_expected", 0),
    ("class 3 comparable", 13924),
    ("class 4 documented_divergence", 16),
    ("oracle_silent", 0),
];

/// The literal pair count of every documented divergence reason.
///
/// The first four reasons are the ones `docs/plans/M2.md` names. This generated
/// set reaches none of them except the float rung: prose never enters the set
/// (the set holds only in-grammar answers), and a SymPy name such as `zoo`
/// leaves 2.0 undecidable, which is class 1. Both stay pinned by literal pairs
/// in `crates/core/tests/answer_divergence.rs`.
const REASON_COUNTS: [(&str, usize); 9] = [
    ("no float tolerance rung (D6)", 2),
    ("a transcendental identity is not simplified (V1)", 0),
    ("prose is not a value (V2)", 0),
    ("a SymPy name is not a value (V2)", 0),
    (
        "the 1.0 exponent-tower guard refuses a legal power (spec 5.1)",
        2,
    ),
    (
        "the 1.0 tokenizer reads a Python number literal (spec 3.1)",
        6,
    ),
    (
        "the 1.0 radical rewrite misses a nested group (spec 2.2)",
        1,
    ),
    ("a chained inequality raises inside 1.0 (spec 7.7)", 4),
    (
        "the 1.0 rewriter deletes a backslash and leaves a brace group (spec 7.7)",
        1,
    ),
];

#[test]
fn the_generated_set_is_deterministic_and_capped() {
    let first = generated_pairs();
    let second = generated_pairs();
    assert_eq!(first.len(), second.len(), "the pair count moved");
    for (left, right) in first.iter().zip(second.iter()) {
        assert_eq!(left.expected, right.expected);
        assert_eq!(left.learner, right.learner);
        assert_eq!(left.generator, right.generator);
    }
    assert!(
        first.len() <= PAIR_CAP,
        "the generated set holds {} pairs, and the cap is {PAIR_CAP}",
        first.len()
    );
    assert_eq!(first.len(), GENERATED_PAIRS, "the generated pair count");
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for pair in &first {
        *counts.entry(pair.generator).or_insert(0) += 1;
    }
    for (name, want) in GENERATOR_COUNTS {
        let found = counts.get(name).copied().unwrap_or(0);
        assert_eq!(found, want, "{name}: pair count");
    }
    for name in counts.keys() {
        assert!(
            GENERATOR_COUNTS.iter().any(|(known, _)| known == name),
            "{name} is a generator the count table does not list"
        );
    }
}

#[test]
fn dump_the_generated_pairs_when_asked() {
    let Ok(path) = std::env::var("CADUS_ORACLE_DUMP") else {
        return;
    };
    let mut out = String::new();
    for pair in generated_pairs() {
        let line = serde_json::json!({
            "expected": pair.expected,
            "learner": pair.learner,
            "kind": pair.kind.as_str(),
            "generator": pair.generator,
            "intent": pair.intent.name(),
        });
        let _ = writeln!(out, "{line}");
    }
    std::fs::write(&path, out).unwrap_or_else(|e| panic!("write {path}: {e}"));
    println!("wrote the generated pairs to {path}");
}

#[test]
fn the_two_checkers_agree_on_every_comparable_pair() {
    let pairs = generated_pairs();
    let verdicts = committed_verdicts();
    let report = build_report(&pairs, &verdicts);
    print_report(&report);
    for (name, want) in CLASS_COUNTS {
        let found = report.per_class.get(name).copied().unwrap_or(0);
        assert_eq!(found, want, "{name}: pair count");
    }
    for (name, want) in REASON_COUNTS {
        let found = report.per_reason.get(name).copied().unwrap_or(0);
        assert_eq!(found, want, "{name}: pair count");
    }
    assert!(
        report.disagreements.is_empty(),
        "R5: {} class 3 pairs disagree with the 1.0 oracle:\n{}",
        report.disagreements.len(),
        report.disagreements.join("\n")
    );
    assert_eq!(
        report.comparable_agreed, report.comparable,
        "class 3 agreement must be 100%"
    );
}

#[test]
fn every_documented_reason_is_one_of_the_named_nine() {
    for (name, _) in REASON_COUNTS {
        assert!(
            DOCUMENTED_REASONS.contains(&name),
            "{name} is not a documented divergence"
        );
    }
    assert_eq!(DOCUMENTED_REASONS.len(), REASON_COUNTS.len());
}

#[test]
fn the_committed_verdict_file_covers_the_generated_set_exactly() {
    let pairs = generated_pairs();
    let verdicts = committed_verdicts();
    let mut wanted: BTreeSet<PairKey> = BTreeSet::new();
    for pair in &pairs {
        wanted.insert((
            pair.expected.clone(),
            pair.learner.clone(),
            pair.kind.as_str().to_string(),
        ));
    }
    let recorded: BTreeSet<PairKey> = verdicts.keys().cloned().collect();
    let missing: Vec<&PairKey> = wanted.difference(&recorded).take(10).collect();
    let extra: Vec<&PairKey> = recorded.difference(&wanted).take(10).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "the recorded set moved: missing {missing:?}, extra {extra:?}"
    );
    assert_eq!(recorded.len(), wanted.len());
}

// ---------------------------------------------------------------------------
// The live oracle
// ---------------------------------------------------------------------------

/// Ask the live 1.0 checker for every verdict of the generated set.
fn live_verdicts(python: &str, pairs: &[Pair]) -> Vec<Option<OracleVerdict>> {
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Command, Stdio};

    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/oracle/check_1_0.py")
        .canonicalize()
        .unwrap_or_else(|e| panic!("find scripts/oracle/check_1_0.py: {e}"));
    let mut child = Command::new(python)
        .arg(&script)
        .arg("--timeout")
        .arg("2.0")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("start {}: {e}", script.display()));
    let mut stdin = child.stdin.take().unwrap_or_else(|| panic!("no stdin"));
    let requests: Vec<String> = pairs
        .iter()
        .map(|pair| {
            serde_json::json!({
                "expected": pair.expected,
                "learner": pair.learner,
                "kind": pair.kind.as_str(),
            })
            .to_string()
        })
        .collect();
    let writer = std::thread::spawn(move || {
        for line in requests {
            if writeln!(stdin, "{line}").is_err() {
                return;
            }
        }
        drop(stdin);
    });
    let stdout = child.stdout.take().unwrap_or_else(|| panic!("no stdout"));
    let mut reader = BufReader::new(stdout);
    let mut ready = String::new();
    reader
        .read_line(&mut ready)
        .unwrap_or_else(|e| panic!("read the ready line: {e}"));
    assert!(ready.contains("\"ready\""), "the oracle said {ready:?}");
    let mut out = Vec::with_capacity(pairs.len());
    for line in reader.lines() {
        let line = line.unwrap_or_else(|e| panic!("read a verdict: {e}"));
        let row: serde_json::Value =
            serde_json::from_str(&line).unwrap_or_else(|e| panic!("verdict {line}: {e}"));
        assert!(row.get("error").is_none(), "the oracle said {line}");
        if row.get("timeout").and_then(serde_json::Value::as_bool) == Some(true) {
            out.push(None);
        } else {
            out.push(Some(OracleVerdict {
                equivalent: row
                    .get("equivalent")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or_else(|| panic!("verdict {line} has no `equivalent`")),
                notation: row
                    .get("notation")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or_else(|| panic!("verdict {line} has no `notation`")),
            }));
        }
    }
    let _ = writer.join();
    let _ = child.wait();
    out
}

/// Prove the committed file still says what the live 1.0 checker says.
///
/// The test needs the 1.0 interpreter, which only this build box has, so it runs
/// when `CADUS_ORACLE_PYTHON` names that interpreter and it skips otherwise. The
/// gate therefore always compares against the committed file, and a person who
/// has 1.0 checks the file itself with one environment variable.
#[test]
fn the_live_oracle_reproduces_the_committed_verdicts() {
    let Ok(python) = std::env::var("CADUS_ORACLE_PYTHON") else {
        println!("skipped: set CADUS_ORACLE_PYTHON to run against the live 1.0 checker");
        return;
    };
    let pairs = generated_pairs();
    let live = live_verdicts(&python, &pairs);
    assert_eq!(live.len(), pairs.len(), "the oracle answered every pair");
    let committed = committed_verdicts();
    let mut moved = Vec::new();
    for (pair, live) in pairs.iter().zip(live.iter()) {
        let key = (
            pair.expected.clone(),
            pair.learner.clone(),
            pair.kind.as_str().to_string(),
        );
        let recorded = committed.get(&key).copied().unwrap_or_else(|| {
            panic!("the committed file has no verdict for {key:?}");
        });
        if recorded != *live {
            moved.push(format!("{key:?}: recorded {recorded:?}, live {live:?}"));
        }
    }
    assert!(
        moved.is_empty(),
        "the live 1.0 checker no longer matches the committed file:\n{}",
        moved.join("\n")
    );
    println!("the live 1.0 checker reproduced {} verdicts", pairs.len());
}

// ---------------------------------------------------------------------------
// The residue: the corpus answers the grammar refuses (V2)
// ---------------------------------------------------------------------------

/// One corpus line, with the fields the residue report needs.
#[derive(serde::Deserialize)]
struct ResidueLine {
    answer: String,
    answer_kind: String,
    shape: String,
    topic_id: String,
    kp_id: String,
    exemplar_index: i64,
}

/// Write the residue of `docs/reference/undecidable-answers.md`, when asked.
///
/// The dump carries the shape bucket, the topic, and the refusal reason of the
/// 2.0 grammar, so the owner and M6 authoring read one file per group.
#[test]
fn dump_the_undecidable_residue_when_asked() {
    let Ok(path) = std::env::var("CADUS_RESIDUE_DUMP") else {
        return;
    };
    let corpus = fixture("corpus_1_0.jsonl");
    let text = std::fs::read_to_string(&corpus)
        .unwrap_or_else(|e| panic!("read {}: {e}", corpus.display()));
    let mut out = String::new();
    let mut count = 0_usize;
    for line in text.lines() {
        let row: ResidueLine =
            serde_json::from_str(line).unwrap_or_else(|e| panic!("row {line}: {e}"));
        let source = normalize(&row.answer).source;
        let Err(refusal) = parse(&source) else {
            continue;
        };
        count += 1;
        let record = serde_json::json!({
            "answer": row.answer,
            "answer_kind": row.answer_kind,
            "shape": row.shape,
            "topic_id": row.topic_id,
            "kp_id": row.kp_id,
            "exemplar_index": row.exemplar_index,
            "source": source,
            "reason": refusal.reason,
        });
        let _ = writeln!(out, "{record}");
    }
    std::fs::write(&path, out).unwrap_or_else(|e| panic!("write {path}: {e}"));
    println!("wrote {count} refused answers to {path}");
}

// ---------------------------------------------------------------------------
// The token-soup fuzz (V3)
// ---------------------------------------------------------------------------

/// Every token the corpus answers use, in sorted order.
///
/// A token is one run of alphanumeric characters or one other character. The
/// soup therefore reaches deeper into the grammar than a random byte string
/// does: it holds real function names, real Unicode glyphs, and real brackets.
fn corpus_vocabulary() -> Vec<String> {
    let path = fixture("corpus_1_0.jsonl");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mut tokens: BTreeSet<String> = BTreeSet::new();
    for line in text.lines() {
        let row: CorpusLine =
            serde_json::from_str(line).unwrap_or_else(|e| panic!("row {line}: {e}"));
        let mut run = String::new();
        for ch in row.answer.chars() {
            if ch.is_alphanumeric() {
                run.push(ch);
                continue;
            }
            if !run.is_empty() {
                tokens.insert(std::mem::take(&mut run));
            }
            tokens.insert(ch.to_string());
        }
        if !run.is_empty() {
            tokens.insert(run);
        }
    }
    tokens.into_iter().collect()
}

/// Build one random answer from the corpus vocabulary.
fn token_soup(rng: &mut Rng, vocabulary: &[String]) -> String {
    let length = 1 + (rng.next() % 24) as usize;
    let mut out = String::new();
    for _ in 0..length {
        let index = (rng.next() as usize) % vocabulary.len();
        out.push_str(vocabulary.get(index).map_or("0", String::as_str));
        if rng.next().is_multiple_of(4) {
            out.push(' ');
        }
    }
    out
}

#[test]
fn ten_seconds_of_corpus_token_soup_never_panics() {
    let vocabulary = corpus_vocabulary();
    assert!(
        vocabulary.len() > 200,
        "the corpus vocabulary holds {} tokens",
        vocabulary.len()
    );
    let mut rng = Rng(0x2026_0826_7f13_0091);
    let start = std::time::Instant::now();
    let mut cases = 0_u64;
    while start.elapsed() < std::time::Duration::from_secs(10) {
        for _ in 0..64 {
            let expected = token_soup(&mut rng, &vocabulary);
            let learner = token_soup(&mut rng, &vocabulary);
            let kind = if rng.next().is_multiple_of(2) {
                AnswerKind::Numeric
            } else {
                AnswerKind::Expression
            };
            let probe = (expected.clone(), learner.clone(), kind);
            let result = std::panic::catch_unwind(move || check(&probe.0, &probe.1, probe.2));
            assert!(
                result.is_ok(),
                "the checker panicked on {expected:?} against {learner:?} on {kind}"
            );
            cases += 1;
        }
    }
    assert!(cases > 1_000, "the soup fuzz ran only {cases} cases");
    println!("the token-soup fuzz ran {cases} cases");
}
