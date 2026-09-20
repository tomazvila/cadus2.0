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
/// A key with no numeric leaf gets the mutant `(<key>) + 1` (pack v9, D40).
fn probe_implicit(key: &str, kind: AnswerKind) -> Option<String> {
    let own = word(&answer::check(key, key, kind));
    if own != "correct" {
        return Some(format!("the key grades {own}"));
    }
    let mutant = mutate_plus_one(key).unwrap_or_else(|| format!("({key}) + 1"));
    let verdict = word(&answer::check(key, &mutant, kind));
    breach_of(&mutant, "plus-one", verdict)
}

/// The breach text of one mutant: each verdict that is not `wrong`.
fn breach_of(learner: &str, rule: &str, verdict: &str) -> Option<String> {
    (verdict != "wrong").then(|| format!("the mutant `{learner}` ({rule}) grades {verdict}"))
}

/// An exemplar with a contract: the `mutants` document of lane B5a decides.
/// `pass` is the authority (pack v9, D38 to D40): a part with no mutant and an
/// empty mutant list fail the item. Only `pass == true` is a pass.
fn probe_contract(key: &str, contract: &Value) -> Option<String> {
    let args = ["--contract", &contract.to_string(), "--expected", key].map(str::to_owned);
    let reply = crate::mutants::run(&args);
    let doc = &reply.docs[0];
    if let Some(error) = doc["error"].as_str() {
        return Some(error.to_owned());
    }
    let key_verdict = doc["key_verdict"].as_str().unwrap_or("-");
    if key_verdict != "correct" {
        return Some(format!("the key grades {key_verdict}"));
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
    let no_pass = (doc["pass"] != true).then(|| {
        format!(
            "`mutants` gives pass false (parts with no mutant: {}; cause: {})",
            doc["no_mutant_parts"], doc["cause"]
        )
    });
    breach.or(no_pass)
}

/// The I3 breach text of a verdict exemplar.
pub fn probe(exemplar: &Exemplar, contract: &Value, kind: AnswerKind) -> Option<String> {
    if contract.is_null() {
        probe_implicit(&exemplar.answer, kind)
    } else {
        probe_contract(&exemplar.answer, contract)
    }
}

/// The findings of I3.
pub fn check(view: &KpView) -> Vec<Finding> {
    view.items
        .iter()
        .filter_map(|item| {
            let detail = item.grader.clone()?;
            Some(finding(view, "grader-self", "I3", Some(item), detail))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::testkit::{exact, invariants, item, view};
    use super::{AnswerKind, breach_of, check, probe_contract, probe_implicit};
    use serde_json::json;

    #[test]
    fn a_key_with_a_wrong_mutant_has_no_breach() {
        assert_eq!(probe_implicit("3/4", AnswerKind::Numeric), None);
        assert_eq!(probe_implicit("pi", AnswerKind::Numeric), None);
        assert_eq!(probe_contract("3/4", &json!({"kind": "exact"})), None);
        assert_eq!(check(&view(vec![exact(1)])), []);
    }

    #[test]
    fn a_key_that_does_not_grade_correct_is_a_breach() {
        // No deterministic checker exists for the kind `multi-step`.
        let breach = probe_implicit("3", AnswerKind::MultiStep);
        assert_eq!(breach.as_deref(), Some("the key grades ungraded"));
        let breach = probe_contract("three", &json!({"kind": "exact"}));
        assert_eq!(breach.as_deref(), Some("the key grades ungraded"));
    }

    #[test]
    fn a_contract_that_does_not_deserialize_is_a_breach() {
        let breach = probe_contract("3", &json!({"kind": "no-such-kind"}));
        assert!(breach.is_some_and(|text| text.contains("does not deserialize")));
    }

    #[test]
    fn a_mutant_verdict_that_is_not_wrong_is_a_breach() {
        assert_eq!(breach_of("6", "plus-one", "wrong"), None);
        assert_eq!(
            breach_of("6", "plus-one", "correct").as_deref(),
            Some("the mutant `6` (plus-one) grades correct")
        );
        // Each candidate of the set `{2, 2}` is equal to the key.
        let breach = probe_contract("{2, 2}", &json!({"kind": "set"}));
        assert!(breach.is_some_and(|text| text.ends_with("(member-removed) grades correct")));
        let ungraded = breach_of("x", "label-other", "ungraded");
        assert!(ungraded.is_some_and(|text| text.ends_with("grades ungraded")));
    }

    #[test]
    fn only_pass_true_is_a_pass() {
        // A label with one option has no other option: `mutants` gives pass false.
        let one = json!({"kind": "label", "options": [["a"]]});
        let breach = probe_contract("a", &one);
        assert!(breach.is_some_and(|text| text.contains("cause: \"no-other-option\"")));
        // A label part with one option has no mutant (D38).
        let parts = json!([{"name": "v", "contract": {"kind": "label", "options": [["yes"]]}},
            {"name": "L", "contract": {"kind": "exact"}}]);
        let contract = json!({"kind": "multipart", "parts": parts});
        let breach = probe_contract("v = yes; L = 2", &contract);
        assert!(breach.is_some_and(|text| text.contains("[\"v\"]")));
        let found = check(&view(vec![item("Pick.", "a", one, None)]));
        assert_eq!(invariants(&found), ["I3"]);
        assert_eq!(
            (found[0].code.as_str(), found[0].ck.as_str()),
            ("grader-self", "CK6")
        );
    }
}
