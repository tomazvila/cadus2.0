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
//! # Where the rewrite spellings come from
//!
//! The six `rewrite_*` families take their learner text from
//! `crates/core/tests/fixtures/answers/rational_rewrites_1_0.jsonl`, which
//! `scripts/oracle/rewrite_1_0.py` wrote from SymPy through the 1.0 parser
//! (M2 review 3, finding #14). SymPy GENERATES a spelling there; it judges
//! nothing. Every verdict still comes from `check_1_0.py`.
//!
//! # Regenerating the two fixtures, in this order
//!
//! ```text
//! CADUS_REWRITE_REGEN=1 CADUS_ORACLE_PYTHON=/home/deploy/dev/cadus/.venv/bin/python \
//!     cargo test -p cadus-core --test answer_oracle regenerate_the_rewrite_fixture
//! CADUS_ORACLE_RECORD=1 CADUS_ORACLE_PYTHON=/home/deploy/dev/cadus/.venv/bin/python \
//!     cargo test -p cadus-core --test answer_oracle record_the_oracle_verdicts
//! ```
//!
//! The spellings change the generated pair set, so the verdict file follows the
//! spelling file and never the other way around.
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

use num_traits::{One as _, Zero as _};

use cadus_core::answer::{Ast, Atom, Canon, Outcome, canonical_form, check, normalize, parse};
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
///
/// # Why the row carries three readings of one answer
///
/// FIXM2g moved every LaTeX and glyph construct out of `normalize` and into the
/// lexer, so [`Row::source`] is no longer SymPy-like source: it keeps `\frac`,
/// `\pi`, `√`, and `^`. A builder that read a value or a structure out of that
/// string read the wrong thing, and 328 pairs left the set without a word (M2
/// review 3, the FIXM2i regeneration ruling). Every such builder now reads
/// [`Row::printed`], which is the parsed tree written back as ASCII, or
/// [`Row::canon`], which is the exact value.
struct Row {
    /// The authored answer, verbatim.
    answer: String,
    /// The normalized parser source of that answer (V4).
    ///
    /// Two families read this string, and both of them ask a question about the
    /// surface spelling and not about the value: `explicit_multiplication` needs
    /// the juxtaposition the author wrote, and `internal_spaces` needs the
    /// author's own operators.
    source: String,
    /// The parsed tree of that source, written back as ASCII reader source.
    ///
    /// The text is a function of the tree alone, so it holds `sqrt(`, `**`,
    /// `pi`, and `*` whatever the author wrote. Every builder that reads a
    /// number, a term, a factor, or a bracket out of an answer reads this.
    printed: String,
    /// The parsed tree of the answer.
    ast: Ast,
    /// The canonical form of the answer, when the checker decides one.
    canon: Option<Canon>,
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
        let Ok(ast) = parse(&source) else {
            continue;
        };
        let printed = print_ast(&ast, PREC_LOWEST);
        let canon = canonical_form(&parsed.answer).ok();
        rows.push(Row {
            answer: parsed.answer,
            source,
            printed,
            ast,
            canon,
            shape: parsed.shape,
            kind,
        });
    }
    rows
}

// ---------------------------------------------------------------------------
// The answer tree, written back as ASCII reader source
// ---------------------------------------------------------------------------

/// The binding power of a sum: the weakest.
const PREC_LOWEST: u8 = 1;
/// The binding power of a product.
const PREC_PRODUCT: u8 = 2;
/// The binding power of a divisor and of a negation.
const PREC_UNARY: u8 = 3;
/// The binding power of a power base: the strongest.
const PREC_POWER: u8 = 4;

/// Write one answer tree back as ASCII reader source.
///
/// The text carries the structure of the tree and nothing of the spelling the
/// author chose, so `\frac{1}{2}`, `½`, and `1/2` all print `1/2`. Both checkers
/// read the result: the operators are `+ - * / **`, the root is `sqrt(…)`, and
/// the constants are `pi` and `e`.
///
/// `parent` is the binding power the position needs. A node that binds more
/// weakly than its position takes a bracket pair.
///
/// A mixed number always prints as the bracketed sum `(2 + 1/2)`. The bare form
/// `2 1/2` is a juxtaposition, and 1.0 reads a juxtaposition as a product, so the
/// bare form would ask the oracle about the mixed-number rule and not about the
/// family that built it.
fn print_ast(ast: &Ast, parent: u8) -> String {
    match ast {
        Ast::Integer(value) => {
            let text = value.to_string();
            let negative = text.starts_with('-');
            bracket_if(text, negative && parent >= PREC_PRODUCT)
        }
        Ast::Decimal { mantissa, scale } => {
            let text = decimal_text(&mantissa.to_string(), *scale);
            let negative = text.starts_with('-');
            bracket_if(text, negative && parent >= PREC_PRODUCT)
        }
        Ast::Fraction {
            numerator,
            denominator,
        } => {
            let text = format!("{numerator}/{denominator}");
            let negative = text.starts_with('-');
            bracket_if(
                text,
                parent > PREC_PRODUCT || (negative && parent >= PREC_PRODUCT),
            )
        }
        Ast::Mixed {
            whole,
            numerator,
            denominator,
        } => format!("({whole} + {numerator}/{denominator})"),
        Ast::Var(name) => name.clone(),
        Ast::Const(value) => value.name().to_string(),
        Ast::Sqrt(inner) => format!("sqrt({})", print_ast(inner, PREC_LOWEST)),
        Ast::Pow(base, exponent) => {
            // A negative exponent takes a bracket pair, so `x**-2` never reaches
            // either parser: 1.0 reads that string with the Python tokenizer.
            let power = if *exponent < 0 {
                format!("({exponent})")
            } else {
                exponent.to_string()
            };
            let body = format!("{}**{power}", print_ast(base, PREC_POWER));
            bracket_if(body, parent >= PREC_POWER)
        }
        Ast::Neg(inner) => bracket_if(
            format!("-{}", print_ast(inner, PREC_UNARY)),
            parent >= PREC_UNARY,
        ),
        Ast::Add(terms) => {
            let mut out = String::new();
            for (index, term) in terms.iter().enumerate() {
                match (index, term) {
                    (0, _) => out.push_str(&print_ast(term, PREC_LOWEST)),
                    // A subtracted term needs no bracket around a product: the
                    // binary `-` binds more weakly than the `*` it holds.
                    (_, Ast::Neg(inner)) => {
                        let _ = write!(out, " - {}", print_ast(inner, PREC_PRODUCT));
                    }
                    _ => {
                        let _ = write!(out, " + {}", print_ast(term, PREC_LOWEST));
                    }
                }
            }
            bracket_if(out, parent > PREC_LOWEST)
        }
        Ast::Mul(factors) => {
            let parts: Vec<String> = factors
                .iter()
                .map(|factor| print_ast(factor, PREC_PRODUCT))
                .collect();
            bracket_if(parts.join("*"), parent > PREC_PRODUCT)
        }
        Ast::Div(left, right) => bracket_if(
            format!(
                "{}/{}",
                print_ast(left, PREC_PRODUCT),
                print_ast(right, PREC_UNARY)
            ),
            parent > PREC_PRODUCT,
        ),
        Ast::Func(name, arguments) => format!("{name}({})", print_list(arguments)),
        Ast::Tuple(items) => format!("({})", print_list(items)),
        Ast::Set(items) => format!("{{{}}}", print_list(items)),
        Ast::List(items) => format!("[{}]", print_list(items)),
        Ast::Interval {
            lo,
            hi,
            lo_closed,
            hi_closed,
        } => format!(
            "{}{}, {}{}",
            if *lo_closed { '[' } else { '(' },
            print_ast(lo, PREC_LOWEST),
            print_ast(hi, PREC_LOWEST),
            if *hi_closed { ']' } else { ')' }
        ),
        Ast::Ineq { var, op, bound } => {
            format!("{var} {} {}", op.symbol(), print_ast(bound, PREC_LOWEST))
        }
        Ast::Assign { var, value } => format!("{var} = {}", print_ast(value, PREC_LOWEST)),
        Ast::Chain {
            lo,
            lo_closed,
            var,
            hi_closed,
            hi,
        } => format!(
            "{} {} {var} {} {}",
            print_ast(lo, PREC_LOWEST),
            if *lo_closed { "<=" } else { "<" },
            if *hi_closed { "<=" } else { "<" },
            print_ast(hi, PREC_LOWEST)
        ),
    }
}

/// Wrap `text` in a bracket pair when the position asks for one.
fn bracket_if(text: String, wrap: bool) -> String {
    if wrap { format!("({text})") } else { text }
}

