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
//! `scripts/oracle/check_1_0.py`. The test always compares against that file.
//!
//! To dump the generated pairs:
//!
//! ```text
//! CADUS_ORACLE_DUMP=/tmp/pairs.jsonl cargo test -p cadus-core --test answer_oracle
//! ```
//!
//! To prove the file still matches the live 1.0 checker, set
//! `CADUS_ORACLE_PYTHON` and run the file:
//!
//! ```text
//! CADUS_ORACLE_PYTHON=/home/deploy/dev/cadus/.venv/bin/python \
//!     cargo test -p cadus-core --test answer_oracle
//! ```
//!
//! No test of this file carries `#[ignore]`. The live tests read
//! `CADUS_ORACLE_PYTHON` and they print a skip line when it is unset, so
//! `-- --ignored` selects nothing and proves nothing (review finding #12).
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
///
/// The generated set holds 17,047 pairs, so the cap drops none of them today.
/// If a new generator takes the set past the cap, [`generated_pairs`] prints the
/// total it drops and one line per dropped pair, because a dropped pair is a
/// pair the parity report never asks the 1.0 oracle about.
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

/// Read one numeric source into a `f64`, or refuse it.
///
/// The reader is the harness's own, and it is deliberately small: it reads the
/// values that the two 1.0 float rungs compare, and nothing else. A free symbol,
/// a bare `sqrt 2`, an implicit product such as `2x`, and every other name give
/// `None`, and the caller then refuses the pair. The grammar is
///
/// ```text
/// sum     := product (('+' | '-') product)*
/// product := unary (('*' | '/') unary)*
/// unary   := ('-' | '+') unary | power
/// power   := primary ('**' unary)?
/// primary := number | 'pi' | 'e' | 'sqrt' '(' sum ')' | '(' sum ')'
/// ```
///
/// The reader owns its arithmetic, so a change in `answer::canon` never moves
/// the pairs it builds or the divergences it explains.
fn numeric_value(source: &str) -> Option<f64> {
    let chars: Vec<char> = source.chars().collect();
    let mut reader = Numbers {
        chars: &chars,
        at: 0,
    };
    let value = reader.sum()?;
    reader.skip_spaces();
    if reader.at != chars.len() {
        return None;
    }
    value.is_finite().then_some(value)
}

/// The reader state of [`numeric_value`].
struct Numbers<'a> {
    chars: &'a [char],
    at: usize,
}

impl Numbers<'_> {
    /// Step over every space in front of the next token.
    fn skip_spaces(&mut self) {
        while matches!(self.chars.get(self.at), Some(c) if c.is_whitespace()) {
            self.at += 1;
        }
    }

    /// The character at the reader position.
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn sum(&mut self) -> Option<f64> {
        let mut total = self.product()?;
        loop {
            self.skip_spaces();
            match self.peek() {
                Some('+') => {
                    self.at += 1;
                    total += self.product()?;
                }
                Some('-') => {
                    self.at += 1;
                    total -= self.product()?;
                }
                _ => return Some(total),
            }
        }
    }

    fn product(&mut self) -> Option<f64> {
        let mut total = self.unary()?;
        loop {
            self.skip_spaces();
            match self.peek() {
                // A `**` is a power, and the power rule owns it.
                Some('*') if self.chars.get(self.at + 1) != Some(&'*') => {
                    self.at += 1;
                    total *= self.unary()?;
                }
                Some('/') => {
                    self.at += 1;
                    let divisor = self.unary()?;
                    if divisor == 0.0 {
                        return None;
                    }
                    total /= divisor;
                }
                _ => return Some(total),
            }
        }
    }

    fn unary(&mut self) -> Option<f64> {
        self.skip_spaces();
        match self.peek() {
            Some('-') => {
                self.at += 1;
                Some(-self.unary()?)
            }
            Some('+') => {
                self.at += 1;
                self.unary()
            }
            _ => self.power(),
        }
    }

    fn power(&mut self) -> Option<f64> {
        let base = self.primary()?;
        self.skip_spaces();
        if self.peek() == Some('*') && self.chars.get(self.at + 1) == Some(&'*') {
            self.at += 2;
            let exponent = self.unary()?;
            return Some(base.powf(exponent));
        }
        Some(base)
    }

    fn primary(&mut self) -> Option<f64> {
        self.skip_spaces();
        let ch = self.peek()?;
        if ch == '(' {
            self.at += 1;
            let value = self.sum()?;
            self.skip_spaces();
            if self.peek() != Some(')') {
                return None;
            }
            self.at += 1;
            return Some(value);
        }
        if ch.is_ascii_digit() || ch == '.' {
            return self.number();
        }
        if ch.is_ascii_alphabetic() {
            return self.name();
        }
        None
    }

    /// Read one decimal literal.
    fn number(&mut self) -> Option<f64> {
        let start = self.at;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.at += 1;
        }
        if self.peek() == Some('.') {
            self.at += 1;
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.at += 1;
            }
        }
        // A name that touches the literal is an implicit product, and the reader
        // reads no product of a number and a name.
        if matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric()) {
            return None;
        }
        let text: String = self.chars.get(start..self.at)?.iter().collect();
        text.parse::<f64>().ok()
    }

    /// Read one of the three names the reader knows.
    fn name(&mut self) -> Option<f64> {
        let start = self.at;
        while matches!(self.peek(), Some(c) if c.is_ascii_alphabetic()) {
            self.at += 1;
        }
        let name: String = self.chars.get(start..self.at)?.iter().collect();
        match name.as_str() {
            "pi" => Some(std::f64::consts::PI),
            "e" => Some(std::f64::consts::E),
            "sqrt" => {
                self.skip_spaces();
                if self.peek() != Some('(') {
                    return None;
                }
                self.at += 1;
                let value = self.sum()?;
                self.skip_spaces();
                if self.peek() != Some(')') {
                    return None;
                }
                self.at += 1;
                if value < 0.0 {
                    return None;
                }
                Some(value.sqrt())
            }
            _ => None,
        }
    }
}

