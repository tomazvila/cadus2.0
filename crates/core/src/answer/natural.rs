//! Natural readings of a learner answer that the strict grammar misreads.
//!
//! Two learner habits gave a decided "wrong" (or no verdict) to a correct
//! value, and both are a question of reading, not of mathematics:
//!
//! - **Percent number.** The authored answer is a percent literal (`65%`) and
//!   the learner writes the percent's number (`65`, `65 percent`, `65 per
//!   cent`). The grammar reads `65%` as `65/100`, so the bare `65` missed.
//! - **Trailing unit.** The authored answer is a unitless number and the
//!   learner names the unit the question asked in (`8 ft`, `4/9 m`, `€18`,
//!   `18 euros`, `165 km`). The grammar refused the word or read the pair as a
//!   quantity against a number.
//!
//! [`rescue`] runs only AFTER the strict verdict is not "correct". It rewrites
//! the learner text once and grades the rewrite with the same strict rule, so
//! the number is still compared exactly: a wrong number stays wrong, and a
//! rewrite that the grammar cannot read gives back the strict outcome. A
//! unit-required contract never reaches this module (the caller passes only the
//! `exact` contract and the contract-free check).

use super::canon::Canon;
use super::check::{Outcome, canonical_form};

/// The longest learner text this module rewrites.
const MAX_CHARS: usize = 64;

/// Unit and currency words the strict grammar does not hold. A single letter
/// is a unit only through this list or the grammar's own unit table, so `18 x`
/// keeps its strict reading.
const UNIT_WORDS: &[&str] = &[
    "mm",
    "cm",
    "m",
    "km",
    "g",
    "kg",
    "mg",
    "ml",
    "l",
    "s",
    "min",
    "h",
    "hr",
    "hrs",
    "ft",
    "feet",
    "foot",
    "in",
    "inch",
    "inches",
    "yd",
    "yard",
    "yards",
    "mi",
    "mile",
    "miles",
    "mph",
    "lb",
    "lbs",
    "oz",
    "metre",
    "metres",
    "meter",
    "meters",
    "centimetre",
    "centimetres",
    "centimeter",
    "centimeters",
    "millimetre",
    "millimetres",
    "kilometre",
    "kilometres",
    "kilometer",
    "kilometers",
    "gram",
    "grams",
    "kilogram",
    "kilograms",
    "litre",
    "litres",
    "liter",
    "liters",
    "millilitre",
    "millilitres",
    "second",
    "seconds",
    "minute",
    "minutes",
    "hour",
    "hours",
    "day",
    "days",
    "euro",
    "euros",
    "dollar",
    "dollars",
    "cent",
    "cents",
];

/// Currency marks a learner writes in front of or behind an amount.
const CURRENCY: [char; 4] = ['€', '$', '£', '°'];

/// Grade a natural reading of `learner` when the strict outcome was not
/// "correct". `grade` is the strict rule of the caller, applied to the
/// rewritten pair. `None` keeps the strict outcome.
pub(crate) fn rescue(
    expected: &str,
    learner: &str,
    strict: Outcome,
    grade: impl Fn(&str, &str) -> Outcome,
) -> Option<Outcome> {
    if matches!(strict, Outcome::Decided(verdict) if verdict.correct)
        || learner.chars().count() > MAX_CHARS
    {
        return None;
    }
    if let Some(number) = percent_number(expected) {
        // A percent key never takes the unit reading: `0.65 ft` or `65 ft` is
        // not a percent, and the rewrite below would grade it as `0.65` or `65`.
        let bare = strip_percent_word(learner);
        if strip_unit(bare).is_some() {
            return None;
        }
        let outcome = grade(number, bare);
        return matches!(outcome, Outcome::Decided(_)).then_some(outcome);
    }
    let plain_number = matches!(canonical_form(expected), Ok(Canon::Rational(_)));
    if !plain_number {
        return None;
    }
    let bare = strip_unit(learner)?;
    // The rewrite must be a plain number on its own, so a unit never hides an
    // expression such as `2 x` or a pair such as `3, 4`.
    if !matches!(canonical_form(&bare), Ok(Canon::Rational(_))) {
        return None;
    }
    Some(grade(expected, &bare))
}

/// The number of a percent literal (`65%` gives `65`), or `None`.
fn percent_number(expected: &str) -> Option<&str> {
    let number = expected.trim().strip_suffix('%')?.trim_end();
    let digits = number.strip_prefix('-').unwrap_or(number);
    let plain = !digits.is_empty()
        && digits.chars().any(|ch| ch.is_ascii_digit())
        && digits
            .chars()
            .all(|ch| ch.is_ascii_digit() || matches!(ch, '.' | '/'));
    plain.then_some(number)
}

/// The learner text without a trailing `percent` or `per cent` word.
fn strip_percent_word(learner: &str) -> &str {
    let text = learner.trim().trim_end_matches('.').trim_end();
    for word in [" per cent", " percent", "percent"] {
        if let Some(head) = text
            .len()
            .checked_sub(word.len())
            .filter(|at| text.is_char_boundary(*at))
            .filter(|at| text[*at..].eq_ignore_ascii_case(word))
            .map(|at| text[..at].trim_end())
        {
            return head;
        }
    }
    text
}

