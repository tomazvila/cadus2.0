//! I3: the key of each verdict exemplar grades CORRECT and each mutant of the
//! key grades WRONG. The mutants come from `mutants.rs` (lane B5a).

use cadus_core::answer::{self, Outcome};
use cadus_core::curriculum::{AnswerKind, Exemplar};
use serde_json::Value;

use super::super::kp_view::KpView;
use super::finding;
use crate::mutate::mutate_plus_one;
use crate::output::Finding;

fn word(outcome: &Outcome) -> &'static str {
    match outcome {
        Outcome::Decided(verdict) if verdict.correct => "correct",
        Outcome::Decided(_) => "wrong",
        Outcome::Undecidable(_) => "ungraded",
    }
}

/// An exemplar with no contract: the checker of the topic kind grades it.
fn probe_implicit(key: &str, kind: AnswerKind) -> (Option<String>, bool) {
    let own = word(&answer::check(key, key, kind));
    if own != "correct" {
        return (Some(format!("the key grades {own}")), false);
    }
    let Some(mutant) = mutate_plus_one(key) else {
        return (None, true);
    };
    let verdict = word(&answer::check(key, &mutant, kind));
    (breach_of(&mutant, "plus-one", verdict), false)
}

/// The breach text of one mutant: each verdict that is not `wrong`.
fn breach_of(learner: &str, rule: &str, verdict: &str) -> Option<String> {
    (verdict != "wrong").then(|| format!("the mutant `{learner}` ({rule}) grades {verdict}"))
}

/// An exemplar with a contract: the `mutants` document of lane B5a decides.
fn probe_contract(key: &str, contract: &Value) -> (Option<String>, bool) {
    let args = ["--contract", &contract.to_string(), "--expected", key].map(str::to_owned);
    let reply = crate::mutants::run(&args);
    let doc = &reply.docs[0];
    if let Some(error) = doc["error"].as_str() {
        return (Some(error.to_owned()), false);
    }
    let key_verdict = doc["key_verdict"].as_str().unwrap_or("-");
    if key_verdict != "correct" {
        return (Some(format!("the key grades {key_verdict}")), false);
    }
    let mutants = doc["mutants"].as_array().map_or(&[][..], Vec::as_slice);
    let text = |mutant: &Value, key: &str| mutant[key].as_str().unwrap_or("-").to_owned();
    let breach = mutants.iter().find_map(|mutant| {
        breach_of(
            &text(mutant, "learner"),
            &text(mutant, "rule"),
            &text(mutant, "verdict"),
        )
    });
    // `pass` is the authority (pack v9, D38): a part with no mutant fails the
    // item also when each listed mutant grades wrong.
    let incomplete = (doc["pass"] != true && !mutants.is_empty()).then(|| {
        format!(
            "`mutants` gives pass false (parts with no mutant: {}; cause: {})",
            doc["no_mutant_parts"], doc["cause"]
        )
    });
    (breach.or(incomplete), mutants.is_empty())
}

/// The I3 breach text of a verdict exemplar, and "the key has no mutant".
pub fn probe(exemplar: &Exemplar, contract: &Value, kind: AnswerKind) -> (Option<String>, bool) {
    if contract.is_null() {
        probe_implicit(&exemplar.answer, kind)
    } else {
        probe_contract(&exemplar.answer, contract)
    }
}

/// The findings of I3. A new item whose key has no mutant is also a finding:
/// the `pass` rule of `mutants` needs 1 or more mutants (CK6).
pub fn check(view: &KpView) -> Vec<Finding> {
    view.items
        .iter()
        .filter_map(|item| {
            let no_mutant = (item.is_new && item.no_mutant)
                .then(|| "the key has no mutant, thus the check has no power".to_owned());
            let detail = item.grader.clone().or(no_mutant)?;
            Some(finding(view, "grader-self", "I3", Some(item), detail))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::testkit::{exact, invariants, item, view};
    use super::*;

    #[test]
    fn a_key_with_a_wrong_mutant_has_no_breach() {
        assert_eq!(probe_implicit("3/4", AnswerKind::Numeric), (None, false));
        assert_eq!(
            probe_contract("3/4", &json!({"kind": "exact"})),
            (None, false)
        );
        assert_eq!(check(&view(vec![exact(1)])), []);
    }

    #[test]
    fn a_key_that_does_not_grade_correct_is_a_breach() {
        // No deterministic checker exists for the kind `multi-step`.
        let (breach, _) = probe_implicit("3", AnswerKind::MultiStep);
        assert_eq!(breach.as_deref(), Some("the key grades ungraded"));
        let (breach, _) = probe_contract("three", &json!({"kind": "exact"}));
        assert_eq!(breach.as_deref(), Some("the key grades ungraded"));
    }

    #[test]
    fn a_contract_that_does_not_deserialize_is_a_breach() {
        let (breach, _) = probe_contract("3", &json!({"kind": "no-such-kind"}));
        assert!(breach.is_some_and(|text| text.contains("does not deserialize")));
    }

    #[test]
    fn a_mutant_that_grades_correct_is_a_breach() {
        // The mutant `6` is in the tolerance of the key `5`.
        let contract = json!({"kind": "approx", "tolerance": "2"});
        let (breach, none) = probe_contract("5", &contract);
        assert_eq!(
            breach.as_deref(),
            Some("the mutant `6` (plus-one) grades correct")
        );
        assert!(!none);
    }

    #[test]
    fn a_key_with_no_mutant_is_a_finding_for_a_new_item_only() {
        assert_eq!(probe_implicit("pi", AnswerKind::Numeric), (None, true));
        let mut old = item("Find the constant.", "pi", json!({"kind": "exact"}), None);
        assert!(old.no_mutant);
        assert_eq!(check(&view(vec![old.clone()])), []);
        old.is_new = true;
        let found = check(&view(vec![old]));
        assert_eq!(invariants(&found), ["I3"]);
        assert_eq!(found[0].code, "grader-self");
        assert_eq!(found[0].ck, "CK6");
    }
}
