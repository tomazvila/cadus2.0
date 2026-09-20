//! The decision "does this authored answer get a key check".
//!
//! The rule is [`Exemplar::verdict_policy`] of `cadus-core`, so this tool,
//! readiness, the serve pool and the lint agree on each item.

use cadus_core::answer::AnswerContract;
use cadus_core::curriculum::{AnswerKind, Exemplar};

/// What the tool does with one authored answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyPolicy {
    /// The item gives a verdict: run the grade rungs.
    Verdict,
    /// The item has no deterministic checker: teach-only content, no failure.
    TeachOnly,
    /// The item has a checker, but the authored key does not validate.
    BadKey(String),
}

/// Decide the policy of one authored answer from the one verdict rule.
pub fn key_policy(answer: &str, contract: Option<&AnswerContract>, kind: AnswerKind) -> KeyPolicy {
    let item = Exemplar {
        problem: String::new(),
        answer_contract: contract.cloned(),
        answer: answer.to_owned(),
        solution_sketch: None,
    };
    match item.verdict_policy(kind) {
        Ok(_) => KeyPolicy::Verdict,
        Err(reason) if has_checker(contract, kind) => KeyPolicy::BadKey(reason.to_string()),
        Err(_) => KeyPolicy::TeachOnly,
    }
}

/// Whether a deterministic checker exists for this contract and topic kind.
pub fn has_checker(contract: Option<&AnswerContract>, kind: AnswerKind) -> bool {
    match contract {
        Some(contract) => contract != &AnswerContract::None,
        None => matches!(kind, AnswerKind::Numeric | AnswerKind::Expression),
    }
}

/// The policy name that a failure line gives.
pub fn policy_of(contract: Option<&AnswerContract>, kind: AnswerKind) -> String {
    match contract {
        Some(_) => "its answer contract".to_owned(),
        None => format!("kind '{kind}'"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KINDS: [AnswerKind; 4] = [
        AnswerKind::Numeric,
        AnswerKind::Expression,
        AnswerKind::MultiStep,
        AnswerKind::Proof,
    ];

    /// The decision agrees with `verdict_policy` for each row of the table.
    #[test]
    fn the_decision_agrees_with_the_verdict_policy() {
        let contracts = [
            Some(AnswerContract::Exact),
            Some(AnswerContract::None),
            None,
        ];
        for contract in &contracts {
            for kind in KINDS {
                for answer in ["5", "many words"] {
                    let item = Exemplar {
                        problem: "Give the value.".to_owned(),
                        answer_contract: contract.clone(),
                        answer: answer.to_owned(),
                        solution_sketch: None,
                    };
                    let verdict = item.verdict_policy(kind).is_ok();
                    let policy = key_policy(answer, contract.as_ref(), kind);
                    assert_eq!(
                        policy == KeyPolicy::Verdict,
                        verdict,
                        "{contract:?} {kind} {answer}"
                    );
                }
            }
        }
    }

    #[test]
    fn an_item_with_no_checker_is_teach_only() {
        let none = Some(AnswerContract::None);
        for kind in KINDS {
            assert_eq!(key_policy("5", none.as_ref(), kind), KeyPolicy::TeachOnly);
        }
        assert_eq!(
            key_policy("5", None, AnswerKind::Proof),
            KeyPolicy::TeachOnly
        );
        assert_eq!(
            key_policy("prose", None, AnswerKind::MultiStep),
            KeyPolicy::TeachOnly
        );
    }

    /// The policy name of one decision, with no reason text.
    fn name(policy: &KeyPolicy) -> &'static str {
        match policy {
            KeyPolicy::Verdict => "verdict",
            KeyPolicy::TeachOnly => "teach-only",
            KeyPolicy::BadKey(_) => "bad-key",
        }
    }

    #[test]
    fn a_key_that_does_not_validate_is_a_bad_key() {
        let exact = Some(AnswerContract::Exact);
        for kind in KINDS {
            let policy = key_policy("many words", exact.as_ref(), kind);
            assert_eq!(name(&policy), "bad-key");
        }
        let policy = key_policy("many words", None, AnswerKind::Numeric);
        assert_eq!(name(&policy), "bad-key");
        assert_eq!(name(&key_policy("5", None, AnswerKind::Numeric)), "verdict");
        assert_eq!(
            name(&key_policy("5", None, AnswerKind::Proof)),
            "teach-only"
        );
    }

    #[test]
    fn the_policy_name_gives_the_contract_or_the_kind() {
        assert_eq!(
            policy_of(Some(&AnswerContract::Exact), AnswerKind::Numeric),
            "its answer contract"
        );
        assert_eq!(policy_of(None, AnswerKind::Proof), "kind 'proof'");
    }
}
