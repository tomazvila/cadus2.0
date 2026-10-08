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

/// The words that start a second thought after an answer word.
const TURNS: &[&str] = &[
    "but", "or", "and", "however", "although", "though", "maybe", "perhaps", "probably", "unless",
    "if",
];

/// What the opening alias of a text says.
enum Lead {
    /// The text opens with the alias of one option and the rest adds nothing.
    Choice(usize),
    /// The text opens with an alias, but the rest names another option, joins
    /// another name by an operator, or ends in a bare `not`.
    Blocked,
    /// The text opens with no alias, or starts a turn.
    Nothing,
    /// The rest holds a negation that contradicts the opening word:
    /// `yes, it is not abelian`.
    Clash,
}

/// The words that negate a clause.
const NEGATIONS: &[&str] = &["not", "never", "neither", "nor"];

/// Aliases that affirm a claim. A text that opens with one of them
/// and then denies the claim contradicts itself.
const VERDICT_WORDS: &[&str] = &["yes", "true"];

/// The words that join two group names: `Z_2 x Z_2`, `Z_2 + Z_2`.
const OPERATOR_WORDS: &[&str] = &["x", "times", "plus", "cross"];

/// Auxiliary verbs. A `not` after one of them negates the clause; it does not
/// start a contrast (`yes, it is not` against `S_3 not A_3`).
const AUXILIARIES: &[&str] = &[
    "is", "are", "was", "does", "do", "did", "can", "will", "would", "must", "has", "have", "be",
    "been", "it", "that", "this", "there",
];

/// Whether the words of `alias` are the whole word list `words`.
fn is_alias(options: &[Vec<String>], word: &str) -> bool {
    options
        .iter()
        .flatten()
        .any(|alias| words(alias) == [word.to_owned()])
}

/// A word that looks like a group or object name: a letter then digits (`z2`, `s3`).
fn name_like(word: &str) -> bool {
    let mut chars = word.chars();
    chars.next().is_some_and(char::is_alphabetic)
        && word.chars().count() >= 2
        && chars.all(|c| c.is_ascii_digit())
}

/// Whether an operator joins the opening name to another name: `Z_2 x Z_2`,
/// `Z_2 + Z_2`, `Z_2 times Z_2`. The product of two groups is another group.
fn operator_joins(options: &[Vec<String>], text: &str) -> bool {
    let mut spaced = String::new();
    for c in text.to_lowercase().chars() {
        if matches!(c, '+' | '×' | '*' | '⋅' | '·' | '∙') {
            spaced.push(' ');
            spaced.push('+');
            spaced.push(' ');
        } else {
            spaced.push(c);
        }
    }
    let tokens: Vec<String> = spaced
        .split_whitespace()
        .map(|token| {
            token
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '+')
                .collect::<String>()
        })
        .filter(|token| !token.is_empty())
        .collect();
    tokens.windows(2).skip(1).any(|pair| {
        (pair[0] == "+" || OPERATOR_WORDS.contains(&pair[0].as_str()))
            && (name_like(&pair[1]) || is_alias(options, &pair[1]))
    })
}

