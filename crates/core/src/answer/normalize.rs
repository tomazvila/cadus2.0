//! Learner-notation tolerance (V4).
//!
//! [`normalize`] produces the two strings that the checker compares on. The 1.0
//! pipeline has two separate rewrites and this module keeps both of them:
//!
//! - `string_key` is the casefolded key of the string rung (1.0 `_normalize`,
//!   spec section 2.1).
//! - `source` is the case-preserving parser input (1.0 `to_sympy_source`,
//!   spec section 2.2) plus the additions that `docs/plans/M2.md` lists for 2.0.
//!
//! The 1.0 order is kept where 1.0 has one. 2.0 adds `\frac{a}{b}`, `\sqrt{a}`,
//! `^{n}`, a `%` that binds to the number in front of it, and a `*` before a
//! `sqrt` that a digit or a closing parenthesis touches.
//!
//! Three readings changed after review round 1 (`docs/reviews/M2-review-1.md`):
//!
//! - A `<var> =` label stays in the source. The parser makes it an
//!   [`crate::answer::Ast::Assign`] node and `check` compares the two labels
//!   (findings #2, #10, #16). 1.0 deleted the label, which made `x = 4` and
//!   `y = 4` one answer.
//! - A `%` divides the number it follows, never the whole body (finding #17).
//! - A vulgar-fraction glyph after a digit run is the fractional part of a mixed
//!   number, so `2⅓` is `2 1/3` and not `2*(1/3)` (findings #1, #9).

/// The largest answer the checker looks at, in characters (spec section 7.9, item 9).
pub const MAX_ANSWER_CHARS: usize = 4_000;

/// The largest brace or parenthesis nesting the rewriter descends into.
///
/// A deeper input keeps its text. The cap bounds the recursion, so a hostile
/// string cannot exhaust the stack.
const MAX_NESTING: usize = 32;

/// The space characters that group digits (1.0 `_SPACE_SEPARATORS`).
const SPACE_SEPARATORS: [char; 5] = [' ', '\u{00a0}', '\u{202f}', '\u{2009}', '\u{2007}'];

/// The plain Unicode substitutions of 1.0 `_UNICODE_SIMPLE`.
const UNICODE_SIMPLE: [(char, &str); 13] = [
    ('π', "pi"),
    ('τ', "(2*pi)"),
    ('∞', "oo"),
    ('·', "*"),
    ('−', "-"),
    ('–', "-"),
    ('≤', "<="),
    ('≥', ">="),
    ('θ', "theta"),
    ('α', "alpha"),
    ('β', "beta"),
    ('λ', "lamda"),
    ('°', ""),
];

/// The vulgar-fraction glyphs, in their two readings.
///
/// The second string is the reading of the glyph on its own. The third string is
/// the reading of the glyph after a digit run: the fractional part of a mixed
/// number, so `2⅓` is the value `7/3` and not `2*(1/3)` (review finding #1).
const VULGAR_FRACTIONS: [(char, &str, &str); 5] = [
    ('½', "(1/2)", " 1/2"),
    ('⅓', "(1/3)", " 1/3"),
    ('⅔', "(2/3)", " 2/3"),
    ('¼', "(1/4)", " 1/4"),
    ('¾', "(3/4)", " 3/4"),
];

/// The result of [`normalize`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Normalized {
    /// The parser input. Case is preserved.
    pub source: String,
    /// The casefolded key of the string-equality rung.
    pub string_key: String,
}

/// Rewrite one answer string into its string key and its parser source (V4).
///
/// The function never fails. An answer that no rung can read still produces two
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

/// Build the parser source (1.0 `to_sympy_source` plus the 2.0 additions).
fn to_source(text: &str) -> String {
    let stripped = strip_dollars(text.trim());
    let no_period = stripped.trim_end_matches('.').trim();
    let collapsed = collapse_whitespace(no_period);
    let collapsed = collapsed.trim();

    // A trailing percent sign leaves before the thousands reading and comes back
    // after it, so `1,500%` still sees its comma group as one grouped integer.
    let (body, is_percent) = match collapsed.strip_suffix('%') {
        Some(rest) => (rest.trim_end(), true),
        None => (collapsed, false),
    };

    let body = rewrite_latex_braces(&body.chars().collect::<Vec<char>>(), 0);
    let body = body.replace('^', "**");
    let body = body
        .replace("\\cdot", "*")
        .replace("\\times", "*")
        .replace("\\left", "")
        .replace("\\right", "")
        .replace('\\', "");
    let body = body.replace('×', "*").replace('÷', "/");
    let body = unicode_math_to_ascii(&body);
    let body = strip_thousands_groups(body.trim());

    let body = if is_percent { body + "%" } else { body };
    let body = bind_percent(&body);
    body.trim().to_string()
}

