//! Part 1 of the helpers of the `answer_oracle` tests.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    unused_imports
)]

use super::*;

/// The largest number of pairs the test runs (the task bound of U3).
///
/// The generated set holds 17,047 pairs, so the cap drops none of them today.
/// If a new generator takes the set past the cap, [`generated_pairs`] prints the
/// total it drops and one line per dropped pair, because a dropped pair is a
/// pair the parity report never asks the 1.0 oracle about.
pub const PAIR_CAP: usize = 20_000;

/// The seed of the selection shuffle. A fixed seed makes the set reproducible.
pub const SHUFFLE_SEED: u64 = 0x2026_0826_5533_1177;

// ---------------------------------------------------------------------------
// The corpus
// ---------------------------------------------------------------------------
/// One line of `crates/core/tests/fixtures/answers/corpus_1_0.jsonl`.
#[derive(serde::Deserialize)]
pub struct CorpusLine {
    pub answer: String,
    pub answer_kind: String,
    pub shape: String,
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
pub struct Row {
    /// The authored answer, verbatim.
    pub answer: String,
    /// The normalized parser source of that answer (V4).
    ///
    /// Two families read this string, and both of them ask a question about the
    /// surface spelling and not about the value: `explicit_multiplication` needs
    /// the juxtaposition the author wrote, and `internal_spaces` needs the
    /// author's own operators.
    pub source: String,
    /// The parsed tree of that source, written back as ASCII reader source.
    ///
    /// The text is a function of the tree alone, so it holds `sqrt(`, `**`,
    /// `pi`, and `*` whatever the author wrote. Every builder that reads a
    /// number, a term, a factor, or a bracket out of an answer reads this.
    pub printed: String,
    /// The parsed tree of the answer.
    pub ast: Ast,
    /// The canonical form of the answer, when the checker decides one.
    pub canon: Option<Canon>,
    /// The shape bucket of spec section 5.
    pub shape: String,
    /// The authored answer kind.
    pub kind: AnswerKind,
}

/// Read the corpus rows the grammar accepts, deduplicated by answer and kind.
///
/// The corpus repeats an answer string across topics (`6` occurs hundreds of
/// times). A repeat adds no pair, so the set is keyed by the answer text and the
/// answer kind.
///
/// An answer that a 2.0 production recovered from the 1.0 residue
/// (`recovered_2_0.jsonl`) stays out of the set. 1.0 reads `9 R2` as `18*R` and
/// refuses `x^(1/2)`, so no 1.0 verdict on such a pair is comparable, and the
/// committed verdict file holds none. The productions pin their own verdicts in
/// their own test files (unit f2-grammar, D-F3).
pub fn in_grammar_rows() -> Vec<Row> {
    let path = fixture("corpus_1_0.jsonl");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let recovered: BTreeSet<String> = committed_recovered()
        .into_iter()
        .map(|row| row.answer)
        .collect();
    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
    let mut rows = Vec::new();
    for line in text.lines() {
        let parsed: CorpusLine =
            serde_json::from_str(line).unwrap_or_else(|e| panic!("row {line}: {e}"));
        if recovered.contains(&parsed.answer) {
            continue;
        }
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
pub const PREC_LOWEST: u8 = 1;

/// The binding power of a product.
pub const PREC_PRODUCT: u8 = 2;

/// The binding power of a divisor and of a negation.
pub const PREC_UNARY: u8 = 3;

/// The binding power of a power base: the strongest.
pub const PREC_POWER: u8 = 4;

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
pub fn print_ast(ast: &Ast, parent: u8) -> String {
    match ast {
        Ast::Integer(value) => print_number(value.to_string(), parent, false),
        Ast::Decimal { mantissa, scale } => {
            print_number(decimal_text(&mantissa.to_string(), *scale), parent, false)
        }
        Ast::Fraction {
            numerator,
            denominator,
        } => print_number(format!("{numerator}/{denominator}"), parent, true),
        Ast::Mixed {
            whole,
            numerator,
            denominator,
        } => format!("({whole} + {numerator}/{denominator})"),
        Ast::Var(name) => name.clone(),
        Ast::Const(value) => value.name().to_string(),
        Ast::Sqrt(inner) => format!("sqrt({})", print_ast(inner, PREC_LOWEST)),
        Ast::Pow(base, exponent) => print_power(base, *exponent, parent),
        Ast::RationalPow {
            base,
            numerator,
            denominator,
        } => bracket_if(
            format!(
                "{}**({numerator}/{denominator})",
                print_ast(base, PREC_POWER)
            ),
            parent >= PREC_POWER,
        ),
        Ast::Neg(inner) => bracket_if(
            format!("-{}", print_ast(inner, PREC_UNARY)),
            parent >= PREC_UNARY,
        ),
        Ast::Add(terms) => print_sum(terms, parent),
        Ast::Mul(factors) => print_product(factors, parent),
        Ast::Div(left, right) => print_quotient(left, right, parent),
        Ast::Func(name, arguments) => format!("{name}({})", print_list(arguments)),
        Ast::Tuple(items) | Ast::Set(items) | Ast::List(items) => print_wrapped(ast, items),
        Ast::Interval {
            lo,
            hi,
            lo_closed,
            hi_closed,
        } => print_interval(lo, hi, *lo_closed, *hi_closed),
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
        } => print_chain(lo, *lo_closed, var, *hi_closed, hi),
    }
}

/// Print a number literal, bracketed where its sign or its bar re-reads.
fn print_number(text: String, parent: u8, fraction: bool) -> String {
    let negative = text.starts_with('-');
    bracket_if(
        text,
        (fraction && parent > PREC_PRODUCT) || (negative && parent >= PREC_PRODUCT),
    )
}

/// Print a power.
///
/// A negative exponent takes a bracket pair, so `x**-2` never reaches either
/// parser: 1.0 reads that string with the Python tokenizer.
fn print_power(base: &Ast, exponent: i64, parent: u8) -> String {
    let power = if exponent < 0 {
        format!("({exponent})")
    } else {
        exponent.to_string()
    };
    let body = format!("{}**{power}", print_ast(base, PREC_POWER));
    bracket_if(body, parent >= PREC_POWER)
}

/// Print a sum. A subtracted term needs no bracket around a product: the binary
/// `-` binds more weakly than the `*` it holds.
fn print_sum(terms: &[Ast], parent: u8) -> String {
    let mut out = String::new();
    for (index, term) in terms.iter().enumerate() {
        match (index, term) {
            (0, _) => out.push_str(&print_ast(term, PREC_LOWEST)),
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

/// Print a product.
fn print_product(factors: &[Ast], parent: u8) -> String {
    let parts: Vec<String> = factors
        .iter()
        .map(|factor| print_ast(factor, PREC_PRODUCT))
        .collect();
    bracket_if(parts.join("*"), parent > PREC_PRODUCT)
}

/// Print a quotient.
fn print_quotient(left: &Ast, right: &Ast, parent: u8) -> String {
    bracket_if(
        format!(
            "{}/{}",
            print_ast(left, PREC_PRODUCT),
            print_ast(right, PREC_UNARY)
        ),
        parent > PREC_PRODUCT,
    )
}

/// Print a bracket interval.
fn print_interval(lo: &Ast, hi: &Ast, lo_closed: bool, hi_closed: bool) -> String {
    format!(
        "{}{}, {}{}",
        if lo_closed { '[' } else { '(' },
        print_ast(lo, PREC_LOWEST),
        print_ast(hi, PREC_LOWEST),
        if hi_closed { ']' } else { ')' }
    )
}

/// Print a chained inequality.
fn print_chain(lo: &Ast, lo_closed: bool, var: &str, hi_closed: bool, hi: &Ast) -> String {
    format!(
        "{} {} {var} {} {}",
        print_ast(lo, PREC_LOWEST),
        if lo_closed { "<=" } else { "<" },
        if hi_closed { "<=" } else { "<" },
        print_ast(hi, PREC_LOWEST)
    )
}

/// Wrap `text` in a bracket pair when the position asks for one.
pub fn bracket_if(text: String, wrap: bool) -> String {
    if wrap { format!("({text})") } else { text }
}

/// Print a tuple, a set, or a list between its delimiters.
fn print_wrapped(ast: &Ast, items: &[Ast]) -> String {
    let (open, close) = match ast {
        Ast::Tuple(_) => ('(', ')'),
        Ast::Set(_) => ('{', '}'),
        _ => ('[', ']'),
    };
    format!("{open}{}{close}", print_list(items))
}

/// Print a comma-separated argument list.
pub fn print_list(items: &[Ast]) -> String {
    items
        .iter()
        .map(|item| print_ast(item, PREC_LOWEST))
        .collect::<Vec<String>>()
        .join(", ")
}

/// Write `mantissa / 10**scale` as a decimal literal.
pub fn decimal_text(mantissa: &str, scale: u32) -> String {
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
pub fn top_level_split(text: &str, separator: char) -> Option<Vec<String>> {
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
pub fn bracket_items(source: &str, open: char, close: char) -> Option<Vec<String>> {
    let inner = source.strip_prefix(open)?.strip_suffix(close)?;
    top_level_split(inner, ',')
}

/// Read a plain integer source into its sign and its digits.
pub fn integer_digits(source: &str) -> Option<(bool, String)> {
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
pub fn decimal_parts(source: &str) -> Option<(bool, String, String)> {
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
pub fn fraction_parts(source: &str) -> Option<(i128, i128)> {
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
pub fn group_digits(digits: &str, separator: &str) -> String {
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
pub fn thousands_grouped(row: &Row, separator: &str) -> Option<String> {
    let (negative, digits) = integer_digits(&row.printed)?;
    if digits.len() < 4 || digits.len() > 15 || digits.starts_with('0') {
        return None;
    }
    let body = group_digits(&digits, separator);
    Some(if negative { format!("-{body}") } else { body })
}

/// The greatest common divisor of two non-negative numbers.
pub fn gcd(a: i128, b: i128) -> i128 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// Write `numerator / denominator` as an exact decimal, when one exists.
pub fn exact_decimal(numerator: i128, denominator: i128) -> Option<String> {
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
