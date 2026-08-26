//! Learner-notation tolerance (V4), the string half.
//!
//! [`normalize`] produces the two strings that the checker compares on. The 1.0
//! pipeline has two separate rewrites and this module keeps both of them:
//!
//! - `string_key` is the casefolded key of the string rung (1.0 `_normalize`,
//!   spec section 2.1).
//! - `source` is the case-preserving reader input (1.0 `to_sympy_source`,
//!   spec section 2.2), cut down to the steps that a string rewrite does
//!   without a grammar.
//!
//! # This module owns no construct
//!
//! Review round 3 (`docs/reviews/M2-review-3.md`, findings #1, #2, #3, #4, #6,
//! #8) measures the cost of a construct that a string rewrite owns. A `\frac`
//! with one space inside a brace fell out of the literal-fraction spelling and
//! became a product; a `%` wrote the bare text `(n)/100`, which re-associated
//! under `/` and under `**`; a product sign written in front of `\sqrt` landed
//! on the last letter of `\cdot`. Every one of those defects graded a wrong
//! answer correct (C4).
//!
//! The orchestrator ruling of round 3 moves every construct out of this module.
//! `\frac{a}{b}`, `\sqrt{a}`, `\sqrt a`, `^{n}`, `\cdot`, `\times`, `\left`,
//! `\right`, `%`, the vulgar glyphs, `√`, the superscript digits, and `°` are
//! tokens of [`crate::answer::lexer`], and [`crate::answer::parse`] builds the
//! tree from those tokens. A token carries structure, so no later pass
//! re-associates it.
//!
//! # What is left
//!
//! Six steps, and every one of them is a property of the whole string:
//!
//! 1. one outer `$…$` pair,
//! 2. trailing periods,
//! 3. whitespace collapse,
//! 4. the casefolded string key,
//! 5. the comma or space thousands group, on a full match,
//! 6. the one-character Unicode table of operators and constants.
//!
//! The `<var> =` label of review round 1 (findings #2, #10, #16) stays in the
//! source: [`crate::answer::parse`] makes it an [`crate::answer::Ast::Assign`]
//! node, and `check` compares the two labels. 1.0 deleted the label, which made
//! `x = 4` and `y = 4` one answer.

/// The largest answer the checker looks at, in characters (spec section 7.9, item 9).
pub const MAX_ANSWER_CHARS: usize = 4_000;

/// The space characters that group digits (1.0 `_SPACE_SEPARATORS`).
///
/// [`collapse_whitespace`] runs first and maps every one of them to an ASCII
/// space, so the group test meets the ASCII space alone. The list stays whole
/// because it names the 1.0 rule.
const SPACE_SEPARATORS: [char; 5] = [' ', '\u{00a0}', '\u{202f}', '\u{2009}', '\u{2007}'];

/// The one-character Unicode substitutions of 1.0 `_UNICODE_SIMPLE`.
///
/// Every entry is one character that stands for one operator or one constant.
/// The constructs that carry an argument (`√`, the vulgar glyphs, the
/// superscript digits, `°`) left this table for [`crate::answer::lexer`] in
/// review round 3.
const UNICODE_SIMPLE: [(char, &str); 14] = [
    ('π', "pi"),
    ('τ', "(2*pi)"),
    ('∞', "oo"),
    ('·', "*"),
    ('×', "*"),
    ('÷', "/"),
    ('−', "-"),
    ('–', "-"),
    ('≤', "<="),
    ('≥', ">="),
    ('θ', "theta"),
    ('α', "alpha"),
    ('β', "beta"),
    ('λ', "lamda"),
];

/// The result of [`normalize`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Normalized {
    /// The reader input. Case is preserved.
    pub source: String,
    /// The casefolded key of the string-equality rung.
    pub string_key: String,
}

/// Rewrite one answer string into its string key and its reader source (V4).
///
/// The function never fails. An answer that no rung reads still produces two
/// strings; [`crate::answer::parse`] is the step that refuses it.
#[must_use]
pub fn normalize(text: &str) -> Normalized {
    Normalized {
        source: to_source(text),
        string_key: to_string_key(text),
    }
}

