//! Rule D35: an item asks its question directly. A problem or a sketch must
//! not attribute a claim, a computation or a mistake to a person, either a
//! first name ("Sam says") or a role ("a student writes"). A passage about
//! "the work below" has no person and is allowed. A name with a scene verb
//! ("Ana buys", "Tom walks") is not a finding.

use super::super::kp_view::{Item, KpView};
use super::finding;
use crate::output::Finding;

/// The verbs that attribute a claim, a computation or a mistake.
const CLAIM_VERBS: &[&str] = &[
    "says",
    "said",
    "claims",
    "thinks",
    "argues",
    "believes",
    "computes",
    "computed",
    "calculates",
    "calculated",
    "writes",
    "wrote",
    "gets",
    "got",
    "concludes",
    "reasons",
    "insists",
    "states",
    "proposes",
    "decides",
    "solves",
    "finds",
    "simplifies",
    "evaluates",
    "estimates",
    "converts",
];

/// The role nouns that stand for a person.
const ROLE_NOUNS: &[&str] = &[
    "student",
    "friend",
    "clerk",
    "classmate",
    "teacher",
    "tutor",
    "colleague",
    "coworker",
    "cashier",
    "someone",
];

/// The first names that the tree uses for the people in its problems. Names of
/// mathematicians (Euler, Gauss, Sylow, Bayes) and the key holders of RSA items
/// (Alice, Bob, Eve) are not in the list, so they never match.
const NAMES: &[&str] = &[
    "Sam", "He", "She", "Ana", "Mia", "Lena", "Tom", "Ben", "Ravi", "Omar", "Leo", "Maya", "Dana",
    "Priya", "Kim", "Kai", "Dev", "Zoe", "Eli", "Jo", "Pia", "Noah", "Dan", "Raj", "Noor", "Tomas",
    "Tia", "Tara", "Lee", "Jonas", "Ella", "Cy", "Bo", "Tess", "Rita", "Nora", "Nia", "Ivy",
    "Ines", "Dia", "Ali", "Alex", "Zara", "Tim", "Theo", "Tariq", "Rui", "Ria", "Rae", "Pat",
    "Ola", "Nina", "Mo", "Max", "Marko", "Maria", "Mara", "Lea", "Ken", "Karim", "Jon", "Jin",
    "Jana", "Ira", "Hana", "Eva", "Emma", "Ellie", "Elena", "Dee", "Carl", "Ava", "Anna", "Ade",
    "Nadia",
];

/// How many words after the person may come before the verb.
const WINDOW: usize = 6;

/// A word of the text: letters and apostrophes only, and whether a sentence
/// ends after it.
struct Word {
    text: String,
    ends_sentence: bool,
    /// A number or a math span comes right after the word.
    math_follows: bool,
}

fn words(text: &str) -> Vec<Word> {
    let mut list: Vec<Word> = Vec::new();
    for raw in text.split_whitespace() {
        let ends_sentence = raw.ends_with(['.', '?', '!', ':', ';']);
        let letters: String = raw
            .chars()
            .filter(|c| c.is_alphabetic() || *c == '\'' || *c == '’')
            .collect();
        let math = raw.starts_with(['$', '\\']) || raw.starts_with(|c: char| c.is_ascii_digit());
        if letters.is_empty() || math {
            if math && let Some(last) = list.last_mut() {
                last.math_follows = true;
            }
            continue;
        }
        list.push(Word {
            text: letters,
            ends_sentence,
            math_follows: false,
        });
    }
    list
}

/// The word is a person: a name from the list or a role noun.
fn is_person(word: &str) -> bool {
    NAMES.contains(&word) || ROLE_NOUNS.contains(&word.to_lowercase().as_str())
}

/// The word is a name from the list, alone or with the possessive ending.
fn is_name(word: &str) -> bool {
    NAMES.contains(&word)
}

