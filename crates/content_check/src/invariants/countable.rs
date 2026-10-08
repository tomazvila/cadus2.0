//! Rule D34: a word problem that asks "how many" of a countable thing needs a
//! whole-number key. A learner counts whole steps, buses or boxes, so a key
//! such as `6/5` makes the whole-number answer look wrong. A ratio question,
//! a "times as" question and a "fraction of" question keep a fraction key.

use super::super::kp_view::{Item, KpView};
use super::finding;
use crate::output::Finding;

/// The words after "how many" that name a measure, not a countable thing.
const MEASURE_WORDS: &[&str] = &[
    "hours",
    "minutes",
    "seconds",
    "days",
    "weeks",
    "months",
    "years",
    "litres",
    "liters",
    "metres",
    "meters",
    "centimetres",
    "centimeters",
    "kilometres",
    "kilometers",
    "miles",
    "grams",
    "kilograms",
    "ounces",
    "pounds",
    "gallons",
    "inches",
    "feet",
    "yards",
    "degrees",
    "units",
    "percent",
    "dollars",
    "cents",
    "euros",
    "square",
    "cubic",
    "times",
    "radians",
    "radii",
    "thousand",
    "thousandths",
    "milligrams",
    "horizontal",
    "standard",
];

/// The words that fill the place between "how many" and the thing counted.
const FILLER_WORDS: [&str; 7] = [
    "more",
    "fewer",
    "less",
    "additional",
    "extra",
    "whole",
    "full",
];

/// The phrases that make a fraction key right.
const RATIO_PHRASES: [&str; 10] = [
    "exactly",
    "times as",
    "fraction of",
    "ratio",
    "as many",
    "as much",
    "how many times",
    "what fraction",
    "mean number",
    "average",
];

/// The noun after the first "how many" of the problem, if it is countable.
fn countable_noun(problem: &str) -> Option<String> {
    let lower = problem.to_lowercase();
    let (_, rest) = lower.split_once("how many ")?;
    let mut words = rest
        .split(|c: char| !c.is_alphabetic())
        .filter(|word| !word.is_empty())
        .skip_while(|word| FILLER_WORDS.contains(word));
    let word = words.next()?;
    // "cups of flour" names an amount of a substance, not a count of things.
    let is_amount = words.next() == Some("of");
    (word.len() > 2 && !is_amount && !MEASURE_WORDS.contains(&word)).then(|| word.to_owned())
}

/// The key is a number with a fraction part: `6/5`, `1.2`, `2 1/3`.
fn non_integer_key(key: &str) -> bool {
    let key = key.trim();
    let number: String = key
        .chars()
        .take_while(|c| c.is_ascii_digit() || matches!(c, '/' | '.' | '-' | ' '))
        .collect();
    let number = number.trim();
    if number.is_empty() || number.len() + 3 < key.len() {
        return false;
    }
    if let Some((top, bottom)) = number.rsplit_once('/') {
        let top = top.rsplit(' ').next().unwrap_or("");
        let (Ok(top), Ok(bottom)) = (top.parse::<u64>(), bottom.trim().parse::<u64>()) else {
            return false;
        };
        return bottom > 1 && !top.is_multiple_of(bottom) || number.contains(' ') && bottom > 1;
    }
    number
        .split_once('.')
        .is_some_and(|(_, tail)| tail.chars().any(|c| c.is_ascii_digit() && c != '0'))
}

/// D34 for one item: the text of the finding, if the item breaks the rule.
pub fn breach(item: &Item) -> Option<String> {
    if !item.is_typed() {
        return None;
    }
    let lower = item.exemplar.problem.to_lowercase();
    if RATIO_PHRASES.iter().any(|phrase| lower.contains(phrase)) {
        return None;
    }
    let noun = countable_noun(&item.exemplar.problem)?;
    non_integer_key(&item.exemplar.answer).then(|| {
        format!(
            "D34: the problem asks how many {noun} but the key `{}` is not a whole number; ask for a ratio or key the whole count",
            item.exemplar.answer
        )
    })
}

/// The findings of D34 for each item of the KP.
pub fn check(view: &KpView) -> Vec<Finding> {
    view.items
        .iter()
        .filter_map(|item| {
            let text = breach(item)?;
            Some(finding(view, "answer-format", "D34", Some(item), text))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::testkit::{item, view};
    use super::*;

    fn found(problem: &str, key: &str) -> bool {
        let one = item(problem, key, json!({"kind": "exact"}), None);
        !check(&view(vec![one])).is_empty()
    }

    #[test]
    fn a_how_many_question_with_a_fraction_key_is_a_finding() {
        let steps = "One step is $\\frac{5}{8}$ m long. How many steps does it take to cover $\\frac{3}{4}$ m?";
        assert!(found(steps, "6/5"));
        assert!(found(
            "How many buses fill $\\frac{7}{2}$ of a lot?",
            "1.75"
        ));
    }

    #[test]
    fn a_whole_key_a_measure_or_a_ratio_wording_is_not_a_finding() {
        let steps = "One step is $\\frac{5}{8}$ m long. How many steps fit in $\\frac{5}{2}$ m?";
        assert!(!found(steps, "4"));
        assert!(!found(
            "A recipe uses some flour. How many cups of flour is that?",
            "23/4"
        ));
        assert!(!found("A pump runs. How many hours does it run?", "3/2"));
        assert!(!found(
            "How many times as long is $\\frac{3}{4}$ m as $\\frac{5}{8}$ m?",
            "6/5"
        ));
        assert!(!found("How many boxes are there exactly?", "5/2"));
        assert!(!found(
            "A jug holds a litre. What fraction of the cup does it fill?",
            "5/12"
        ));
    }
}