/// Split `text` into its top-level terms, each with its own sign.
///
/// A `+` or a `-` is a term separator only when an operand ends in front of it,
/// so the leading sign of `-2*x` stays with its term and `x**-2` keeps its
/// exponent. Returns `None` when the brackets do not balance or a term is blank.
fn signed_terms(text: &str) -> Option<Vec<(bool, String)>> {
    let chars: Vec<char> = text.chars().collect();
    let mut depth = 0_i32;
    let mut terms: Vec<(bool, String)> = Vec::new();
    let mut negative = false;
    let mut current = String::new();
    for (index, ch) in chars.iter().enumerate() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth -= 1;
                if depth < 0 {
                    return None;
                }
            }
            _ => {}
        }
        if depth == 0 && (*ch == '+' || *ch == '-') && ends_an_operand(&chars, index) {
            terms.push((negative, current.trim().to_string()));
            negative = *ch == '-';
            current = String::new();
            continue;
        }
        current.push(*ch);
    }
    if depth != 0 {
        return None;
    }
    terms.push((negative, current.trim().to_string()));
    // A leading `-` on the first term is a sign, not a separator, so it is still
    // in the term text. The caller reads the flag, and the flag is false here.
    if terms.iter().any(|(_, body)| body.is_empty()) {
        return None;
    }
    Some(terms)
}

/// Whether an operand ends immediately in front of `index`.
fn ends_an_operand(chars: &[char], index: usize) -> bool {
    let mut back = index;
    while back > 0 {
        back -= 1;
        let Some(ch) = chars.get(back).copied() else {
            return false;
        };
        if ch.is_whitespace() {
            continue;
        }
        return ch.is_ascii_alphanumeric() || ch == ')' || ch == ']' || ch == '}' || ch == '.';
    }
    false
}

/// Whether `text` is one multiplicative term: no `+` and no binary `-` outside a
/// bracket.
fn is_one_term(text: &str) -> bool {
    matches!(signed_terms(text), Some(terms) if terms.len() == 1)
}

/// Split `text` into its top-level factors, at every `*` that is not a `**`.
///
/// Returns `None` when the brackets do not balance or a factor is blank.
fn product_factors(text: &str) -> Option<Vec<String>> {
    let chars: Vec<char> = text.chars().collect();
    let mut depth = 0_i32;
    let mut factors: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut index = 0;
    while let Some(ch) = chars.get(index).copied() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth -= 1;
                if depth < 0 {
                    return None;
                }
            }
            _ => {}
        }
        if depth == 0 && ch == '*' {
            if chars.get(index + 1) == Some(&'*') {
                current.push_str("**");
                index += 2;
                continue;
            }
            factors.push(current.trim().to_string());
            current = String::new();
            index += 1;
            continue;
        }
        current.push(ch);
        index += 1;
    }
    if depth != 0 {
        return None;
    }
    factors.push(current.trim().to_string());
    if factors.iter().any(String::is_empty) {
        return None;
    }
    Some(factors)
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

