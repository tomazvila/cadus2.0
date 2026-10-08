//! Keys that are a short phrase and not a value: "no solution", "does not exist".
//!
//! The `exact` contract reads a value. A phrase key names a case instead. The
//! learner answer is correct if it names the same case in the common words, and
//! wrong if it is a value or names another case.

use super::{Canon, canonical_form};
use crate::answer::Outcome;

/// The cases a phrase can name.
const NO_SOLUTION: &str = "phrase:no solution";
const DOES_NOT_EXIST: &str = "phrase:does not exist";
const ALL_REAL: &str = "phrase:all real numbers";

/// The case a phrase names, or `None` if the text is not a known phrase.
pub(super) fn class(text: &str) -> Option<&'static str> {
    let trimmed = text.trim();
    if matches!(trimmed, "∅" | "{}" | "{ }" | "\\emptyset" | "\\varnothing") {
        return Some(NO_SOLUTION);
    }
    let lowered = trimmed.to_lowercase();
    let squeezed: String = lowered.split_whitespace().collect::<Vec<_>>().join(" ");
    let squeezed = squeezed.trim_end_matches('.');
    match squeezed {
        "dne"
        | "does not exist"
        | "doesn't exist"
        | "the limit does not exist"
        | "limit does not exist"
        | "undefined" => return Some(DOES_NOT_EXIST),
        "all real numbers" | "all reals" | "every real number" | "any real number" => {
            return Some(ALL_REAL);
        }
        "none"
        | "empty set"
        | "the empty set"
        | "no answer"
        | "no solution"
        | "no solutions"
        | "no real solution"
        | "no real solutions"
        | "no real number solution"
        | "no roots"
        | "no real roots"
        | "there is no solution"
        | "there are no solutions"
        | "there is no real solution"
        | "there are no real solutions"
        | "no zeros"
        | "the equation has no solution"
        | "the equation has no real solution"
        | "the system has no solution"
        | "the inequality has no solution"
        | "the solution set is empty"
        | "solution set is empty"
        | "inconsistent"
        | "no solution exists"
        | "no real number satisfies it" => {
            return Some(NO_SOLUTION);
        }
        _ => {}
    }
    None
}

/// The expected value of a phrase key.
pub(super) fn expected(text: &str) -> Option<Canon> {
    class(text).map(|case| Canon::Label(case.to_owned()))
}

/// Grade a learner answer against a phrase key; `None` if the key is no phrase
/// or the learner answer is neither a phrase nor a value.
pub(super) fn grade(key: &str, learner: &str) -> Option<Outcome> {
    let case = class(key)?;
    let decided = |correct: bool| {
        Outcome::Decided(crate::answer::Verdict {
            correct,
            notation: false,
        })
    };
    match class(learner) {
        Some(other) => Some(decided(other == case)),
        None => canonical_form(learner).ok().map(|_| decided(false)),
    }
}

/// A quotient with a remainder in spoken words, as `q R r`:
/// "9 boxes and 3 left over", "quotient 9, remainder 3", "9 with 3 remaining".
/// `None` if the text is not such a sentence.
pub(super) fn spoken_quotient(learner: &str) -> Option<String> {
    let lowered = learner.trim().trim_end_matches('.').to_lowercase();
    let tokens: Vec<&str> = lowered
        .split(|ch: char| ch.is_whitespace() || ch == ',')
        .filter(|token| !token.is_empty())
        .collect();
    let numbers: Vec<(usize, &str)> = tokens
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, token)| token.chars().all(|ch| ch.is_ascii_digit()))
        .collect();
    let [(first, quotient), (second, remainder)] = numbers.as_slice() else {
        return None;
    };
    let words_only = tokens
        .iter()
        .enumerate()
        .filter(|(at, _)| at != first && at != second)
        .all(|(_, token)| token.chars().all(|ch| ch.is_ascii_alphabetic()));
    let lead = tokens[..*first].iter().all(|token| {
        matches!(
            *token,
            "quotient" | "is" | "the" | "answer" | "it's" | "its"
        )
    });
    let marker = tokens[*second + 1..].iter().any(|token| {
        matches!(
            *token,
            "left" | "remaining" | "remainder" | "over" | "leftover"
        )
    }) || tokens[*first + 1..*second]
        .iter()
        .any(|token| matches!(*token, "remainder" | "rem" | "r"));
    (words_only && lead && marker && *second > *first).then(|| format!("{quotient} R{remainder}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phrases_name_their_case() {
        assert_eq!(class("no solution"), Some(NO_SOLUTION));
        assert_eq!(class("None"), Some(NO_SOLUTION));
        assert_eq!(class("There is no solution."), Some(NO_SOLUTION));
        assert_eq!(class("∅"), Some(NO_SOLUTION));
        assert_eq!(class("DNE"), Some(DOES_NOT_EXIST));
        assert_eq!(class("x"), None);
        assert_eq!(class("5"), None);
    }
}
