//! Mechanical authoring rules the loader cannot see: a verdict exemplar that
//! contradicts its knowledge point. One slipped past review once (step 5b), so
//! the whole tree is read here.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_core::answer::{AnswerContract, Canon, canonical_form};
use common::paths::tree;

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
