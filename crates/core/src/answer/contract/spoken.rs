//! A value written inside a short sentence or in spoken words.
//!
//! The strict reader takes a value, and nothing else. A learner often writes
//! `the limit is 1/2`, `≈ 3 m`, `two thirds`, or `the committee {1, 2, 3}`.
//! This module reads the value out of such a text and hands the value to the
//! contract again. It never turns a refusal into a "correct" without the value
//! passing the contract, and it takes no text that holds a negation, a doubt,
//! or a second choice: `the limit is not 1/2` and `1/2 or 1/3` keep the strict
//! outcome.
//!
//! Only a refusal changes: a verdict of the strict reader stands.

use super::AnswerContract;
use crate::answer::Outcome;

/// The longest text this module reads, in characters.
const MAX_CHARS: usize = 120;

/// The most words in front of the verb of a sentence.
const MAX_HEAD_WORDS: usize = 7;

/// The words that make a text something other than a plain statement of a value.
const BLOCKED: &[&str] = &[
    "not", "no", "never", "nor", "neither", "cannot", "without", "except", "but", "or", "and",
    "if", "maybe", "perhaps", "probably", "because", "since", "however", "unless", "although",
    "than", "isn't", "aren't", "doesn't", "don't", "can't", "won't",
];

/// The marks and words that open an approximate statement of a value.
const APPROXIMATE: &[&str] = &[
    "about",
    "around",
    "roughly",
    "approximately",
    "approx",
    "nearly",
];

/// Read a value out of the learner text, and grade the value, when the strict
/// outcome is a refusal.
pub(super) fn rescue(
    expected: &str,
    learner: &str,
    contract: &AnswerContract,
    strict: Outcome,
) -> Outcome {
    if !matches!(strict, Outcome::Undecidable(_))
        || matches!(
            contract,
            AnswerContract::Label { .. }
                | AnswerContract::Multipart { .. }
                | AnswerContract::Property { .. }
                | AnswerContract::OrderedWord
                | AnswerContract::Written
                | AnswerContract::None
        )
        || learner.chars().count() > MAX_CHARS
    {
        return strict;
    }
    let mut first_wrong = None;
    for (at, candidate) in candidates(learner).into_iter().enumerate() {
        match super::evaluate::check_contract(expected, &candidate, contract.clone()) {
            Outcome::Decided(verdict) if verdict.correct => return Outcome::Decided(verdict),
            // Only the value that the text names itself may be called wrong: the
            // first candidate is the text without its lead or its mark.
            Outcome::Decided(verdict) if at == 0 && first_wrong.is_none() => {
                first_wrong = Some(verdict);
            }
            _ => {}
        }
    }
    first_wrong.map_or(strict, Outcome::Decided)
}

/// The readings of a text, each shorter than the text, in the order of trust.
fn candidates(learner: &str) -> Vec<String> {
    let text = learner.trim().trim_end_matches(['.', '!']).trim();
    let mut readings: Vec<String> = Vec::new();
    if let Some(value) = approximate_lead(text) {
        readings.push(value.to_owned());
    }
    if let Some(value) = sentence_tail(text).or_else(|| word_lead_set(text)) {
        readings.push(value.to_owned());
        if let Some(bare) = strip_label(value) {
            readings.push(bare.to_owned());
        }
    }
    if let Some(value) = inverse_phrase(text) {
        readings.push(value);
    }
    if let Some(tail) = comma_tail(text) {
        readings.push(tail.to_owned());
    }
    for source in std::iter::once(text).chain(readings.clone().iter().map(String::as_str)) {
        if let Some(number) = spoken_number(source) {
            readings.push(number);
        }
    }
    readings.retain(|reading| !reading.is_empty() && reading != learner);
    let mut seen: Vec<String> = Vec::new();
    for reading in readings {
        if !seen.contains(&reading) {
            seen.push(reading);
        }
    }
    seen
}

/// `g inverse`, `the inverse of g`, and `the inverse g^{-1}` as a power of `-1`.
fn inverse_phrase(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let single = |symbol: &str| {
        let mut letters = symbol.chars();
        matches!((letters.next(), letters.next()), (Some(c), None) if c.is_alphabetic())
    };
    let symbol = lower
        .strip_suffix(" inverse")
        .or_else(|| lower.strip_prefix("the inverse of "))
        .or_else(|| lower.strip_prefix("the inverse "))
        .map(str::trim)?;
    if single(symbol) {
        return Some(format!("{symbol}^(-1)"));
    }
    // `g^{-1}` written after the words keeps its own power.
    let power = symbol.contains("^") && single(&symbol[..symbol.find('^')?]);
    power.then(|| symbol.to_owned())
}

