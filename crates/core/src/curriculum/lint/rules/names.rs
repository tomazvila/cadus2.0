//! The name math rule: `name_math_unmarked`.
//!
//! The SPA shows the name of a topic and the name of a knowledge point as a
//! heading or a label. It renders the text between `$` marks as math. A name
//! that holds math outside the marks shows as raw text. The rule finds such a
//! name. The fix is to wrap the math in `$...$` or to say it in words.
//!
//! A math-like token is one of these:
//!
//! - a `^`, a backslash, a `=`, or a `_`
//! - a `/` next to a one-letter or digit run, or next to a bracket
//! - a Greek letter, as a symbol or as a name next to an operator or a bracket
//! - a one-letter variable next to an operator or a tight bracket
//! - a function name (`sin`, `ln`, ...) followed by a one-letter variable
//! - an odd count of `$` marks

use super::super::super::finding::Finding;
use super::super::Lint;

const GREEK_NAMES: [&str; 14] = [
    "alpha", "beta", "gamma", "delta", "epsilon", "theta", "lambda", "mu", "pi", "rho", "sigma",
    "tau", "phi", "omega",
];

const FUNCTIONS: [&str; 9] = ["sin", "cos", "tan", "sec", "csc", "cot", "ln", "log", "exp"];

/// Rate units: plain text, not a formula.
const UNIT_PAIRS: [&str; 6] = ["km/h", "m/s", "cm/s", "km/s", "m/min", "g/cm"];

/// The characters that join a variable to another term.
const OPERATORS: &str = "+*<>≤≥≠±·×÷−=^_→←";

/// Remove every `$...$` run. A run leaves one `#` in its place. Return `None`
/// when a `$` has no partner.
fn strip_math(name: &str) -> Option<String> {
    let mut out = String::new();
    let mut inside = false;
    for ch in name.chars() {
        if ch == '$' {
            if inside {
                out.push('#');
            }
            inside = !inside;
        } else if !inside {
            out.push(ch);
        }
    }
    if inside { None } else { Some(out) }
}

fn is_greek_char(ch: char) -> bool {
    ('\u{391}'..='\u{3C9}').contains(&ch)
}

/// The first math-like token of a name outside the `$...$` runs, as a short
/// reason. `None` means the name is plain, or all its math is marked.
pub(in crate::curriculum::lint) fn math_token(name: &str) -> Option<&'static str> {
    let Some(text) = strip_math(name) else {
        return Some("an unpaired $");
    };
    let chars: Vec<char> = text.chars().collect();
    let at = |i: isize| -> char {
        usize::try_from(i)
            .ok()
            .and_then(|i| chars.get(i))
            .copied()
            .unwrap_or(' ')
    };
    for (i, &ch) in chars.iter().enumerate() {
        match ch {
            '^' => return Some("the sign ^"),
            '\\' => return Some("a backslash"),
            '=' => return Some("the sign ="),
            '_' => return Some("the sign _"),
            c if is_greek_char(c) => return Some("a Greek letter"),
            _ => {}
        }
        if ch == '/'
            && let Some(reason) = slash_reason(&chars, i)
        {
            return Some(reason);
        }
    }
    // Words: alphanumeric runs with their position.
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_alphanumeric() {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && chars[i].is_alphanumeric() {
            i += 1;
        }
        let run: String = chars[start..i].iter().collect();
        let lower = run.to_lowercase();
        let before = at(start as isize - 1);
        let after = at(i as isize);
        let is_single = run.chars().count() == 1 && run.is_ascii();
        let letter = is_single && run.chars().all(char::is_alphabetic);
        if GREEK_NAMES.contains(&lower.as_str()) {
            let near = |c: char| "()[]{}+*/^_:=<>≤≥|".contains(c);
            let (prev, next) = (neighbor(&chars, start, false), neighbor(&chars, i, true));
            if near(before) || near(after) || near(prev) || near(next) {
                return Some("a Greek letter name used as a symbol");
            }
        }
        if letter && before != '\'' {
            let operator = |c: char| OPERATORS.contains(c);
            let prev = neighbor(&chars, start, false);
            let next = neighbor(&chars, i, true);
            let spaced_minus = |c: char, at_dash: isize| {
                c == '-' && at(at_dash - 1) == ' ' && at(at_dash + 1) == ' '
            };
            let dash_before = spaced_minus(prev, back_index(&chars, start));
            let dash_after = spaced_minus(next, forward_index(&chars, i));
            let plural = lower == "s" && before == '(' && after == ')';
            let tight = (after == '('
                || after == '['
                || before == '['
                || after == ']'
                || (before == '(' && (after == ')' || after == ',')))
                && !plural
                || (after == '\'' && at(i as isize + 1) == '(');
            let lower_letter = run.chars().all(|c| c.is_ascii_lowercase());
            let digit_minus = lower_letter
                && ((after == '-' && at(i as isize + 1).is_ascii_digit())
                    || (before == '-' && at(start as isize - 2).is_ascii_digit()));
            if digit_minus || operator(prev) || operator(next) || dash_before || dash_after || tight
            {
                return Some("a one-letter variable in a formula");
            }
        }
        if (FUNCTIONS.contains(&lower.as_str()) || ["sqrt", "det", "gcd"].contains(&lower.as_str()))
            && after == '('
        {
            return Some("a function call");
        }
        if FUNCTIONS.contains(&lower.as_str()) && after == ' ' {
            // A function name followed by a one-letter variable: `ln x`.
            let mut j = i;
            while j < chars.len() && chars[j] == ' ' {
                j += 1;
            }
            let end = (j..chars.len())
                .find(|&k| !chars[k].is_alphanumeric())
                .unwrap_or(chars.len());
            if end == j + 1 && chars[j].is_ascii_alphabetic() {
                return Some("a function name with a variable");
            }
        }
    }
    None
}