/// Divide the number in front of each `%` by 100 (review finding #17).
///
/// A percent binds to its own number, so `3 + 4%` is `3 + (4)/100` and not
/// `(3 + 4)/100`. A `%` that no number touches stays in the string, and the
/// lexer then refuses the answer.
fn bind_percent(body: &str) -> String {
    let mut out = String::with_capacity(body.len() + 8);
    // The byte index where the `100` of the last rewrite starts. A second `%` on
    // that `100` is a second reading of one number, so the pass refuses it.
    let mut guard: Option<usize> = None;
    for c in body.chars() {
        if c != '%' {
            out.push(c);
            continue;
        }
        let trimmed = out.trim_end().len();
        match number_start(out.get(..trimmed).unwrap_or("")) {
            Some(start) if guard != Some(start) => {
                let number = out.get(start..trimmed).unwrap_or("").to_string();
                out.truncate(start);
                out.push('(');
                out.push_str(&number);
                out.push_str(")/100");
                guard = Some(out.len() - "100".len());
            }
            _ => out.push('%'),
        }
    }
    out
}

/// Find the byte index where the number literal at the end of `text` starts.
///
/// The scan reads bytes, which is safe: an ASCII digit byte and the `.` byte
/// never stand inside a multi-byte character.
fn number_start(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut start = bytes.len();
    let mut digits = 0_usize;
    while start > 0 {
        let byte = *bytes.get(start - 1)?;
        if byte.is_ascii_digit() {
            digits += 1;
            start -= 1;
        } else if byte == b'.' {
            start -= 1;
        } else {
            break;
        }
    }
    if digits == 0 { None } else { Some(start) }
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

/// Rewrite the LaTeX brace forms that 2.0 adds: `\frac{a}{b}`, `\sqrt{a}`, `^{n}`.
///
/// The rewrite runs before the backslash deletion, because 1.0 deletes every
/// remaining backslash and that is what hides `\frac` from the parser.
fn rewrite_latex_braces(chars: &[char], depth: usize) -> String {
    if depth > MAX_NESTING {
        return chars.iter().collect();
    }
    let mut out = String::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        if let Some((numerator, denominator, next)) = read_frac(chars, i) {
            out.push_str("((");
            out.push_str(&rewrite_latex_braces(numerator, depth + 1));
            out.push_str(")/(");
            out.push_str(&rewrite_latex_braces(denominator, depth + 1));
            out.push_str("))");
            i = next;
            continue;
        }
        if let Some((body, next)) = read_braced_after(chars, i, "\\sqrt") {
            out.push_str("sqrt(");
            out.push_str(&rewrite_latex_braces(body, depth + 1));
            out.push(')');
            i = next;
            continue;
        }
        if let Some((body, next)) = read_braced_after(chars, i, "^") {
            out.push_str("**(");
            out.push_str(&rewrite_latex_braces(body, depth + 1));
            out.push(')');
            i = next;
            continue;
        }
        if let Some(c) = chars.get(i) {
            out.push(*c);
        }
        i += 1;
    }
    out
}

/// Match `keyword` at `at`, immediately followed by a balanced `{…}` group.
///
/// Return the group body and the index after the closing brace.
fn read_braced_after<'a>(
    chars: &'a [char],
    at: usize,
    keyword: &str,
) -> Option<(&'a [char], usize)> {
    let after_keyword = match_literal(chars, at, keyword)?;
    if chars.get(after_keyword) != Some(&'{') {
        return None;
    }
    let close = matching_delimiter(chars, after_keyword, '{', '}')?;
    Some((chars.get(after_keyword + 1..close)?, close + 1))
}

/// Match `\frac{a}{b}` at `at` and return the two bodies and the index after it.
fn read_frac(chars: &[char], at: usize) -> Option<(&[char], &[char], usize)> {
    let (numerator, after_first) = read_braced_after(chars, at, "\\frac")?;
    if chars.get(after_first) != Some(&'{') {
        return None;
    }
    let close = matching_delimiter(chars, after_first, '{', '}')?;
    let denominator = chars.get(after_first + 1..close)?;
    Some((numerator, denominator, close + 1))
}

/// Match the characters of `literal` at `at` and return the index after them.
fn match_literal(chars: &[char], at: usize, literal: &str) -> Option<usize> {
    let mut index = at;
    for want in literal.chars() {
        if chars.get(index) != Some(&want) {
            return None;
        }
        index += 1;
    }
    Some(index)
}

/// Find the delimiter that closes the `open` at `at`. Nesting is counted.
fn matching_delimiter(chars: &[char], at: usize, open: char, close: char) -> Option<usize> {
    if chars.get(at) != Some(&open) {
        return None;
    }
    let mut depth = 0_usize;
    for (offset, c) in chars.get(at..)?.iter().enumerate() {
        if *c == open {
            depth += 1;
        } else if *c == close {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(at + offset);
            }
        }
    }
    None
}

