//! Reject an explanation that calls a method wrong and then names the same method as right.
//!
//! "You divided 3/4 by 5/8 incorrectly. The correct method is to divide 3/4 by 5/8" says
//! nothing. The check reads the operation and its operands from both halves. When they are
//! equal the explanation is circular, and the caller replaces it with the neutral sentence.

/// The words that carry no operation or operand.
const FILLER: [&str; 9] = ["the", "a", "an", "you", "your", "is", "to", "of", "then"];

/// The markers that end a clause naming the wrong method.
const WRONG_MARKERS: [&str; 3] = [" incorrectly", " wrongly", " the wrong way"];

/// The markers that start a clause naming the right method.
const RIGHT_MARKERS: [&str; 4] = [
    "correct method is to ",
    "correct approach is to ",
    "right method is to ",
    "correct way is to ",
];

/// True when the prose names one method as wrong and the same method as right.
#[must_use]
pub fn is_circular(prose: &str) -> bool {
    let flat = flatten(prose);
    let sentences: Vec<&str> = flat
        .split(['.', '!', '?', '\n'])
        .map(str::trim)
        .filter(|sentence| !sentence.is_empty())
        .collect();
    let wrong: Vec<Vec<String>> = sentences.iter().filter_map(|s| wrong_clause(s)).collect();
    let right: Vec<Vec<String>> = sentences.iter().filter_map(|s| right_clause(s)).collect();
    wrong
        .iter()
        .any(|w| !w.is_empty() && right.iter().any(|r| r == w))
}

/// The tokens of the method a sentence calls wrong, sorted.
fn wrong_clause(sentence: &str) -> Option<Vec<String>> {
    let marker = WRONG_MARKERS
        .iter()
        .filter_map(|marker| sentence.find(marker))
        .min()?;
    let before = &sentence[..marker];
    let clause = before.rfind("you ").map_or(before, |at| &before[at + 4..]);
    Some(tokens(clause))
}

/// The tokens of the method a sentence calls correct, sorted.
fn right_clause(sentence: &str) -> Option<Vec<String>> {
    let (marker, at) = RIGHT_MARKERS
        .iter()
        .find_map(|marker| sentence.find(marker).map(|at| (marker, at)))?;
    let clause = &sentence[at + marker.len()..];
    let end = clause.find([',', ';']).unwrap_or(clause.len());
    Some(tokens(&clause[..end]))
}

/// Lowercase text with each `\frac{a}{b}` written `a/b` and the math marks removed.
fn flatten(prose: &str) -> String {
    let mut text = prose.to_lowercase();
    for command in ["\\dfrac{", "\\frac{"] {
        while let Some(at) = text.find(command) {
            let body = &text[at + command.len()..];
            let Some((top, rest)) = body.split_once("}{") else {
                break;
            };
            let Some((bottom, tail)) = rest.split_once('}') else {
                break;
            };
            text = format!("{} {top}/{bottom} {tail}", &text[..at]);
        }
    }
    text.replace(['$', '\\'], " ")
}

/// The sorted stems of the words and numbers of a clause, without filler.
fn tokens(clause: &str) -> Vec<String> {
    let mut stems: Vec<String> = clause
        .split(|c: char| !(c.is_alphanumeric() || c == '/' || c == '.'))
        .map(|word| word.trim_matches('.'))
        .filter(|word| !word.is_empty() && !FILLER.contains(word))
        .map(stem)
        .collect();
    stems.sort();
    stems
}

/// A crude stem, so "divided", "divide" and "divides" are one word.
fn stem(word: &str) -> String {
    if !word.chars().all(char::is_alphabetic) || word.len() < 4 {
        return word.to_owned();
    }
    let mut stem = word;
    for suffix in ["ing", "ed", "es", "s"] {
        if let Some(cut) = stem.strip_suffix(suffix) {
            stem = cut;
            break;
        }
    }
    let stem = stem.strip_suffix('e').unwrap_or(stem);
    stem.strip_suffix('y')
        .map_or_else(|| stem.to_owned(), |base| format!("{base}i"))
}

#[cfg(test)]
mod tests {
    use super::is_circular;

    /// The wording of the production fault is circular.
    #[test]
    fn the_correct_method_that_repeats_the_wrong_one_is_circular() {
        assert!(is_circular(
            "You divided $\\frac{3}{4}$ by $\\frac{5}{8}$ incorrectly. The correct method is to \
             divide $\\frac{3}{4}$ by $\\frac{5}{8}$."
        ));
        assert!(is_circular(
            "You multiplied 3/4 by 5/8 incorrectly. The correct approach is to multiply 5/8 by 3/4."
        ));
    }

    /// A different operation or different operands is a real correction.
    #[test]
    fn a_different_method_is_not_circular() {
        assert!(!is_circular(
            "You multiplied 3/4 by 5/8 incorrectly. The correct method is to multiply 3/4 by 8/5."
        ));
        assert!(!is_circular(
            "You rounded 1.2 up to 2. The correct method is to leave the quotient as 6/5."
        ));
        assert!(!is_circular("Watch the sign."));
    }
}