/// Print a comma-separated argument list.
fn print_list(items: &[Ast]) -> String {
    items
        .iter()
        .map(|item| print_ast(item, PREC_LOWEST))
        .collect::<Vec<String>>()
        .join(", ")
}

/// Write `mantissa / 10**scale` as a decimal literal.
fn decimal_text(mantissa: &str, scale: u32) -> String {
    let negative = mantissa.starts_with('-');
    let mut digits = mantissa.trim_start_matches('-').to_string();
    let scale = scale as usize;
    if scale == 0 {
        return if negative {
            format!("-{digits}")
        } else {
            digits
        };
    }
    while digits.len() <= scale {
        digits.insert(0, '0');
    }
    let point = digits.len() - scale;
    digits.insert(point, '.');
    if negative {
        format!("-{digits}")
    } else {
        digits
    }
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
    let (negative, digits) = integer_digits(&row.printed)?;
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
                // A juxtaposed name or group is a product, because 1.0 parses
                // with `implicit_multiplication_application`: `3pi` is `3*pi`
                // and `2sqrt(3)` is `2*sqrt(3)` (`sympy_check.py:253`). Two
                // numbers that only touch stay two answers, so the rule needs a
                // name or a bracket on the right.
                Some(ch) if ch.is_ascii_alphabetic() || ch == '(' => {
                    total *= self.power()?;
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
            // A bracket-free radical is a call too, because SymPy
            // `implicit_application` writes the brackets: 1.0 reads `2sqrt 2 - 2`
            // as `2*sqrt(2) - 2` (`sympy_check.py:253`).
            "sqrt" => {
                self.skip_spaces();
                let value = if self.peek() == Some('(') {
                    self.at += 1;
                    let inner = self.sum()?;
                    self.skip_spaces();
                    if self.peek() != Some(')') {
                        return None;
                    }
                    self.at += 1;
                    inner
                } else {
                    self.primary()?
                };
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

/// Rewrite every `√` of `text` into a `sqrt(…)` call, at every depth.
///
/// 1.0 runs its two radical patterns to a fixed point (`sympy_check.py:148`), so
/// `√(2 + √3)` becomes `sqrt(2 + sqrt(3))` and not `sqrt(2 + √3)`.
fn radical_to_call(text: &str) -> String {
    let mut out = text.to_string();
    loop {
        let next = radical_to_call_once(&out);
        if next == out {
            return out;
        }
        out = next;
    }
}

/// Rewrite the radicals of one pass.
fn radical_to_call_once(text: &str) -> String {
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

/// The same rule for a generator that rewrites the printed tree.
fn changed_printed(row: &Row, candidate: String) -> Option<String> {
    (candidate != row.printed).then_some(candidate)
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
    let (numerator, denominator) = fraction_parts(&row.printed)?;
    let factor = 2 + (row.printed.len() % 8) as i128;
    let numerator = numerator.checked_mul(factor)?;
    let denominator = denominator.checked_mul(factor)?;
    Some(format!("{numerator}/{denominator}"))
}

fn generate_fraction_to_decimal(row: &Row) -> Option<String> {
    let (numerator, denominator) = fraction_parts(&row.printed)?;
    exact_decimal(numerator, denominator)
}

fn generate_decimal_to_fraction(row: &Row) -> Option<String> {
    let (negative, whole, fraction) = decimal_parts(&row.printed)?;
    if fraction.len() > 12 {
        return None;
    }
    let mantissa: i128 = format!("{whole}{fraction}").parse().ok()?;
    let denominator = 10_i128.checked_pow(u32::try_from(fraction.len()).ok()?)?;
    let sign = if negative { "-" } else { "" };
    Some(format!("{sign}{mantissa}/{denominator}"))
}

fn generate_trailing_zero(row: &Row) -> Option<String> {
    if decimal_parts(&row.printed).is_some() {
        return Some(format!("{}0", row.printed));
    }
    let (_, digits) = integer_digits(&row.printed)?;
    if digits.len() > 15 {
        return None;
    }
    Some(format!("{}.0", row.printed))
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
///
/// The family reads the printed tree, so it reaches `2*x` whatever the author
/// wrote. A `*` that follows a divisor keeps its star: `4/3*sin(x)` and
/// `4/3sin(x)` are two readings of one string, and the family asks about the
/// juxtaposition and not about the precedence of an implicit product.
fn generate_implicit_multiplication(row: &Row) -> Option<String> {
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
fn follows_a_divisor(chars: &[char], index: usize) -> bool {
    let mut back = index;
    while back > 0 {
        back -= 1;
        let Some(ch) = chars.get(back).copied() else {
            return false;
        };
        if ch.is_ascii_digit() || ch == '.' {
            continue;
        }
        return ch == '/';
    }
    false
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
    let parts = top_level_split(&row.printed, '+')?;
    if parts.len() < 2 {
        return None;
    }
    let mut rotated = parts.clone();
    rotated.rotate_left(1);
    changed(row, rotated.join("+"))
}

fn generate_set_reordered(row: &Row) -> Option<String> {
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
fn takes_a_rounded_decimal(canon: &Canon) -> bool {
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
fn an_irrational_number(poly: &cadus_core::answer::Poly) -> bool {
    let mut irrational = false;
    for (atom, _) in poly.keys().flatten() {
        match atom {
            Atom::Var(_) => return false,
            Atom::Call(name, _) if name != "sqrt" => return false,
            _ => irrational = true,
        }
    }
    irrational
}

/// Whether the decimal expansion of an exact rational ends.
fn terminates_exactly(value: &num_rational::BigRational) -> bool {
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
    if !is_one_term(&row.printed) {
        return None;
    }
    let factors = product_factors(&row.printed)?;
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
    changed_printed(row, joined.join("*"))
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
    let candidate = factor_a_difference_of_squares(&row.printed)
        .or_else(|| multiply_out_last_group(&row.printed));
    changed_printed(row, candidate?)
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

// ---------------------------------------------------------------------------
// The rational-rewrite family (M2 review 3, finding #14)
// ---------------------------------------------------------------------------

/// One line of `crates/core/tests/fixtures/answers/rational_rewrites_1_0.jsonl`.
///
/// The file holds one spelling per (answer, kind, rule). SymPy wrote every
/// spelling through `scripts/oracle/rewrite_1_0.py`, from the tree the 1.0 parser
/// itself reads. SymPy judges nothing: `check_1_0.py` still records the verdict of
/// every pair, and 1.0 grades a spelling like any other learner answer.
#[derive(Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
struct RewriteLine {
    /// The authored corpus answer, verbatim.
    answer: String,
    /// The authored answer kind.
    kind: String,
    /// The SymPy rule that wrote the spelling.
    rule: String,
    /// The spelling, as `str()` printed it.
    learner: String,
    /// Whether SymPy `cancel(expected - learner)` is zero.
    cancel_zero: bool,
    /// Whether SymPy `radsimp(expected - learner)` is zero.
    radsimp_zero: bool,
}

/// The rules of `scripts/oracle/rewrite_1_0.py`, in the order that file runs them.
const REWRITE_RULES: [&str; 6] = ["together", "apart", "cancel", "factor", "expand", "radsimp"];

/// Read the committed rewrite spellings.
fn committed_rewrites() -> &'static Vec<RewriteLine> {
    static CACHE: std::sync::OnceLock<Vec<RewriteLine>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        let path = fixture("rational_rewrites_1_0.jsonl");
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        text.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).unwrap_or_else(|e| panic!("row {line}: {e}")))
            .collect()
    })
}

/// The spelling of one rule, keyed by (answer, kind, rule).
fn rewrite_by_rule() -> &'static BTreeMap<(String, String, String), String> {
    static CACHE: std::sync::OnceLock<BTreeMap<(String, String, String), String>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        let mut map = BTreeMap::new();
        for row in committed_rewrites() {
            map.insert(
                (row.answer.clone(), row.kind.clone(), row.rule.clone()),
                row.learner.clone(),
            );
        }
        map
    })
}

/// The SymPy difference evidence of one pair, keyed by (expected, learner, kind).
///
/// The two flags are the only reason a pair may leave class 3 under the two
/// rewrite divergences. They are recorded facts about SymPy, not verdicts: a
/// verdict always comes from the 1.0 checker.
fn rewrite_evidence() -> &'static BTreeMap<PairKey, (bool, bool)> {
    static CACHE: std::sync::OnceLock<BTreeMap<PairKey, (bool, bool)>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        let mut map = BTreeMap::new();
        for row in committed_rewrites() {
            map.insert(
                (row.answer.clone(), row.learner.clone(), row.kind.clone()),
                (row.cancel_zero, row.radsimp_zero),
            );
        }
        map
    })
}

