//! The handed value rule: `handed_value`.
//!
//! A question never gives the learner a value that the question exists to make
//! the learner produce. The learner reads `e^x`, a logarithm, a trigonometric
//! value, a root or a power on their own calculator, or keeps the exact form. A
//! question states the rounding it wants and nothing else.
//!
//! [`handed_value`] finds the defect in one problem text. It finds:
//!
//! - an `≈` or an `\approx` with a number after it
//! - a trigger word (`use`, `using`, `take`, `taking`, `given`, `assume`,
//!   `assuming`, `with`) before a function value or a constant that `=` sets
//!   to a number, such as `using e^{0.3} = 1.35`
//! - `π ≈` in any form
//!
//! The rule covers an exemplar, a diagnostic and a try-first problem. A
//! `written` item and a teach text are not covered.

use super::super::super::finding::Finding;
use super::super::Lint;
use crate::answer::AnswerContract;

const TRIGGERS: [&str; 8] = [
    "use", "using", "take", "taking", "given", "assume", "assuming", "with",
];

/// The longest stretch of text that a trigger word may stand from its value.
const REACH: usize = 60;

/// The fix that every `handed_value` message ends with.
pub const HANDED_FIX: &str = "State the rounding only. The learner finds the value with their own calculator, or keeps the exact form.";

fn is_digit(c: char) -> bool {
    c.is_ascii_digit()
}

/// The end of a number that starts at `start` (after spaces and one `-`), or
/// `None` when no number stands there.
fn number_end(chars: &[char], start: usize) -> Option<usize> {
    let mut i = start;
    while i < chars.len() && chars[i] == ' ' {
        i += 1;
    }
    if i < chars.len() && matches!(chars[i], '-' | '−') {
        i += 1;
    }
    let digits_from = i;
    while i < chars.len() && is_digit(chars[i]) {
        i += 1;
    }
    if i == digits_from {
        return None;
    }
    if i + 1 < chars.len() && matches!(chars[i], '.' | ',') && is_digit(chars[i + 1]) {
        i += 1;
        while i < chars.len() && is_digit(chars[i]) {
            i += 1;
        }
    }
    Some(i)
}

fn starts_with_at(chars: &[char], at: usize, word: &str) -> bool {
    word.chars()
        .enumerate()
        .all(|(k, w)| chars.get(at + k) == Some(&w))
}

/// Strip the words that a numeric argument may hold. Return the rest.
fn argument_rest(arg: &str) -> String {
    let mut rest = arg.to_owned();
    for word in [
        "\\left", "\\right", "\\circ", "\\pi", "\\cdot", "degrees", "degree", "rad", "π", "°",
    ] {
        rest = rest.replace(word, "");
    }
    rest
}

/// Whether `arg` is a number-only argument: digits, signs, brackets, a slash.
fn is_numeric_argument(arg: &str) -> bool {
    let had_pi = arg.contains("\\pi") || arg.contains('π');
    let rest = argument_rest(arg);
    let allowed =
        |c: char| c.is_ascii_digit() || " .-−+/(){}[]^_*".contains(c) || c == '\\' || c == ',';
    rest.chars().all(allowed) && (had_pi || rest.chars().any(is_digit))
}

/// The value of `base^exp` for a number-only pair, when it reads.
fn power_value(text: &str) -> Option<f64> {
    let (base, exp) = text.split_once('^')?;
    let base: f64 = base.trim().parse().ok()?;
    let exp = exp
        .trim()
        .trim_start_matches('{')
        .trim_end_matches('}')
        .trim();
    let exp = if let Some((n, d)) = exp.split_once('/') {
        n.trim().parse::<f64>().ok()? / d.trim().parse::<f64>().ok()?
    } else {
        exp.parse::<f64>().ok()?
    };
    Some(base.powf(exp))
}

/// Whether the whole `left` is one function value or constant with a number
/// argument, as `e^{0.3}`, `\ln 2`, `sin 30°`, `\sqrt{2}`, `π`, `2^{0.5}`.
fn is_handed_left(left: &str) -> bool {
    let left = left.trim().trim_matches('$').trim();
    if left == "π" || left == "\\pi" {
        return true;
    }
    if let Some(arg) = left.strip_prefix("e^") {
        return is_numeric_argument(arg);
    }
    for name in ["ln", "log", "sin", "cos", "tan", "sqrt"] {
        let rest = left
            .strip_prefix('\\')
            .unwrap_or(left)
            .strip_prefix(name)
            .map(str::to_owned);
        if let Some(arg) = rest {
            return is_numeric_argument(&arg);
        }
    }
    let plain = left.replace("\\left", "").replace("\\right", "");
    if plain.chars().next().is_some_and(is_digit)
        && let Some(value) = power_value(&plain)
    {
        return value.is_finite() && (value - value.round()).abs() > 1e-9;
    }
    false
}

/// The char offsets at which a left side may start inside `chars[..end]`.
fn left_starts(chars: &[char], end: usize) -> Vec<usize> {
    let mut starts = Vec::new();
    for at in 0..end {
        let before = at.checked_sub(1).map(|b| chars[b]);
        let alpha_before = before.is_some_and(char::is_alphabetic);
        let digit_start = is_digit(chars[at]) && !before.is_some_and(|b| is_digit(b) || b == '.');
        let word_start = !alpha_before
            && (chars[at] == 'e'
                || chars[at] == 'π'
                || chars[at] == '\\'
                || ["ln", "log", "sin", "cos", "tan", "sqrt"]
                    .iter()
                    .any(|name| starts_with_at(chars, at, name)));
        if digit_start || word_start {
            starts.push(at);
        }
    }
    starts
}

