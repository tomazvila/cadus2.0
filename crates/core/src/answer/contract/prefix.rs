//! A name written before a value: `d = 13 km`, `f(g(x)) = 3x - 10`, `a_n = 5n - 3`,
//! `slope is 0`. The name says what the value is and it is not part of the value,
//! so a contract that grades a value or a formula reads the text after the name.

use crate::answer::Undecidable;

/// The learner text without a leading name and its `=` (or `is`), or `None` when
/// the text has no such name.
///
/// The name is one or two words of letters, or a function or sequence term such
/// as `f(x)`, `f'(x)`, `f^-1(x)`, `(f+g)(x)`, `A(t)`, `a_n`, `a_{n-1}`, `dA/dx`.
/// The text after the name holds no second `=` and no comparison sign, so a
/// relation such as `y = 2x + 1` is never stripped twice and `x <= 5` is not
/// read as a name and a value.
#[must_use]
pub(super) fn strip_name(learner: &str) -> Option<String> {
    let text = learner.trim();
    if let Some((name, value)) = split_at_equals(text) {
        return (is_name(name) && is_value(value) && !self_referring(name, value))
            .then(|| value.trim().to_owned());
    }
    // `slope is 0`, `the coefficient is -5`.
    let (name, value) = text.split_once(" is ")?;
    let words: Vec<&str> = name.split_whitespace().collect();
    let plain = !words.is_empty()
        && words.len() <= 3
        && words
            .iter()
            .all(|word| word.chars().all(|c| c.is_alphabetic() || c == '\''));
    (plain && is_value(value)).then(|| value.trim().to_owned())
}

/// The name and the value of `name = value`, or `None` when the text has no such
/// name. Unlike [`strip_name`] it keeps the name, and it reads `=` only.
#[must_use]
pub(super) fn name_and_value(text: &str) -> Option<(&str, &str)> {
    let (name, value) = split_at_equals(text.trim())?;
    (is_name(name) && is_value(value) && !self_referring(name, value))
        .then_some((name.trim(), value.trim()))
}

/// The named parts of `x = 2, y = 3` as one list of values `2, 3`, or `None`.
#[must_use]
pub(super) fn strip_named_list(learner: &str) -> Option<String> {
    let parts = split_top_level(learner.trim().trim_matches(['(', ')']), ',');
    if parts.len() < 2 {
        return None;
    }
    let mut values = Vec::new();
    for part in parts {
        let (name, value) = split_at_equals(part.trim())?;
        let single =
            name.trim().chars().count() == 1 || name.trim().chars().all(char::is_alphabetic);
        if !single || !is_value(value) {
            return None;
        }
        values.push(value.trim().to_owned());
    }
    Some(values.join(", "))
}

/// The values of `x = 2, y = 3` in the order of their names, or `None` when
/// some part has no name. A name that stands twice has no reading.
///
/// Names from `x`, `y`, `z` read in that order, and so do names from `a` to
/// `e` in the alphabet, whatever order the learner wrote them in: `y = 3, x = 2`
/// is the point `(2, 3)`. Other names keep the order of the text.
pub(super) fn named_parts(learner: &str) -> Option<Result<Vec<(String, String)>, Undecidable>> {
    let text = learner.trim().trim_end_matches('.').trim();
    let text = match text
        .strip_prefix('(')
        .and_then(|rest| rest.strip_suffix(')'))
    {
        Some(inner) if !inner.contains(['(', ')']) => inner,
        _ => text,
    };
    let mut parts: Vec<&str> = Vec::new();
    for piece in split_top_level(text, ',') {
        parts.extend(piece.split(" and "));
    }
    if parts.len() < 2 {
        return None;
    }
    let mut named: Vec<(String, String)> = Vec::new();
    for part in parts {
        let part = part.trim();
        let (name, value) = match split_at_equals(part) {
            Some((name, value)) => (name.trim(), value),
            // A label word before the value: `amplitude 3`, `min 5`.
            None => {
                let (word, value) = part.split_once(' ')?;
                let label = (2..=12).contains(&word.chars().count())
                    && word.chars().all(char::is_alphabetic)
                    && !matches!(
                        word.to_lowercase().as_str(),
                        "sqrt"
                            | "sin"
                            | "cos"
                            | "tan"
                            | "cot"
                            | "sec"
                            | "csc"
                            | "ln"
                            | "log"
                            | "exp"
                            | "abs"
                            | "pi"
                            | "inf"
                            | "infinity"
                            | "and"
                            | "or"
                    );
                if !label {
                    return None;
                }
                (word, value)
            }
        };
        let plain = name.chars().next().is_some_and(char::is_alphabetic)
            && name
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '_' | '\''))
            && name.chars().count() <= 12;
        if !plain || !is_value(value) {
            return None;
        }
        named.push((name.to_lowercase(), value.trim().to_owned()));
    }
    for (at, (name, _)) in named.iter().enumerate() {
        if named[..at].iter().any(|(other, _)| other == name) {
            return Some(Err(Undecidable::new(
                "the answer gives the same name twice",
            )));
        }
    }
    let single = |name: &str| name.chars().count() == 1;
    if named
        .iter()
        .all(|(name, _)| "xyz".contains(name.as_str()) && single(name))
        || named
            .iter()
            .all(|(name, _)| "abcde".contains(name.as_str()) && single(name))
    {
        named.sort_by(|left, right| left.0.cmp(&right.0));
    }
    Some(Ok(named))
}

