//! Part 2 of the helpers of the `answer_oracle` tests.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    unused_imports
)]

use super::*;

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
pub fn numeric_value(source: &str) -> Option<f64> {
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
pub struct Numbers<'a> {
    pub chars: &'a [char],
    pub at: usize,
}

impl Numbers<'_> {
    /// Step over every space in front of the next token.
    pub fn skip_spaces(&mut self) {
        while matches!(self.chars.get(self.at), Some(c) if c.is_whitespace()) {
            self.at += 1;
        }
    }

    /// The character at the reader position.
    pub fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    pub fn sum(&mut self) -> Option<f64> {
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

    pub fn product(&mut self) -> Option<f64> {
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

    pub fn unary(&mut self) -> Option<f64> {
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

    pub fn power(&mut self) -> Option<f64> {
        let base = self.primary()?;
        self.skip_spaces();
        if self.peek() == Some('*') && self.chars.get(self.at + 1) == Some(&'*') {
            self.at += 2;
            let exponent = self.unary()?;
            return Some(base.powf(exponent));
        }
        Some(base)
    }

    pub fn primary(&mut self) -> Option<f64> {
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
    pub fn number(&mut self) -> Option<f64> {
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
    pub fn name(&mut self) -> Option<f64> {
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
pub fn signed_terms(text: &str) -> Option<Vec<(bool, String)>> {
    let chars: Vec<char> = text.chars().collect();
    let mut depth = 0_i32;
    let mut terms: Vec<(bool, String)> = Vec::new();
    let mut negative = false;
    let mut current = String::new();
    for (index, ch) in chars.iter().enumerate() {
        step_depth(&mut depth, *ch)?;
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
pub fn ends_an_operand(chars: &[char], index: usize) -> bool {
    last_before(chars, index, char::is_whitespace).is_some_and(|ch| {
        ch.is_ascii_alphanumeric() || ch == ')' || ch == ']' || ch == '}' || ch == '.'
    })
}

/// The last character in front of `index` that `skip` does not pass over.
pub fn last_before(chars: &[char], index: usize, skip: impl Fn(char) -> bool) -> Option<char> {
    chars
        .get(..index)?
        .iter()
        .rev()
        .copied()
        .find(|ch| !skip(*ch))
}

/// Track the bracket depth over one character, or report a close with no open.
pub fn step_depth(depth: &mut i32, ch: char) -> Option<()> {
    match ch {
        '(' | '[' | '{' => *depth += 1,
        ')' | ']' | '}' => {
            *depth -= 1;
            if *depth < 0 {
                return None;
            }
        }
        _ => {}
    }
    Some(())
}

/// Whether `text` is one multiplicative term: no `+` and no binary `-` outside a
/// bracket.
pub fn is_one_term(text: &str) -> bool {
    matches!(signed_terms(text), Some(terms) if terms.len() == 1)
}

/// Split `text` into its top-level factors, at every `*` that is not a `**`.
///
/// Returns `None` when the brackets do not balance or a factor is blank.
pub fn product_factors(text: &str) -> Option<Vec<String>> {
    let chars: Vec<char> = text.chars().collect();
    let mut depth = 0_i32;
    let mut factors: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut index = 0;
    while let Some(ch) = chars.get(index).copied() {
        step_depth(&mut depth, ch)?;
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
pub const UNICODE_TO_ASCII: [(&str, &str); 30] = [
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
pub fn radical_to_call(text: &str) -> String {
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
pub fn radical_to_call_once(text: &str) -> String {
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
        index = if chars.get(index) == Some(&'(') {
            call_over_group(&chars, index, &mut out)
        } else {
            call_over_word(&chars, index, &mut out)
        };
    }
    out
}

/// Write `sqrt` in front of the bracket group at `start`, and return the index after it.
fn call_over_group(chars: &[char], start: usize, out: &mut String) -> usize {
    let mut depth = 0_i32;
    let mut index = start;
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
    index
}

/// Wrap the alphanumeric run at `start` in a `sqrt(…)` call, and return the index after it.
fn call_over_word(chars: &[char], start: usize, out: &mut String) -> usize {
    let mut index = start;
    while matches!(chars.get(index), Some(c) if c.is_alphanumeric()) {
        index += 1;
    }
    let token: String = chars.get(start..index).unwrap_or_default().iter().collect();
    if token.is_empty() {
        out.push('√');
    } else {
        let _ = write!(out, "sqrt({token})");
    }
    index
}

/// Replace the whole word `from` with `to`.
pub fn replace_word(text: &str, from: &str, to: &str) -> String {
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
pub fn bump_last_digit(text: &str) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let position = chars.iter().rposition(char::is_ascii_digit)?;
    let digit = chars.get(position)?.to_digit(10)?;
    let mut out = chars;
    *out.get_mut(position)? = char::from_digit((digit + 1) % 10, 10)?;
    Some(out.into_iter().collect())
}

/// Bump the first number that follows `marker` by one.
pub fn bump_number_after(text: &str, marker: &str) -> Option<String> {
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