/// The verbs `gets` and `got` and the verb `finds` also describe a scene
/// ("each friend gets 6 cookies", "Mia finds an item"). They count as a claim
/// only with a number, a math span or "the answer" after them. A role noun
/// after "each" or "every" is a scene, and so is any other object.
fn counts_as_claim(list: &[Word], person_at: usize, verb: &Word) -> bool {
    let verb_lower = verb.text.to_lowercase();
    let scene_verb = matches!(verb_lower.as_str(), "gets" | "got" | "finds");
    if !scene_verb {
        return true;
    }
    let distributed = person_at > 0
        && matches!(
            list[person_at - 1].text.to_lowercase().as_str(),
            "each" | "every" | "all" | "both"
        );
    if distributed && verb_lower != "finds" {
        return false;
    }
    let verb_at = list
        .iter()
        .skip(person_at)
        .position(|w| std::ptr::eq(w, verb))
        .map_or(person_at, |offset| offset + person_at);
    if list[verb_at].math_follows {
        return true;
    }
    let Some(after) = list.get(verb_at + 1) else {
        return false;
    };
    after.text.eq_ignore_ascii_case("the")
        && (verb_lower == "finds"
            || list
                .get(verb_at + 2)
                .is_some_and(|w| w.text.eq_ignore_ascii_case("answer")))
}

/// The text of the first attribution in the passage, if any.
fn attribution(text: &str) -> Option<String> {
    let list = words(text);
    for (at, word) in list.iter().enumerate() {
        let lower = word.text.to_lowercase();
        if is_person(&word.text) {
            let tail = list[at + 1..].iter().take(WINDOW);
            for next in tail {
                if CLAIM_VERBS.contains(&next.text.to_lowercase().as_str())
                    && counts_as_claim(&list, at, next)
                {
                    return Some(format!("{} ... {}", word.text, next.text));
                }
                if next.ends_sentence {
                    break;
                }
            }
            if word.ends_sentence {
                continue;
            }
        }
        let next = list.get(at + 1).map(|next| next.text.to_lowercase());
        let next = next.as_deref().unwrap_or("");
        if lower == "is"
            && matches!(next_after(&list, at, 2), Some(w) if matches!(w, "right" | "correct"))
            && list.get(at + 1).is_some_and(|w| is_person(&w.text))
        {
            return Some(format!("Is {} {}", list[at + 1].text, list[at + 2].text));
        }
        if lower == "what" && next == "mistake" && list.get(at + 2).is_some_and(|w| w.text == "did")
        {
            let who = list.get(at + 3).map(|w| w.text.as_str()).unwrap_or("");
            let who = if matches!(who, "a" | "the" | "your") {
                list.get(at + 4).map(|w| w.text.as_str()).unwrap_or("")
            } else {
                who
            };
            if is_person(who) {
                return Some(format!("What mistake did {who}"));
            }
        }
        let owner = word
            .text
            .strip_suffix("'s")
            .or(word.text.strip_suffix("’s"));
        if let Some(owner) = owner
            && is_name(owner)
            && matches!(next, "answer" | "work")
        {
            return Some(format!("{} {}", word.text, next));
        }
    }
    None
}

/// The lowercase word `back` places after `at`.
fn next_after(list: &[Word], at: usize, back: usize) -> Option<&str> {
    let word = list.get(at + back)?;
    match word.text.to_lowercase().as_str() {
        "right" => Some("right"),
        "correct" => Some("correct"),
        _ => None,
    }
}

/// D35 for one item: the text of the finding, if the item breaks the rule.
pub fn breach(item: &Item) -> Option<String> {
    let found = std::iter::once(item.exemplar.problem.as_str())
        .chain(item.exemplar.solution_sketch.as_deref())
        .find_map(attribution)?;
    Some(format!(
        "D35: the text attributes a claim, a computation or a mistake to a person (`{found}`); ask the question directly or refer to \"the work below\""
    ))
}