/// Pairs of labels whose first word names the smaller value.
const BOUND_PAIRS: &[(&str, &str)] = &[
    ("min", "max"),
    ("minimum", "maximum"),
    ("lower", "upper"),
    ("low", "high"),
];

/// Whether the labels `max` and `min` (or another pair of bounds) stand on
/// values in the wrong order: a maximum below its minimum is never right.
pub(super) fn bounds_reversed(named: &[(String, String)]) -> bool {
    let number = |text: &str| text.trim().parse::<f64>().ok();
    BOUND_PAIRS.iter().any(|(small, large)| {
        let find = |word: &str| named.iter().position(|(label, _)| label == word);
        match (find(small), find(large)) {
            (Some(lo), Some(hi)) => match (number(&named[lo].1), number(&named[hi].1)) {
                (Some(lo), Some(hi)) => lo > hi,
                _ => false,
            },
            _ => false,
        }
    })
}

/// Split at the first `=` that is not part of `<=`, `>=`, `!=`, or `==`.
fn split_at_equals(text: &str) -> Option<(&str, &str)> {
    let bytes = text.as_bytes();
    let mut depth = 0_usize;
    for (at, byte) in bytes.iter().enumerate() {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            // An `=` inside brackets belongs to the name: `P(X=1)`.
            b'=' if depth == 0 => {
                let before = at.checked_sub(1).and_then(|i| bytes.get(i));
                let after = bytes.get(at + 1);
                if matches!(before, Some(b'<' | b'>' | b'!' | b'=')) || after == Some(&b'=') {
                    return None;
                }
                return Some((&text[..at], &text[at + 1..]));
            }
            _ => {}
        }
    }
    None
}

/// Split at a separator outside every bracket.
fn split_top_level(text: &str, separator: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0_usize;
    let mut start = 0;
    for (at, c) in text.char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            c if c == separator && depth == 0 => {
                parts.push(&text[start..at]);
                start = at + c.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
}

/// The text after the name is a value: not empty, no second `=`, no comparison.
fn is_value(text: &str) -> bool {
    let text = text.trim();
    !text.is_empty() && !text.contains(['=', '<', '>', '≤', '≥', '≠'])
}

/// Words that start a function call, never a name.
const FUNCTION_WORDS: &[&str] = &[
    "sqrt", "cbrt", "sin", "cos", "tan", "cot", "sec", "csc", "sinh", "cosh", "tanh", "asin",
    "acos", "atan", "arcsin", "arccos", "arctan", "ln", "log", "lg", "exp", "abs", "sgn", "floor",
    "ceil", "min", "max", "gcd", "lcm", "det",
];

/// The length in bytes of the identifier at the start of `text`: a letter, then
/// letters, digits, primes, and a subscript (`a_n`, `a_{n-1}`, `x_1`).
fn identifier_length(text: &str) -> usize {
    let mut end = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        let first = at == 0;
        if (first && !c.is_alphabetic()) || !(c.is_alphanumeric() || matches!(c, '\'' | '_')) {
            break;
        }
        end = at + c.len_utf8();
        if c == '_'
            && let Some(&(open, '{')) = chars.peek()
        {
            let Some(close) = text[open..].find('}') else {
                return end;
            };
            end = open + close + 1;
            while chars.peek().is_some_and(|&(next, _)| next < end) {
                chars.next();
            }
        }
    }
    end
}