/// Build the casefolded string-rung key (1.0 `_normalize`, spec section 2.1).
fn to_string_key(text: &str) -> String {
    let stripped = strip_dollars(text.trim()).trim();
    let no_period = stripped.trim_end_matches('.');
    let collapsed = collapse_whitespace(no_period);
    casefold(&collapsed).trim().to_string()
}

/// Build the reader source: the string steps of the V4 table.
///
/// The Unicode table runs in front of the thousands step, because the table
/// maps the Unicode minus to the ASCII minus and the group shape reads that
/// sign.
fn to_source(text: &str) -> String {
    let stripped = strip_dollars(text.trim());
    let no_period = stripped.trim_end_matches('.').trim();
    let collapsed = collapse_whitespace(no_period);
    let body = unicode_operators_to_ascii(collapsed.trim());
    strip_thousands_groups(body.trim()).trim().to_string()
}

/// Remove one outer `$…$` pair (1.0 `sympy_check.py:40-41`).
fn strip_dollars(s: &str) -> &str {
    if s.len() > 1 && s.starts_with('$') && s.ends_with('$') {
        // Both delimiters are one ASCII byte, so the slice is on char boundaries.
        s.get(1..s.len() - 1).unwrap_or(s)
    } else {
        s
    }
}

/// Replace every run of whitespace with one ASCII space (1.0 `_WS_RE`).
fn collapse_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_space = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !in_space {
                out.push(' ');
                in_space = true;
            }
        } else {
            out.push(c);
            in_space = false;
        }
    }
    out
}

/// Fold case for the string rung.
///
/// Rust has no full Unicode case folding in the standard library. `to_lowercase`
/// covers every case the corpus holds; the sharp s is mapped by hand because
/// Python `casefold` maps it to `ss` and `to_lowercase` does not.
fn casefold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'ß' | 'ẞ' => out.push_str("ss"),
            _ => out.extend(c.to_lowercase()),
        }
    }
    out
}

/// Replace the one-character operator and constant glyphs (1.0 `_UNICODE_SIMPLE`).
fn unicode_operators_to_ascii(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match UNICODE_SIMPLE.iter().find(|(from, _)| *from == c) {
            Some((_, to)) => out.push_str(to),
            None => out.push(c),
        }
    }
    out
}

/// Delete comma or space thousands groups, but only on a full match (1.0 `:102-105`).
///
/// The rule reads the whole answer, so a group is one value only when it is the
/// whole answer. `1 000` is 1000. `x/1 000`, `3 + 1 500%` and `1 500%` are not
/// the whole answer, so the group stays apart and the parser refuses the second
/// run instead of inventing a factor (review round 2 finding #11, review round 3
/// finding #6).
fn strip_thousands_groups(s: &str) -> String {
    if is_grouped_integer(s, &[',']) {
        return s.chars().filter(|c| *c != ',').collect();
    }
    if is_grouped_integer(s, &SPACE_SEPARATORS) {
        return s.chars().filter(|c| !c.is_whitespace()).collect();
    }
    s.to_string()
}

/// Whether the whole string is one integer grouped by `separators`.
///
/// The shape is `-? d{1,3} (sep d{3})+`, which is 1.0 `_COMMA_GROUPS_RE` and
/// `_SPACE_GROUPS_RE`.
fn is_grouped_integer(s: &str, separators: &[char]) -> bool {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    if chars.first() == Some(&'-') {
        i = 1;
    }
    let lead_start = i;
    while i < chars.len() && matches!(chars.get(i), Some(c) if c.is_ascii_digit()) {
        i += 1;
    }
    let lead_len = i - lead_start;
    if !(1..=3).contains(&lead_len) {
        return false;
    }
    let mut groups = 0_usize;
    while let Some(c) = chars.get(i) {
        if !separators.contains(c) {
            return false;
        }
        i += 1;
        let mut digits = 0;
        while matches!(chars.get(i), Some(d) if d.is_ascii_digit()) {
            i += 1;
            digits += 1;
        }
        if digits != 3 {
            return false;
        }
        groups += 1;
    }
    groups >= 1 && i == chars.len()
}
