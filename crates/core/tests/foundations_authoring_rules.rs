//! Mechanical authoring rules the loader cannot see: a step check whose step
//! fragment stops mid-phrase, and a verdict exemplar that contradicts its
//! knowledge point. Both slipped past review once (step 5b), so the whole tree
//! is read here.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_core::answer::{AnswerContract, Canon, canonical_form};
use cadus_core::curriculum::StepRef;
use common::paths::tree;

/// Words a step fragment cannot end on: the fragment would stop mid-phrase.
const DANGLING: [&str; 24] = [
    "a", "an", "the", "as", "of", "to", "and", "or", "by", "into", "with", "for", "from", "is",
    "are", "then", "so", "that", "in", "on", "at", "gives", "get", "becomes",
];

/// The verdict family of one label key: `Some(false)` for "no solution",
/// `Some(true)` for "every real number", `None` for anything else.
fn verdict_family(contract: &AnswerContract, key: &str) -> Option<bool> {
    let AnswerContract::Label { options } = contract else {
        return None;
    };
    let lower = key.trim().to_lowercase();
    let option = options.iter().find(|aliases| {
        aliases
            .iter()
            .any(|alias| alias.trim().to_lowercase() == lower)
    })?;
    let words: Vec<String> = option.iter().map(|alias| alias.to_lowercase()).collect();
    if words.iter().any(|alias| alias == "no solution") {
        Some(false)
    } else if words.iter().any(|alias| alias == "all real numbers") {
        Some(true)
    } else {
        None
    }
}

/// The classification of `Solve $lhs = rhs$.` when the variable cancels:
/// `Some(true)` for an identity, `Some(false)` for a contradiction, `None`
/// when the equation has one solution or does not read.
fn cancelled(problem: &str) -> Option<bool> {
    let body = problem.strip_prefix("Solve $")?.strip_suffix("$.")?;
    if body.contains('|') {
        return None;
    }
    let (lhs, rhs) = body.split_once('=')?;
    let difference = canonical_form(&format!("({lhs}) - ({rhs})")).ok()?;
    matches!(difference, Canon::Rational(_)).then(|| Ok(difference) == canonical_form("0"))
}

#[test]
fn every_step_check_names_a_whole_phrase_and_asks_a_question() {
    let graph = tree();
    let mut seen = 0;
    for topic in graph.topics() {
        for point in &topic.knowledge_points {
            let Some(check) = &point.step_check else {
                continue;
            };
            seen += 1;
            let key = format!("{}/{}", topic.id, point.id);
            if let StepRef::Text(fragment) = &check.step {
                let trimmed = fragment.trim();
                let last = trimmed
                    .rsplit(char::is_whitespace)
                    .next()
                    .unwrap_or("")
                    .to_lowercase();
                assert!(
                    !DANGLING.contains(&last.as_str()) && !trimmed.ends_with([',', ':', '(', '=']),
                    "{key}: the step fragment {fragment:?} stops mid-phrase"
                );
                assert_eq!(
                    trimmed.matches('$').count() % 2,
                    0,
                    "{key}: the step fragment {fragment:?} cuts a formula in half"
                );
            }
            assert!(
                check.question.trim_end().ends_with('?'),
                "{key}: the step check question does not end with a question mark"
            );
        }
    }
    assert!(seen >= 100, "only {seen} step checks were read");
}

#[test]
fn verdict_exemplars_agree_with_their_knowledge_point() {
    let graph = tree();
    let mut checked = 0;
    for topic in graph.topics() {
        for point in &topic.knowledge_points {
            let key = format!("{}/{}", topic.id, point.id);
            let rule = point.constraints.as_deref().unwrap_or("").to_lowercase();
            let wants = if rule.contains("false statement") || rule.contains("equals a negative") {
                Some(false)
            } else if rule.contains("true statement") {
                Some(true)
            } else {
                None
            };
            for exemplar in &point.exemplars {
                let Some(contract) = &exemplar.answer_contract else {
                    continue;
                };
                let family = verdict_family(contract, &exemplar.answer);
                if let (Some(wants), Some(family)) = (wants, family) {
                    checked += 1;
                    assert_eq!(
                        family, wants,
                        "{key}: {:?} has the key {:?}, against the constraint {rule:?}",
                        exemplar.problem, exemplar.answer
                    );
                }
                if let (Some(family), Some(identity)) = (family, cancelled(&exemplar.problem)) {
                    checked += 1;
                    assert_eq!(
                        family,
                        identity,
                        "{key}: {:?} has the key {:?}, but the equation is {}",
                        exemplar.problem,
                        exemplar.answer,
                        if identity {
                            "an identity"
                        } else {
                            "a contradiction"
                        }
                    );
                }
            }
        }
    }
    assert!(
        checked >= 6,
        "only {checked} verdict exemplars were checked"
    );
}