/// The learner text without one currency mark and one trailing unit, or
/// `None` when the text carries neither.
fn strip_unit(learner: &str) -> Option<String> {
    let mut text = learner.trim().trim_end_matches('.').trim_end();
    let mut changed = false;
    // `$...$` is a math delimiter pair, not a dollar amount; the grammar owns it.
    if text.len() > 1 && text.starts_with('$') && text.ends_with('$') {
        return None;
    }
    if let Some(rest) = text.strip_prefix(CURRENCY) {
        text = rest.trim_start();
        changed = true;
    }
    if let Some(rest) = text.strip_suffix(CURRENCY) {
        text = rest.trim_end();
        changed = true;
    }
    if let Some((head, tail)) = text.rsplit_once(char::is_whitespace)
        && is_unit_word(tail)
    {
        text = head.trim_end();
        changed = true;
    }
    changed.then(|| text.to_owned())
}

/// True for a unit or currency word the strict grammar does not read as a
/// value: the word list above, or a word of two letters or more that names no
/// function, constant or variable of the grammar.
fn is_unit_word(word: &str) -> bool {
    let lower = word.to_lowercase();
    if UNIT_WORDS.contains(&lower.as_str())
        || matches!(word, "L" | "m^2" | "cm^2" | "m^3" | "cm^3" | "km/h" | "m/s")
    {
        return true;
    }
    word.chars().count() >= 2
        && word.chars().all(char::is_alphabetic)
        && !super::parse::FUNCTIONS.contains(&lower.as_str())
        && !matches!(
            lower.as_str(),
            "pi" | "e" | "inf" | "infinity" | "or" | "and"
        )
        && canonical_form(word).is_err()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::answer::{AnswerContract, check, check_contract};
    use crate::curriculum::AnswerKind;

    fn exact(expected: &str, learner: &str) -> Option<bool> {
        match check_contract(expected, learner, AnswerContract::Exact) {
            Outcome::Decided(verdict) => Some(verdict.correct),
            Outcome::Undecidable(_) => None,
        }
    }

    fn plain(expected: &str, learner: &str) -> Option<bool> {
        match check(expected, learner, AnswerKind::Numeric) {
            Outcome::Decided(verdict) => Some(verdict.correct),
            Outcome::Undecidable(_) => None,
        }
    }

    #[test]
    fn a_percent_key_takes_its_bare_number() {
        for learner in [
            "65",
            "65%",
            "65 %",
            "65 percent",
            "65 per cent",
            "65 Percent.",
            "0.65",
            "13/20",
        ] {
            assert_eq!(exact("65%", learner), Some(true), "{learner}");
            assert_eq!(plain("65%", learner), Some(true), "{learner}");
        }
        assert_eq!(exact("12.5%", "12.5"), Some(true));
        for learner in ["66", "6.5", "650", "0.66", "66 percent"] {
            assert_eq!(exact("65%", learner), Some(false), "{learner}");
        }
    }

    #[test]
    fn a_percent_key_never_takes_a_unit() {
        for learner in ["0.65 ft", "0.65 kg", "65 ft", "65 kg", "€65", "0.65 m"] {
            assert_ne!(exact("65%", learner), Some(true), "{learner}");
            assert_ne!(plain("65%", learner), Some(true), "{learner}");
        }
        for learner in ["65", "65 percent", "65%"] {
            assert_eq!(exact("65%", learner), Some(true), "{learner}");
        }
    }

    #[test]
    fn a_named_unit_is_not_checked_against_the_question() {
        // Accepted limitation (checker spec 8.6): the unit is dropped, not
        // compared with the unit the question asked in, and no notation tag
        // marks the reading.
        let outcome = check_contract("8", "8 cm", AnswerContract::Exact);
        assert_eq!(
            outcome,
            Outcome::Decided(crate::answer::Verdict {
                correct: true,
                notation: false
            })
        );
    }

    #[test]
    fn a_bare_number_key_ignores_a_named_unit() {
        for (key, learner) in [
            ("8", "8 ft"),
            ("4/9", "4/9 m"),
            ("18", "€18"),
            ("18", "€ 18"),
            ("18", "18€"),
            ("18", "$18"),
            ("18", "18 $"),
            ("18", "18 euros"),
            ("165", "165 km"),
            ("2.5", "2.5 litres"),
            ("30", "30°"),
            ("21", "21 beads"),
        ] {
            assert_eq!(exact(key, learner), Some(true), "{key} vs {learner}");
            assert_eq!(plain(key, learner), Some(true), "{key} vs {learner}");
        }
    }

    #[test]
    fn a_unit_never_turns_a_wrong_number_right() {
        for (key, learner) in [
            ("8", "9 ft"),
            ("18", "€19"),
            ("165", "16.5 km"),
            ("4/9", "4/7 m"),
            ("21", "21 x"),
            ("21", "21 pi"),
            ("21", "21 sqrt"),
            ("6", "2 x 4 cm"),
            ("21", "21 theta"),
            ("21", "21 ln"),
            ("9/2", "$9/4$."),
        ] {
            assert_ne!(exact(key, learner), Some(true), "{key} vs {learner}");
        }
    }

    #[test]
    fn an_expression_key_is_not_rewritten() {
        assert_eq!(strip_unit("2 x"), None);
        let strict = Outcome::Decided(crate::answer::Verdict {
            correct: false,
            notation: false,
        });
        assert_eq!(rescue("x + 1", "x + 1 cm", strict, |_, _| strict), None);
    }

    #[test]
    fn a_unit_contract_stays_strict() {
        let unit = AnswerContract::Unit {
            quantity: crate::answer::Quantity::Length,
            unit: "cm".to_owned(),
        };
        assert!(!matches!(
            check_contract("18 cm", "18", unit),
            Outcome::Decided(verdict) if verdict.correct
        ));
    }
}