/// The characters the internal-space family puts a space around.
///
/// A run of them stays one token, so `**` stays `**`, `<=` stays `<=`, and
/// `x^-2` stays `x ^- 2`. Splitting a run would build an answer no learner types
/// and would ask the oracle a question about the split, not about the space.
const SPACED_OPERATORS: [char; 11] = ['+', '-', '*', '/', '^', '(', ')', ',', '=', '<', '>'];

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
fn generate_internal_spaces(row: &Row) -> Option<String> {
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
fn space_out(text: &str) -> Option<String> {
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
fn push_spaced(out: &mut String, token: &[char]) {
    out.push(' ');
    out.extend(token);
    out.push(' ');
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

/// Write the answer in upper case (spec section 9.3, "case flip").
///
/// The expected verdict is True on both sides, and both checkers reach it on
/// their string rung: 1.0 `_normalize` casefolds (`sympy_check.py:44`), and 2.0
/// builds the same casefolded `string_key` (`answer::normalize`, spec section
/// 2.1). Neither parser casefolds, so the family measures the rung and not the
/// grammar: `SQRT(2)` never reaches a `sqrt` call, and `2*X` never reaches the
/// variable `x`.
fn generate_case_flip(row: &Row) -> Option<String> {
    changed(row, row.answer.to_ascii_uppercase())
}

/// Write the answer as a decimal of ten significant digits (spec section 9.3,
/// "decimal to >= 8 significant digits").
///
/// The family applies to a rational with no exact decimal and to a radical. 1.0
/// grades the pair True on a float rung; 2.0 holds exact values only, so it
/// grades the pair False (D6). The divergence is the documented class "no float
/// tolerance rung (D6)", and [`the_1_0_float_rung_closes_the_gap`] names it.
fn generate_significant_decimal(row: &Row) -> Option<String> {
    if !takes_a_rounded_decimal(&row.source) {
        return None;
    }
    let value = numeric_value(&row.source)?;
    changed_source(row, ten_significant_digits(value)?)
}

/// Whether the value of `source` is a rational with no exact decimal, or a
/// radical.
///
/// Every other value takes no rounded decimal, and the pair reaches no float
/// rung: an integer and a terminating decimal are the decimal they write, and a
/// value with a free symbol leaves the rung altogether.
fn takes_a_rounded_decimal(source: &str) -> bool {
    if source.contains("sqrt(") {
        return true;
    }
    match fraction_parts(source) {
        Some((numerator, denominator)) => !terminates(numerator, denominator),
        None => false,
    }
}

/// Whether the decimal expansion of `numerator / denominator` ends.
fn terminates(numerator: i128, denominator: i128) -> bool {
    let divisor = gcd(numerator.abs(), denominator.abs());
    if divisor == 0 {
        return true;
    }
    let mut rest = (denominator / divisor).abs();
    while rest % 2 == 0 {
        rest /= 2;
    }
    while rest % 5 == 0 {
        rest /= 5;
    }
    rest == 1
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
fn ten_significant_digits(value: f64) -> Option<String> {
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

/// Reorder the factors of a product (spec section 9.3, "commutative reorder").
///
/// `2*x` becomes `x*2`, and the expected verdict is True on both sides. The rule
/// applies only when the whole source is one multiplicative term: a rotation
/// across a `+` or a binary `-` builds a different value, and the family asks
/// about commutativity and not about arithmetic.
fn generate_product_reorder(row: &Row) -> Option<String> {
    if !is_one_term(&row.source) {
        return None;
    }
    let factors = product_factors(&row.source)?;
    if factors.len() < 2 {
        return None;
    }
    let mut rotated = factors;
    rotated.rotate_left(1);
    let joined: Vec<String> = rotated
        .iter()
        .map(|factor| {
            // A factor that starts with its own sign takes a bracket, because
            // `x*-2` is a spelling no learner types.
            if factor.starts_with('-') || factor.starts_with('+') {
                format!("({factor})")
            } else {
                factor.clone()
            }
        })
        .collect();
    changed_source(row, joined.join("*"))
}

/// Write the answer with one algebraic step applied (spec section 9.3,
/// "algebraic refactor").
///
/// The expected verdict is True on both sides. Two rules run, in a fixed order:
///
/// 1. A difference of two squares takes its factored form, which is the pair the
///    spec names: `(x-1)*(x+1)` for `x**2-1`.
/// 2. A product whose last factor is a parenthesized sum multiplies out, which
///    is the same step in the other direction: the corpus authors the factored
///    form (`(x + 3)(x - 3)`) and the learner multiplies it out.
fn generate_algebraic_refactor(row: &Row) -> Option<String> {
    let candidate = factor_a_difference_of_squares(&row.source)
        .or_else(|| multiply_out_last_group(&row.source));
    changed_source(row, candidate?)
}

/// Write `a**2 - b**2` as `(a - b)*(a + b)`.
fn factor_a_difference_of_squares(source: &str) -> Option<String> {
    let terms = signed_terms(source)?;
    if terms.len() != 2 {
        return None;
    }
    let (left_negative, left) = terms.first()?;
    let (right_negative, right) = terms.get(1)?;
    if *left_negative || !*right_negative {
        return None;
    }
    let a = square_root_text(left)?;
    let b = square_root_text(right)?;
    Some(format!("({a} - {b})*({a} + {b})"))
}

/// The square root of one term, when the term is a plain square.
///
/// The term is a whole number that is a perfect square, or `name**2`, or
/// `k*name**2` and `kname**2` with a perfect-square `k`.
fn square_root_text(term: &str) -> Option<String> {
    let term = term.trim();
    if let Some((negative, digits)) = integer_digits(term) {
        if negative {
            return None;
        }
        let value: i128 = digits.parse().ok()?;
        return Some(integer_square_root(value)?.to_string());
    }
    let digits: String = term.chars().take_while(char::is_ascii_digit).collect();
    let rest = term.get(digits.len()..)?.trim_start_matches('*');
    let name = rest.strip_suffix("**2")?;
    if name.is_empty()
        || !name.chars().all(|c| c.is_ascii_alphanumeric())
        || name.starts_with(|c: char| c.is_ascii_digit())
    {
        return None;
    }
    if digits.is_empty() {
        return Some(name.to_string());
    }
    let coefficient: i128 = digits.parse().ok()?;
    let root = integer_square_root(coefficient)?;
    Some(format!("{root}*{name}"))
}

/// The exact square root of a whole number, when the number is a square.
fn integer_square_root(value: i128) -> Option<i128> {
    if value < 0 {
        return None;
    }
    let mut root = 0_i128;
    while root.checked_mul(root)? < value {
        root += 1;
    }
    (root * root == value).then_some(root)
}

/// Multiply the last parenthesized sum of a product out over its other factor.
fn multiply_out_last_group(source: &str) -> Option<String> {
    let text = source.trim();
    let chars: Vec<char> = text.chars().collect();
    if chars.last() != Some(&')') {
        return None;
    }
    let start = matching_open(&chars)?;
    if start == 0 {
        return None;
    }
    // The group must be a factor and not a function argument. A function name
    // puts a letter in front of the bracket, and a `/` in front of it makes the
    // group a divisor.
    let before = chars.get(start - 1).copied()?;
    if !(before == ')' || before.is_ascii_digit() || before == '*') {
        return None;
    }
    let prefix_end = if before == '*' { start - 1 } else { start };
    let prefix: String = chars.get(..prefix_end)?.iter().collect();
    let prefix = prefix.trim();
    if prefix.is_empty() || !is_one_term(prefix) {
        return None;
    }
    let inner: String = chars.get(start + 1..chars.len() - 1)?.iter().collect();
    let terms = signed_terms(&inner)?;
    if terms.len() < 2 {
        return None;
    }
    let mut out = String::new();
    for (index, (negative, body)) in terms.iter().enumerate() {
        if index == 0 {
            if *negative {
                out.push('-');
            }
        } else if *negative {
            out.push_str(" - ");
        } else {
            out.push_str(" + ");
        }
        let _ = write!(out, "{prefix}*({body})");
    }
    Some(out)
}

/// The index of the bracket that the last character of `chars` closes.
fn matching_open(chars: &[char]) -> Option<usize> {
    let mut depth = 0_i32;
    for index in (0..chars.len()).rev() {
        match chars.get(index).copied()? {
            ')' => depth += 1,
            '(' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
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
///
/// The array holds every family the spec names except one, and the exception is
/// "word anagram" (`sey` for `yes`). That family applies to prose only, and no
/// prose answer reaches the pair set: `yes` leaves the 2.0 grammar, so the pair
/// is class 1 for every generator, and it measures nothing. The 1.0 anagram
/// defect stays pinned by its literal pair in
/// `crates/core/tests/answer_divergence.rs`.
///
/// M2 review 2, finding 16, showed that this doc comment was false before
/// FIXM2e and FIXM2f: five families were missing, and the 100% class-3
/// agreement measured the generators that were written and not the parity of
/// the checker.
const GENERATORS: [Generator; 38] = [
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
        name: "internal_spaces",
        intent: Intent::Same,
        make: generate_internal_spaces,
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
    // The four families of spec section 9.3 that M2 review 2, finding 16, names
    // as missing. They come last, so the pair order of every older generator
    // does not move.
    Generator {
        name: "case_flip",
        intent: Intent::Same,
        make: generate_case_flip,
    },
    Generator {
        name: "significant_decimal",
        intent: Intent::Same,
        make: generate_significant_decimal,
    },
    Generator {
        name: "product_reorder",
        intent: Intent::Same,
        make: generate_product_reorder,
    },
    Generator {
        name: "algebraic_refactor",
        intent: Intent::Same,
        make: generate_algebraic_refactor,
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
    let kept: BTreeSet<usize> = order.iter().copied().collect();
    // The cap is a task bound, not a measurement. A dropped pair is a pair the
    // parity report never asks the oracle about, so the harness names every one
    // of them and it names the total.
    println!(
        "the pair cap dropped {} of {} pairs",
        pairs.len() - kept.len(),
        pairs.len()
    );
    for (index, pair) in pairs.iter().enumerate() {
        if !kept.contains(&index) {
            println!(
                "CAP DROPPED {}: {:?} against {:?} on {}",
                pair.generator,
                pair.expected,
                pair.learner,
                pair.kind.as_str()
            );
        }
    }
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
///
/// A reason moves a pair out of class 3 only when a predicate of
/// [`documented_reason`] names it, and every one of those predicates cites the
/// 1.0 line it ports. Two reasons carry no predicate, and both count 0 in this
/// generated set: prose never enters the set, and a SymPy name leaves 2.0
/// undecidable. "A transcendental identity is not simplified" is the third: 1.0
/// reaches it through `simplify(lhs - rhs) == 0` (`sympy_check.py:358-359`), and
/// no test of the two answer strings decides whether SymPy needed that rung. The
/// old substring test claimed it did, and it excused five parse divergences that
/// hold no identity (M2 review 2, findings 10 and 14). The narrowing keeps its
/// literal pairs in `answer_divergence.rs` instead.
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

/// Whether the gap between the two values is inside a 1.0 float rung.
///
/// "Both sides are numbers" is not a reason on its own. The old predicate asked
/// only that, and it excused every numeric disagreement; a wrong number is a
/// wrong answer in 1.0 too. This predicate reads the two values with the harness
/// reader of [`numeric_value`] and asks the question the rung asks.
///
/// 1.0 runs two float rungs. `_numeric_equal` compares two plain `float()`
/// values at 1e-9 (`sympy_check.py:172-174`), and it is the only rung that runs
/// when Python `float()` reads both sources. `_sympy_equivalent` compares two
/// `evalf()` results at 1e-6 when neither side holds a free symbol
/// (`sympy_check.py:345-352`). 2.0 holds exact values only (D6), so a decimal of
/// ten significant digits is not the rational or the radical it approximates.
///
/// The reader is also the free-symbol test that the 1e-6 rung needs: it knows
/// the number literals, `pi`, `e`, and `sqrt`, and it refuses every other name.
/// A numeric pair that is farther apart than the rung tolerance keeps no reason.
/// It stays in class 3 and it fails the parity assertion (R5).
fn the_1_0_float_rung_closes_the_gap(pair: &Pair) -> bool {
    let expected_source = normalize(&pair.expected).source;
    let learner_source = normalize(&pair.learner).source;
    let (Some(expected_value), Some(learner_value)) = (
        numeric_value(&expected_source),
        numeric_value(&learner_source),
    ) else {
        return false;
    };
    let tolerance =
        if reads_as_a_python_float(&expected_source) && reads_as_a_python_float(&learner_source) {
            1e-9
        } else {
            1e-6
        };
    (expected_value - learner_value).abs() <= tolerance * expected_value.abs().max(1.0)
}

/// Whether Python `float()` reads the whole source (1.0 `_numeric_equal`).
fn reads_as_a_python_float(source: &str) -> bool {
    integer_digits(source).is_some() || decimal_parts(source).is_some()
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
        if the_1_0_float_rung_closes_the_gap(pair) {
            return Some("no float tolerance rung (D6)");
        }
        // No catch-all sits here. A 1.0 `simplify` result is not readable from
        // the two answer strings, so a pair that names a transcendental function
        // is NOT a transcendental identity by that fact alone: the five
        // `cos 2*x` pairs of M2 review 2, findings 10 and 14, are a parse
        // divergence and the old substring test hid them. An unexplained
        // divergence stays in class 3 and fails the parity assertion (R5).
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
const GENERATED_PAIRS: usize = 17_047;

/// The literal pair count of every generator, in name order.
const GENERATOR_COUNTS: [(&str, usize); 41] = [
    ("algebraic_refactor", 58),
    ("appended_junk", 1562),
    ("ascii_to_unicode", 70),
    ("caret_power", 0),
    ("case_flip", 735),
    ("coarse_decimal", 92),
    ("comma_space_removed", 197),
    ("comma_thousands", 44),
    ("decimal_to_fraction", 105),
    ("digit_transposition", 604),
    ("dollar_wrapped", 1298),
    ("dot_thousands", 44),
    ("equivalent_fraction", 165),
    ("explicit_multiplication", 404),
    ("figure_space_thousands", 44),
    ("fraction_to_decimal", 79),
    ("identity", 1562),
    ("internal_spaces", 1063),
    ("implicit_multiplication", 63),
    ("last_digit_bumped", 1519),
    ("narrow_space_thousands", 44),
    ("nbsp_thousands", 44),
    ("over_thousand", 256),
    ("plus_spaced", 337),
    ("product_reorder", 48),
    ("set_element_changed", 9),
    ("set_reordered", 11),
    ("sign_flipped", 1559),
    ("significant_decimal", 154),
    ("space_thousands", 44),
    ("star_power", 333),
    ("sum_reorder", 206),
    ("thin_space_thousands", 44),
    ("times_thousand", 300),
    ("trailing_period", 1562),
    ("trailing_zero", 408),
    ("tuple_swapped", 151),
    ("unicode_to_ascii", 61),
    ("whitespace_padding", 1562),
    ("wrong_exponent", 173),
    ("wrong_radicand", 33),
];

/// The literal pair count of every divergence class.
///
/// The counts are measured against the live 1.0 checker, not read back from the
/// committed file.
const CLASS_COUNTS: [(&str, usize); 5] = [
    ("class 1 outside_grammar", 931),
    ("class 2 prose_expected", 0),
    ("class 3 comparable", 15940),
    ("class 4 documented_divergence", 176),
    ("oracle_silent", 0),
];

/// The literal pair count of every documented divergence reason.
///
/// The first four reasons are the ones `docs/plans/M2.md` names. This generated
/// set reaches none of them except the float rung: prose never enters the set
/// (the set holds only in-grammar answers), a SymPy name such as `zoo` leaves
/// 2.0 undecidable, which is class 1, and no predicate claims a 1.0
/// simplification. All three stay pinned by literal pairs in
/// `crates/core/tests/answer_divergence.rs`.
///
/// The five pairs that the deleted substring test moved under the transcendental
/// reason are the `cos 2*x` parse divergence of M2 review 2, findings 10 and 14.
/// They belong to class 3, and they agree once the juxtaposed-argument rule of
/// the parser reads `cos 2*x` as `cos(2*x)`.
///
/// The float rung carries 156 pairs, and 154 of them come from the
/// `significant_decimal` family that spec section 9.3 names: a rational with no
/// exact decimal, or a radical, against its own value in ten significant digits.
/// 1.0 grades every one of them True on a float rung, and 2.0 grades them False
/// (D6). `crates/core/tests/answer_divergence.rs` pins one pair of each shape.
const REASON_COUNTS: [(&str, usize); 9] = [
    ("no float tolerance rung (D6)", 156),
    ("a transcendental identity is not simplified (V1)", 0),
    ("prose is not a value (V2)", 0),
    ("a SymPy name is not a value (V2)", 0),
    (
        "the 1.0 exponent-tower guard refuses a legal power (spec 5.1)",
        3,
    ),
    (
        "the 1.0 tokenizer reads a Python number literal (spec 3.1)",
        8,
    ),
    (
        "the 1.0 radical rewrite misses a nested group (spec 2.2)",
        1,
    ),
    ("a chained inequality raises inside 1.0 (spec 7.7)", 6),
    (
        "the 1.0 rewriter deletes a backslash and leaves a brace group (spec 7.7)",
        2,
    ),
];

#[test]
fn a_divergence_leaves_class_3_only_when_a_predicate_names_a_1_0_line() {
    // M2 review 2, findings 10 and 14. The old ladder ended in a substring test
    // for a function name, so every 1.0-True / 2.0-False pair whose text held
    // `sin`, `cos`, `ln`, `log`, or `exp` left class 3 with the reason "a
    // transcendental identity is not simplified". The five `cos 2*x` pairs of
    // the `explicit_multiplication` generator hold no identity: they are the
    // juxtaposed-function-argument parse divergence, and they belong in class 3.
    let parse_divergence = probe_pair("cos 2x", "cos 2*x", "expression_symbolic");
    let one_zero_says_yes = OracleVerdict {
        equivalent: true,
        notation: false,
    };
    assert_eq!(
        documented_reason(&parse_divergence, false, one_zero_says_yes),
        None
    );
    // The same answer with a `sin` in it, and with a `log` in it.
    let with_sin = probe_pair("(4/3)sin 3t", "(4/3)*sin 3*t", "expression_symbolic");
    assert_eq!(documented_reason(&with_sin, false, one_zero_says_yes), None);
    let with_log = probe_pair("log(2x)", "2*log(x)", "expression_symbolic");
    assert_eq!(documented_reason(&with_log, false, one_zero_says_yes), None);
    // The identity that IS the documented narrowing gets no reason here either:
    // no test of the two strings reads a 1.0 `simplify` result. The pair is
    // pinned by its literal verdict in `answer_divergence.rs`.
    let identity = probe_pair("sin(x)**2+cos(x)**2", "1", "expression_symbolic");
    assert_eq!(documented_reason(&identity, false, one_zero_says_yes), None);
    // The one predicate of this branch still names its own pairs: two numbers
    // that 1.0 calls equal at its float tolerance, and 2.0 does not (D6).
    let two_numbers = probe_pair("1/1000", "1/1001", "fraction");
    assert_eq!(
        documented_reason(&two_numbers, false, one_zero_says_yes),
        Some("no float tolerance rung (D6)")
    );
    // A prose answer keeps its own reason, whatever the two verdicts are.
    let prose = probe_pair("yes", "no", "prose_or_words");
    assert_eq!(
        documented_reason(&prose, false, one_zero_says_yes),
        Some("prose is not a value (V2)")
    );
}

#[test]
fn a_numeric_divergence_needs_the_1_0_float_tolerance() {
    // M2 review 2 asked for a specific predicate and not a catch-all. "Both
    // sides are numbers" excused every numeric disagreement; the rung the
    // predicate ports compares two floats at a tolerance, so the predicate
    // compares two floats at that tolerance.
    let one_zero_says_yes = OracleVerdict {
        equivalent: true,
        notation: false,
    };
    // Inside the 1e-6 `evalf` rung (`sympy_check.py:345-352`).
    let inside = probe_pair("1/1000", "1/1001", "fraction");
    assert_eq!(
        documented_reason(&inside, false, one_zero_says_yes),
        Some("no float tolerance rung (D6)")
    );
    let decimal_of_a_fraction = probe_pair("5/12", "0.4166666667", "fraction");
    assert_eq!(
        documented_reason(&decimal_of_a_fraction, false, one_zero_says_yes),
        Some("no float tolerance rung (D6)")
    );
    let decimal_of_a_radical = probe_pair("8*sqrt(2)", "11.31370850", "expression_numeric");
    assert_eq!(
        documented_reason(&decimal_of_a_radical, false, one_zero_says_yes),
        Some("no float tolerance rung (D6)")
    );
    let nested_radical = probe_pair("√(2 + √3)/2", "0.9659258263", "expression_numeric");
    assert_eq!(
        documented_reason(&nested_radical, false, one_zero_says_yes),
        Some("no float tolerance rung (D6)")
    );
    // Outside every rung. Two numbers that differ keep no reason, and the pair
    // stays in class 3 (R5).
    let coarse = probe_pair("2/3", "0.667", "fraction");
    assert_eq!(documented_reason(&coarse, false, one_zero_says_yes), None);
    let far_apart = probe_pair("7329", "7330", "integer");
    assert_eq!(
        documented_reason(&far_apart, false, one_zero_says_yes),
        None
    );
    // Two plain decimals take the tighter 1e-9 rung (`sympy_check.py:172-174`),
    // so a gap inside the 1e-6 rung keeps no reason here.
    let two_decimals = probe_pair("1.0000000", "1.0000005", "decimal");
    assert_eq!(
        documented_reason(&two_decimals, false, one_zero_says_yes),
        None
    );
    // A free symbol leaves the rung: the reader refuses the side, so no reason.
    let with_a_symbol = probe_pair("x/3", "0.3333333333*x", "expression_symbolic");
    assert_eq!(
        documented_reason(&with_a_symbol, false, one_zero_says_yes),
        None
    );
}

#[test]
fn the_harness_reader_reads_a_number_and_refuses_a_name() {
    assert_eq!(numeric_value("1/8"), Some(0.125));
    assert_eq!(numeric_value("-5/6"), Some(-5.0 / 6.0));
    assert_eq!(numeric_value("2*sqrt(4)/4"), Some(1.0));
    assert_eq!(numeric_value("sqrt(2 + sqrt(4))"), Some(2.0));
    assert_eq!(numeric_value("(2 + 4)/3"), Some(2.0));
    assert_eq!(numeric_value("2**3"), Some(8.0));
    assert_eq!(numeric_value("-2**2"), Some(-4.0));
    assert_eq!(numeric_value("2**-1"), Some(0.5));
    assert_eq!(numeric_value("pi"), Some(std::f64::consts::PI));
    assert_eq!(numeric_value("e"), Some(std::f64::consts::E));
    // A free symbol, an unknown name, a bare radical, an implicit product, an
    // unbalanced bracket, and a division by zero are all refusals.
    assert_eq!(numeric_value("x"), None);
    assert_eq!(numeric_value("2*cos(0)"), None);
    assert_eq!(numeric_value("2*sqrt 2"), None);
    assert_eq!(numeric_value("2x"), None);
    assert_eq!(numeric_value("(1 + 2"), None);
    assert_eq!(numeric_value("1/0"), None);
    // Two values that only touch are two answers, not one.
    assert_eq!(numeric_value("1 2"), None);
}

/// Build one corpus row for a generator test.
fn probe_row(answer: &str, shape: &str) -> Row {
    Row {
        answer: answer.to_string(),
        source: normalize(answer).source,
        shape: shape.to_string(),
        kind: AnswerKind::Expression,
    }
}

#[test]
fn the_case_flip_family_writes_the_answer_in_upper_case() {
    assert_eq!(
        generate_case_flip(&probe_row("sqrt(2)", "expression_numeric")),
        Some("SQRT(2)".to_string())
    );
    assert_eq!(
        generate_case_flip(&probe_row("2*x + 1", "expression_symbolic")),
        Some("2*X + 1".to_string())
    );
    // An answer with no lower-case letter is no variant.
    assert_eq!(generate_case_flip(&probe_row("7329", "integer")), None);
    assert_eq!(
        generate_case_flip(&probe_row("(4, 17)", "ordered_tuple")),
        None
    );
}

#[test]
fn the_significant_decimal_family_writes_ten_significant_digits() {
    assert_eq!(
        generate_significant_decimal(&probe_row("1/3", "fraction")),
        Some("0.3333333333".to_string())
    );
    assert_eq!(
        generate_significant_decimal(&probe_row("-5/6", "fraction")),
        Some("-0.8333333333".to_string())
    );
    assert_eq!(
        generate_significant_decimal(&probe_row("8*sqrt(2)", "expression_numeric")),
        Some("11.31370850".to_string())
    );
    assert_eq!(
        generate_significant_decimal(&probe_row("√(2 + √3)/2", "expression_numeric")),
        Some("0.9659258263".to_string())
    );
    // A rational with an exact decimal, an integer, and a value with a free
    // symbol are all refusals: the pair carries no float rung.
    assert_eq!(
        generate_significant_decimal(&probe_row("1/2", "fraction")),
        None
    );
    assert_eq!(
        generate_significant_decimal(&probe_row("7329", "integer")),
        None
    );
    assert_eq!(
        generate_significant_decimal(&probe_row("sqrt(x)", "expression_symbolic")),
        None
    );
}

#[test]
fn the_product_reorder_family_rotates_the_factors() {
    assert_eq!(
        generate_product_reorder(&probe_row("2*x", "expression_symbolic")),
        Some("x*2".to_string())
    );
    assert_eq!(
        generate_product_reorder(&probe_row("8*sqrt(2)", "expression_numeric")),
        Some("sqrt(2)*8".to_string())
    );
    assert_eq!(
        generate_product_reorder(&probe_row("2*sqrt(3)/3", "expression_numeric")),
        Some("sqrt(3)/3*2".to_string())
    );
    // A sum is no product, and a rotation across its `-` builds another value.
    assert_eq!(
        generate_product_reorder(&probe_row("9*pi - 18", "expression_numeric")),
        None
    );
    assert_eq!(
        generate_product_reorder(&probe_row("x**2", "expression_symbolic")),
        None
    );
}

#[test]
fn the_algebraic_refactor_family_factors_and_multiplies_out() {
    // The pair spec section 9.3 names.
    assert_eq!(
        generate_algebraic_refactor(&probe_row("x**2 - 1", "expression_symbolic")),
        Some("(x - 1)*(x + 1)".to_string())
    );
    assert_eq!(
        generate_algebraic_refactor(&probe_row("4x**2 - 49", "expression_symbolic")),
        Some("(2*x - 7)*(2*x + 7)".to_string())
    );
    // The same step in the other direction: the corpus authors the factored
    // form and the learner multiplies it out.
    assert_eq!(
        generate_algebraic_refactor(&probe_row("(x + 3)(x - 3)", "expression_symbolic")),
        Some("(x + 3)*(x) - (x + 3)*(3)".to_string())
    );
    assert_eq!(
        generate_algebraic_refactor(&probe_row("2(x + 3)(x - 3)", "expression_symbolic")),
        Some("2(x + 3)*(x) - 2(x + 3)*(3)".to_string())
    );
    // A function argument is no factor, and a sum of two terms that are not
    // both squares takes neither rule.
    assert_eq!(
        generate_algebraic_refactor(&probe_row("sqrt(1 + x**2)", "expression_symbolic")),
        None
    );
    assert_eq!(
        generate_algebraic_refactor(&probe_row("x**2 - 3", "expression_symbolic")),
        None
    );
    assert_eq!(
        generate_algebraic_refactor(&probe_row("1/(x + 1)", "expression_symbolic")),
        None
    );
}

/// Build one pair for a predicate test.
fn probe_pair(expected: &str, learner: &str, shape: &str) -> Pair {
    Pair {
        generator: "probe",
        intent: Intent::Same,
        expected: expected.to_string(),
        learner: learner.to_string(),
        kind: AnswerKind::Expression,
        shape: shape.to_string(),
    }
}

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

/// Ask the live 1.0 oracle for one pair, and return the raw response line.
///
/// The helper starts one harness process, sends one request, and reads one
/// response. It exists for the guard test below, which needs its own guard.
fn one_live_response(python: &str, request: &serde_json::Value, timeout_s: &str) -> String {
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Command, Stdio};

    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/oracle/check_1_0.py")
        .canonicalize()
        .unwrap_or_else(|e| panic!("find scripts/oracle/check_1_0.py: {e}"));
    let mut child = Command::new(python)
        .arg(&script)
        .arg("--timeout")
        .arg(timeout_s)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("start {}: {e}", script.display()));
    {
        let mut stdin = child.stdin.take().unwrap_or_else(|| panic!("no stdin"));
        writeln!(stdin, "{request}").unwrap_or_else(|e| panic!("write the request: {e}"));
    }
    let stdout = child.stdout.take().unwrap_or_else(|| panic!("no stdout"));
    let mut reader = BufReader::new(stdout);
    let mut ready = String::new();
    reader
        .read_line(&mut ready)
        .unwrap_or_else(|e| panic!("read the ready line: {e}"));
    assert!(ready.contains("\"ready\""), "the oracle said {ready:?}");
    let mut response = String::new();
    reader
        .read_line(&mut response)
        .unwrap_or_else(|e| panic!("read the response: {e}"));
    let _ = child.wait();
    response.trim().to_string()
}

/// A pair the guard stops is recorded as a timeout, never as a decided verdict.
///
/// Review finding #21: the old guard raised a `Timeout` exception inside the 1.0
/// process, and the bare `except Exception` handlers of 1.0 `_sympy_equivalent`
/// caught it and returned `False`. The harness now runs every 1.0 call in a
/// child process and terminates that process on the deadline, so 1.0 cannot
/// swallow the guard. The pair below is the work bomb of spec section 3.2.
#[test]
fn a_stopped_oracle_call_is_a_timeout_and_never_a_decided_verdict() {
    let Ok(python) = std::env::var("CADUS_ORACLE_PYTHON") else {
        println!("skipped: set CADUS_ORACLE_PYTHON to run against the live 1.0 checker");
        return;
    };
    let request = serde_json::json!({
        "expected": "(x+1)**200",
        "learner": "x**200+1",
        "kind": "expression",
        "id": "stall",
    });
    let response = one_live_response(&python, &request, "0.05");
    println!("stall response: {response}");
    assert_eq!(
        response, r#"{"equivalent": null, "id": "stall", "notation": null, "timeout": true}"#,
        "the guard must report a timeout, and it must decide nothing"
    );
    // The worker respawns, so the next pair still gets a real 1.0 verdict.
    let next = serde_json::json!({
        "expected": "7329",
        "learner": "7.329",
        "kind": "numeric",
        "id": "after",
    });
    let after = one_live_response(&python, &next, "0.05");
    println!("after response: {after}");
    assert_eq!(
        after, r#"{"equivalent": true, "id": "after", "notation": true, "timeout": false}"#,
        "a fast pair keeps its decided 1.0 verdict"
    );
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