/// Index of the nearest non-space character before `start` (or 0).
fn back_index(chars: &[char], start: usize) -> isize {
    let mut j = start as isize - 1;
    while j >= 0 && chars[j as usize] == ' ' {
        j -= 1;
    }
    j
}

fn forward_index(chars: &[char], end: usize) -> isize {
    let mut j = end;
    while j < chars.len() && chars[j] == ' ' {
        j += 1;
    }
    j as isize
}

/// The nearest non-space character before `index` (`forward` false) or from
/// `index` on (`forward` true). A space stands for "none".
fn neighbor(chars: &[char], index: usize, forward: bool) -> char {
    let at = if forward {
        forward_index(chars, index)
    } else {
        back_index(chars, index)
    };
    usize::try_from(at)
        .ok()
        .and_then(|i| chars.get(i))
        .copied()
        .unwrap_or(' ')
}

/// A `/` joins math when it touches a one-letter or digit run, or a bracket.
fn slash_reason(chars: &[char], i: usize) -> Option<&'static str> {
    let left_end = i;
    let mut left_start = left_end;
    while left_start > 0 && chars[left_start - 1].is_alphanumeric() {
        left_start -= 1;
    }
    let mut right_end = i + 1;
    while right_end < chars.len() && chars[right_end].is_alphanumeric() {
        right_end += 1;
    }
    let left = &chars[left_start..left_end];
    let right = &chars[i + 1..right_end];
    let pair: String = left.iter().chain(&['/']).chain(right).collect();
    if UNIT_PAIRS.contains(&pair.as_str()) {
        return None;
    }
    let bracket = |c: Option<&char>| c.is_some_and(|c| "()[]#∞".contains(*c));
    let marked = |run: &[char]| {
        run.len() == 1
            || (run.len() == 2 && run[0] == 'd' && run[1].is_ascii_lowercase())
            || run.iter().any(|c| c.is_ascii_digit())
    };
    let left_bracket = left.is_empty() && i > 0 && bracket(chars.get(i - 1));
    let right_bracket = right.is_empty() && bracket(chars.get(i + 1));
    let left_text = !left.is_empty();
    let right_text = !right.is_empty();
    if (left_text && right_text && (marked(left) || marked(right)))
        || (left_text && right_bracket)
        || (right_text && left_bracket)
        || (left_bracket && right_bracket)
    {
        return Some("a / between math terms");
    }
    None
}

/// File `name_math_unmarked` for each topic and knowledge-point name.
pub(in crate::curriculum::lint) fn check_names(lint: &mut Lint<'_>) {
    let mut found = Vec::new();
    for topic in &lint.table.topics {
        let names = std::iter::once((topic.id.as_str(), "topic", topic.name.as_str())).chain(
            topic
                .knowledge_points
                .iter()
                .map(|kp| (kp.id.as_str(), "knowledge point", kp.name.as_str())),
        );
        for (id, kind, name) in names {
            if let Some(reason) = math_token(name) {
                found.push(
                    Finding::new(
                        "name_math_unmarked",
                        format!(
                            "{kind} name \"{name}\" holds {reason} outside $...$. Wrap the math in $...$ or say it in words."
                        ),
                    )
                    .with_topic(topic.id.as_str())
                    .with_context(vec![id.to_owned(), name.to_owned()]),
                );
            }
        }
    }
    lint.findings.extend(found);
}
