//! A label answer written as a sentence: `Yes, it must be connected`,
//! `No, Sam is wrong`, `it doesn't exist`.
//!
//! The sentence names one option when exactly one option has an alias in it, as
//! whole words. An alias inside a longer alias (`no` inside `no solution`) does
//! not count. The alias must open the sentence and end at a mark (`No, it never
//! reaches 25 m`), or every other word must be a small function word (`it
//! doesn't exist`): a noun beside the alias (`right triangle`) is another answer.

/// The most words of a sentence that this reading takes.
const MAX_WORDS: usize = 14;

/// Aliases that are too short or too common to name an option inside a sentence.
const STOP_WORDS: &[&str] = &[
    "a", "an", "the", "it", "is", "are", "of", "to", "in", "i", "be",
];

/// Words of a reason or a doubt. A sentence that holds one is not a plain answer.
const HEDGES: &[&str] = &[
    "because", "since", "but", "however", "if", "unless", "although", "maybe", "perhaps",
    "probably", "guess", "think", "looks", "seems",
];

/// The words that may stand around an alias in a plain sentence: pronouns,
/// auxiliaries, and small adverbs. A noun beside the alias (`right triangle`)
/// makes the text another answer.
const FUNCTION_WORDS: &[&str] = &[
    "it",
    "is",
    "are",
    "was",
    "the",
    "a",
    "an",
    "there",
    "this",
    "that",
    "they",
    "he",
    "she",
    "we",
    "i",
    "you",
    "does",
    "do",
    "did",
    "can",
    "will",
    "would",
    "must",
    "be",
    "been",
    "has",
    "have",
    "here",
    "so",
    "also",
    "really",
    "definitely",
    "certainly",
    "just",
    "then",
    "of",
    "to",
    "its",
];

/// The words of a sentence in spoken form: lower case, punctuation removed, and
/// `n't` read as `not` (`doesn't` is `does not`, `can't` is `can not`).
fn words(text: &str) -> Vec<String> {
    let lower = text.to_lowercase().replace(['’', '‘'], "'");
    let mut out = Vec::new();
    for raw in lower.split_whitespace() {
        let word: String = raw
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '\'' || *c == '-')
            .collect();
        let word = word.trim_matches(['\'', '-']).to_owned();
        if word.is_empty() {
            continue;
        }
        if let Some(stem) = word.strip_suffix("n't") {
            let stem = match stem {
                "ca" => "can",
                "wo" => "will",
                "sha" => "shall",
                other => other,
            };
            out.push(stem.to_owned());
            out.push("not".to_owned());
        } else if word == "cannot" {
            out.push("can".to_owned());
            out.push("not".to_owned());
        } else {
            out.push(word);
        }
    }
    out
}

/// The index of the one option that the sentence names, or `None`.
pub(super) fn contained_choice(options: &[Vec<String>], text: &str) -> Option<usize> {
    // A formula or a labelled value is no sentence: `x = yes`.
    if text.contains(['=', '<', '>', '+', '*', '/', '^', '|']) {
        return None;
    }
    let sentence = words(text);
    if sentence.len() < 2
        || sentence.len() > MAX_WORDS
        || sentence.iter().any(|word| HEDGES.contains(&word.as_str()))
    {
        return None;
    }
    // Each match is (start, end, option); a match inside a longer one is dropped.
    let mut matches: Vec<(usize, usize, usize)> = Vec::new();
    for (option, aliases) in options.iter().enumerate() {
        for alias in aliases {
            let alias = words(alias);
            let plain = alias.join(" ");
            if alias.is_empty()
                || plain.chars().count() < 2
                || (alias.len() == 1 && STOP_WORDS.contains(&alias[0].as_str()))
            {
                continue;
            }
            for start in 0..sentence.len() {
                if start + alias.len() <= sentence.len()
                    && sentence[start..start + alias.len()] == alias[..]
                {
                    matches.push((start, start + alias.len(), option));
                }
            }
        }
    }
    let kept: Vec<(usize, usize, usize)> = matches
        .iter()
        .filter(|(start, end, _)| {
            !matches.iter().any(|(other_start, other_end, _)| {
                other_start <= start
                    && end <= other_end
                    && (other_end - other_start) > (end - start)
            })
        })
        .copied()
        .collect();
    let option = kept.first()?.2;
    if kept.iter().any(|(_, _, other)| *other != option) {
        return None;
    }
    let (start, end, _) = kept[0];
    // The alias opens the sentence and a mark ends it: `No, it never reaches 25 m`.
    let head = text.split([',', '.', ';', ':', '!']).next().unwrap_or("");
    let opens = start == 0
        && text.trim_start().len() > head.trim_start().len()
        && words(head) == sentence[..end];
    // Or every other word is a small function word: `it doesn't exist`.
    let plain = sentence
        .iter()
        .enumerate()
        .all(|(at, word)| (start..end).contains(&at) || FUNCTION_WORDS.contains(&word.as_str()));
    (opens || plain).then_some(option)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> Vec<Vec<String>> {
        vec![
            vec!["yes".into(), "it is".into(), "must be connected".into()],
            vec!["no".into(), "does not exist".into(), "no solution".into()],
        ]
    }

    #[test]
    fn a_sentence_with_one_alias_names_its_option() {
        for (text, option) in [
            ("Yes, it must be connected", 0),
            ("Yes, it is.", 0),
            ("yes, definitely", 0),
            ("No, Sam is wrong", 1),
            ("no, it never reaches 25 m", 1),
            ("it doesn't exist", 1),
            ("It does not exist.", 1),
            ("There is no solution here", 1),
        ] {
            assert_eq!(contained_choice(&options(), text), Some(option), "{text}");
        }
    }

    #[test]
    fn a_sentence_with_two_options_or_a_late_negation_names_nothing() {
        for text in [
            "yes and no",
            "right triangle",
            "yes sir",
            "I think yes but maybe no",
            "yes because it looks right",
            "it is not connected",
            "maybe",
            "this sentence is far too long to be an answer of a label question at all, really",
        ] {
            assert_eq!(contained_choice(&options(), text), None, "{text}");
        }
    }
}