/// Whether the answer is one scalar value that holds a denominator or a radical.
///
/// The gate reads the TREE and never the source text: FIXM2g leaves `\frac`,
/// `√`, and `\sqrt` in the source, and a text test would miss every one of them.
///
/// A tuple, a set, a list, a range, an inequality, and a labeled value are all
/// refused, whatever they hold. SymPy reads `(1/2, 8)` as a plain Python tuple,
/// and `cancel` of a tuple returns the numerator, the denominator, and the terms
/// of a rational function, so the "spelling" would be a value no learner ever
/// writes and no rule of the family names.
fn holds_a_denominator_or_a_radical(ast: &Ast) -> bool {
    match ast {
        Ast::Fraction { .. } | Ast::Mixed { .. } | Ast::Div(_, _) | Ast::Sqrt(_) => true,
        Ast::Pow(base, exponent) => *exponent < 0 || holds_a_denominator_or_a_radical(base),
        Ast::Integer(_) | Ast::Decimal { .. } | Ast::Var(_) | Ast::Const(_) => false,
        Ast::Neg(inner) => holds_a_denominator_or_a_radical(inner),
        Ast::Add(items) | Ast::Mul(items) | Ast::Func(_, items) => {
            items.iter().any(holds_a_denominator_or_a_radical)
        }
        Ast::Tuple(_)
        | Ast::Set(_)
        | Ast::List(_)
        | Ast::Interval { .. }
        | Ast::Chain { .. }
        | Ast::Ineq { .. }
        | Ast::Assign { .. } => false,
    }
}

/// Whether SymPy reads the answer as the value 2.0 reads.
///
/// The family asks about a REWRITE, so it needs a spelling of the SAME value.
/// SymPy reads the answer through the 1.0 rewrite, and two constructs of the V4
/// table make that reading another value:
///
/// 1. A mixed number. 1.0 has no mixed-number reading, so `3 1/2` is the product
///    `3*(1/2)`, and `together` of that product is `3/2`.
/// 2. A spaced `x` as the times sign. 1.0 reads the letter as a free symbol, so
///    `3 x 10^-2` is `3*x/100` and not the number 0.03.
///
/// Both are documented divergences with literal pairs in
/// `crates/core/tests/answer_divergence.rs`. A spelling built on top of one of
/// them measures the base divergence again, and never the rewrite.
fn the_two_checkers_read_the_answer_alike(row: &Row) -> bool {
    if holds_a_mixed_number(&row.ast) {
        return false;
    }
    !(names_a_bare_word(&one_zero_source(&row.answer), "x") && !holds_the_variable_x(&row.answer))
}

/// Whether the answer tree holds a mixed number at any depth.
fn holds_a_mixed_number(ast: &Ast) -> bool {
    match ast {
        Ast::Mixed { .. } => true,
        Ast::Neg(inner) | Ast::Sqrt(inner) | Ast::Pow(inner, _) => holds_a_mixed_number(inner),
        Ast::Add(items)
        | Ast::Mul(items)
        | Ast::Func(_, items)
        | Ast::Tuple(items)
        | Ast::Set(items)
        | Ast::List(items) => items.iter().any(holds_a_mixed_number),
        Ast::Div(left, right) => holds_a_mixed_number(left) || holds_a_mixed_number(right),
        Ast::Interval { lo, hi, .. } | Ast::Chain { lo, hi, .. } => {
            holds_a_mixed_number(lo) || holds_a_mixed_number(hi)
        }
        Ast::Ineq { bound, .. } => holds_a_mixed_number(bound),
        Ast::Assign { value, .. } => holds_a_mixed_number(value),
        Ast::Integer(_) | Ast::Decimal { .. } | Ast::Fraction { .. } => false,
        Ast::Var(_) | Ast::Const(_) => false,
    }
}

/// The committed spelling of one rule, when the family applies to the row.
fn rewrite_spelling(row: &Row, rule: &str) -> Option<String> {
    if !holds_a_denominator_or_a_radical(&row.ast) {
        return None;
    }
    if !the_two_checkers_read_the_answer_alike(row) {
        return None;
    }
    let key = (
        row.answer.clone(),
        row.kind.as_str().to_string(),
        rule.to_string(),
    );
    let learner = rewrite_by_rule().get(&key)?.clone();
    let learner = changed(row, learner)?;
    changed_printed(row, learner)
}

/// Put the fractions of the answer over one common denominator.
fn generate_rewrite_together(row: &Row) -> Option<String> {
    rewrite_spelling(row, "together")
}

/// Split the answer into partial fractions.
fn generate_rewrite_apart(row: &Row) -> Option<String> {
    rewrite_spelling(row, "apart")
}

/// Cancel the common factors of the numerator and the denominator.
fn generate_rewrite_cancel(row: &Row) -> Option<String> {
    rewrite_spelling(row, "cancel")
}

/// Factor the answer.
fn generate_rewrite_factor(row: &Row) -> Option<String> {
    rewrite_spelling(row, "factor")
}

/// Multiply the answer out.
fn generate_rewrite_expand(row: &Row) -> Option<String> {
    rewrite_spelling(row, "expand")
}

/// Rationalize the radicals of the answer.
fn generate_rewrite_radsimp(row: &Row) -> Option<String> {
    rewrite_spelling(row, "radsimp")
}

fn generate_last_digit_bumped(row: &Row) -> Option<String> {
    bump_last_digit(&row.printed)
}

fn generate_sign_flipped(row: &Row) -> Option<String> {
    if row.printed.chars().all(|c| c == '0' || c == '-') {
        return None;
    }
    match row.printed.strip_prefix('-') {
        Some(rest) => Some(rest.to_string()),
        None => Some(format!("-{}", row.printed)),
    }
}

fn generate_digit_transposition(row: &Row) -> Option<String> {
    let chars: Vec<char> = row.printed.chars().collect();
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
    let (negative, digits) = integer_digits(&row.printed)?;
    if digits.len() > 12 || digits == "0" {
        return None;
    }
    let sign = if negative { "-" } else { "" };
    Some(format!("{sign}{digits}000"))
}

fn generate_over_thousand(row: &Row) -> Option<String> {
    let (negative, digits) = integer_digits(&row.printed)?;
    if digits.len() > 3 || digits == "0" {
        return None;
    }
    let sign = if negative { "-" } else { "" };
    Some(format!("{sign}0.{digits:0>3}"))
}

fn generate_coarse_decimal(row: &Row) -> Option<String> {
    let (numerator, denominator) = fraction_parts(&row.printed)?;
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
    let mut items = bracket_items(&row.printed, '(', ')')?;
    if items.len() < 2 || items.first() == items.get(1) {
        return None;
    }
    items.swap(0, 1);
    Some(format!("({})", items.join(", ")))
}

fn generate_set_element_changed(row: &Row) -> Option<String> {
    let items = bracket_items(&row.printed, '{', '}')?;
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
    bump_number_after(&row.printed, "sqrt(")
}