fn lead(options: &[Vec<String>], text: &str) -> Lead {
    let sentence = words(text);
    if sentence.len() < 2 || sentence.len() > 40 {
        return Lead::Nothing;
    }
    let mut best: Option<(usize, usize)> = None;
    let mut tied = false;
    for (option, aliases) in options.iter().enumerate() {
        for alias in aliases {
            let alias = words(alias);
            if alias.is_empty()
                || (alias.len() == 1 && STOP_WORDS.contains(&alias[0].as_str()))
                || alias.join(" ").chars().count() < 2
                || sentence.len() <= alias.len()
                || sentence[..alias.len()] != alias[..]
            {
                continue;
            }
            match best {
                Some((_, length)) if length > alias.len() => {}
                Some((other, length)) if length == alias.len() && other != option => tied = true,
                _ => {
                    if best.is_none_or(|(_, length)| alias.len() > length) {
                        tied = false;
                    }
                    best = Some((option, alias.len()));
                }
            }
        }
    }
    let Some((option, length)) = best else {
        return Lead::Nothing;
    };
    if tied || TURNS.contains(&sentence[length].as_str()) {
        return Lead::Nothing;
    }
    // The alias ends where a word ends: `yes,` and `yes` count, `yesterday` does not.
    let rest = &sentence[length..];
    // The text may start a longer alias and then change it: `the empty function into
    // {0,2}` starts the alias `the empty function into {0,1}`. That is a different
    // object, not an explanation, so it names no option (rule 2 of grader pass 4).
    let diverges = options
        .iter()
        .flatten()
        .map(|alias| words(alias))
        .any(|alias| {
            alias.len() > length + 1
                && alias[..length] == sentence[..length]
                && alias[length] == sentence[length]
                && sentence[..alias.len().min(sentence.len())] != alias[..]
        });
    if diverges {
        return Lead::Nothing;
    }
    if operator_joins(options, text) {
        return Lead::Blocked;
    }
    // An option that is itself a negation (`neither`) may go on with `nor`.
    let own_negation = options[option].iter().any(|alias| {
        words(alias)
            .iter()
            .any(|word| NEGATIONS.contains(&word.as_str()))
    });
    if own_negation {
        return Lead::Choice(option);
    }
    // A verdict word (`yes`) that the rest denies contradicts itself. A negation
    // after a reason (`yes, because it does not matter`) explains the verdict.
    // `no` is a negation only when it is no alias of any option.
    let verdict_word = options[option]
        .iter()
        .any(|alias| VERDICT_WORDS.contains(&alias.trim().to_lowercase().as_str()));
    let reason = rest
        .iter()
        .position(|word| matches!(word.as_str(), "because" | "since" | "as" | "for"))
        .unwrap_or(rest.len());
    if verdict_word
        && let Some(at) = rest.iter().take(reason).position(|word| {
            NEGATIONS.contains(&word.as_str()) || (word == "no" && !is_alias(options, "no"))
        })
    {
        return if at + 1 == rest.len() {
            Lead::Blocked
        } else {
            Lead::Clash
        };
    }
    let own_last: Vec<String> = options[option]
        .iter()
        .flat_map(|alias| words(alias))
        .collect();
    let other_alias = options
        .iter()
        .enumerate()
        .filter(|(at, _)| *at != option)
        .flat_map(|(_, aliases)| aliases)
        .map(|alias| words(alias))
        .filter(|alias| {
            !alias.is_empty() && !(alias.len() == 1 && STOP_WORDS.contains(&alias[0].as_str()))
        })
        .any(|alias| {
            rest.windows(alias.len()).any(|window| window == &alias[..])
                // The head noun of a longer name: `cone, cylinder` names `circular cylinder`.
                || (alias.len() > 1
                    && alias.last().is_some_and(|last| {
                        last.chars().count() > 3
                            && !own_last.contains(last)
                            && rest.contains(last)
                    }))
        });
    if other_alias {
        Lead::Blocked
    } else {
        Lead::Choice(option)
    }
}

/// The option that opens the text, when the rest is an explanation.
///
/// `yes, because the group is abelian`, `no it does not factor`, and `Step 3 is
/// not valid` each open with one alias. The explanation after the alias may
/// hold any word except an alias of another option, a negation, an operator
/// that joins another name, and a turn such as `but`, so `yes or no`, `no, yes
/// it does`, `yes, it is not`, and `Z_2 x Z_2` name nothing.
pub(super) fn leading_choice(options: &[Vec<String>], text: &str) -> Option<usize> {
    match lead(options, text) {
        Lead::Choice(option) => Some(option),
        _ => None,
    }
}

/// Whether the text opens with an option word that the rest of the text
/// turns into another answer, so that no other reading may take it.
pub(super) fn opening_is_spoiled(options: &[Vec<String>], text: &str) -> bool {
    matches!(lead(options, text), Lead::Blocked | Lead::Clash)
}

/// Whether the text opens with an option word and then negates it:
/// `yes, it is not abelian`. The text is then ungraded, not wrong.
pub(super) fn contradicts_opening(options: &[Vec<String>], text: &str) -> bool {
    matches!(lead(options, text), Lead::Clash)
}

