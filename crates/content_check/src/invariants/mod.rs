//! The invariants of content-spec 3.3 for one KP view, and the pack rules D27
//! and D28. Each rule gives findings in the frozen form of `output::Finding`.

pub mod countable;
pub mod counts;
pub mod duplicate;
pub mod file_rule;
pub mod format;
pub mod grader;
pub mod label;
pub mod r3;
pub mod words;

use super::kp_view::{Item, KpView};
use crate::output::Finding;

/// The check id of a finding code (`reason-codes.json`, block `check`).
fn ck_of(code: &str) -> &'static str {
    match code {
        "base-coverage" => "CK2",
        "contract-rule" => "CK4",
        "answer-format" => "CK5",
        "grader-self" => "CK6",
        "duplicate" => "CK7",
        "label-quality" => "CK8",
        "sketch" => "CK9",
        _ => "CK12",
    }
}

/// Build one finding of a KP. `item` gives the item id and the hash.
pub fn finding(
    view: &KpView,
    code: &str,
    invariant: &str,
    item: Option<&Item>,
    detail: String,
) -> Finding {
    Finding {
        ck: ck_of(code).to_owned(),
        code: code.to_owned(),
        invariant: Some(invariant.to_owned()),
        kp: view.kp.clone(),
        item: item.and_then(|item| item.id.clone()),
        hash: item.map(|item| item.hash.clone()),
        detail,
    }
}

/// Each KP rule: I2 to I9, I11 to I14, D27, D28, D34, R3 (D31). I12, I13 and I14 read only
/// the items with `is_new`. The file rule I10 and the database rule I15 are
/// not KP rules; the subcommand runs them.
pub fn check(view: &KpView) -> Vec<Finding> {
    let mut findings = counts::check(view);
    findings.extend(grader::check(view));
    findings.extend(duplicate::check(view));
    findings.extend(label::check(view));
    findings.extend(format::check(view));
    findings.extend(r3::check(view));
    findings.extend(words::check(view));
    findings.extend(countable::check(view));
    findings
}

#[cfg(test)]
pub mod testkit {
    //! Views for the unit tests of the invariants.

    use cadus_core::curriculum::{AnswerKind, Exemplar};
    use serde_json::{Value, json};

    use super::super::kp_view::{Item, KpView};

    /// An item with the given contract JSON (`Null` = no contract).
    pub fn item(problem: &str, answer: &str, contract: Value, sketch: Option<&str>) -> Item {
        let exemplar = Exemplar {
            problem: problem.to_owned(),
            answer_contract: serde_json::from_value(contract.clone()).ok(),
            answer: answer.to_owned(),
            solution_sketch: sketch.map(str::to_owned),
            visual: None,
        };
        Item::new(exemplar, contract, AnswerKind::Numeric)
    }

    /// An `exact` item with the problem `Find <n> + 0.` and the key `<n>`.
    pub fn exact(n: usize) -> Item {
        let words = [
            "zero", "one", "two", "three", "four", "five", "six", "seven",
        ];
        let problem = format!("Find the value of item {}.", words[n % words.len()]);
        item(&problem, &n.to_string(), json!({"kind": "exact"}), None)
    }

    /// A numeric KP of the course `precalculus` with the given items.
    pub fn view(items: Vec<Item>) -> KpView {
        KpView {
            kp: "precalculus/topic/kp1".to_owned(),
            store_key: "topic/kp1".to_owned(),
            course: "precalculus".to_owned(),
            file: "curriculum/precalculus/00-unit.yaml".to_owned(),
            unit: 0,
            answer_kind: AnswerKind::Numeric,
            topic: Value::Null,
            kp_block: Value::Null,
            proof_kp: false,
            floor: 6,
            items,
            diagnostic_hash: None,
            teach_hash: None,
        }
    }

    /// The `invariant` values of the findings, in order.
    pub fn invariants(findings: &[crate::output::Finding]) -> Vec<&str> {
        findings
            .iter()
            .filter_map(|finding| finding.invariant.as_deref())
            .collect()
    }
}