/// The part after the one comma of `a half-turn, 180 degrees`, when the part
/// before it is a few plain words.
fn comma_tail(text: &str) -> Option<&str> {
    let (head, tail) = text.split_once(',')?;
    let words: Vec<&str> = head.split_whitespace().collect();
    let plain = !words.is_empty()
        && words.len() <= 4
        && words
            .iter()
            .enumerate()
            .all(|(at, word)| plain_word(word, at == 0));
    (plain && !tail.contains(',')).then(|| tail.trim())
}

/// The text after `≈`, `~`, or a word such as `about`.
fn approximate_lead(text: &str) -> Option<&str> {
    if let Some(rest) = text.strip_prefix(['≈', '~']) {
        return Some(rest.trim());
    }
    let (word, rest) = text.split_once(char::is_whitespace)?;
    APPROXIMATE
        .contains(&word.trim_end_matches('.').to_lowercase().as_str())
        .then(|| rest.trim())
}

/// Whether a word of the head of a sentence is a plain word.
fn plain_word(word: &str, first: bool) -> bool {
    let lower = word.to_lowercase();
    !BLOCKED.contains(&lower.as_str())
        && word.chars().all(|c| c.is_alphabetic() || c == '-')
        && word.chars().any(char::is_alphabetic)
        && word
            .chars()
            .enumerate()
            .all(|(at, c)| !c.is_uppercase() || (first && at == 0))
}

/// The value after the verb of `the limit is 1/2`, `the roots are 2 and 3`, or
/// `the limit = 1/2`.
fn sentence_tail(text: &str) -> Option<&str> {
    let mut head_words = 0_usize;
    let mut rest = text;
    loop {
        let (word, after) = rest.split_once(char::is_whitespace)?;
        let lower = word.to_lowercase();
        if matches!(lower.as_str(), "is" | "are" | "equals" | "=") {
            let tail = after.trim();
            let opens_with_block = tail
                .split_whitespace()
                .next()
                .is_some_and(|first| BLOCKED.contains(&first.to_lowercase().as_str()));
            // A second `=` makes a chain such as `x = 1 = 2`. The chain names no one
            // value, so the strict refusal stands (rule 2 of grader pass 4).
            let chain = lower == "=" && tail.contains(['=', '<', '>']);
            // One word before `=` is a name; the name reading strips it.
            let name_only = lower == "=" && head_words < 2;
            return (head_words > 0
                && !tail.is_empty()
                && !opens_with_block
                && !chain
                && !name_only)
                .then_some(tail);
        }
        if !plain_word(word, head_words == 0) {
            return None;
        }
        head_words += 1;
        if head_words > MAX_HEAD_WORDS {
            return None;
        }
        rest = after.trim_start();
    }
}

/// A braced set after a few plain words: `the committee {1, 2, 3}`.
fn word_lead_set(text: &str) -> Option<&str> {
    let brace = text.find('{')?;
    let (head, set) = text.split_at(brace);
    let words: Vec<&str> = head.split_whitespace().collect();
    let plain = !words.is_empty()
        && words.len() <= 4
        && words
            .iter()
            .enumerate()
            .all(|(at, word)| plain_word(word, at == 0));
    (plain && set.ends_with('}')).then_some(set)
}

/// The value of `y = 0`: the text after a one-name label.
fn strip_label(text: &str) -> Option<&str> {
    let (name, value) = text.split_once('=')?;
    let name = name.trim();
    let named = !name.is_empty()
        && name.chars().count() <= 12
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '\'');
    (named && !value.trim().is_empty() && !value.contains('=')).then(|| value.trim())
}

/// The cardinal words, each with its value.
const CARDINALS: &[(&str, u32)] = &[
    ("zero", 0),
    ("one", 1),
    ("two", 2),
    ("three", 3),
    ("four", 4),
    ("five", 5),
    ("six", 6),
    ("seven", 7),
    ("eight", 8),
    ("nine", 9),
    ("ten", 10),
    ("eleven", 11),
    ("twelve", 12),
    ("thirteen", 13),
    ("fourteen", 14),
    ("fifteen", 15),
    ("sixteen", 16),
    ("seventeen", 17),
    ("eighteen", 18),
    ("nineteen", 19),
    ("twenty", 20),
    ("thirty", 30),
    ("forty", 40),
    ("fifty", 50),
    ("sixty", 60),
    ("seventy", 70),
    ("eighty", 80),
    ("ninety", 90),
];