/// Rewrite the house Unicode maths glyphs (1.0 `_unicode_math_to_ascii`).
///
/// 2.0 adds two things: a `*` in front of the `sqrt` when the glyph touches a
/// digit, a letter, or a closing parenthesis, so `15√3` becomes `15*sqrt(3)`;
/// and the mixed-number reading of a vulgar fraction that follows a digit run,
/// so `3½` becomes `3 1/2` and keeps the value seven halves.
fn unicode_math_to_ascii(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let with_roots = rewrite_roots(&chars, 0);
    let with_powers = rewrite_superscripts(&with_roots);
    let mut out = String::with_capacity(with_powers.len());
    for c in with_powers.chars() {
        if let Some((_, alone, after_digits)) =
            VULGAR_FRACTIONS.iter().find(|(from, _, _)| *from == c)
        {
            if matches!(out.chars().last(), Some(last) if last.is_ascii_digit()) {
                out.push_str(after_digits);
            } else {
                out.push_str(alone);
            }
            continue;
        }
        match UNICODE_SIMPLE.iter().find(|(from, _)| *from == c) {
            Some((_, to)) => out.push_str(to),
            None => out.push(c),
        }
    }
    out
}

/// Rewrite `√(…)` and `√token` into `sqrt(…)` (1.0 `sympy_check.py:153-157`).
fn rewrite_roots(chars: &[char], depth: usize) -> String {
    if depth > MAX_NESTING {
        return chars.iter().collect();
    }
    let mut out = String::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        if chars.get(i) != Some(&'√') {
            if let Some(c) = chars.get(i) {
                out.push(*c);
            }
            i += 1;
            continue;
        }
        if matches!(out.chars().last(), Some(c) if c.is_alphanumeric() || c == ')') {
            out.push('*');
        }
        i += 1;
        while matches!(chars.get(i), Some(c) if c.is_whitespace()) {
            i += 1;
        }
        if chars.get(i) == Some(&'(') {
            match matching_delimiter(chars, i, '(', ')') {
                Some(close) => {
                    let body = chars.get(i + 1..close).unwrap_or(&[]);
                    out.push_str("sqrt(");
                    out.push_str(&rewrite_roots(body, depth + 1));
                    out.push(')');
                    i = close + 1;
                }
                None => out.push_str("sqrt"),
            }
            continue;
        }
        match read_root_token(chars, i) {
            Some(next) => {
                out.push_str("sqrt(");
                for c in chars.get(i..next).unwrap_or(&[]) {
                    out.push(*c);
                }
                out.push(')');
                i = next;
            }
            None => out.push_str("sqrt"),
        }
    }
    out
}

/// Read the bare token a `√` takes: an identifier, or a number (1.0 `:156`).
fn read_root_token(chars: &[char], at: usize) -> Option<usize> {
    let first = *chars.get(at)?;
    let mut index = at;
    if first.is_alphabetic() {
        while matches!(chars.get(index), Some(c) if c.is_alphanumeric() || *c == '_') {
            index += 1;
        }
        return Some(index);
    }
    if !first.is_ascii_digit() {
        return None;
    }
    while matches!(chars.get(index), Some(c) if c.is_ascii_digit()) {
        index += 1;
    }
    if chars.get(index) == Some(&'.')
        && matches!(chars.get(index + 1), Some(c) if c.is_ascii_digit())
    {
        index += 1;
        while matches!(chars.get(index), Some(c) if c.is_ascii_digit()) {
            index += 1;
        }
    }
    Some(index)
}

/// Rewrite a run of superscript digits after a word character or `)` into `**n`.
fn rewrite_superscripts(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pending = String::new();
    for c in s.chars() {
        if let Some(digit) = superscript_digit(c) {
            pending.push(digit);
            continue;
        }
        flush_superscripts(&mut out, &mut pending);
        out.push(c);
    }
    flush_superscripts(&mut out, &mut pending);
    out
}

/// Append a collected superscript run to `out`, as `**n` where 1.0 writes `**n`.
fn flush_superscripts(out: &mut String, pending: &mut String) {
    if pending.is_empty() {
        return;
    }
    let attaches =
        matches!(out.chars().last(), Some(c) if c.is_alphanumeric() || c == '_' || c == ')');
    if attaches {
        out.push_str("**");
        out.push_str(pending);
    } else {
        // 1.0 leaves a detached superscript alone, and the parser then refuses it.
        for c in pending.chars() {
            out.push(superscript_char(c));
        }
    }
    pending.clear();
}

/// Map a superscript digit to its ASCII digit.
const fn superscript_digit(c: char) -> Option<char> {
    match c {
        '⁰' => Some('0'),
        '¹' => Some('1'),
        '²' => Some('2'),
        '³' => Some('3'),
        '⁴' => Some('4'),
        '⁵' => Some('5'),
        '⁶' => Some('6'),
        '⁷' => Some('7'),
        '⁸' => Some('8'),
        '⁹' => Some('9'),
        _ => None,
    }
}

/// Map an ASCII digit back to its superscript, for a detached run.
const fn superscript_char(c: char) -> char {
    match c {
        '0' => '⁰',
        '1' => '¹',
        '2' => '²',
        '3' => '³',
        '4' => '⁴',
        '5' => '⁵',
        '6' => '⁶',
        '7' => '⁷',
        '8' => '⁸',
        '9' => '⁹',
        _ => c,
    }
}

/// Delete comma or space thousands groups, but only on a full match (1.0 `:102-105`).
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