/// Whether a trigger word stands in the sentence before `at`, within reach.
fn trigger_before(chars: &[char], at: usize) -> bool {
    let from = at.saturating_sub(REACH);
    let mut window: String = chars[from..at].iter().collect();
    if let Some(cut) = window.rfind(['?', '!']) {
        window = window[cut + 1..].to_owned();
    }
    if let Some(cut) = window.rfind(". ") {
        window = window[cut + 2..].to_owned();
    }
    let lower = window.to_lowercase();
    lower
        .split(|c: char| !c.is_alphabetic())
        .any(|word| TRIGGERS.contains(&word))
}

fn quote(chars: &[char], from: usize, to: usize) -> String {
    chars[from..to].iter().collect::<String>().trim().to_owned()
}

/// The handed value of one problem text, quoted, or `None`.
#[must_use]
pub fn handed_value(text: &str) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    for (at, &c) in chars.iter().enumerate() {
        let (sign_len, is_approx) = if c == '≈' {
            (1, true)
        } else if starts_with_at(&chars, at, "\\approx") {
            (7, true)
        } else {
            (1, false)
        };
        if is_approx {
            let mut from = at.saturating_sub(30);
            if from > 0
                && let Some(space) = chars[from..at].iter().position(|&x| x == ' ')
            {
                from += space + 1;
            }
            if let Some(stop) = chars[from..at].windows(2).rposition(|w| w == ['.', ' ']) {
                from += stop + 2;
            }
            let left = quote(&chars, from, at);
            let left_is_pi = left.ends_with('π') || left.ends_with("\\pi");
            if let Some(end) = number_end(&chars, at + sign_len) {
                return Some(quote(&chars, from, end));
            }
            if left_is_pi {
                return Some(format!(
                    "{left} {}",
                    if c == '≈' { "≈" } else { "\\approx" }
                ));
            }
        } else if c == '=' && chars.get(at + 1) != Some(&'=') {
            let Some(end) = number_end(&chars, at + 1) else {
                continue;
            };
            let begin = at.saturating_sub(REACH);
            for start in left_starts(&chars, at) {
                if start < begin {
                    continue;
                }
                let left = quote(&chars, start, at);
                if is_handed_left(&left) && trigger_before(&chars, start) {
                    return Some(quote(&chars, start, end));
                }
            }
        }
    }
    None
}

fn is_written(contract: Option<&AnswerContract>) -> bool {
    matches!(contract, Some(AnswerContract::Written))
}

/// File `handed_value` for each exemplar, diagnostic and try-first problem.
pub(in crate::curriculum::lint) fn check_handed(lint: &mut Lint<'_>) {
    let mut found = Vec::new();
    let file = |topic: &str, kp: &str, item: String, text: &str, found: &mut Vec<Finding>| {
        if let Some(value) = handed_value(text) {
            found.push(
                Finding::new(
                    "handed_value",
                    format!(
                        "topic {topic}, point {kp}, {item} hands the value \"{value}\". {HANDED_FIX}"
                    ),
                )
                .with_topic(topic)
                .with_context(vec![kp.to_owned(), item, value]),
            );
        }
    };
    for topic in &lint.table.topics {
        let tid = topic.id.as_str();
        if let Some(probe) = &topic.diagnostic_exemplar
            && !is_written(probe.answer_contract.as_ref())
        {
            file(
                tid,
                "-",
                "diagnostic".to_owned(),
                &probe.problem,
                &mut found,
            );
        }
        for kp in &topic.knowledge_points {
            let kid = kp.id.as_str();
            for (index, exemplar) in kp.exemplars.iter().enumerate() {
                if !is_written(exemplar.answer_contract.as_ref()) {
                    let item = format!("exemplar {}", index + 1);
                    file(tid, kid, item, &exemplar.problem, &mut found);
                }
            }
            if let Some(first) = &kp.try_first
                && !is_written(Some(&first.answer_contract))
            {
                file(tid, kid, "try_first".to_owned(), &first.problem, &mut found);
            }
        }
    }
    lint.findings.extend(found);
}

#[cfg(test)]
mod tests {
    use super::handed_value;

    #[test]
    fn handed_values_are_found() {
        for text in [
            "Using $e^{0.3} \\approx 1.34986$, find the population.",
            "Find x. Take $\\sin 30^\\circ = 0.5$.",
            "Given that $\\ln 2 = 0.693$, find $\\ln 8$.",
            "Assume $\\sqrt{2} = 1.414$ and find the diagonal.",
            "Use $\\pi \\approx 3.14$.",
            "Use π ≈ 3.14 for the area.",
            "with $2^{0.5} = 1.414$ compute it",
            "Use π ≈ and round.",
        ] {
            assert!(handed_value(text).is_some(), "{text}");
        }
        assert_eq!(
            handed_value("Using e^{0.3} ≈ 1.34986, find P.").as_deref(),
            Some("Using e^{0.3} ≈ 1.34986")
        );
    }

    #[test]
    fn rounding_only_and_plain_equations_pass() {
        for text in [
            "Find the population after 10 years, to the nearest whole number.",
            "Find the area in terms of $\\pi$.",
            "Solve $2^x = 8$ with $x$ real.",
            "Given that $f(x) = 3$, find $x$.",
            "Round $\\sqrt{2}$ to two decimal places.",
            "Use the fact that $2^{3} = 8$ to find $2^{4}$.",
            "Evaluate $\\sin 30^\\circ = ?$",
        ] {
            assert_eq!(handed_value(text), None, "{text}");
        }
    }
}