/// The words of a fraction part, singular and plural, with the denominator.
const PARTS: &[(&str, &str, u32)] = &[
    ("half", "halves", 2),
    ("third", "thirds", 3),
    ("quarter", "quarters", 4),
    ("fourth", "fourths", 4),
    ("fifth", "fifths", 5),
    ("sixth", "sixths", 6),
    ("seventh", "sevenths", 7),
    ("eighth", "eighths", 8),
    ("ninth", "ninths", 9),
    ("tenth", "tenths", 10),
    ("twelfth", "twelfths", 12),
    ("hundredth", "hundredths", 100),
];

/// A whole text of number words as a number: `twelve`, `two thirds`, `a half`,
/// `minus three`, `forty-two`.
fn spoken_number(text: &str) -> Option<String> {
    let lower = text.to_lowercase().replace('-', " ");
    let mut words: Vec<&str> = lower.split_whitespace().collect();
    let negative = matches!(words.first(), Some(&("minus" | "negative")));
    if negative {
        words.remove(0);
    }
    let sign = if negative { "-" } else { "" };
    let part = |word: &str| {
        PARTS
            .iter()
            .find(|(one, many, _)| *one == word || *many == word)
            .map(|(_, _, denominator)| *denominator)
    };
    if let [article, word] = words.as_slice()
        && matches!(*article, "a" | "an")
        && let Some(denominator) = part(word)
    {
        return Some(format!("{sign}1/{denominator}"));
    }
    if let [word] = words.as_slice()
        && let Some(denominator) = part(word).filter(|_| *word == "half")
    {
        return Some(format!("{sign}1/{denominator}"));
    }
    let (last, head) = words.split_last()?;
    if let Some(denominator) = part(last)
        && let Some(numerator) = cardinal(head)
    {
        return Some(format!("{sign}{numerator}/{denominator}"));
    }
    cardinal(&words).map(|value| format!("{sign}{value}"))
}

/// The value of cardinal words up to 999.
fn cardinal(words: &[&str]) -> Option<u32> {
    let mut total = 0_u32;
    let mut current = 0_u32;
    let mut seen = false;
    for word in words {
        if *word == "and" && seen {
            continue;
        }
        if *word == "hundred" {
            if current == 0 || current > 9 {
                return None;
            }
            total += current * 100;
            current = 0;
            seen = true;
            continue;
        }
        let value = CARDINALS
            .iter()
            .find(|(name, _)| name == word)
            .map(|(_, value)| *value)?;
        // A tens word may take a unit word after it, and nothing else may.
        let fits = match (current, value) {
            (0, _) => true,
            (tens, unit) => tens % 10 == 0 && tens >= 20 && (1..=9).contains(&unit),
        };
        if !fits {
            return None;
        }
        current += value;
        seen = true;
    }
    (seen && words.last() != Some(&"and")).then_some(total + current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_words_read_as_numbers() {
        for (text, value) in [
            ("two thirds", "2/3"),
            ("a half", "1/2"),
            ("three quarters", "3/4"),
            ("twelve", "12"),
            ("forty-two", "42"),
            ("minus five", "-5"),
            ("one hundred and twenty five", "125"),
        ] {
            assert_eq!(spoken_number(text).as_deref(), Some(value), "{text}");
        }
        for text in ["", "the", "twenty twenty", "hundred", "three and"] {
            assert_eq!(spoken_number(text), None, "{text}");
        }
    }

    #[test]
    fn a_sentence_gives_its_value() {
        assert_eq!(sentence_tail("the limit is 1/2"), Some("1/2"));
        assert_eq!(
            sentence_tail("The horizontal asymptote is y = 0"),
            Some("y = 0")
        );
        assert_eq!(sentence_tail("the limit is not 1/2"), None);
        assert_eq!(sentence_tail("it is 1/2 or 1/3"), Some("1/2 or 1/3"));
        assert_eq!(sentence_tail("x^2 is 5"), None);
        assert_eq!(strip_label("y = 0"), Some("0"));
    }
}