fn generate_wrong_exponent(row: &Row) -> Option<String> {
    bump_number_after(&row.printed, "**")
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
///
/// M2 review 3, finding 14, showed the same gap one level deeper: no family
/// rewrote a rational expression, so the pair set never reached the shape where
/// the two checkers disagree. The six `rewrite_*` families close it.
const GENERATORS: [Generator; 44] = [
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
    // The rational-rewrite family of M2 review 3, finding #14. Every one of the
    // six is a step a learner performs by hand on an answer with a denominator
    // or a radical, and SymPy writes the spelling. The six come last, so the
    // pair order of every older generator does not move.
    Generator {
        name: "rewrite_together",
        intent: Intent::Same,
        make: generate_rewrite_together,
    },
    Generator {
        name: "rewrite_apart",
        intent: Intent::Same,
        make: generate_rewrite_apart,
    },
    Generator {
        name: "rewrite_cancel",
        intent: Intent::Same,
        make: generate_rewrite_cancel,
    },
    Generator {
        name: "rewrite_factor",
        intent: Intent::Same,
        make: generate_rewrite_factor,
    },
    Generator {
        name: "rewrite_expand",
        intent: Intent::Same,
        make: generate_rewrite_expand,
    },
    Generator {
        name: "rewrite_radsimp",
        intent: Intent::Same,
        make: generate_rewrite_radsimp,
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
const DOCUMENTED_REASONS: [&str; 14] = [
    // The two narrowings of the canonical rational form (FIXM2h). Both mark a
    // correct learner WRONG in 2.0, and both carry a SPECIFIC predicate: the
    // recorded SymPy evidence must say the difference is zero, AND the two
    // canonical forms must differ in the named place.
    "no polynomial GCD (V1 narrowing)",
    "no radical rationalization (V1 narrowing)",
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
    "the 1.0 namespace reads a bare `e` as a free symbol (spec 3.1)",
    // Two rulings of the review rounds. Both are DIVERGENCES and not defects:
    // 2.0 reads a construct that 1.0 hands to SymPy as a symbol.
    "2.0 reads a spaced `x` as the times sign (review 1, finding 18)",
    "the juxtaposed argument stops at a function name (review 3, finding 5)",
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
    // The rung reads the 1.0 rewrite of the two answers, and it never reads the
    // 2.0 normalized source. FIXM2g left every construct in that source as a
    // token, so `√`, `\pi`, and `^` stay in it; a reader that took it for SymPy
    // source read the wrong string (M2 review 3, the FIXM2i ruling). 1.0
    // evaluates `to_sympy_source(text)` (`sympy_check.py:78`), so the harness
    // ports that rewrite and reads its result.
    let expected_source = one_zero_source(&pair.expected);
    let learner_source = one_zero_source(&pair.learner);
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
///
/// The caller passes the 1.0 rewrite of [`one_zero_source`], which is the exact
/// string `sympy_check.py:180` hands to `float()`.
fn reads_as_a_python_float(source: &str) -> bool {
    integer_digits(source).is_some() || decimal_parts(source).is_some()
}

/// The harness port of 1.0 `to_sympy_source` (`sympy_check.py:78-105`).
///
/// The function writes the string that the two 1.0 float rungs evaluate. It is a
/// port of 1.0, not a call into 2.0: the harness must not ask the code under test
/// what the other checker reads.
///
/// The steps keep the order of the 1.0 function, because the order decides the
/// result: the caret becomes `**` before the backslash goes away, and the radical
/// takes its group before the plain glyph table runs.
fn one_zero_source(text: &str) -> String {
    let mut out = text.trim().to_string();
    if out.chars().count() > 1 && out.starts_with('$') && out.ends_with('$') {
        out = out
            .get(1..out.len().saturating_sub(1))
            .unwrap_or_default()
            .to_string();
    }
    out = out.trim_end_matches('.').trim().to_string();
    out = out.replace('^', "**");
    out = out.replace("\\cdot", "*").replace("\\times", "*");
    out = out.replace("\\left", "").replace("\\right", "");
    out = out.replace('\\', "");
    out = out.replace('×', "*").replace('÷', "/");
    out = radical_to_call(&out);
    out = superscript_to_power(&out);
    for (glyph, ascii) in UNICODE_TO_ASCII {
        // The superscript rows of the table write a caret, and the step above
        // already wrote the `**` that 1.0 writes there.
        if ascii.starts_with('^') {
            continue;
        }
        out = out.replace(glyph, ascii);
    }
    strip_thousands_groups(&out)
}

/// Rewrite `x²` into `x**2` (1.0 `_unicode_math_to_ascii`).
fn superscript_to_power(text: &str) -> String {
    const SUPERSCRIPTS: [(char, char); 10] = [
        ('⁰', '0'),
        ('¹', '1'),
        ('²', '2'),
        ('³', '3'),
        ('⁴', '4'),
        ('⁵', '5'),
        ('⁶', '6'),
        ('⁷', '7'),
        ('⁸', '8'),
        ('⁹', '9'),
    ];
    let digit_of = |ch: char| {
        SUPERSCRIPTS
            .iter()
            .find(|(glyph, _)| *glyph == ch)
            .map(|(_, digit)| *digit)
    };
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut index = 0;
    while let Some(ch) = chars.get(index).copied() {
        let Some(digit) = digit_of(ch) else {
            out.push(ch);
            index += 1;
            continue;
        };
        // The 1.0 pattern needs a word character or a `)` in front of the run.
        let anchored = matches!(out.chars().last(), Some(previous)
            if previous.is_alphanumeric() || previous == '_' || previous == ')');
        if !anchored {
            out.push(ch);
            index += 1;
            continue;
        }
        out.push_str("**");
        out.push(digit);
        index += 1;
        while let Some(next) = chars.get(index).copied().and_then(digit_of) {
            out.push(next);
            index += 1;
        }
    }
    out
}

/// Delete the thousands separators of a plain grouped integer (1.0
/// `_COMMA_GROUPS_RE` and `_SPACE_GROUPS_RE`, both applied as a full match).
fn strip_thousands_groups(text: &str) -> String {
    let trimmed = text.trim();
    for separator in [",", " ", "\u{00a0}", "\u{202f}", "\u{2009}", "\u{2007}"] {
        let Some((negative, digits)) = split_groups(trimmed, separator) else {
            continue;
        };
        return if negative {
            format!("-{digits}")
        } else {
            digits
        };
    }
    text.to_string()
}

/// Read `-?\d{1,3}(SEP\d{3})+` as a whole, and return the sign and the digits.
fn split_groups(text: &str, separator: &str) -> Option<(bool, String)> {
    let (negative, rest) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let parts: Vec<&str> = rest.split(separator).collect();
    if parts.len() < 2 {
        return None;
    }
    let head = parts.first()?;
    if head.is_empty() || head.len() > 3 || !head.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    for group in parts.get(1..)? {
        if group.len() != 3 || !group.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
    }
    Some((negative, parts.concat()))
}

/// The radical atoms of one canonical form, as sorted text.
///
/// The set holds every root the value carries: an `Atom::Sqrt` of an integer, an
/// `Atom::Call` of the name `sqrt` over a value the grammar keeps whole, and the
/// radicand of a [`Basis`] that is not 1.
fn radical_atoms(canon: &Canon) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    collect_radical_atoms(canon, &mut out);
    out
}

/// Walk one canonical form and collect its radical atoms.
fn collect_radical_atoms(canon: &Canon, out: &mut BTreeSet<String>) {
    match canon {
        Canon::Rational(_) => {}
        Canon::Radical(terms) => {
            for basis in terms.keys() {
                if !basis.radicand.is_one() {
                    out.insert(format!("sqrt({})", basis.radicand));
                }
            }
        }
        Canon::Poly(poly) => collect_poly_radicals(poly, out),
        Canon::Value { num, den } => {
            collect_poly_radicals(num, out);
            collect_poly_radicals(den, out);
        }
        Canon::Func(_, args) | Canon::Tuple(args) | Canon::List(args) => {
            for arg in args {
                collect_radical_atoms(arg, out);
            }
        }
        Canon::Set(members) => {
            for member in members {
                collect_radical_atoms(member, out);
            }
        }
        Canon::Interval { lo, hi, .. } => {
            for end in [lo, hi].into_iter().flatten() {
                collect_radical_atoms(end, out);
            }
        }
        Canon::Assign { value, .. } => collect_radical_atoms(value, out),
    }
}

/// Collect the radical atoms of one polynomial.
fn collect_poly_radicals(poly: &cadus_core::answer::Poly, out: &mut BTreeSet<String>) {
    for (atom, exponent) in poly.keys().flatten() {
        match atom {
            Atom::Sqrt(radicand) => {
                out.insert(format!("sqrt({radicand})**{exponent}"));
            }
            Atom::Call(name, args) if name == "sqrt" => {
                out.insert(format!("sqrt({args:?})**{exponent}"));
                for arg in args {
                    collect_radical_atoms(arg, out);
                }
            }
            Atom::Call(_, args) => {
                for arg in args {
                    collect_radical_atoms(arg, out);
                }
            }
            Atom::Exp(inner) => collect_radical_atoms(inner, out),
            Atom::Pi | Atom::E | Atom::Var(_) => {}
        }
    }
}

/// The denominator of one canonical form, as text. Empty means "no denominator".
///
/// A [`Canon::Value`] carries its denominator in the `den` field. A
/// [`Canon::Poly`] carries a MONOMIAL denominator as the negative exponents of
/// its atoms, because FIXM2h moves a monomial divisor into the numerator.
fn denominator_key(canon: &Canon) -> String {
    match canon {
        Canon::Value { den, .. } => format!("{den:?}"),
        Canon::Poly(poly) => {
            let mut divisors: BTreeSet<String> = BTreeSet::new();
            for (atom, exponent) in poly.keys().flatten() {
                if *exponent < 0 {
                    divisors.insert(format!("{atom:?}**{}", -exponent));
                }
            }
            if divisors.is_empty() {
                String::new()
            } else {
                divisors.into_iter().collect::<Vec<String>>().join("*")
            }
        }
        _ => String::new(),
    }
}

/// The two canonical forms of one pair, when the checker decides both.
fn both_canonical_forms(pair: &Pair) -> Option<(Canon, Canon)> {
    let expected = canonical_form(&pair.expected).ok()?;
    let learner = canonical_form(&pair.learner).ok()?;
    Some((expected, learner))
}

/// Whether 2.0 refuses the pair because it runs no polynomial GCD (FIXM2h).
///
/// The predicate is SPECIFIC, and it names two facts that must both hold:
///
/// 1. `scripts/oracle/rewrite_1_0.py` recorded `cancel(expected - learner) == 0`,
///    so the two answers are one value and the difference needs a polynomial GCD.
/// 2. The two canonical forms differ IN A DENOMINATOR.
///
/// A pair that misses either fact keeps no reason. It stays in class 3, and it
/// fails the parity assertion as a 2.0 bug (R5). No catch-all sits here.
fn no_polynomial_gcd(pair: &Pair) -> bool {
    let key = (
        pair.expected.clone(),
        pair.learner.clone(),
        pair.kind.as_str().to_string(),
    );
    let Some((cancel_zero, _)) = rewrite_evidence().get(&key).copied() else {
        return false;
    };
    if !cancel_zero {
        return false;
    }
    let Some((expected, learner)) = both_canonical_forms(pair) else {
        return false;
    };
    let left = denominator_key(&expected);
    let right = denominator_key(&learner);
    left != right && !(left.is_empty() && right.is_empty())
}

/// Whether 2.0 refuses the pair because it rationalizes no radical (FIXM2h).
///
/// The predicate is SPECIFIC, and it names two facts that must both hold:
///
/// 1. `scripts/oracle/rewrite_1_0.py` recorded `radsimp(expected - learner) == 0`,
///    so the two answers are one value under the radical laws.
/// 2. The two canonical forms differ IN A RADICAL ATOM.
///
/// `sqrt(x)*sqrt(x)` is the shape: 2.0 keeps two `sqrt(x)` atoms and never folds
/// them into `x`, because the fold holds for a non-negative `x` only.
fn no_radical_rationalization(pair: &Pair) -> bool {
    let key = (
        pair.expected.clone(),
        pair.learner.clone(),
        pair.kind.as_str().to_string(),
    );
    let Some((_, radsimp_zero)) = rewrite_evidence().get(&key).copied() else {
        return false;
    };
    if !radsimp_zero {
        return false;
    }
    let Some((expected, learner)) = both_canonical_forms(pair) else {
        return false;
    };
    let left = radical_atoms(&expected);
    let right = radical_atoms(&learner);
    left != right && !(left.is_empty() && right.is_empty())
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
        // The two narrowings of the canonical rational form, in a fixed order.
        // A radical shape takes the radical reason, and every other shape takes
        // the GCD reason, so one pair gets one reason.
        if no_radical_rationalization(pair) {
            return Some("no radical rationalization (V1 narrowing)");
        }
        if no_polynomial_gcd(pair) {
            return Some("no polynomial GCD (V1 narrowing)");
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
        if one_side_alone_names_a_bare_e(pair) {
            return Some("the 1.0 namespace reads a bare `e` as a free symbol (spec 3.1)");
        }
        if a_spaced_times_x(pair) {
            return Some("2.0 reads a spaced `x` as the times sign (review 1, finding 18)");
        }
        if a_bracket_free_argument_meets_a_function(&pair.expected)
            || a_bracket_free_argument_meets_a_function(&pair.learner)
        {
            return Some("the juxtaposed argument stops at a function name (review 3, finding 5)");
        }
    }
    None
}

/// The names the 2.0 grammar reads as functions (`answer::parse`, `FUNCTIONS`).
///
/// The list is a literal copy, so a change in the grammar cannot quietly change
/// the reason a pair leaves class 3.
const FUNCTION_NAMES: [&str; 17] = [
    "sqrt", "sin", "cos", "tan", "sec", "csc", "cot", "asin", "acos", "atan", "sinh", "cosh",
    "tanh", "exp", "ln", "log", "abs",
];

/// Whether one side writes a times sign as the spaced letter `x` (or `X`).
///
/// 2.0 reads a spaced `x` between two values as multiplication (M2 review 1,
/// finding #18), and 1.0 hands the letter to SymPy as a free symbol. The
/// predicate is specific: the 1.0 source of the side must name the bare letter,
/// AND the 2.0 value of that same side must carry no variable of that name. A
/// side that really does hold the variable `x` keeps no reason here.
fn a_spaced_times_x(pair: &Pair) -> bool {
    [&pair.expected, &pair.learner]
        .into_iter()
        .any(|side| names_a_bare_word(&one_zero_source(side), "x") && !holds_the_variable_x(side))
}

/// Whether the 2.0 value of `answer` carries the variable `x`.
fn holds_the_variable_x(answer: &str) -> bool {
    let Ok(canon) = canonical_form(answer) else {
        return false;
    };
    let mut names = BTreeSet::new();
    collect_variable_names(&canon, &mut names);
    names.contains("x")
}

/// Collect the variable names of one canonical form.
fn collect_variable_names(canon: &Canon, out: &mut BTreeSet<String>) {
    match canon {
        Canon::Rational(_) | Canon::Radical(_) => {}
        Canon::Poly(poly) => collect_poly_variables(poly, out),
        Canon::Value { num, den } => {
            collect_poly_variables(num, out);
            collect_poly_variables(den, out);
        }
        Canon::Func(_, args) | Canon::Tuple(args) | Canon::List(args) => {
            for arg in args {
                collect_variable_names(arg, out);
            }
        }
        Canon::Set(members) => {
            for member in members {
                collect_variable_names(member, out);
            }
        }
        Canon::Interval { var, lo, hi, .. } => {
            if let Some(name) = var {
                out.insert(name.clone());
            }
            for end in [lo, hi].into_iter().flatten() {
                collect_variable_names(end, out);
            }
        }
        Canon::Assign { value, .. } => collect_variable_names(value, out),
    }
}

/// Collect the variable names of one polynomial.
fn collect_poly_variables(poly: &cadus_core::answer::Poly, out: &mut BTreeSet<String>) {
    for (atom, _) in poly.keys().flatten() {
        match atom {
            Atom::Var(name) => {
                out.insert(name.clone());
            }
            Atom::Call(_, args) => {
                for arg in args {
                    collect_variable_names(arg, out);
                }
            }
            Atom::Exp(inner) => collect_variable_names(inner, out),
            Atom::Sqrt(_) | Atom::Pi | Atom::E => {}
        }
    }
}

/// Whether `text` holds the whole word `word`.
fn names_a_bare_word(text: &str, word: &str) -> bool {
    let mut run = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            run.push(ch);
            continue;
        }
        if run.eq_ignore_ascii_case(word) {
            return true;
        }
        run.clear();
    }
    run.eq_ignore_ascii_case(word)
}

/// Whether `text` writes a bracket-free function argument that a second function
/// name follows.
///
/// `sec x tan x` is the shape. 2.0 stops the argument at the second name and
/// reads `sec(x)*tan(x)`; SymPy `implicit_multiplication_application` swallows
/// the name and reads `sec(x*tan(x))` (M2 review 3, finding #5, and the ruling of
/// that round). The predicate names the construct, and it fires on no other:
/// the first name must carry NO bracket, and a second function name must follow
/// it before any bracket or operator.
fn a_bracket_free_argument_meets_a_function(text: &str) -> bool {
    let words = word_runs(text);
    for (index, (word, follows_open)) in words.iter().enumerate() {
        if !FUNCTION_NAMES.contains(&word.as_str()) || *follows_open {
            continue;
        }
        // The argument runs on until the next function name, so the second name
        // is the first one that follows, at any distance.
        let follows = words
            .get(index + 1..)
            .unwrap_or_default()
            .iter()
            .any(|(later, _)| FUNCTION_NAMES.contains(&later.as_str()));
        if follows {
            return true;
        }
    }
    false
}

/// The word runs of `text`, each with a flag for a `(` that follows it.
fn word_runs(text: &str) -> Vec<(String, bool)> {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<(String, bool)> = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars.get(index).copied().unwrap_or(' ');
        if !ch.is_ascii_alphabetic() {
            index += 1;
            continue;
        }
        let start = index;
        while matches!(chars.get(index), Some(c) if c.is_ascii_alphanumeric() || *c == '_') {
            index += 1;
        }
        let word: String = chars.get(start..index).unwrap_or_default().iter().collect();
        let mut scan = index;
        while matches!(chars.get(scan), Some(c) if c.is_whitespace()) {
            scan += 1;
        }
        out.push((word, chars.get(scan) == Some(&'(')));
    }
    out
}