/// The text of a bracket that opens at the start of `text`, and what follows it.
fn bracket(text: &str) -> Option<(&str, &str)> {
    let mut depth = 0_usize;
    for (at, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some((&text[1..at], &text[at + 1..]));
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether `inner` holds the identifier `name` as a whole word.
fn mentions(inner: &str, name: &str) -> bool {
    let mut from = 0;
    while let Some(found) = inner[from..].find(name) {
        let at = from + found;
        let end = at + name.len();
        let before = inner[..at].chars().next_back();
        let after = inner[end..].chars().next();
        if !before.is_some_and(char::is_alphabetic) && !after.is_some_and(char::is_alphanumeric) {
            return true;
        }
        from = end;
    }
    false
}

/// Whether `name = value` is an equation in `name` and no label: the value
/// holds the name as a variable (`x = 2x+1`, `y = y + 3`). A unit with the
/// letter of the name (`m = 5 m`) is no variable.
fn self_referring(name: &str, value: &str) -> bool {
    let name = name.trim();
    if identifier_length(name) != name.len() {
        return false;
    }
    let mut from = 0;
    while let Some(found) = value[from..].find(name) {
        let at = from + found;
        let end = at + name.len();
        let before = value[..at].chars().next_back();
        let after = value[end..].chars().next();
        if !before.is_some_and(char::is_alphabetic)
            && !after.is_some_and(char::is_alphanumeric)
            && before.is_some_and(|c| {
                c.is_ascii_digit() || matches!(c, '+' | '-' | '*' | '/' | '^' | '(' | ')')
            })
        {
            return true;
        }
        from = end;
    }
    false
}

/// A name that stands before a value: one identifier (`d`, `dim`, `a_n`,
/// `a_{n-1}`), a derivative (`dA/dx`), a combination such as `(f+g)(x)`, and
/// for each of them a call with its arguments (`f(x)`, `g'(x)`, `P(X=1)`) or an
/// inverse mark (`f^-1(x)`). An expression is no name: `x + 1`, `x^2`,
/// `sqrt(x)`, `e^x`, `x/2`, and `x(x+1)` are things to compare, not labels.
fn is_name(text: &str) -> bool {
    let text = text.trim();
    let length = text.chars().count();
    if length == 0 || length > 24 {
        return false;
    }
    // `dA/dx`.
    if let Some((top, bottom)) = text.split_once('/') {
        let simple = |part: &str| part.starts_with('d') && identifier_length(part) == part.len();
        return simple(top) && simple(bottom) && top.len() > 1 && bottom.len() > 1;
    }
    let (head, mut rest) = if text.starts_with('(') {
        // `(f+g)(x)`: sums, products, and compositions of function names.
        let Some((inner, after)) = bracket(text) else {
            return false;
        };
        let combined = !inner.is_empty()
            && inner
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '+' | '-' | '*' | '∘' | ' '));
        if !combined {
            return false;
        }
        (String::new(), after)
    } else {
        let end = identifier_length(text);
        if end == 0 {
            return false;
        }
        let head = &text[..end];
        if FUNCTION_WORDS.contains(&head.to_lowercase().as_str()) {
            return false;
        }
        (head.to_owned(), &text[end..])
    };
    for inverse in ["^-1", "^{-1}", "⁻¹"] {
        if let Some(after) = rest.strip_prefix(inverse) {
            rest = after;
            break;
        }
    }
    if rest.is_empty() {
        return !head.is_empty();
    }
    if !rest.starts_with('(') {
        return false;
    }
    // One call, with arguments: `f(x)`, `P(X=1)`. A call that holds its own
    // name is a product: `x(x+1)`.
    let Some((inner, after)) = bracket(rest) else {
        return false;
    };
    after.is_empty() && !inner.trim().is_empty() && (head.is_empty() || !mentions(inner, &head))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_before_a_value_falls_away() {
        for (text, value) in [
            ("d = 13 km", "13 km"),
            ("x = 75 cm", "75 cm"),
            ("f(g(x)) = 3x - 10", "3x - 10"),
            ("f^-1(x) = (x+3)/4", "(x+3)/4"),
            ("f(-5) = 16", "16"),
            ("(f+g)(x) = 3x - 3", "3x - 3"),
            ("A(t) = 500e^(0.06t)", "500e^(0.06t)"),
            ("a_n = 5n - 3", "5n - 3"),
            ("a_{n-1} = 4", "4"),
            ("dA/dx = 2x", "2x"),
            ("f'(x) = 2x", "2x"),
            ("θ = 35", "35"),
            ("slope is 0", "0"),
            ("the coefficient is -5", "-5"),
        ] {
            assert_eq!(strip_name(text).as_deref(), Some(value), "{text}");
        }
    }

    #[test]
    fn a_relation_or_a_bare_value_keeps_its_text() {
        for text in [
            "x <= 5",
            "x >= 5",
            "y = x = 2",
            "3 = 3",
            "2x + 1",
            "x != 3",
            "= 5",
            "x =",
        ] {
            assert_eq!(strip_name(text), None, "{text}");
        }
    }

    #[test]
    fn named_parts_become_one_list() {
        assert_eq!(strip_named_list("x = 2, y = 3").as_deref(), Some("2, 3"));
        assert_eq!(strip_named_list("m = 3, b = 5").as_deref(), Some("3, 5"));
        assert_eq!(strip_named_list("2, 3"), None);
        assert_eq!(strip_named_list("x = 2, 3"), None);
    }
}