/// The findings of D35 for each item of the KP.
pub fn check(view: &KpView) -> Vec<Finding> {
    view.items
        .iter()
        .filter_map(|item| {
            let text = breach(item)?;
            Some(finding(view, "answer-format", "D35", Some(item), text))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::testkit::{item, view};
    use super::*;

    fn found(problem: &str, sketch: Option<&str>) -> bool {
        let one = item(problem, "5", json!({"kind": "exact"}), sketch);
        !check(&view(vec![one])).is_empty()
    }

    #[test]
    fn a_named_person_who_says_a_claim_is_a_finding() {
        assert!(found("Sam says $-15 > -3$. Which number is greater?", None));
    }

    #[test]
    fn a_role_noun_with_a_computation_verb_is_a_finding() {
        assert!(found("A student computes $2+3$ as $6$. Find $2+3$.", None));
        assert!(found(
            "Your friend wrote the sum on the board. Find it.",
            None
        ));
    }

    #[test]
    fn a_verb_within_six_words_of_the_person_is_a_finding() {
        assert!(found(
            "Ana adds two fractions and then gets $3/4$. Find the sum.",
            None
        ));
    }

    #[test]
    fn the_question_forms_about_a_person_are_findings() {
        assert!(found("Is Tom right that $0.5 > 0.7$?", None));
        assert!(found("What mistake did Mia make in the step?", None));
        assert!(found("Check Lena's answer of $12$ for the area.", None));
    }

    #[test]
    fn a_person_in_the_sketch_is_a_finding() {
        assert!(found(
            "Find $2+3$.",
            Some("Dana concludes that the sum is $5$.")
        ));
    }

    #[test]
    fn the_work_below_has_no_person() {
        assert!(!found(
            "The work below argues $-15 > -3$ from $15 > 3$. What is wrong with that step?",
            None
        ));
    }

    #[test]
    fn a_scene_verb_is_not_a_finding() {
        assert!(!found("Sam buys 3 pens at $2$ each. Find the cost.", None));
        assert!(!found(
            "Ana walks 4 km and cycles 6 km. Find the total.",
            None
        ));
    }

    #[test]
    fn a_mathematician_name_or_a_key_holder_is_not_a_finding() {
        assert!(!found(
            "Euler states a formula for $e^{i\\pi}$. Find it.",
            None
        ));
        assert!(!found("Alice sends Bob a ciphertext. Find the key.", None));
        assert!(!found("Fermat's work on primes. Is $7$ prime?", None));
    }

    #[test]
    fn a_verb_after_the_sentence_end_is_not_a_finding() {
        assert!(!found(
            "Tom walks home. The sum gets larger. Find it.",
            None
        ));
    }

    #[test]
    fn each_person_who_gets_a_share_is_a_scene() {
        assert!(!found(
            "Each student gets exactly one of $3$ different prizes. How many ways are there?",
            None
        ));
        assert!(!found(
            "Share equally, so each friend gets $6$ cookies.",
            None
        ));
    }

    #[test]
    fn gets_with_a_non_number_object_is_a_scene() {
        assert!(!found(
            "A friend gives you 3 apples and a student gets a key. Find the sum.",
            None
        ));
        assert!(!found(
            "Lena buys 3 shirts and gets €2 off each shirt. Find the price.",
            None
        ));
        assert!(!found("Bo gets half as much as Ana. Find the share.", None));
        assert!(found("Ana gets $3/4$ for the sum. Find it.", None));
        assert!(found("Ana got the answer $5$. Find it.", None));
    }

    #[test]
    fn finds_counts_only_with_a_math_object() {
        assert!(!found(
            "Mia finds an item that costs too much. Find the price.",
            None
        ));
        assert!(found("Ava finds the probability as $1/12$. Find it.", None));
        assert!(found("Ava finds $1/12$. Find it.", None));
    }

    #[test]
    fn a_direct_question_is_not_a_finding() {
        assert!(!found(
            "Which is greater, $-15$ or $-3$?",
            Some("Compare the two.")
        ));
    }
}