/// Whether one side alone writes Euler's number as the bare name `e`.
///
/// `_safe_sympy_globals` runs `from sympy import *`, and that namespace holds
/// `E` and `exp` and no lowercase `e` (`sympy_check.py:170`). `parse_expr` then
/// reads `e` as a free symbol, so 1.0 grades `e**2` against `exp(2)` False. 2.0
/// reads `e` and `E` as one constant (`crates/core/tests/answer_divergence.rs`,
/// `e_is_eulers_number_on_both_sides`).
///
/// The predicate is specific: it fires only when ONE side carries the bare name.
/// Two sides that both carry it reach the same free symbol in 1.0, so 1.0 and
/// 2.0 agree on that pair and the divergence has another cause.
fn one_side_alone_names_a_bare_e(pair: &Pair) -> bool {
    names_a_bare_e(&one_zero_source(&pair.expected))
        != names_a_bare_e(&one_zero_source(&pair.learner))
}

/// Whether `source` names Euler's number as the bare token `e`.
///
/// A letter in front of the `e` makes it part of a longer name (`sec`, `exp`).
/// A digit in front does NOT: SymPy `implicit_multiplication` splits `3e**x`
/// into `3*e**x`, measured on 2026-08-27. A digit AFTER the `e` makes the whole
/// run a float literal, and `3e5` is the number 300000.
fn names_a_bare_e(source: &str) -> bool {
    let chars: Vec<char> = source.chars().collect();
    for (index, ch) in chars.iter().enumerate() {
        if *ch != 'e' {
            continue;
        }
        let before = index
            .checked_sub(1)
            .and_then(|i| chars.get(i))
            .copied()
            .unwrap_or(' ');
        let after = chars.get(index + 1).copied().unwrap_or(' ');
        if !before.is_alphabetic() && before != '_' && !after.is_alphanumeric() && after != '_' {
            return true;
        }
    }
    false
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
    /// One line per pair of each documented reason, for the review record.
    per_reason_pairs: BTreeMap<&'static str, Vec<String>>,
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
        per_reason_pairs: BTreeMap::new(),
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
            report
                .per_reason_pairs
                .entry(reason)
                .or_default()
                .push(format!(
                    "{}: {:?} against {:?}",
                    pair.generator, pair.expected, pair.learner
                ));
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
    // The two rewrite narrowings are the new divergences of FIXM2i, and the M2
    // plan quotes their pairs, so the report names every one of them.
    for reason in [
        "no polynomial GCD (V1 narrowing)",
        "no radical rationalization (V1 narrowing)",
    ] {
        for line in report.per_reason_pairs.get(reason).into_iter().flatten() {
            println!("[{reason}] {line}");
        }
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
const GENERATED_PAIRS: usize = 17_874;

/// The literal pair count of every generator, in name order.
const GENERATOR_COUNTS: [(&str, usize); 47] = [
    ("algebraic_refactor", 65),
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
    ("explicit_multiplication", 351),
    ("figure_space_thousands", 44),
    ("fraction_to_decimal", 79),
    ("identity", 1562),
    ("implicit_multiplication", 270),
    ("internal_spaces", 1063),
    ("last_digit_bumped", 1519),
    ("narrow_space_thousands", 44),
    ("nbsp_thousands", 44),
    ("over_thousand", 256),
    ("plus_spaced", 337),
    ("product_reorder", 276),
    ("rewrite_apart", 50),
    ("rewrite_cancel", 78),
    ("rewrite_expand", 25),
    ("rewrite_factor", 27),
    ("rewrite_radsimp", 17),
    ("rewrite_together", 116),
    ("set_element_changed", 9),
    ("set_reordered", 11),
    ("sign_flipped", 1559),
    ("significant_decimal", 248),
    ("space_thousands", 44),
    ("star_power", 333),
    ("sum_reorder", 206),
    ("thin_space_thousands", 44),
    ("times_thousand", 300),
    ("trailing_period", 1562),
    ("trailing_zero", 408),
    ("tuple_swapped", 183),
    ("unicode_to_ascii", 54),
    ("whitespace_padding", 1562),
    ("wrong_exponent", 175),
    ("wrong_radicand", 37),
];

/// The literal pair count of every divergence class.
///
/// The counts are measured against the live 1.0 checker, not read back from the
/// committed file.
const CLASS_COUNTS: [(&str, usize); 5] = [
    ("class 1 outside_grammar", 965),
    ("class 2 prose_expected", 0),
    ("class 3 comparable", 16554),
    ("class 4 documented_divergence", 355),
    ("oracle_silent", 0),
];

/// The literal pair count of every documented divergence reason.
///
/// The reasons come in three groups.
///
/// 1. The two narrowings of the canonical rational form (FIXM2h). Both carry 6
///    pairs, and `print_report` names every one of them, so the M2 plan quotes
///    them by their text. Both mark a correct learner WRONG in 2.0.
/// 2. The four reasons `docs/plans/M2.md` names. This generated set reaches none
///    of them except the float rung: prose never enters the set (the set holds
///    only in-grammar answers), a SymPy name such as `zoo` leaves 2.0
///    undecidable, which is class 1, and no predicate claims a 1.0
///    simplification. All three stay pinned by literal pairs in
///    `crates/core/tests/answer_divergence.rs`.
/// 3. The 1.0 defects and the grammar rulings that this set reaches. Every one of
///    them marks a correct learner WRONG in 1.0, and 2.0 decides it correctly.
///
/// The float rung carries 240 pairs, and 238 of them come from the
/// `significant_decimal` family that spec section 9.3 names: a rational with no
/// exact decimal, a radical, `pi`, or `e`, against its own value in ten
/// significant digits. 1.0 grades every one of them True on a float rung, and
/// 2.0 grades them False (D6). `crates/core/tests/answer_divergence.rs` pins one
/// pair of each shape.
const REASON_COUNTS: [(&str, usize); 14] = [
    ("no polynomial GCD (V1 narrowing)", 6),
    ("no radical rationalization (V1 narrowing)", 6),
    ("no float tolerance rung (D6)", 240),
    ("a transcendental identity is not simplified (V1)", 0),
    ("prose is not a value (V2)", 0),
    ("a SymPy name is not a value (V2)", 0),
    (
        "the 1.0 exponent-tower guard refuses a legal power (spec 5.1)",
        11,
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
        3,
    ),
    (
        "the 1.0 namespace reads a bare `e` as a free symbol (spec 3.1)",
        46,
    ),
    (
        "2.0 reads a spaced `x` as the times sign (review 1, finding 18)",
        24,
    ),
    (
        "the juxtaposed argument stops at a function name (review 3, finding 5)",
        4,
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
    // A juxtaposed name is a product, and a bracket-free radical is a call,
    // because 1.0 parses with `implicit_multiplication_application`
    // (`sympy_check.py:253`): `3pi` is `3*pi` and `2sqrt 2 - 2` is
    // `2*sqrt(2) - 2`. FIXM2i: the old reader refused both, and the float rung
    // then explained none of the 78 `pi` and radical pairs of the
    // `significant_decimal` family.
    assert_eq!(numeric_value("3pi"), Some(3.0 * std::f64::consts::PI));
    assert_eq!(numeric_value("2sqrt(3)"), Some(2.0 * 3.0_f64.sqrt()));
    assert_eq!(numeric_value("2*sqrt 2"), Some(2.0 * 2.0_f64.sqrt()));
    assert_eq!(numeric_value("2(3)"), Some(6.0));
    // A free symbol, an unknown name, an implicit product of a number and a
    // free symbol, an unbalanced bracket, and a division by zero are all
    // refusals.
    assert_eq!(numeric_value("x"), None);
    assert_eq!(numeric_value("2*cos(0)"), None);
    assert_eq!(numeric_value("2x"), None);
    assert_eq!(numeric_value("(1 + 2"), None);
    assert_eq!(numeric_value("1/0"), None);
    // Two numbers that only touch are two answers, not one.
    assert_eq!(numeric_value("1 2"), None);
}

/// Build one corpus row for a generator test.
fn probe_row(answer: &str, shape: &str) -> Row {
    let source = normalize(answer).source;
    let ast = parse(&source).unwrap_or_else(|e| panic!("parse {answer:?}: {e}"));
    let printed = print_ast(&ast, PREC_LOWEST);
    Row {
        answer: answer.to_string(),
        source,
        printed,
        ast,
        canon: canonical_form(answer).ok(),
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
    // FIXM2i widened the gate to every irrational number the grammar holds, so
    // `pi` and `e` join the roots. The old gate read the source for `sqrt(`, and
    // it missed both, and it missed `√` after FIXM2g.
    assert_eq!(
        generate_significant_decimal(&probe_row("2π", "expression_numeric")),
        Some("6.283185307".to_string())
    );
    assert_eq!(
        generate_significant_decimal(&probe_row("$3\\pi$", "expression_numeric")),
        Some("9.424777961".to_string())
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
    // The family reads the PRINTED tree, so the juxtaposed `2(x + 3)` of the
    // author reaches the rule as the product `2*(x + 3)` (FIXM2i).
    assert_eq!(
        generate_algebraic_refactor(&probe_row("2(x + 3)(x - 3)", "expression_symbolic")),
        Some("2*(x + 3)*(x) - 2*(x + 3)*(3)".to_string())
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

#[test]
fn the_printer_writes_the_tree_and_not_the_spelling() {
    // FIXM2g leaves every construct in the source as a token, so the printed
    // tree is the only ASCII reading of an answer the builders can trust.
    let printed = |answer: &str| probe_row(answer, "expression_symbolic").printed;
    assert_eq!(printed("\\frac{1}{2}"), "1/2");
    assert_eq!(printed("½"), "1/2");
    assert_eq!(printed("$(-2, 5\\pi/4)$"), "(-2, 5*pi/4)");
    assert_eq!(printed("$(1, \\sqrt 3)$"), "(1, sqrt(3))");
    assert_eq!(printed("36x^2y^2"), "36*x**2*y**2");
    assert_eq!(printed("15√3"), "15*sqrt(3)");
    assert_eq!(printed("x^3 - 6x^2 + 12x - 8"), "x**3 - 6*x**2 + 12*x - 8");
    assert_eq!(printed("1/(x*(x + 1))"), "1/(x*(x + 1))");
    assert_eq!(printed("7.2 x 10^-4"), "7.2*10**(-4)");
    // A mixed number prints as a bracketed sum, because 1.0 reads the bare
    // juxtaposition `2 1/2` as the product `2*(1/2)`.
    assert_eq!(printed("2\\frac{1}{2}"), "(2 + 1/2)");
    assert_eq!(printed("-1 ≤ x ≤ 3"), "-1 <= x <= 3");
    assert_eq!(printed("{1, 3, 5}"), "{1, 3, 5}");
}

#[test]
fn the_rewrite_family_reads_the_committed_sympy_spelling() {
    // The six families take their spelling from the committed fixture, and the
    // fixture holds the `str()` of the SymPy rule. The four rows below are
    // literals of `crates/core/tests/fixtures/answers/rational_rewrites_1_0.jsonl`.
    let row = probe_row("2/x + 1/(x + 1)", "expression_symbolic");
    assert_eq!(
        generate_rewrite_together(&row),
        Some("(3*x + 2)/(x*(x + 1))".to_string())
    );
    assert_eq!(
        generate_rewrite_cancel(&row),
        Some("(3*x + 2)/(x**2 + x)".to_string())
    );
    let radical = probe_row("1/(2√x)", "expression_symbolic");
    assert_eq!(
        generate_rewrite_radsimp(&radical),
        Some("sqrt(x)/(2*x)".to_string())
    );
    // An answer with no denominator and no radical takes no spelling, whatever
    // the fixture holds.
    let whole = probe_row("x**2 - 1", "expression_symbolic");
    assert_eq!(generate_rewrite_factor(&whole), None);
    // A mixed number takes none either: 1.0 reads `3 1/2` as `3*(1/2)`, so the
    // gate keeps the whole row out of the file and out of the family.
    let mixed = probe_row("3 1/2", "fraction");
    assert!(!the_two_checkers_read_the_answer_alike(&mixed));
    assert_eq!(generate_rewrite_together(&mixed), None);
    // A spaced `x` is the times sign in 2.0 and a free symbol in 1.0.
    let times = probe_row("7.2 x 10^-4", "decimal");
    assert!(!the_two_checkers_read_the_answer_alike(&times));
    assert_eq!(generate_rewrite_together(&times), None);
    // The two rows above are the only shapes the gate refuses. A plain rational
    // and a radical both pass it.
    assert!(the_two_checkers_read_the_answer_alike(&row));
    assert!(the_two_checkers_read_the_answer_alike(&radical));
    assert!(the_two_checkers_read_the_answer_alike(&whole));
}

#[test]
fn the_two_rewrite_narrowings_need_the_recorded_sympy_evidence() {
    let one_zero_says_yes = OracleVerdict {
        equivalent: true,
        notation: false,
    };
    // A denominator that needs a polynomial GCD. The evidence line of the
    // fixture says `cancel(expected - learner) == 0`, and the two canonical
    // forms differ in a denominator.
    let gcd = probe_pair(
        "-4(x + 1)/(x - 1)^3",
        "-4/(x - 1)**2 - 8/(x - 1)**3",
        "expression_symbolic",
    );
    assert_eq!(
        documented_reason(&gcd, false, one_zero_says_yes),
        Some("no polynomial GCD (V1 narrowing)")
    );
    // A radical the rewrite moves out of the denominator.
    let radical = probe_pair("1/(2√x)", "sqrt(x)/(2*x)", "expression_symbolic");
    assert_eq!(
        documented_reason(&radical, false, one_zero_says_yes),
        Some("no radical rationalization (V1 narrowing)")
    );
    // The SAME two answers, without the recorded evidence, keep no reason: the
    // predicate is not "the two canonical forms differ".
    let unrecorded = probe_pair("1/(2*sqrt(x))", "sqrt(x)/(2*x)", "expression_symbolic");
    assert_eq!(
        documented_reason(&unrecorded, false, one_zero_says_yes),
        None
    );
    // A pair of the fixture whose canonical forms agree in every denominator and
    // every radical atom keeps no reason either. The fixture records
    // `radsimp_zero: true` for the row below, and neither side holds a radical,
    // so the canonical-form test is the one that refuses the reason.
    let no_radical = probe_pair("$(4/3)\\sin 3t$", "4*sin(3*t)/3", "expression_symbolic");
    assert_eq!(
        documented_reason(&no_radical, false, one_zero_says_yes),
        None
    );
    let same_shape = probe_pair(
        "2/x + 1/(x + 1)",
        "(3*x + 2)/(x*(x + 1))",
        "expression_symbolic",
    );
    assert_eq!(
        documented_reason(&same_shape, false, one_zero_says_yes),
        None
    );
}

#[test]
fn the_three_grammar_rulings_name_their_own_pairs() {
    let one_zero_says_no = OracleVerdict {
        equivalent: false,
        notation: false,
    };
    // 1.0 has no lowercase `e`, so `3e**x` is a free symbol to the power `x`.
    let euler = probe_pair("3e^(3x)", "3exp(3x)", "expression_symbolic");
    assert_eq!(
        documented_reason(&euler, true, one_zero_says_no),
        Some("the 1.0 namespace reads a bare `e` as a free symbol (spec 3.1)")
    );
    // Two sides that both write the bare `e` reach one free symbol in 1.0.
    let both_sides = probe_pair("3e^(3x)", "3*e^(3*x)", "expression_symbolic");
    assert_eq!(documented_reason(&both_sides, true, one_zero_says_no), None);
    // The spaced times sign.
    let times = probe_pair("2 x 2 x 3", "2*3*2", "expression_symbolic");
    assert_eq!(
        documented_reason(&times, true, one_zero_says_no),
        Some("2.0 reads a spaced `x` as the times sign (review 1, finding 18)")
    );
    // An answer that really holds the variable `x` keeps no times-sign reason.
    let variable = probe_pair("2*x", "x*2", "expression_symbolic");
    assert_eq!(documented_reason(&variable, true, one_zero_says_no), None);
    // The juxtaposed argument that stops at a function name.
    let chain = probe_pair("sec x tan x", "tan(x)*sec(x)", "expression_symbolic");
    assert_eq!(
        documented_reason(&chain, true, one_zero_says_no),
        Some("the juxtaposed argument stops at a function name (review 3, finding 5)")
    );
    // One function name with a bracket-free argument is no chain.
    let single = probe_pair("ln x + C", "C + log(x)", "expression_symbolic");
    assert_eq!(documented_reason(&single, true, one_zero_says_no), None);
    // Two function names that both carry their own brackets are no chain either:
    // SymPy swallows nothing there, and both checkers read one product.
    let bracketed = probe_pair("sin(x)*cos(x)", "cos(x)*sin(x)", "expression_symbolic");
    assert_eq!(documented_reason(&bracketed, true, one_zero_says_no), None);
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
fn every_documented_reason_is_one_of_the_named_fourteen() {
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

/// Ask the live rewrite helper for the spellings of every row that qualifies.
///
/// The helper is `scripts/oracle/rewrite_1_0.py`. It runs one request at a time,
/// and the caller reads one response before it writes the next one, so neither
/// pipe ever fills.
fn live_rewrites(python: &str) -> Vec<RewriteLine> {
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Command, Stdio};

    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/oracle/rewrite_1_0.py")
        .canonicalize()
        .unwrap_or_else(|e| panic!("find scripts/oracle/rewrite_1_0.py: {e}"));
    let mut child = Command::new(python)
        .arg(&script)
        .arg("--timeout")
        .arg("10.0")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("start {}: {e}", script.display()));
    let mut stdin = child.stdin.take().unwrap_or_else(|| panic!("no stdin"));
    let stdout = child.stdout.take().unwrap_or_else(|| panic!("no stdout"));
    let mut reader = BufReader::new(stdout);
    let mut ready = String::new();
    reader
        .read_line(&mut ready)
        .unwrap_or_else(|e| panic!("read the ready line: {e}"));
    assert!(ready.contains("\"ready\""), "the helper said {ready:?}");

    let mut ask = |request: &serde_json::Value| -> serde_json::Value {
        writeln!(stdin, "{request}").unwrap_or_else(|e| panic!("write the request: {e}"));
        stdin.flush().unwrap_or_else(|e| panic!("flush: {e}"));
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .unwrap_or_else(|e| panic!("read the response: {e}"));
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("response {line}: {e}"))
    };

    let mut out: Vec<RewriteLine> = Vec::new();
    let mut offered = 0_usize;
    let mut refused_by_1_0 = 0_usize;
    for row in in_grammar_rows() {
        if !holds_a_denominator_or_a_radical(&row.ast)
            || !the_two_checkers_read_the_answer_alike(&row)
        {
            continue;
        }
        offered += 1;
        let response = ask(&serde_json::json!({
            "op": "rewrite",
            "answer": row.answer,
        }));
        // 1.0 refuses some corpus answers itself (`\frac{1}{2}` reaches SymPy as
        // `frac{1}{2}`). The helper reports the error, and the row carries no
        // spelling. That is a 1.0 defect, and other tests pin it.
        if response.get("error").is_some() {
            refused_by_1_0 += 1;
            continue;
        }
        let spellings = response
            .get("spellings")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        for spelling in spellings {
            let rule = spelling
                .get("rule")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            let learner = spelling
                .get("text")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            if !REWRITE_RULES.contains(&rule.as_str()) {
                panic!("the helper wrote the unknown rule {rule:?}");
            }
            // A spelling that repeats the answer or the printed tree is no
            // variant, and it would only repeat a pair another family builds.
            if learner == row.answer || learner == row.printed {
                continue;
            }
            let difference = ask(&serde_json::json!({
                "op": "difference",
                "expected": row.answer,
                "learner": learner,
            }));
            let flag = |name: &str| {
                difference
                    .get(name)
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false)
            };
            out.push(RewriteLine {
                answer: row.answer.clone(),
                kind: row.kind.as_str().to_string(),
                rule,
                learner,
                cancel_zero: flag("cancel_zero"),
                radsimp_zero: flag("radsimp_zero"),
            });
        }
    }
    drop(stdin);
    let _ = child.wait();
    println!(
        "rewrite: {offered} rows offered, {refused_by_1_0} refused by the 1.0 parser, \
         {} spellings kept",
        out.len()
    );
    out.sort_by(|left, right| {
        (&left.answer, &left.kind, &left.rule).cmp(&(&right.answer, &right.kind, &right.rule))
    });
    out
}

/// Write `rational_rewrites_1_0.jsonl` from the live helper, when asked.
///
/// The step runs once, by hand, and it commits its result:
///
/// ```text
/// CADUS_REWRITE_REGEN=1 CADUS_ORACLE_PYTHON=/home/deploy/dev/cadus/.venv/bin/python \
///     cargo test -p cadus-core --test answer_oracle regenerate_the_rewrite_fixture
/// ```
#[test]
fn regenerate_the_rewrite_fixture_when_asked() {
    if std::env::var("CADUS_REWRITE_REGEN").is_err() {
        return;
    }
    let Ok(python) = std::env::var("CADUS_ORACLE_PYTHON") else {
        panic!("set CADUS_ORACLE_PYTHON to regenerate the rewrite fixture");
    };
    let rows = live_rewrites(&python);
    let mut out = String::new();
    for row in &rows {
        let line = serde_json::to_string(row).unwrap_or_else(|e| panic!("write a row: {e}"));
        let _ = writeln!(out, "{line}");
    }
    let path = fixture("rational_rewrites_1_0.jsonl");
    std::fs::write(&path, out).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    println!(
        "wrote {} rewrite spellings to {}",
        rows.len(),
        path.display()
    );
}

/// Prove the committed spellings still say what the live helper says.
#[test]
fn the_live_rewrite_helper_reproduces_the_committed_spellings() {
    let Ok(python) = std::env::var("CADUS_ORACLE_PYTHON") else {
        println!("skipped: set CADUS_ORACLE_PYTHON to run the live rewrite helper");
        return;
    };
    if std::env::var("CADUS_REWRITE_REGEN").is_ok() {
        println!("skipped: the regeneration test owns the file in this run");
        return;
    }
    let live = live_rewrites(&python);
    let committed = committed_rewrites();
    assert_eq!(
        live.len(),
        committed.len(),
        "the live helper wrote {} spellings and the file holds {}",
        live.len(),
        committed.len()
    );
    let mut moved = Vec::new();
    for (live, recorded) in live.iter().zip(committed.iter()) {
        if live != recorded {
            moved.push(format!(
                "{:?} {}: recorded {:?}, live {:?}",
                recorded.answer, recorded.rule, recorded.learner, live.learner
            ));
        }
    }
    assert!(
        moved.is_empty(),
        "the live rewrite helper no longer matches the committed file:\n{}",
        moved.join("\n")
    );
    println!(
        "the live rewrite helper reproduced {} spellings",
        live.len()
    );
}

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

/// Write `oracle_verdicts_1_0.jsonl` from the live 1.0 checker, when asked.
///
/// The step runs once, by hand, after a generator change, and it commits its
/// result:
///
/// ```text
/// CADUS_ORACLE_RECORD=1 CADUS_ORACLE_PYTHON=/home/deploy/dev/cadus/.venv/bin/python \
///     cargo test -p cadus-core --test answer_oracle record_the_oracle_verdicts
/// ```
#[test]
fn record_the_oracle_verdicts_when_asked() {
    if std::env::var("CADUS_ORACLE_RECORD").is_err() {
        return;
    }
    let Ok(python) = std::env::var("CADUS_ORACLE_PYTHON") else {
        panic!("set CADUS_ORACLE_PYTHON to record the 1.0 verdicts");
    };
    let pairs = generated_pairs();
    let live = live_verdicts(&python, &pairs);
    assert_eq!(live.len(), pairs.len(), "the oracle answered every pair");
    let mut out = String::new();
    for (pair, verdict) in pairs.iter().zip(live.iter()) {
        let line = match verdict {
            Some(verdict) => serde_json::json!({
                "expected": pair.expected,
                "learner": pair.learner,
                "kind": pair.kind.as_str(),
                "equivalent": verdict.equivalent,
                "notation": verdict.notation,
                "timeout": false,
            }),
            None => serde_json::json!({
                "expected": pair.expected,
                "learner": pair.learner,
                "kind": pair.kind.as_str(),
                "equivalent": serde_json::Value::Null,
                "notation": serde_json::Value::Null,
                "timeout": true,
            }),
        };
        let _ = writeln!(out, "{line}");
    }
    let path = fixture("oracle_verdicts_1_0.jsonl");
    std::fs::write(&path, out).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    println!(
        "recorded {} 1.0 verdicts in {}",
        pairs.len(),
        path.display()
    );
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
    if std::env::var("CADUS_ORACLE_RECORD").is_ok() {
        println!("skipped: the record test owns the file in this run");
        return;
    }
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
