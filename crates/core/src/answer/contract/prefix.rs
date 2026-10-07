//! A name written before a value: `d = 13 km`, `f(g(x)) = 3x - 10`, `a_n = 5n - 3`,
//! `slope is 0`. The name says what the value is and it is not part of the value,
//! so a contract that grades a value or a formula reads the text after the name.

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
        return (is_name(name) && is_value(value)).then(|| value.trim().to_owned());
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

/// Split at the first `=` that is not part of `<=`, `>=`, `!=`, or `==`.
fn split_at_equals(text: &str) -> Option<(&str, &str)> {
    let bytes = text.as_bytes();
    for (at, byte) in bytes.iter().enumerate() {
        if *byte != b'=' {
            continue;
        }
        let before = at.checked_sub(1).and_then(|i| bytes.get(i));
        let after = bytes.get(at + 1);
        if matches!(before, Some(b'<' | b'>' | b'!' | b'=')) || after == Some(&b'=') {
            return None;
        }
        return Some((&text[..at], &text[at + 1..]));
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

/// A name that stands before a value.
fn is_name(text: &str) -> bool {
    let text = text.trim();
    let length = text.chars().count();
    if length == 0 || length > 24 {
        return false;
    }
    let first = text.chars().next().unwrap_or(' ');
    if !(first.is_alphabetic() || first == '(') {
        return false;
    }
    let allowed = |c: char| {
        c.is_alphanumeric()
            || matches!(
                c,
                '_' | '\'' | '^' | '(' | ')' | '+' | '-' | '*' | '/' | '{' | '}' | ',' | ' ' | '.'
            )
    };
    if !text.chars().all(allowed) {
        return false;
    }
    // A comma belongs inside brackets only: `f(x,y)`.
    let mut depth = 0_i32;
    for c in text.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth <= 0 => return false,
            _ => {}
        }
        if depth < 0 {
            return false;
        }
    }
    if depth != 0 {
        return false;
    }
    // Spaces only inside brackets or around a sign: no plain two-word name.
    let outside_spaces = text
        .split(['(', ')'])
        .step_by(2)
        .any(|piece| piece.trim().contains(' ') && !piece.contains(['+', '-', '*', '/']));
    !outside_spaces
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
