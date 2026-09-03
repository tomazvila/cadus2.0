//! Part 6 of the helpers of the `answer_oracle` tests.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    unused_imports
)]

use super::*;

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
pub const DOCUMENTED_REASONS: [&str; 16] = [
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
    // The two readings of ruling `D6-dec`. 2.0 reads a learner decimal as the
    // exact rounding of the authored value and marks the FORM; 1.0 had a float
    // tolerance instead, which accepted a rounding inside 1e-6 with no note and
    // refused every rounding outside it.
    "the exact rounding carries the notation tag (D6-dec)",
    "an exact rounding 1.0 refused is correct (D6-dec)",
];

/// Whether 1.0 refuses `source` for a tower of powers (1.0 `_POW_TOWER_RE`).
///
/// The predicate is a literal port of `sympy_check.py:215`,
/// `r"\*\*\s*[^*+\-/()\s]+\s*\*\*"`, applied to the string 1.0 evaluates. That
/// guard is the reason 1.0 refuses the legal answer `36x**2y**2`
/// (spec section 5.1).
pub fn nineteen_zero_reads_a_power_tower(text: &str) -> bool {
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
pub fn nineteen_zero_reads_a_python_literal(text: &str) -> bool {
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
pub fn a_unicode_radical_over_a_nested_group(text: &str) -> bool {
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
pub fn nineteen_zero_leaves_a_brace_group(text: &str) -> bool {
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
pub fn a_chained_inequality(text: &str) -> bool {
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
pub fn the_1_0_float_rung_closes_the_gap(pair: &Pair) -> bool {
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

/// Whether the learner wrote the exact rounding of the authored value (`D6-dec`).
///
/// The predicate is the harness's own arithmetic, and it calls nothing of the
/// code under test. It reads the learner answer as a plain decimal of `n` digits
/// and writes the authored value with `n` digits: Rust formats a `f64` with
/// correct rounding and breaks a tie to the even digit, which is the rule of the
/// ruling. A learner answer that is not a plain decimal, and an authored answer
/// the harness reader refuses, both give false.
///
/// The predicate CLASSIFIES a divergence and pins no verdict. Every literal
/// verdict of the rule stands in `crates/core/tests/answer_decimal.rs`, where
/// the arithmetic is exact.
pub fn the_learner_wrote_the_exact_rounding(pair: &Pair) -> bool {
    let learner_source = one_zero_source(&pair.learner);
    let Some((negative, whole, fraction)) = decimal_parts(&learner_source) else {
        return false;
    };
    if fraction.is_empty() {
        return false;
    }
    let Some(value) = authored_value(&pair.expected) else {
        return false;
    };
    if !value.is_finite() {
        return false;
    }
    let sign = if negative { "-" } else { "" };
    format!("{value:.*}", fraction.len()) == format!("{sign}{whole}.{fraction}")
}

/// The authored value, as the 2.0 grammar reads it.
///
/// [`numeric_value`] is a port of the reader of the two 1.0 float rungs, so it
/// reads no mixed number: 1.0 has no such production and takes `4 1/6` for the
/// product `4*1/6` (spec section 7.8). The 2.0 grammar holds the mixed number
/// (spec section 8.1), and a rounding of `4 1/6` is a rounding of 25/6. The
/// order matters: the mixed number is tried first, because the 1.0 reader
/// accepts the same string as a product.
pub fn authored_value(text: &str) -> Option<f64> {
    let source = one_zero_source(text);
    mixed_number_value(&source).or_else(|| numeric_value(&source))
}

/// Read `a b/c` as `a + b/c`, with the sign of the whole part.
pub fn mixed_number_value(source: &str) -> Option<f64> {
    let (whole_text, fraction_text) = source.trim().split_once(' ')?;
    let (negative, digits) = integer_digits(whole_text.trim())?;
    let whole: f64 = digits.parse().ok()?;
    let (numerator, denominator) = fraction_parts(fraction_text.trim())?;
    if denominator == 0 || numerator < 0 {
        return None;
    }
    let magnitude = whole + numerator as f64 / denominator as f64;
    Some(if negative { -magnitude } else { magnitude })
}

/// Whether Python `float()` reads the whole source (1.0 `_numeric_equal`).
///
/// The caller passes the 1.0 rewrite of [`one_zero_source`], which is the exact
/// string `sympy_check.py:180` hands to `float()`.
pub fn reads_as_a_python_float(source: &str) -> bool {
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
pub fn one_zero_source(text: &str) -> String {
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
pub fn superscript_to_power(text: &str) -> String {
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
pub fn strip_thousands_groups(text: &str) -> String {
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
pub fn split_groups(text: &str, separator: &str) -> Option<(bool, String)> {
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
pub fn radical_atoms(canon: &Canon) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    collect_radical_atoms(canon, &mut out);
    out
}
