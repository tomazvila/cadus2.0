//! Numeric template gates distinguish unit names from unresolved variables.
#![allow(clippy::unwrap_used)]
mod common;
use cadus_core::answer::AnswerContract;
use common::gate::*;
use serde_json::json;

#[test]
fn numeric_templates_accept_measured_answers_and_keep_their_contract() {
    for (quantity, unit) in [
        ("angle", "degree"),
        ("length", "m"),
        ("mass", "mg"),
        ("euro", "euro"),
    ] {
        for omitted in [false, true] {
            let policy =
                json!({"kind":"unit","quantity":quantity,"unit":unit,"allow_omitted":omitted});
            let samples = json!([
                {"params":{"a":1},"expected":format!("1 {unit}")},
                {"params":{"a":12},"expected":format!("144 {unit}")}
            ]);
            let body = body_with(&[
                ("answer_contract", &policy.to_string()),
                ("samples", &samples.to_string()),
            ]);
            let doc = doc_of(&body);
            let spec = GateSpec::new(AnswerKind::Numeric, &[]);
            let verified = gate(&doc, &spec).unwrap();
            assert_eq!(verified.instances_checked, 12);
            let item = Compiled::new(&doc)
                .unwrap()
                .instantiate(bind(&[("a", 2)]))
                .unwrap();
            let contract: AnswerContract = serde_json::from_value(policy).unwrap();
            assert_eq!(
                item.answer,
                if omitted {
                    "4".to_owned()
                } else {
                    format!("4 {unit}")
                }
            );
            assert_eq!(
                item.canon,
                contract.validate_expected(&item.answer).unwrap()
            );
        }
    }
}

#[test]
fn numeric_unit_contracts_still_refuse_unresolved_variables() {
    let body = body_with(&[
        (
            "answer_contract",
            r#"{"kind":"unit","quantity":"length","unit":"m","allow_omitted":true}"#,
        ),
        ("answer_expr", r#""a*x""#),
    ]);
    let doc = doc_of(&body);
    assert!(gate(&doc, &GateSpec::new(AnswerKind::Numeric, &[])).is_err());
}
