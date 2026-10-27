//! Agreement between a figure and the text that uses it.
//!
//! A figure names labels and values. The text of an item must state them.
//! A figure label must not reuse a letter that the text uses as a quantity.
//! The lint rules of the curriculum and the teach check share these two tests.

use super::VisualSpec;

/// Letters that name a quantity more often than a point.
const QUANTITY_LETTERS: &[&str] = &[
    "A", "P", "V", "C", "S", "b", "h", "l", "w", "r", "d", "x", "y",
];

/// Words that introduce a quantity letter ("area $A$").
const QUANTITY_WORDS: &[&str] = &[
    "area",
    "perimeter",
    "volume",
    "circumference",
    "radius",
    "diameter",
    "height",
    "width",
    "length",
    "base",
];

/// The labels and values of the figure that `text` never states.
#[must_use]
pub fn absent_terms(spec: &VisualSpec, text: &str) -> Vec<String> {
    let terms = spec.item_terms();
    let numbers = number_tokens(text);
    let words = letter_tokens(text);
    let mut absent: Vec<String> = Vec::new();
    // A slider figure: the worked example must use the name of the parameter.
    if let Some(motion) = spec.motion() {
        let name = motion.parameter.name.trim().to_lowercase();
        let lower = text.to_lowercase();
        if !name.is_empty() && !name.split_whitespace().all(|word| lower.contains(word)) {
            absent.push(motion.parameter.name.trim().to_owned());
        }
    }
    for label in &terms.labels {
        let digits = number_tokens(label);
        if digits.is_empty() {
            if !label_appears(label, &words) {
                absent.push(label.clone());
            }
        } else if !digits.iter().all(|number| numbers.contains(number)) {
            absent.push(label.clone());
        }
    }
    for value in &terms.values {
        let digits = number_tokens(value);
        if !digits.iter().all(|number| numbers.contains(number)) {
            absent.push(value.clone());
        }
    }
    absent
}

/// The figure labels that reuse a quantity letter of `problem`.
#[must_use]
pub fn clashing_labels(spec: &VisualSpec, problem: &str) -> Vec<String> {
    spec.item_terms()
        .labels
        .iter()
        .filter_map(|label| clashing_letter(label, problem).map(str::to_owned))
        .collect()
}

/// Every unsigned number in `text`, as written without a leading zero run.
fn number_tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = text.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        let joins = c == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit);
        if c.is_ascii_digit() || (joins && !current.is_empty()) {
            current.push(c);
        } else if !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// The alphabetic runs of `text` outside of `\command` names.
fn letter_tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_command = false;
    for c in text.chars() {
        if c == '\\' {
            in_command = true;
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
        } else if c.is_alphabetic() {
            if !in_command {
                current.push(c);
            }
        } else {
            in_command = false;
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// Whether the problem names the label: as its own token, or as one capital
/// inside a run of capitals such as `ABC` or `AB`.
fn label_appears(label: &str, words: &[String]) -> bool {
    let name: String = label.chars().take_while(|c| c.is_alphabetic()).collect();
    if name.is_empty() {
        return true;
    }
    words.iter().any(|word| {
        *word == name
            || (name.chars().count() == 1
                && word.chars().count() <= 4
                && word.chars().all(char::is_uppercase)
                && word.contains(&name))
    })
}

/// A quantity letter the figure label reuses, when the problem uses it as a
/// quantity and never as a point.
fn clashing_letter<'a>(label: &'a str, problem: &str) -> Option<&'a str> {
    let name: String = label.chars().take_while(|c| c.is_alphabetic()).collect();
    let letter = QUANTITY_LETTERS.iter().find(|letter| **letter == name)?;
    let words = letter_tokens(problem);
    let lower = problem.to_ascii_lowercase();
    let wanted = letter.to_ascii_lowercase();
    let words_lower: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    let as_point = words_lower.windows(2).any(|pair| {
        ["point", "points", "vertex", "vertices"].contains(&pair[0]) && pair[1] == wanted
    });
    if as_point || !words.iter().any(|word| word == letter) {
        return None;
    }
    let compact: String = problem.chars().filter(|c| !c.is_whitespace()).collect();
    let assigned = compact.contains(&format!("{letter}="));
    let introduced = QUANTITY_WORDS.iter().any(|word| {
        lower.match_indices(word).any(|(at, _)| {
            let rest = problem[at + word.len()..].trim_start_matches([' ', '$', '(']);
            rest.strip_prefix(letter)
                .is_some_and(|after| !after.starts_with(char::is_alphabetic))
        })
    });
    (assigned || introduced).then_some(label)
}