/// A text of the form `<answer>, not <other answer>`: the answer part and the
/// words after `not`. The word before `not` must be no auxiliary verb.
pub(super) fn split_contrast(text: &str) -> Option<(&str, &str)> {
    let lower = text.to_lowercase();
    if lower.len() != text.len() {
        return None;
    }
    let bytes = lower.as_bytes();
    let at = lower.match_indices("not").map(|(at, _)| at).find(|at| {
        let before = *at == 0 || !bytes[*at - 1].is_ascii_alphanumeric();
        let after = bytes.get(at + 3).is_none_or(|b| !b.is_ascii_alphanumeric());
        before && after
    })?;
    let head = text[..at].trim_end();
    let head = head.trim_end_matches([',', ';']).trim_end();
    let head = match head.rsplit_once(char::is_whitespace) {
        Some((rest, last)) if last.eq_ignore_ascii_case("and") => rest.trim_end(),
        _ => head,
    };
    let head = head.trim_end_matches([',', ';']).trim_end();
    let tail = text[at + 3..].trim();
    let last = words(head).pop()?;
    (!tail.is_empty() && !AUXILIARIES.contains(&last.as_str())).then_some((head, tail))
}

/// Whether the words after `not` name an option other than `option`, and
/// name `option` nowhere: `cone, not a cylinder`.
pub(super) fn contrast_names_other(
    options: &[Vec<String>],
    option: usize,
    tail: &str,
) -> Option<bool> {
    let tail = words(tail);
    if tail.iter().any(|word| NEGATIONS.contains(&word.as_str())) {
        return None;
    }
    let find = |aliases: &[String]| {
        aliases.iter().map(|alias| words(alias)).any(|alias| {
            !alias.is_empty()
                && !(alias.len() == 1 && STOP_WORDS.contains(&alias[0].as_str()))
                && tail.windows(alias.len()).any(|window| window == &alias[..])
        })
    };
    if find(&options[option]) {
        return Some(false);
    }
    let own: Vec<String> = options[option]
        .iter()
        .flat_map(|alias| words(alias))
        .collect();
    let other = options.iter().enumerate().any(|(at, aliases)| {
        at != option
            && (find(aliases)
                || aliases.iter().map(|alias| words(alias)).any(|alias| {
                    alias.len() > 1
                        && alias.last().is_some_and(|last| {
                            last.chars().count() > 3 && !own.contains(last) && tail.contains(last)
                        })
                }))
    });
    other.then_some(true)
}

/// The form of a choice text that ignores case, articles, spaces, underscores,
/// LaTeX wrappers, and the spelling of a product sign.
///
/// `Z_2×Z_2`, `Z2xZ2`, and `$Z_2 \\times Z_2$` share one form, and so do
/// `y = 2x`, `y=2x`, and `y = 2*x`. The answer holds two forms: one that drops
/// each `*`, and one that reads each `*` as the letter `x`.
fn loose_forms(text: &str) -> Vec<String> {
    let mut form = text.trim().to_lowercase().replace('$', "");
    for word in ["\\cdot", "\\times"] {
        form = form.replace(word, "*");
    }
    for word in [
        "\\mathbb",
        "\\mathbf",
        "\\mathrm",
        "\\text",
        "\\operatorname",
        "\\left",
        "\\right",
    ] {
        form = form.replace(word, "");
    }
    form = form
        .replace(['\\', '{', '}', '_'], "")
        .replace(['×', '⋅', '·', '∙'], "*");
    let form = form
        .trim()
        .trim_end_matches(['.', ',', ';', ':', '!', '?'])
        .trim();
    let mut body = form;
    for article in ["the ", "a ", "an "] {
        if let Some(rest) = body.strip_prefix(article) {
            body = rest;
        }
    }
    let compact: String = body.chars().filter(|c| !c.is_whitespace()).collect();
    vec![compact.replace('*', ""), compact.replace('*', "x")]
}

/// The one option whose alias has the same loose form as the text.
pub(super) fn loose_choice(options: &[Vec<String>], text: &str) -> Option<usize> {
    let forms = loose_forms(text);
    if forms.iter().all(|form| form.is_empty()) {
        return None;
    }
    let mut found = options.iter().enumerate().filter(|(_, aliases)| {
        aliases
            .iter()
            .any(|alias| loose_forms(alias).iter().any(|other| forms.contains(other)))
    });
    let (option, _) = found.next()?;
    found.next().is_none().then_some(option)
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
